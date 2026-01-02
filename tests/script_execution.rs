#![cfg(unix)]

// Script execution guard rails.
#[path = "support/common.rs"]
mod common;
mod support;

use anyhow::{Context, Result, bail};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::process::Command;
use support::{fencerunner_binary, repo_root};
use tempfile::TempDir;

use common::{FileGuard, FixtureRunDir, repo_guard};

// Exercises the guard rails that keep script execution deterministic and
// confined to a run dir by rejecting symlinked scripts.
#[test]
fn script_resolution_guards() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let outside = TempDir::new().context("failed to allocate outside dir")?;
    let marker = outside.path().join("should_never_run.marker");
    let outside_script = outside.path().join("outside_script.sh");
    fs::write(
        &outside_script,
        format!(
            r#"#!/bin/bash
set -euo pipefail
echo ran > "{marker}"
"#,
            marker = marker.display()
        ),
    )?;
    let mut perms = fs::metadata(&outside_script)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&outside_script, perms)?;

    let symlink_path = run_dir.path().join("tests_script_resolution_symlink.sh");
    if symlink_path.exists() {
        bail!(
            "symlink fixture already exists at {}",
            symlink_path.display()
        );
    }
    symlink(&outside_script, &symlink_path)?;
    let _symlink_guard = FileGuard {
        path: symlink_path.clone(),
    };

    let runner = fencerunner_binary(&repo_root);
    let symlink_result = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner with a symlink escape")?;
    assert!(
        !symlink_result.status.success(),
        "fencerunner followed a symlink that escapes the run dir (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&symlink_result.stdout),
        String::from_utf8_lossy(&symlink_result.stderr)
    );
    assert!(
        !marker.exists(),
        "outside script should not run when it escapes the run dir"
    );

    Ok(())
}

#[test]
fn script_discovery_rejects_symlinked_scripts_even_when_target_is_in_run_dir() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let real_script = run_dir.path().join("real_script.sh");
    fs::write(
        &real_script,
        r#"#!/bin/bash
exit 0
"#,
    )?;
    let mut perms = fs::metadata(&real_script)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&real_script, perms)?;

    let symlink_path = run_dir.path().join("symlinked_script.sh");
    symlink(&real_script, &symlink_path)?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner with a symlinked script")?;

    assert!(
        !output.status.success(),
        "fencerunner should reject symlinked scripts (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Symlinked scripts are not allowed"),
        "expected a symlink rejection error, got stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}
