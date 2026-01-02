#![allow(dead_code)]

// Shared helpers for tests that need repo-relative paths, helper binaries, or
// consistent command execution. These are intentionally small so test logic
// stays readable and focused on contracts rather than boilerplate.
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Locate the repository root for integration tests.
///
/// Tests are compiled from the repository checkout, so the build-time manifest
/// directory is a stable, unambiguous root.
pub fn repo_root() -> PathBuf {
    std::fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// Resolve a helper binary by name, after ensuring helpers are built.
/// This mirrors the harness helper search order: target/ builds.
pub fn fencerunner_binary(repo_root: &Path) -> PathBuf {
    ensure_helpers_built(repo_root).expect("failed to build fencerunner");
    let candidates = [
        repo_root.join("target").join("debug").join("fencerunner"),
        repo_root.join("target").join("release").join("fencerunner"),
    ];
    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("unable to locate fencerunner (checked target/debug, target/release)");
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

/// Build helper binaries once per test run.
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
        .arg("--bin")
        .arg("fencerunner")
        .arg("--quiet")
        .current_dir(repo_root)
        .status()
        .context("failed to compile fencerunner")?;
    if status.success() {
        BUILT.store(true, Ordering::SeqCst);
        Ok(())
    } else {
        bail!("cargo build --bins exited with {}", status);
    }
}
