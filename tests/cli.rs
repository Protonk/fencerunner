#![cfg(unix)]

// CLI and harness behavior guard rails for fencerunner and helper binaries.
#[path = "support/common.rs"]
mod common;
mod support;

use anyhow::{Context, Result};
use fencerunner::commitments::model::CommitmentHelp;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use support::{fencerunner_binary, repo_root, run_command};
use tempfile::TempDir;

use common::{FixtureProbe, FixtureRunDir, parse_boundary_object, repo_guard};

// Ensures fencerunner --strict surfaces malformed probe output without blocking
// the remaining probes from running. The runner should still emit valid records
// from other probes.
#[test]
fn fencerunner_strict_continues_after_malformed_probe() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    // Name the malformed probe so it sorts first; this makes the test
    // non-vacuous by ensuring fencerunner --strict still runs later probes after
    // an early failure.
    let broken_contents = r#"#!/usr/bin/env bash
set -euo pipefail
echo not-json
exit 0
"#;
    let broken = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "aaa_malformed_probe",
        broken_contents,
    )?;
    let good = FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "zzz_fixture_probe")?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = cmd
        .output()
        .context("failed to execute fencerunner --strict with malformed probe")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail when a probe emits invalid JSON"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected only the valid probe output to remain on stdout"
    );
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, good.probe_id());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(broken.probe_id()),
        "stderr should mention the malformed probe id; stderr was: {stderr}"
    );

    Ok(())
}

// Smoke-tests fencerunner end-to-end with a run dir containing a single probe.
// This validates helper resolution, probe execution, and NDJSON output format.
#[test]
fn fencerunner_runs_single_probe_in_run_dir() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let fixture =
        FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "tests_fixture_probe")?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg(run_dir.path()).current_dir(&repo_root);
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one record for a single probe"
    );
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, fixture.probe_id());
    assert!(
        record
            .context
            .run
            .as_ref()
            .map(|run| run.command == "true")
            .unwrap_or(false),
        "expected fixture command to be 'true'"
    );

    Ok(())
}

// Smoke-tests fencerunner over an isolated run dir with multiple probes.
// This validates multi-probe NDJSON streaming.
#[test]
fn fencerunner_runs_all_probes_in_run_dir() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let first =
        FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "tests_fixture_probe_first")?;
    let second =
        FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "tests_fixture_probe_second")?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg(run_dir.path()).current_dir(&repo_root);
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("bang stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "expected exactly two records for two probes"
    );

    let mut saw_first = false;
    let mut saw_second = false;
    for line in lines {
        let (record, _) = parse_boundary_object(line.as_bytes())?;
        let has_command = record
            .context
            .run
            .as_ref()
            .map(|run| !run.command.is_empty())
            .unwrap_or(false);
        assert!(has_command);

        if record.probe.id == first.probe_id() {
            saw_first = true;
        }
        if record.probe.id == second.probe_id() {
            saw_second = true;
        }
    }
    assert!(saw_first, "expected to see first fixture probe record");
    assert!(saw_second, "expected to see second fixture probe record");

    Ok(())
}

// Run dirs are flat: nested subdirectories are ignored during probe discovery.
#[test]
fn fencerunner_ignores_nested_probe_scripts() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let top_level =
        FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "tests_fixture_probe")?;

    let nested_dir = run_dir.path().join("nested");
    fs::create_dir_all(&nested_dir).context("create nested directory")?;
    let marker = run_dir.path().join("nested_probe_ran.marker");
    let nested_script = nested_dir.join("should_not_run.sh");
    let nested_contents = r#"#!/usr/bin/env bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"

touch "${FENCERUNNER_RUN_DIR}/nested_probe_ran.marker"

emit-record \
  --probe-name "should_not_run" \
  --command "true" \
  --operation-kind "test.nested" \
  --target "nested" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "note" "nested probe ran"
"#;
    fs::write(&nested_script, nested_contents).context("write nested probe script")?;
    let mut perms = fs::metadata(&nested_script)
        .context("read nested probe permissions")?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&nested_script, perms).context("make nested probe executable")?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected only top-level probes to run, nested scripts should be ignored"
    );
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, top_level.probe_id());

    assert!(
        !marker.exists(),
        "nested probe script should never run, marker file was created at {}",
        marker.display()
    );

    Ok(())
}

// In supervised mode, fencerunner forwards valid boundary objects and writes
// monitoring to stderr.
#[test]
fn fencerunner_supervised_forwards_boundary_object_from_run_dir() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let fixture =
        FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "tests_fixture_probe")?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, fixture.probe_id());
    Ok(())
}

// Supervised mode must keep stdout a well-formed NDJSON stream even when a
// probe emits pretty-printed (multi-line) JSON. The runner should normalize
// the record to a single NDJSON line.
#[test]
fn fencerunner_supervised_outputs_single_line_ndjson_when_probe_emits_pretty_json() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

probe_id="$(basename "${BASH_SOURCE[0]}" .sh)"

cat <<EOF
{
  "probe": { "id": "${probe_id}" },
  "operation": { "kind": "test.pretty", "target": "x" },
  "result": { "outcome": "success" },
  "context": { "commitments": [] }
}
EOF
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_pretty_json_probe",
        contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one NDJSON line");
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, probe.probe_id());
    Ok(())
}

// If a probe fails to emit a boundary object (for example, it only writes to
// stderr), supervised mode emits a synthetic boundary object so downstream
// tooling still gets one record per probe.
#[test]
fn fencerunner_supervised_emits_synthetic_record_when_probe_only_writes_stderr() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail
echo "probe wrote to stderr" >&2
exit 0
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_stderr_only_probe",
        contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");

    let (record, value) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, probe.probe_id());
    assert_eq!(record.result.outcome, "error");
    assert_eq!(record.operation.kind, "harness.supervised");
    assert!(
        value
            .get("extensions")
            .and_then(|ext| ext.get("synthetic"))
            .is_some(),
        "expected extensions.synthetic marker"
    );
    let stderr_snippet = value
        .pointer("/payload/stderr_snippet")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        stderr_snippet.contains("probe wrote to stderr"),
        "expected captured stderr in payload"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("probe wrote to stderr"),
        "expected stderr traffic to be surfaced on stderr; stderr was: {stderr}"
    );
    assert!(
        stderr.contains("synthetic"),
        "expected stderr to mention synthetic record emission; stderr was: {stderr}"
    );

    Ok(())
}

// If a probe enrolls in commitments and then breaks the stdout contract,
// supervised mode should emit a synthetic record that still carries those
// enrollments in /context/commitments.
#[test]
fn fencerunner_supervised_synthetic_record_includes_commitment_enrollments() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"

commit_help_me ensure python3

echo not-json
exit 0
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_enrollment_before_failure",
        contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");

    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, probe.probe_id());
    assert_eq!(record.result.outcome, "error");
    assert!(
        record
            .context
            .commitments
            .iter()
            .any(|enrollment| enrollment.id == "python3"
                && enrollment
                    .helps
                    .iter()
                    .any(|help| help == &CommitmentHelp::Ensure)),
        "expected python3 ensure enrollment to be preserved in synthetic record"
    );

    Ok(())
}

// If a probe emits a schema-valid record whose probe.id does not match the
// script filename stem, treat it as a contract break.
#[test]
fn fencerunner_supervised_emits_synthetic_record_when_probe_id_mismatches_filename() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

probe_id="$(basename "${BASH_SOURCE[0]}" .sh)"

emit-record \
  --probe-name "wrong_id" \
  --command "true" \
  --operation-kind "test.identity" \
  --target "probe.id" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "example" "probe-id-mismatch"
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_probe_id_mismatch",
        contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");

    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(
        record.probe.id,
        probe.probe_id(),
        "synthetic record should use the script-derived probe id"
    );
    assert_eq!(record.result.outcome, "error");
    assert!(
        record
            .result
            .details
            .as_ref()
            .and_then(|details| details.message.as_deref())
            .unwrap_or("")
            .contains("does not match script id"),
        "expected synthetic record to explain the id mismatch"
    );

    Ok(())
}

// Strict mode treats probe.id mismatches as failures and does not emit a record.
#[test]
fn fencerunner_strict_fails_when_probe_id_mismatches_filename() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let contents = r#"#!/usr/bin/env bash
set -euo pipefail

emit-record \
  --probe-name "wrong_id" \
  --command "true" \
  --operation-kind "test.identity" \
  --target "probe.id" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "example" "probe-id-mismatch"
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_probe_id_mismatch_strict",
        contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --strict with probe.id mismatch")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail when probe.id does not match filename"
    );
    assert!(
        output.stdout.is_empty(),
        "strict mode should not emit a record for a probe.id mismatch"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(probe.probe_id()),
        "stderr should mention the probe id; stderr was: {stderr}"
    );
    assert!(
        stderr.contains("does not match script id"),
        "stderr should mention the mismatch; stderr was: {stderr}"
    );

    Ok(())
}

// If a probe script is not executable, that probe cannot be spawned; treat it
// as a preflight/runner failure (non-zero exit) rather than a synthetic probe
// record.
#[test]
fn fencerunner_supervised_fails_when_probe_is_not_executable() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let probe_id = "tests_non_executable_probe";
    let probe_path = run_dir.path().join(format!("{probe_id}.sh"));
    fs::write(
        &probe_path,
        r#"#!/usr/bin/env bash
set -euo pipefail
exit 0
"#,
    )?;
    let mut perms = fs::metadata(&probe_path)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&probe_path, perms)?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --supervised with non-executable probe")?;
    assert!(
        !output.status.success(),
        "fencerunner --supervised should fail when a probe cannot be spawned"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when probe spawn fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not executable"),
        "expected stderr to mention not executable; stderr was: {stderr}"
    );

    Ok(())
}

// Preflight should fail before running any probes when a run dir is missing a
// required contract file (boundaries.json, commitments.json, gates.json).
#[test]
fn fencerunner_strict_aborts_on_missing_boundaries_contract_without_running_probes() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    fs::remove_file(run_dir.path().join("boundaries.json"))?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = cmd
        .output()
        .context("failed to execute fencerunner --strict with missing boundaries.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail preflight when boundaries.json is missing"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("boundaries.json"),
        "stderr should mention boundaries.json; stderr was: {stderr}"
    );

    Ok(())
}

#[test]
fn fencerunner_strict_aborts_on_missing_commitments_contract_without_running_probes() -> Result<()>
{
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    fs::remove_file(run_dir.path().join("commitments.json"))?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --strict with missing commitments.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail preflight when commitments.json is missing"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("commitments.json"),
        "stderr should mention commitments.json; stderr was: {stderr}"
    );

    Ok(())
}

#[test]
fn fencerunner_strict_aborts_on_missing_gates_contract_without_running_probes() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    fs::remove_file(run_dir.path().join("gates.json"))?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --strict with missing gates.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail preflight when gates.json is missing"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("gates.json"),
        "stderr should mention gates.json; stderr was: {stderr}"
    );

    Ok(())
}

#[test]
fn fencerunner_supervised_aborts_on_missing_boundaries_contract_without_running_probes()
-> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    fs::remove_file(run_dir.path().join("boundaries.json"))?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --supervised with missing boundaries.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --supervised should fail preflight when boundaries.json is missing"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("boundaries.json"),
        "stderr should mention boundaries.json; stderr was: {stderr}"
    );

    Ok(())
}

#[test]
fn fencerunner_supervised_aborts_on_missing_commitments_contract_without_running_probes()
-> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    fs::remove_file(run_dir.path().join("commitments.json"))?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --supervised with missing commitments.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --supervised should fail preflight when commitments.json is missing"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("commitments.json"),
        "stderr should mention commitments.json; stderr was: {stderr}"
    );

    Ok(())
}

#[test]
fn fencerunner_supervised_aborts_on_missing_gates_contract_without_running_probes() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    fs::remove_file(run_dir.path().join("gates.json"))?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --supervised with missing gates.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --supervised should fail preflight when gates.json is missing"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("gates.json"),
        "stderr should mention gates.json; stderr was: {stderr}"
    );

    Ok(())
}

#[test]
fn fencerunner_strict_aborts_on_invalid_gates_contract_without_running_probes() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let invalid_gates = serde_json::json!({
        "schema_version": "gates_v1",
        "gates": {
            "enforced_checks": ["bogus.check"]
        }
    });
    fs::write(
        run_dir.path().join("gates.json"),
        serde_json::to_string_pretty(&invalid_gates)?,
    )?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --strict with invalid gates.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail preflight when gates.json is invalid"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("gates.json"),
        "stderr should mention gates.json; stderr was: {stderr}"
    );
    Ok(())
}

#[test]
fn fencerunner_strict_aborts_on_invalid_commitments_contract_without_running_probes() -> Result<()>
{
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let invalid_commitments = serde_json::json!({
        "schema_version": "unexpected",
        "commitments": []
    });
    fs::write(
        run_dir.path().join("commitments.json"),
        serde_json::to_string_pretty(&invalid_commitments)?,
    )?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --strict with invalid commitments.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail preflight when commitments.json is invalid"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("commitments.json"),
        "stderr should mention commitments.json; stderr was: {stderr}"
    );
    Ok(())
}

#[test]
fn fencerunner_strict_aborts_on_invalid_boundaries_contract_without_running_probes() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let invalid_boundaries = serde_json::json!({
        "schema_version": "boundaries_v1",
        "stdout": { "format": "text" },
        "record_schema": {
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "required": ["probe", "result", "context"],
            "properties": {
                "probe": { "type": "object", "required": ["id"], "properties": { "id": { "type": "string" } } },
                "result": { "type": "object", "required": ["outcome"], "properties": { "outcome": { "type": "string" } } },
                "context": { "type": "object", "required": ["commitments"], "properties": { "commitments": { "type": "array" } } }
            }
        }
    });
    fs::write(
        run_dir.path().join("boundaries.json"),
        serde_json::to_string_pretty(&invalid_boundaries)?,
    )?;

    let marker = run_dir.path().join("probe_should_not_run.marker");
    let probe_contents = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo ran > "{marker}"
exit 0
"#,
        marker = marker.display()
    );
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "zzz_should_not_run",
        &probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner --strict with invalid boundaries.json")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail preflight when boundaries.json is invalid"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when preflight fails"
    );
    assert!(
        !marker.exists(),
        "probe should not run when preflight fails"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("boundaries.json"),
        "stderr should mention boundaries.json; stderr was: {stderr}"
    );
    Ok(())
}

// When gates.json enrolls in the stderr.empty gate, supervised mode should
// treat any stderr output as a probe contract violation and emit a synthetic
// record.
#[test]
fn fencerunner_supervised_emits_synthetic_record_when_stderr_empty_is_enforced() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let gates_contract = serde_json::json!({
        "schema_version": "gates_v1",
        "gates": {
            "enforced_checks": ["stderr.empty"]
        }
    });
    fs::write(
        run_dir.path().join("gates.json"),
        serde_json::to_string_pretty(&gates_contract)?,
    )?;

    let probe_contents = r#"#!/usr/bin/env bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"

probe_id="$(basename "${BASH_SOURCE[0]}" .sh)"

echo "probe produced stderr noise" >&2

 commit_help_me emit emit.record

emit-record \
  --probe-name "${probe_id}" \
  --command "true" \
  --operation-kind "test.gate" \
  --target "stderr.empty" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "example" "stderr-empty-gate"
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_stderr_empty_enforced",
        probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");

    let (record, value) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, probe.probe_id());
    assert_eq!(record.operation.kind, "harness.supervised");
    assert_eq!(record.result.outcome, "error");
    assert!(
        record
            .result
            .details
            .as_ref()
            .and_then(|details| details.message.as_deref())
            .unwrap_or("")
            .contains("stderr.empty"),
        "expected synthetic record to explain stderr.empty enforcement"
    );
    assert!(
        value
            .pointer("/payload/stderr_snippet")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("probe produced stderr noise"),
        "expected probe stderr to be captured in the synthetic payload"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("synthetic"),
        "stderr should mention synthetic record emission; stderr was: {stderr}"
    );

    Ok(())
}

// In strict mode, stderr.empty gate violations should make fencerunner exit non-zero.
#[test]
fn fencerunner_strict_fails_when_stderr_empty_is_enforced() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let gates_contract = serde_json::json!({
        "schema_version": "gates_v1",
        "gates": {
            "enforced_checks": ["stderr.empty"]
        }
    });
    fs::write(
        run_dir.path().join("gates.json"),
        serde_json::to_string_pretty(&gates_contract)?,
    )?;

    let probe_contents = r#"#!/usr/bin/env bash
set -euo pipefail
echo "probe produced stderr noise" >&2
echo "{}"
exit 0
"#;
    let _probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_stderr_empty_enforced_strict",
        probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--strict")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = cmd
        .output()
        .context("failed to execute fencerunner --strict with stderr.empty enforcement")?;

    assert!(
        !output.status.success(),
        "fencerunner --strict should fail when stderr.empty is enforced"
    );
    assert!(
        output.stdout.is_empty(),
        "fencerunner --strict should not emit records when a probe fails early"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stderr.empty"),
        "stderr should mention stderr.empty enforcement; stderr was: {stderr}"
    );
    assert!(
        stderr.contains("probe produced stderr noise"),
        "probe stderr should be forwarded for diagnostics; stderr was: {stderr}"
    );

    Ok(())
}

// In supervised mode, fencerunner validates probe output against the run-dir
// boundaries contract; if the probe emits JSON that parses but violates
// boundaries.json, it emits a synthetic record rather than forwarding the
// invalid JSON.
#[test]
fn fencerunner_supervised_emits_synthetic_record_when_probe_emits_schema_invalid_json() -> Result<()>
{
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;

    let probe_contents = r#"#!/usr/bin/env bash
set -euo pipefail

probe_id="$(basename "${BASH_SOURCE[0]}" .sh)"

# Valid JSON object, but invalid boundary record: /context/commitments is missing.
echo "{\"probe\":{\"id\":\"${probe_id}\"},\"operation\":{\"kind\":\"test.schema\",\"target\":\"x\"},\"result\":{\"outcome\":\"success\"},\"context\":{}}"
exit 0
"#;
    let probe = FixtureProbe::install_from_contents_in_run_dir(
        run_dir.path(),
        "tests_schema_invalid_record",
        probe_contents,
    )?;

    let runner = fencerunner_binary(&repo_root);
    let mut cmd = Command::new(&runner);
    cmd.arg("--supervised")
        .arg(run_dir.path())
        .current_dir(&repo_root);
    let output = run_command(cmd)?;

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");

    let (record, value) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, probe.probe_id());
    assert_eq!(record.operation.kind, "harness.supervised");
    assert_eq!(record.result.outcome, "error");
    assert!(
        record
            .result
            .details
            .as_ref()
            .and_then(|details| details.message.as_deref())
            .unwrap_or("")
            .contains("run-dir schema validation"),
        "expected schema validation failure to be recorded in result.details.message"
    );
    assert!(
        value
            .pointer("/payload/stdout_snippet")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("\"context\":{}"),
        "expected invalid stdout to be captured in the synthetic payload"
    );

    Ok(())
}

// Error handling: fencerunner requires at least one run dir.
#[test]
fn fencerunner_errors_when_no_run_dirs_provided() -> Result<()> {
    let repo_root = repo_root();
    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .output()
        .context("failed to execute fencerunner without args")?;
    assert!(
        !output.status.success(),
        "fencerunner should fail when no run dirs are provided"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage: fencerunner"),
        "stderr should include usage; got: {stderr}"
    );
    Ok(())
}

// Error handling: a run dir must contain one or more executable *.sh probes.
#[test]
fn fencerunner_errors_when_run_dir_has_no_probes() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let run_dir = FixtureRunDir::new(&repo_root)?;
    let runner = fencerunner_binary(&repo_root);

    let output = Command::new(&runner)
        .arg(run_dir.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner with empty run dir")?;

    assert!(
        !output.status.success(),
        "fencerunner should fail when a run dir contains no probes"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when a run dir contains no probes"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No probes found"),
        "stderr should explain the empty run dir; got: {stderr}"
    );

    Ok(())
}

// Error handling: probe ids are global; duplicates across run dirs are rejected.
#[test]
fn fencerunner_errors_on_duplicate_probe_ids_across_run_dirs() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let runner = fencerunner_binary(&repo_root);
    let run_dir_a = FixtureRunDir::new(&repo_root)?;
    let run_dir_b = FixtureRunDir::new(&repo_root)?;
    let _a = FixtureProbe::install_in_run_dir(&repo_root, run_dir_a.path(), "tests_fixture_probe")?;
    let _b = FixtureProbe::install_in_run_dir(&repo_root, run_dir_b.path(), "tests_fixture_probe")?;

    let output = Command::new(&runner)
        .arg(run_dir_a.path())
        .arg(run_dir_b.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner with duplicate probe ids")?;
    assert!(
        !output.status.success(),
        "fencerunner should fail when probe ids collide across run dirs"
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout when probe ids collide"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Duplicate probe id"),
        "stderr should explain the probe id collision; got: {stderr}"
    );
    Ok(())
}

// Commitments are dir-local; duplicate commitment ids across run dirs are allowed.
#[test]
fn fencerunner_allows_duplicate_commitment_ids_across_run_dirs() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let runner = fencerunner_binary(&repo_root);

    let run_dir_a = FixtureRunDir::new(&repo_root)?;
    let run_dir_b = FixtureRunDir::new(&repo_root)?;

    // Install two probes with distinct ids so only commitments collide.
    let probe_contents = r#"#!/usr/bin/env bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"

probe_id="$(basename "${BASH_SOURCE[0]}" .sh)"

commit_help_me ensure shared-commitment

emit-record \
  --probe-name "${probe_id}" \
  --command "true" \
  --operation-kind "test.commitments" \
  --target "shared-commitment" \
  --outcome success \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "probe" "fixture"
"#;

    let _a = FixtureProbe::install_from_contents_in_run_dir(
        run_dir_a.path(),
        "cap_fixture_a",
        probe_contents,
    )?;
    let _b = FixtureProbe::install_from_contents_in_run_dir(
        run_dir_b.path(),
        "cap_fixture_b",
        probe_contents,
    )?;

    // Overwrite both registries so they intentionally collide on id.
    let shared_registry = serde_json::json!({
        "schema_version": "commitments_v1",
        "commitments": [
            {
                "id": "shared-commitment",
                "provider": "system",
                "helps": ["ensure"],
                "is": "shared test commitment",
                "at": "shared-commitment",
                "version": "v1"
            }
        ]
    });
    fs::write(
        run_dir_a.path().join("commitments.json"),
        serde_json::to_string_pretty(&shared_registry)?,
    )?;
    fs::write(
        run_dir_b.path().join("commitments.json"),
        serde_json::to_string_pretty(&shared_registry)?,
    )?;

    let output = Command::new(&runner)
        .arg(run_dir_a.path())
        .arg(run_dir_b.path())
        .current_dir(&repo_root)
        .output()
        .context("failed to execute fencerunner with duplicate commitment ids")?;
    assert!(
        output.status.success(),
        "fencerunner should allow duplicate commitment ids across run dirs; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let records: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(records.len(), 2, "expected one record per probe");

    for line in records {
        let (record, _) = parse_boundary_object(line.as_bytes())?;
        assert!(
            record
                .context
                .commitments
                .iter()
                .any(|cap| cap.id == "shared-commitment"),
            "expected shared-commitment enrollment to be recorded"
        );
    }

    Ok(())
}

// Run dirs may live anywhere on disk, not just under the repository root.
#[test]
fn fencerunner_accepts_run_dir_outside_repo() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let runner = fencerunner_binary(&repo_root);

    let run_dir = TempDir::new().context("failed to allocate temp run dir")?;
    fs::copy(
        repo_root.join("probes/commitments.json"),
        run_dir.path().join("commitments.json"),
    )?;
    fs::copy(
        repo_root.join("probes/gates.json"),
        run_dir.path().join("gates.json"),
    )?;
    fs::copy(
        repo_root.join("probes/boundaries.json"),
        run_dir.path().join("boundaries.json"),
    )?;
    let probe =
        FixtureProbe::install_in_run_dir(&repo_root, run_dir.path(), "tests_fixture_probe")?;

    let mut cmd = Command::new(&runner);
    cmd.arg(run_dir.path()).current_dir(&repo_root);
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("target stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(lines.len(), 1, "expected exactly one record");
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, probe.probe_id());

    Ok(())
}

// fencerunner should reject unknown flags.
#[test]
fn fencerunner_rejects_unknown_flags() -> Result<()> {
    let repo_root = repo_root();
    let runner = fencerunner_binary(&repo_root);
    let output = Command::new(&runner)
        .arg("--bang")
        .output()
        .context("failed to execute fencerunner with unknown flag")?;
    assert!(!output.status.success(), "unknown flags should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--bang"),
        "stderr should mention the unknown flag; got: {stderr}"
    );
    assert!(
        stderr.contains("unexpected argument")
            || stderr.contains("unknown argument")
            || stderr.contains("Found argument"),
        "stderr should describe an unknown argument; got: {stderr}"
    );
    Ok(())
}
