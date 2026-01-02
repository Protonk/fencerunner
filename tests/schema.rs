#![cfg(unix)]

// Schema and serialization guard rails: boundary object shape and serde round-trips.
#[path = "support/common.rs"]
mod common;
mod support;

use anyhow::{Context, Result};
use fencerunner::boundary::{BoundaryContractIndex, BoundaryObject, CommitmentEnrollment, RunInfo};
use fencerunner::commitments::model::CommitmentHelp;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::sync::OnceLock;
use support::{fencerunner_binary, repo_root, run_command};
use tempfile::NamedTempFile;

use common::{parse_boundary_object, sample_boundary_object};

// Ensures emit-record produces schema-valid boundary objects, including the
// required context.commitments field.
#[test]
fn boundary_object_schema() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);
    let payload = json!({
        "stdout_snippet": "fixture-stdout",
        "stderr_snippet": "fixture-stderr",
        "raw": {"detail": "schema-test"}
    });

    let mut payload_file = NamedTempFile::new().context("failed to allocate payload file")?;
    serde_json::to_writer(&mut payload_file, &payload)?;

    let enrollments_file = NamedTempFile::new().context("failed to allocate enrollments file")?;
    fs::write(enrollments_file.path(), "python3|ensure\n")
        .context("failed to write enrollments file")?;

    let mut emit_cmd = std::process::Command::new(&fencerunner);
    emit_cmd
        .arg("__emit-record")
        .arg("--script-name")
        .arg("schema_test_fixture")
        .arg("--command")
        .arg("printf fixture")
        .arg("--operation-kind")
        .arg("fs.read")
        .arg("--target")
        .arg("/dev/null")
        .arg("--outcome")
        .arg("success")
        .arg("--exit-code")
        .arg("0")
        .arg("--message")
        .arg("fixture")
        .arg("--operation-args")
        .arg("{\"fixture\":true}")
        .arg("--payload-file")
        .arg(payload_file.path());
    emit_cmd.env(
        "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH",
        enrollments_file.path(),
    );
    emit_cmd.env("FENCERUNNER_RUN_DIR", repo_root.join("scripts"));
    let output = run_command(emit_cmd)?;

    let (record, value) = parse_boundary_object(&output.stdout)?;
    assert_eq!(record.script.id, "schema_test_fixture");
    assert_eq!(record.operation.kind, "fs.read");
    assert_eq!(record.operation.target, "/dev/null");

    assert!(
        value
            .pointer("/context/commitments")
            .map(|caps| caps.is_array())
            .unwrap_or(false),
        "context.commitments must be present"
    );
    assert!(
        record
            .context
            .commitments
            .iter()
            .any(|cap| cap.id == "python3"),
        "expected python3 enrollment to be recorded"
    );

    // Cache schema parsing across tests; JSONSchema compilation is expensive.
    static BOUNDARY_CONTRACT: OnceLock<BoundaryContractIndex> = OnceLock::new();
    let contract = BOUNDARY_CONTRACT.get_or_init(|| {
        let path = repo_root.join("scripts/boundaries.json");
        BoundaryContractIndex::load(&path).expect("load boundary contract")
    });
    contract.validate_record(&value)?;

    Ok(())
}

#[test]
fn boundary_object_round_trips_structs() -> Result<()> {
    let bo = sample_boundary_object();
    let value = serde_json::to_value(&bo)?;
    let back: BoundaryObject = serde_json::from_value(value)?;
    assert_eq!(back.operation.kind, "fs.read");
    assert_eq!(back.result.outcome, "success");
    let run_command = back.context.run.as_ref().map(|run| run.command.as_str());
    assert_eq!(run_command, Some("echo test"));
    Ok(())
}

#[test]
fn commitment_enrollment_serializes_to_expected_shape() -> Result<()> {
    let enrollment = CommitmentEnrollment {
        id: "python3".to_string(),
        helps: vec![CommitmentHelp::Ensure],
    };
    let value = serde_json::to_value(&enrollment)?;
    assert_eq!(value.get("id").and_then(Value::as_str), Some("python3"));
    assert_eq!(
        value.pointer("/helps/0").and_then(Value::as_str),
        Some("ensure")
    );
    Ok(())
}

#[test]
fn run_info_omits_workspace_root_when_none() -> Result<()> {
    let run = RunInfo {
        workspace_root: None,
        command: "true".to_string(),
    };
    let value = serde_json::to_value(&run)?;
    assert!(value.get("workspace_root").is_none());
    Ok(())
}

// schema-validate is a user-facing helper; smoke-test that it can validate
// boundaries.json and boundary records using the manual arg parser.
#[test]
fn schema_validate_boundary_contract_smoke() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);
    let output = std::process::Command::new(&fencerunner)
        .arg("__schema-validate")
        .arg("--mode")
        .arg("boundaries-contract")
        .arg("--file")
        .arg("scripts/boundaries.json")
        .current_dir(&repo_root)
        .output()
        .context("failed to execute schema_validate boundaries-contract")?;
    assert!(
        output.status.success(),
        "schema_validate boundaries-contract should succeed (stderr: {})",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn schema_validate_boundary_accepts_stdin() -> Result<()> {
    let repo_root = repo_root();
    let fencerunner = fencerunner_binary(&repo_root);

    let record = sample_boundary_object();
    let input = serde_json::to_string(&record)?;

    let mut child = std::process::Command::new(&fencerunner)
        .arg("__schema-validate")
        .arg("--mode")
        .arg("boundary")
        .arg("--contract")
        .arg("scripts/boundaries.json")
        .current_dir(&repo_root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn schema_validate boundary")?;

    let mut stdin = child.stdin.take().expect("stdin piped");
    stdin.write_all(input.as_bytes())?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .context("failed to wait for schema_validate boundary")?;
    assert!(
        output.status.success(),
        "schema_validate boundary should succeed (stderr: {})",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}
