//! Top-level CLI wrapper that delegates to the synced helper binaries.
//!
//! The binary keeps the public `codex-fence --bang/--listen` interface stable
//! while resolving the real helper paths (preferring the synced `bin/`
//! artifacts). It also injects `CODEX_FENCE_ROOT` when possible so helpers can
//! locate probes and fixtures even when invoked from an installed location.

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
    let repo_root = find_repo_root().ok();

    run_helper(&cli, repo_root.as_deref())
}

struct Cli {
    command: CommandTarget,
    trailing_args: Vec<OsString>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandTarget {
    Bang,
    Listen,
    Rattle,
}

impl CommandTarget {
    fn helper_name(self) -> &'static str {
        match self {
            CommandTarget::Bang => "fence-bang",
            CommandTarget::Listen => "fence-listen",
            CommandTarget::Rattle => "fence-rattle",
        }
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
            "--rattle" => CommandTarget::Rattle,
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
        "Usage: codex-fence (--bang | --listen | --rattle) [args]\n\nCommands:\n  --bang, -b   Run the full probe matrix once and emit cfbo-v1 records (NDJSON).\n  --listen, -l Read cfbo-v1 JSON from stdin and print a human summary.\n  --rattle     Run a targeted probe subset (see fence-rattle --help).\n\nExamples:\n  codex-fence --bang | codex-fence --listen\n  codex-fence --rattle --probe fs_read_workspace_readme --mode baseline"
    );
    std::process::exit(code);
}

/// Locate the requested helper, preferring the repo-synced binaries.
///
/// The search order mirrors the harness contract: repo root, sibling directory
/// to the current executable (useful for installed binaries), then PATH.
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

/// Execute the resolved helper, wiring CODEX_FENCE_ROOT when available.
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
