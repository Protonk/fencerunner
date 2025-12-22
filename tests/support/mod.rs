#![allow(dead_code)]

// Shared helpers for tests that need repo-relative paths, helper binaries, or
// consistent command execution. These are intentionally small so test logic
// stays readable and focused on contracts rather than boilerplate.
use anyhow::{Context, Result, bail};
use fencerunner::repo_tools::find_repo_root;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Locate the repository root using the same heuristics as production code.
/// This keeps tests honest about how helpers find the repo.
pub fn repo_root() -> PathBuf {
    find_repo_root().expect("tests require repository root")
}

/// Resolve a helper binary by name, after ensuring helpers are built.
/// This mirrors the harness helper search order: target/ builds and then bin/.
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

/// Run a command and convert non-zero exits into detailed errors.
/// This keeps tests concise and ensures failures show stdout/stderr.
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

/// Mark a file executable on Unix (no-op on non-Unix builds).
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

/// Build helper binaries once per test run to keep `bin/` and `target/` in sync.
/// The AtomicBool avoids repeated work; the Mutex prevents concurrent builds.
fn ensure_helpers_built(repo_root: &Path) -> Result<()> {
    static BUILT: AtomicBool = AtomicBool::new(false);
    if BUILT.load(Ordering::SeqCst) {
        return Ok(());
    }

    // OnceLock gives us a lazily initialized static Mutex without unsafe code.
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
