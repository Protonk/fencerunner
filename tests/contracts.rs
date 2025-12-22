#![cfg(unix)]

// Contract gates and emit-record guard rails.
mod support;
#[path = "support/common.rs"]
mod common;

use anyhow::{Context, Result};
use fencerunner::emit_support::{normalize_secondary_ids, validate_status};
use std::fs;
use std::process::Command;
use support::{helper_binary, repo_root, run_command};
use tempfile::TempDir;

use common::{
    FixtureProbe, parse_boundary_object, repo_guard, sample_capability_index,
};

// Confirms the static contract gate accepts the canonical fixture probe.
#[test]
fn static_probe_contract_accepts_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut cmd = Command::new(repo_root.join("tools/validate_contract_gate.sh"));
    cmd.arg("--probe")
        .arg(fixture.probe_id())
        .arg("--static-only");
    let output = run_command(cmd)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("[PASS]"),
        "expected static contract to report PASS, stdout was: {stdout}"
    );

    Ok(())
}

// Ensures static contract enforcement rejects probes missing strict-mode
// shell options so safety rules stay consistent.
#[test]
fn static_probe_contract_rejects_missing_strict_mode() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let contents = r#"#!/usr/bin/env bash
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
probe_name="tests_static_contract_broken"
primary_capability_id="cap_fs_read_workspace_tree"
"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "true" \
  --category "fs" \
  --verb "read" \
  --target "/dev/null" \
  --status "success" \
  --errno "" \
  --message "fixture" \
  --raw-exit-code "0" \
  --payload-file /dev/null \
  --operation-args "{}"
"#;
    let broken =
        FixtureProbe::install_from_contents(&repo_root, "tests_static_contract_broken", contents)?;

    let mut cmd = Command::new(repo_root.join("tools/validate_contract_gate.sh"));
    cmd.arg("--probe")
        .arg(broken.probe_id())
        .arg("--static-only");
    let output = cmd
        .output()
        .context("failed to execute static probe contract")?;
    assert!(
        !output.status.success(),
        "static contract should fail when strict mode is missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("set -euo pipefail"),
        "expected strict-mode failure, stderr was: {stderr}"
    );

    Ok(())
}

// Exercises the dynamic probe contract gate to ensure the stub parser stays in
// sync with emit-record flag usage.
#[test]
fn dynamic_probe_contract_accepts_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut cmd = Command::new(repo_root.join("tools/validate_contract_gate.sh"));
    cmd.arg("--probe")
        .arg(fixture.probe_id())
        .env("TEST_PREFER_TARGET", "1");
    let output = cmd
        .output()
        .context("failed to execute dynamic probe contract")?;
    assert!(
        output.status.success(),
        "dynamic contract gate failed: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("dynamic gate passed"),
        "expected dynamic gate to report pass; stdout was: {stdout}"
    );
    Ok(())
}

// Ensures probe-contract-gate fails fast when static issues are present.
#[test]
fn contract_gate_rejects_static_violation() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let contents = r#"#!/usr/bin/env bash
probe_name="tests_contract_gate_static_violation"
primary_capability_id="cap_fs_read_workspace_tree"
exit 0
"#;
    let broken = FixtureProbe::install_from_contents(
        &repo_root,
        "tests_contract_gate_static_violation",
        contents,
    )?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(broken.probe_id());
    let output = cmd
        .output()
        .context("failed to execute probe-contract-gate")?;
    assert!(
        !output.status.success(),
        "probe-contract-gate should fail when static contract is violated"
    );
    Ok(())
}

// Confirms probe-contract-gate runs the fixture probe through the dynamic gate.
#[test]
fn contract_gate_dynamic_accepts_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(fixture.probe_id());
    let output = run_command(cmd)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("all gates passed"),
        "expected contract gate success summary in stdout, got: {stdout}"
    );
    Ok(())
}

// Verifies the dynamic gate detects probes that skip emit-record entirely.
#[test]
fn contract_gate_dynamic_flags_missing_emit_record() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

probe_name="tests_contract_gate_missing_emit"
primary_capability_id="cap_fs_read_workspace_tree"

# Intentionally skip emit-record to trigger dynamic gate failure.
exit 0
"#;
    let broken = FixtureProbe::install_from_contents(
        &repo_root,
        "tests_contract_gate_missing_emit",
        contents,
    )?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(broken.probe_id());
    let output = cmd
        .output()
        .context("failed to execute probe-contract-gate for missing emit-record fixture")?;
    assert!(
        !output.status.success(),
        "dynamic gate should fail when emit-record is never called"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("emit-record not called") || stderr.contains("dynamic gate failed"),
        "expected dynamic gate failure message, stderr was: {stderr}"
    );
    Ok(())
}

// Ensures the dynamic gate fails when probes emit a probe name that differs
// from the script metadata.
#[test]
fn contract_gate_dynamic_flags_probe_name_mismatch() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
probe_name="tests_contract_gate_probe_name_mismatch"
primary_capability_id="cap_fs_read_workspace_tree"
wrong_probe_name="tests_contract_gate_probe_name_mismatch_wrong"

"${emit_record_bin}" \
  --probe-name "${wrong_probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "true" \
  --category "fs" \
  --verb "read" \
  --target "/dev/null" \
  --status "success" \
  --raw-exit-code "0" \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw "{}" \
  --operation-args "{}"
"#;
    let broken = FixtureProbe::install_from_contents(
        &repo_root,
        "tests_contract_gate_probe_name_mismatch",
        contents,
    )?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(broken.probe_id());
    let output = cmd
        .output()
        .context("failed to execute probe-contract-gate for probe name mismatch")?;
    assert!(
        !output.status.success(),
        "dynamic gate should fail when probe-name mismatches script metadata"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--probe-name") && stderr.contains("does not match expected"),
        "expected probe-name mismatch error, stderr was: {stderr}"
    );
    Ok(())
}

// Ensures the dynamic gate fails when probes emit a primary capability id that
// differs from the script metadata.
#[test]
fn contract_gate_dynamic_flags_primary_capability_mismatch() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
probe_name="tests_contract_gate_primary_capability_mismatch"
primary_capability_id="cap_fs_read_workspace_tree"
wrong_primary_capability_id="cap_fs_write_workspace_tree"

"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${wrong_primary_capability_id}" \
  --command "true" \
  --category "fs" \
  --verb "read" \
  --target "/dev/null" \
  --status "success" \
  --raw-exit-code "0" \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw "{}" \
  --operation-args "{}"
"#;
    let broken = FixtureProbe::install_from_contents(
        &repo_root,
        "tests_contract_gate_primary_capability_mismatch",
        contents,
    )?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(broken.probe_id());
    let output = cmd
        .output()
        .context("failed to execute probe-contract-gate for capability mismatch")?;
    assert!(
        !output.status.success(),
        "dynamic gate should fail when primary capability mismatches script metadata"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--primary-capability-id") && stderr.contains("does not match expected"),
        "expected primary capability mismatch error, stderr was: {stderr}"
    );
    Ok(())
}

// Ensures payload size limits are enforced by the dynamic gate.
#[test]
fn contract_gate_dynamic_rejects_payload_over_limit() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1 && pwd)
emit_record_bin="${repo_root}/bin/emit-record"
probe_name="tests_contract_gate_payload_too_large"
primary_capability_id="cap_fs_read_workspace_tree"

big_payload=$(printf 'a%.0s' {1..5000})
payload_json=$(printf '{"big":"%s"}' "${big_payload}")

"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "true" \
  --category "fs" \
  --verb "read" \
  --target "/dev/null" \
  --status "success" \
  --raw-exit-code "0" \
  --payload-raw "${payload_json}" \
  --operation-args "{}"
"#;
    let broken = FixtureProbe::install_from_contents(
        &repo_root,
        "tests_contract_gate_payload_too_large",
        contents,
    )?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(broken.probe_id());
    let output = cmd
        .output()
        .context("failed to execute probe-contract-gate for payload size limit")?;
    assert!(
        !output.status.success(),
        "dynamic gate should fail when payload exceeds size limit"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("payload exceeds") && stderr.contains("4096 bytes"),
        "expected payload size error, stderr was: {stderr}"
    );
    Ok(())
}

// Runs the `probe-gate` binary so `cargo test` fails whenever the static
// contract gate over the full probe set rejects any checked-in probe.
#[test]
fn probe_test_contract_gate_succeeds() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let probe_test = helper_binary(&repo_root, "probe-gate");

    let mut cmd = Command::new(probe_test);
    cmd.current_dir(&repo_root);
    run_command(cmd)?;

    Ok(())
}

// === emit-record builders and payload helpers ===

#[test]
fn validate_status_allows_known_values() {
    for value in ["success", "denied", "partial", "error"] {
        validate_status(value).expect("status should pass");
    }
    assert!(validate_status("bogus").is_err());
}

#[test]
fn normalize_secondary_deduplicates_and_trims() -> Result<()> {
    let caps = sample_capability_index(&[
        ("cap_a", "filesystem", "os_sandbox"),
        ("cap_b", "process", "agent_runtime"),
    ])?;
    let input = vec![
        fencerunner::CapabilityId(" cap_a ".to_string()),
        fencerunner::CapabilityId("cap_b".to_string()),
        fencerunner::CapabilityId("".to_string()),
        fencerunner::CapabilityId("cap_a".to_string()),
    ];
    let normalized = normalize_secondary_ids(&caps, &input)?;
    assert_eq!(
        normalized,
        vec![
            fencerunner::CapabilityId("cap_a".to_string()),
            fencerunner::CapabilityId("cap_b".to_string())
        ]
    );
    Ok(())
}

#[test]
fn normalize_secondary_rejects_unknown() -> Result<()> {
    let caps = sample_capability_index(&[("cap_a", "filesystem", "os_sandbox")])?;
    let input = vec![
        fencerunner::CapabilityId("cap_a".to_string()),
        fencerunner::CapabilityId("cap_missing".to_string()),
    ];
    assert!(normalize_secondary_ids(&caps, &input).is_err());
    Ok(())
}

#[test]
fn emit_record_requires_primary_capability() -> Result<()> {
    let repo_root = repo_root();
    let emit_record = helper_binary(&repo_root, "emit-record");
    let output = Command::new(&emit_record)
        .arg("--probe-name")
        .arg("missing_cap")
        .arg("--probe-version")
        .arg("1")
        .arg("--command")
        .arg("true")
        .arg("--category")
        .arg("fs")
        .arg("--verb")
        .arg("read")
        .arg("--target")
        .arg("/tmp")
        .arg("--status")
        .arg("success")
        .arg("--operation-args")
        .arg("{}")
        .output()
        .context("failed to execute emit-record without primary capability")?;
    assert!(
        !output.status.success(),
        "emit-record should fail when primary capability is missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Missing required flag") || stderr.contains("primary capability"),
        "stderr should mention missing primary capability; got {stderr}"
    );
    Ok(())
}

#[test]
fn emit_record_rejects_unknown_capability() -> Result<()> {
    let repo_root = repo_root();
    let emit_record = helper_binary(&repo_root, "emit-record");

    let output = Command::new(&emit_record)
        .arg("--probe-name")
        .arg("tests_unknown_cap")
        .arg("--probe-version")
        .arg("1")
        .arg("--primary-capability-id")
        .arg("cap_missing")
        .arg("--command")
        .arg("true")
        .arg("--category")
        .arg("fs")
        .arg("--verb")
        .arg("read")
        .arg("--target")
        .arg("/tmp")
        .arg("--status")
        .arg("success")
        .arg("--operation-args")
        .arg("{}")
        .output()
        .context("failed to execute emit-record with unknown capability")?;

    assert!(
        !output.status.success(),
        "emit-record should fail when capability id is missing"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("primary capability id") && stderr.contains("cap_missing"),
        "stderr should mention the missing capability; got: {stderr}"
    );
    Ok(())
}

#[test]
fn emit_record_falls_back_to_pwd_for_workspace_root() -> Result<()> {
    let repo_root = repo_root();
    let emit_record = helper_binary(&repo_root, "emit-record");
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let pwd = fs::canonicalize(temp.path())?;

    let output = Command::new(&emit_record)
        .current_dir(&pwd)
        .env("FENCE_WORKSPACE_ROOT", "")
        .env("PWD", &pwd)
        .arg("--probe-name")
        .arg("tests_workspace_fallback")
        .arg("--probe-version")
        .arg("1")
        .arg("--primary-capability-id")
        .arg("cap_fs_read_workspace_tree")
        .arg("--command")
        .arg("true")
        .arg("--category")
        .arg("fs")
        .arg("--verb")
        .arg("read")
        .arg("--target")
        .arg("/tmp")
        .arg("--status")
        .arg("success")
        .arg("--operation-args")
        .arg("{}")
        .output()
        .context("failed to execute emit-record for workspace fallback")?;
    assert!(output.status.success(), "emit-record should succeed");
    let (record, _) = parse_boundary_object(&output.stdout)?;
    let recorded = record.run.workspace_root.expect("workspace_root present");
    assert_eq!(fs::canonicalize(recorded)?, pwd);
    Ok(())
}
