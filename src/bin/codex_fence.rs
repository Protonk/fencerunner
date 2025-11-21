use anyhow::{Context, Result, bail};
use codex_fence::{find_repo_root, resolve_helper_binary};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse()?;
    let repo_root = match find_repo_root() {
        Ok(path) => Some(path),
        Err(err) => {
            if cli.command.requires_repo_root() {
                return Err(err);
            }
            None
        }
    };

    match cli.command {
        _ => run_helper(&cli, repo_root.as_deref()),
    }
}

struct Cli {
    command: CommandTarget,
    trailing_args: Vec<OsString>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandTarget {
    Bang,
    Listen,
    Test,
}

impl CommandTarget {
    fn helper_name(self) -> &'static str {
        match self {
            CommandTarget::Bang => "fence-bang",
            CommandTarget::Listen => "fence-listen",
            CommandTarget::Test => "fence-test",
        }
    }

    fn requires_repo_root(self) -> bool {
        matches!(self, CommandTarget::Test)
    }
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();

        let Some(flag) = args.next() else {
            usage(1);
        };

        let flag_str = flag
            .to_str()
            .with_context(|| "Invalid UTF-8 in command flag")?;

        let command = match flag_str {
            "--bang" | "-b" => CommandTarget::Bang,
            "--listen" | "-l" => CommandTarget::Listen,
            "--test" | "-t" => CommandTarget::Test,
            "--help" | "-h" => usage(0),
            _ => usage(1),
        };

        let trailing_args = args.collect();
        Ok(Self {
            command,
            trailing_args,
        })
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: codex-fence (--bang | --listen | --test) [args]\n\nCommands:\n  --bang, -b   Run the probe matrix and emit cfbo-v1 records to stdout (NDJSON).\n  --listen, -l Read cfbo-v1 JSON from stdin and print a human summary.\n  --test, -t   Run the static probe contract across every probes/*.sh script.\n\nExample:\n  codex-fence --bang | codex-fence --listen"
    );
    std::process::exit(code);
}

fn resolve_helper(name: &str, repo_root: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = repo_root {
        if let Ok(path) = resolve_helper_binary(root, name) {
            return Ok(path);
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join(name);
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }

    if let Some(path) = find_on_path(name) {
        return Ok(path);
    }

    bail!(
        "Unable to locate helper '{name}'. Run 'make build-bin' (or tools/sync_bin_helpers.sh) or set CODEX_FENCE_ROOT."
    )
}

fn run_helper(cli: &Cli, repo_root: Option<&Path>) -> Result<()> {
    let helper_path = resolve_helper(cli.command.helper_name(), repo_root)?;
    let mut command = Command::new(&helper_path);
    command.args(&cli.trailing_args);

    if let Some(root) = repo_root {
        if env::var_os("CODEX_FENCE_ROOT").is_none() {
            command.env("CODEX_FENCE_ROOT", root);
        }
    }

    let status = command
        .status()
        .with_context(|| format!("Failed to execute {}", helper_path.display()))?;

    if status.success() {
        return Ok(());
    }

    if let Some(code) = status.code() {
        std::process::exit(code);
    }

    bail!("Helper terminated by signal")
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn finds_helper_in_repo_root() {
        let temp = TempDir::new();
        let bin_dir = temp.root.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let helper = bin_dir.join("fence-bang");
        fs::write(&helper, "#!/bin/sh\n").unwrap();
        make_executable(&helper);
        let resolved = resolve_helper("fence-bang", Some(&temp.root)).unwrap();
        assert_eq!(resolved, helper);
    }

    #[test]
    fn finds_helper_on_path() {
        let temp = TempDir::new();
        let helper = temp.root.join("fence-test");
        fs::write(&helper, "#!/bin/sh\n").unwrap();
        make_executable(&helper);
        let original_path = env::var_os("PATH");
        unsafe {
            env::set_var("PATH", temp.root.to_str().unwrap());
        }
        let resolved = resolve_helper("fence-test", None).unwrap();
        assert_eq!(resolved, helper);
        if let Some(path) = original_path {
            unsafe {
                env::set_var("PATH", path);
            }
        } else {
            unsafe {
                env::remove_var("PATH");
            }
        }
    }

    struct TempDir {
        root: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let mut dir = env::temp_dir();
            dir.push(format!(
                "codex-fence-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self { root: dir }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}
}
