#![cfg(unix)]

// emit-record guard rails.
#[path = "support/common.rs"]
mod common;
mod support;

use anyhow::{Context, Result};
use fencerunner::harness::payload::validate_outcome;
use std::fs;
use std::process::Command;
use support::{fencerunner_binary, repo_root};
use tempfile::{NamedTempFile, TempDir};

use common::parse_boundary_object;

// === payload helpers ===

// validate_outcome is a small guard; ensure it rejects unknown values.
#[test]
fn validate_outcome_allows_known_values() {
    for value in ["success", "denied", "partial", "error"] {
        validate_outcome(value).expect("outcome should pass");
    }
    assert!(validate_outcome("bogus").is_err());
}

#[test]
fn emit_record_rejects_duplicate_commitment_enrollment() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);

    let enrollments_file = NamedTempFile::new().context("failed to allocate enrollments file")?;
    fs::write(enrollments_file.path(), "python3|ensure\npython3|ensure\n")
        .context("failed to write enrollments file")?;

    let output = Command::new(&fencerunner)
        .env(
            "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH",
            enrollments_file.path(),
        )
        .env("FENCERUNNER_RUN_DIR", repo_root.join("scripts"))
        .arg("__emit-record")
        .arg("--script-name")
        .arg("tests_duplicate_commitment_help")
        .arg("--command")
        .arg("true")
        .arg("--operation-kind")
        .arg("fs.read")
        .arg("--target")
        .arg("/tmp")
        .arg("--outcome")
        .arg("success")
        .arg("--payload-stdout")
        .arg("")
        .arg("--payload-stderr")
        .arg("")
        .arg("--operation-args")
        .arg("{}")
        .output()
        .context("failed to execute emit-record with duplicate commitment enrollment")?;

    assert!(
        !output.status.success(),
        "emit-record should fail on duplicate (id, help) pairs"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Duplicate commitment enrollment"),
        "stderr should mention duplicate enrollment; got: {stderr}"
    );
    Ok(())
}

// emit-record should omit context.run.workspace_root by default.
#[test]
fn emit_record_omits_workspace_root_by_default() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let pwd = fs::canonicalize(temp.path())?;

    let output = Command::new(&fencerunner)
        .current_dir(&pwd)
        .env("FENCERUNNER_RUN_DIR", repo_root.join("scripts"))
        .arg("__emit-record")
        .arg("--script-name")
        .arg("tests_workspace_fallback")
        .arg("--command")
        .arg("true")
        .arg("--operation-kind")
        .arg("fs.read")
        .arg("--target")
        .arg("/tmp")
        .arg("--outcome")
        .arg("success")
        .arg("--payload-stdout")
        .arg("")
        .arg("--payload-stderr")
        .arg("")
        .arg("--operation-args")
        .arg("{}")
        .output()
        .context("failed to execute emit-record for workspace fallback")?;
    assert!(output.status.success(), "emit-record should succeed");
    let (record, _) = parse_boundary_object(&output.stdout)?;
    let workspace_root = record
        .context
        .run
        .as_ref()
        .and_then(|run| run.workspace_root.clone());
    assert!(
        workspace_root.is_none(),
        "expected workspace_root to be omitted, got: {workspace_root:?}"
    );
    Ok(())
}

// emit-record should require stdout/stderr payload snippets to keep payload shape uniform.
#[test]
fn emit_record_requires_payload_snippets() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);

    let output = Command::new(&fencerunner)
        .env("FENCERUNNER_RUN_DIR", repo_root.join("scripts"))
        .arg("__emit-record")
        .arg("--script-name")
        .arg("tests_missing_payload")
        .arg("--command")
        .arg("true")
        .arg("--operation-kind")
        .arg("fs.read")
        .arg("--target")
        .arg("/tmp")
        .arg("--outcome")
        .arg("success")
        .arg("--operation-args")
        .arg("{}")
        .output()
        .context("failed to execute emit-record without payload snippets")?;

    assert!(
        !output.status.success(),
        "emit-record should fail when payload snippets are missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("payload-stdout") || stderr.contains("payload-stderr"),
        "expected missing payload hint; stderr was: {stderr}"
    );

    Ok(())
}

// commit-help-me should append enrollments in a stable format and reject duplicate
// (id, help) pairs. emit-record relies on this file being de-duplicated.
#[test]
fn commit_help_me_records_enrollment_and_rejects_duplicates() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);

    let enrollments_file = NamedTempFile::new().context("failed to allocate enrollments file")?;

    let first = Command::new(&fencerunner)
        .env(
            "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH",
            enrollments_file.path(),
        )
        .arg("__commit-help-me")
        .arg("ensure")
        .arg("python3")
        .output()
        .context("failed to execute commit-help-me")?;
    assert!(
        first.status.success(),
        "commit-help-me should succeed on first enrollment (stderr: {})",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = Command::new(&fencerunner)
        .env(
            "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH",
            enrollments_file.path(),
        )
        .arg("__commit-help-me")
        .arg("ensure")
        .arg("python3")
        .output()
        .context("failed to execute commit-help-me for duplicate enrollment")?;
    assert!(
        !second.status.success(),
        "commit-help-me should fail on duplicate enrollment"
    );

    let contents =
        fs::read_to_string(enrollments_file.path()).context("failed to read enrollments file")?;
    assert_eq!(contents.trim(), "python3|ensure");

    Ok(())
}
