use anyhow::{Context, Result, bail};
use codex_fence::find_repo_root;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

pub fn repo_root() -> PathBuf {
    find_repo_root().expect("tests require repository root")
}

pub fn helper_binary(repo_root: &Path, name: &str) -> PathBuf {
    ensure_helpers_built(repo_root).expect("failed to build helper binaries");
    let candidates = [
        repo_root.join("target").join("debug").join(name),
        repo_root.join("target").join("release").join(name),
        repo_root.join("bin").join(name),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!(
        "unable to locate helper {} (checked target/debug, target/release, bin)",
        name
    );
}

pub fn run_command(mut cmd: Command) -> Result<Output> {
    let output = cmd
        .output()
        .with_context(|| format!("failed to run command: {:?}", cmd))?;
    if output.status.success() {
        Ok(output)
    } else {
        bail!(
            "command {:?} failed: status {:?}\nstdout: {}\nstderr: {}",
            cmd,
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
}

pub fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn ensure_helpers_built(repo_root: &Path) -> Result<()> {
    static BUILT: AtomicBool = AtomicBool::new(false);
    if BUILT.load(Ordering::SeqCst) {
        return Ok(());
    }

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let mutex = LOCK.get_or_init(|| Mutex::new(()));
    let _guard = mutex.lock().unwrap_or_else(|err| err.into_inner());

    if BUILT.load(Ordering::SeqCst) {
        return Ok(());
    }

    let status = Command::new("cargo")
        .arg("build")
        .arg("--bins")
        .arg("--quiet")
        .current_dir(repo_root)
        .status()
        .context("failed to compile helper binaries")?;
    if status.success() {
        BUILT.store(true, Ordering::SeqCst);
        Ok(())
    } else {
        bail!("cargo build --bins exited with {}", status);
    }
}
