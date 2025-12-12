#![cfg(unix)]

// Centralized integration suite for the probe harness; exercises schema validation,
// probe resolution rules, and helper utilities so changes surface in one place.
mod support;

use anyhow::{Context, Result, bail};
use fencerunner::emit_support::{
    JsonObjectBuilder, PayloadArgs, TextSource, normalize_secondary_ids, validate_status,
};
use fencerunner::fence_run_support::{
    WorkspaceOverride, canonicalize_path, resolve_probe_metadata, workspace_plan_from_override,
    workspace_tmpdir_plan,
};
use fencerunner::{
    self, BoundaryObject, BoundarySchema, CANONICAL_BOUNDARY_SCHEMA_PATH, CapabilityCategory,
    CapabilityContext, CapabilityId, CapabilityIndex, CapabilityLayer, CapabilitySnapshot,
    CatalogKey, CatalogRepository, OperationInfo, Payload, Probe, ProbeInfo, ProbeMetadata,
    ResultInfo, RunInfo, StackInfo, default_boundary_descriptor_path, default_catalog_path,
    list_probes, load_catalog_from_path, resolve_boundary_schema_path, resolve_helper_binary,
    resolve_probe,
};
use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use support::{helper_binary, make_executable, repo_root, run_command};
use tempfile::{NamedTempFile, TempDir};

// Ensures boundary objects emitted via emit-record satisfy the boundary schema and
// contain the required contextual metadata.
#[test]
fn boundary_object_schema() -> Result<()> {
    let repo_root = repo_root();
    let emit_record = helper_binary(&repo_root, "emit-record");
    let payload = json!({
        "stdout_snippet": "fixture-stdout",
        "stderr_snippet": "fixture-stderr",
        "raw": {"detail": "schema-test"}
    });

    let mut payload_file = NamedTempFile::new().context("failed to allocate payload file")?;
    serde_json::to_writer(&mut payload_file, &payload)?;

    let mut emit_cmd = Command::new(&emit_record);
    emit_cmd
        .arg("--run-mode")
        .arg("baseline")
        .arg("--probe-name")
        .arg("schema_test_fixture")
        .arg("--probe-version")
        .arg("1")
        .arg("--primary-capability-id")
        .arg("cap_fs_read_workspace_tree")
        .arg("--command")
        .arg("printf fixture")
        .arg("--category")
        .arg("fs")
        .arg("--verb")
        .arg("read")
        .arg("--target")
        .arg("/dev/null")
        .arg("--status")
        .arg("success")
        .arg("--raw-exit-code")
        .arg("0")
        .arg("--message")
        .arg("fixture")
        .arg("--operation-args")
        .arg("{\"fixture\":true}")
        .arg("--payload-file")
        .arg(payload_file.path());
    emit_cmd.env("TEST_PREFER_TARGET", "1");
    let output = run_command(emit_cmd)?;

    let (record, value) = parse_boundary_object(&output.stdout)?;

    assert_eq!(record.schema_version, boundary_schema_version());
    assert_eq!(
        record.schema_key.as_deref(),
        boundary_schema_key().as_deref()
    );
    let schema_key = value
        .get("schema_key")
        .and_then(Value::as_str)
        .expect("schema_key present");
    assert!(
        schema_key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')),
        "schema_key must match ^[A-Za-z0-9_.-]+$"
    );
    let cap_schema = value
        .get("capabilities_schema_version")
        .and_then(Value::as_str)
        .expect("capabilities_schema_version present");
    assert!(
        cap_schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')),
        "capabilities_schema_version must match ^[A-Za-z0-9_.-]+$"
    );

    assert!(value.get("stack").map(|s| s.is_object()).unwrap_or(false));
    assert_eq!(record.probe.id, "schema_test_fixture");
    assert_eq!(record.probe.version, "1");
    assert_eq!(
        record.probe.primary_capability_id.0,
        "cap_fs_read_workspace_tree"
    );
    assert!(
        value
            .get("probe")
            .and_then(|probe| probe.get("secondary_capability_ids"))
            .map(|ids| ids.is_array())
            .unwrap_or(false)
    );

    assert_eq!(record.run.mode.as_str(), "baseline");
    assert!(record.run.workspace_root.is_some());
    assert!(
        value
            .get("run")
            .and_then(|run| run.get("command"))
            .and_then(Value::as_str)
            .is_some()
    );

    assert_eq!(record.operation.category, "fs");
    assert_eq!(record.operation.verb, "read");
    assert_eq!(record.operation.target, "/dev/null");
    assert!(
        value
            .get("operation")
            .and_then(|op| op.get("args"))
            .map(|args| args.is_object())
            .unwrap_or(false)
    );

    assert!(matches!(
        record.result.observed_result.as_str(),
        "success" | "denied" | "partial" | "error"
    ));
    let result_obj = value.get("result").expect("result present");
    for key in ["raw_exit_code", "errno", "message", "error_detail"] {
        assert!(result_obj.get(key).is_some(), "result missing {key}");
    }
    assert!(
        result_obj.get("duration_ms").is_none(),
        "result should not include duration_ms"
    );

    assert_eq!(
        value
            .pointer("/payload/stdout_snippet")
            .and_then(Value::as_str),
        Some("fixture-stdout")
    );
    assert_eq!(
        value
            .pointer("/payload/stderr_snippet")
            .and_then(Value::as_str),
        Some("fixture-stderr")
    );
    assert!(
        value
            .pointer("/payload/raw")
            .map(|raw| raw.is_object())
            .unwrap_or(false)
    );

    let capability_context = value
        .get("capability_context")
        .expect("capability_context present");
    assert!(capability_context.is_object());
    let primary_ctx = capability_context
        .get("primary")
        .expect("primary context present");
    assert_eq!(
        primary_ctx.get("id").and_then(Value::as_str),
        Some("cap_fs_read_workspace_tree")
    );
    for key in ["category", "layer"] {
        assert!(
            primary_ctx.get(key).is_some(),
            "primary context missing {key}"
        );
    }
    assert!(
        capability_context
            .get("secondary")
            .map(|sec| sec.is_array())
            .unwrap_or(false)
    );

    static BOUNDARY_OBJECT_SCHEMA: OnceLock<BoundarySchema> = OnceLock::new();
    let schema = BOUNDARY_OBJECT_SCHEMA.get_or_init(|| {
        let schema_path =
            resolve_boundary_schema_path(&repo_root, None).expect("resolve boundary schema");
        BoundarySchema::load(&schema_path).expect("load boundary schema")
    });
    schema.validate(&value)?;

    Ok(())
}

// Confirms the bundled capability catalog satisfies the generic catalog schema.
#[test]
fn capability_catalog_schema() -> Result<()> {
    let repo_root = repo_root();
    let schema_path = repo_root.join("schema/capability_catalog.schema.json");
    let catalog_path = default_catalog_path(&repo_root);

    static CATALOG_SCHEMA: OnceLock<Value> = OnceLock::new();
    let schema_value = if let Some(existing) = CATALOG_SCHEMA.get() {
        existing
    } else {
        let loaded: Value = serde_json::from_reader(File::open(&schema_path)?)?;
        CATALOG_SCHEMA.get_or_init(move || loaded)
    };
    let catalog_value: Value = serde_json::from_reader(File::open(&catalog_path)?)?;

    let compiled = JSONSchema::compile(schema_value)?;
    if let Err(errors) = compiled.validate(&catalog_value) {
        let details = errors
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        bail!("capability catalog failed schema validation:\n{details}");
    }

    Ok(())
}

// Confirms the bundled boundary schema descriptor embeds the canonical schema.
#[test]
fn boundary_schema_matches_canonical() -> Result<()> {
    let repo_root = repo_root();
    let descriptor_path = default_boundary_descriptor_path(&repo_root);
    let canonical_path = repo_root.join(CANONICAL_BOUNDARY_SCHEMA_PATH);

    let descriptor_value: Value = serde_json::from_reader(File::open(&descriptor_path)?)?;
    assert!(
        descriptor_value.get("schema").is_none(),
        "boundary descriptor should defer to the canonical schema file"
    );

    let schema_path = descriptor_value
        .get("schema_path")
        .and_then(Value::as_str)
        .context("schema_path missing from boundary descriptor")?;
    let resolved_schema = descriptor_path
        .parent()
        .unwrap_or(&repo_root)
        .join(schema_path)
        .canonicalize()
        .context("canonicalizing schema_path from boundary descriptor")?;
    assert_eq!(
        resolved_schema,
        canonical_path.canonicalize()?,
        "boundary descriptor should point at the canonical schema"
    );

    let descriptor_schema = BoundarySchema::load(&descriptor_path)?;
    let canonical_schema = BoundarySchema::load(&canonical_path)?;
    assert_eq!(
        descriptor_schema.schema_version(),
        canonical_schema.schema_version(),
        "descriptor and canonical schema should expose the same version"
    );
    assert_eq!(
        descriptor_schema.raw_schema(),
        canonical_schema.raw_schema(),
        "descriptor and canonical schema should compile the same payload"
    );
    Ok(())
}

// Runs the minimal fixture probe through probe-exec baseline to confirm the
// generated record reflects success and payload propagation.
#[test]
fn harness_smoke_probe_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut baseline_cmd = Command::new(helper_binary(&repo_root, "probe-exec"));
    baseline_cmd
        .env("TEST_PREFER_TARGET", "1")
        .arg("baseline")
        .arg(fixture.probe_id());
    let output = run_command(baseline_cmd)?;

    let (record, value) = parse_boundary_object(&output.stdout)?;

    assert_eq!(record.probe.id, fixture.probe_id());
    assert_eq!(record.operation.category, "fs");
    assert_eq!(record.result.observed_result, "success");
    assert_eq!(
        value.pointer("/payload/raw/probe").and_then(Value::as_str),
        Some("fixture")
    );
    assert_eq!(
        record.run.workspace_root.as_deref(),
        Some(repo_root.to_str().expect("repo root utf-8"))
    );

    Ok(())
}

// Checks that workspace_root falls back to the caller's cwd when the env hint
// is blank, matching legacy agent expectations.
#[test]
fn workspace_root_fallback() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;
    let temp_run_dir = TempDir::new()?;

    let mut fallback_cmd = Command::new(helper_binary(&repo_root, "probe-exec"));
    fallback_cmd
        .current_dir(temp_run_dir.path())
        .env("FENCE_WORKSPACE_ROOT", "")
        .env("TEST_PREFER_TARGET", "1")
        .arg("baseline")
        .arg(fixture.probe_id());
    let output = run_command(fallback_cmd)?;
    let (record, _) = parse_boundary_object(&output.stdout)?;
    let expected_workspace = fs::canonicalize(temp_run_dir.path())?;
    let actual_root = record
        .run
        .workspace_root
        .as_deref()
        .expect("workspace_root recorded");
    let actual_workspace = fs::canonicalize(Path::new(actual_root))?;
    assert_eq!(actual_workspace, expected_workspace);

    Ok(())
}

// Exercises the guard rails that keep probe execution inside probes/, blocking
// both absolute paths and escaping symlinks.
#[test]
fn probe_resolution_guards() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();

    let mut script = NamedTempFile::new()?;
    writeln!(script, "#!/usr/bin/env bash")?;
    writeln!(script, "echo should_never_run")?;
    writeln!(script, "exit 0")?;
    let temp_path = script.into_temp_path();
    let outside_script = temp_path.to_path_buf();
    let mut perms = fs::metadata(&outside_script)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&outside_script, perms)?;

    let abs_result = Command::new(helper_binary(&repo_root, "probe-exec"))
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1")
        .arg(&outside_script)
        .output()
        .context("failed to execute probe-exec outside script")?;
    assert!(
        !abs_result.status.success(),
        "probe-exec executed script outside probes/ (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&abs_result.stdout),
        String::from_utf8_lossy(&abs_result.stderr)
    );

    let symlink_path = repo_root.join("probes/tests_probe_resolution_symlink.sh");
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

    let symlink_result = Command::new(helper_binary(&repo_root, "probe-exec"))
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1")
        .arg("tests_probe_resolution_symlink")
        .output()
        .context("failed to execute probe-exec via symlink")?;
    assert!(
        !symlink_result.status.success(),
        "probe-exec followed a symlink that escapes probes/ (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&symlink_result.stdout),
        String::from_utf8_lossy(&symlink_result.stderr)
    );

    Ok(())
}

// Ensures probe-matrix surfaces malformed probe output without blocking the
// remaining probes from running.
#[test]
fn probe_matrix_continues_after_malformed_probe() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let good = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;
    let broken_contents = r#"#!/usr/bin/env bash
set -euo pipefail
echo not-json
exit 0
"#;
    let broken =
        FixtureProbe::install_from_contents(&repo_root, "tests_malformed_probe", broken_contents)?;

    let mut cmd = Command::new(helper_binary(&repo_root, "probe-matrix"));
    cmd.env(
        "PROBES",
        format!("{},{}", broken.probe_id(), good.probe_id()),
    )
    .env("MODES", "baseline")
    .env("TEST_PREFER_TARGET", "1");
    let output = cmd
        .output()
        .context("failed to execute probe-matrix with malformed probe")?;

    assert!(
        !output.status.success(),
        "probe-matrix should fail when a probe emits invalid JSON"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
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

// Smoke-tests the probe --target CLI end-to-end with a single probe.
#[test]
fn probe_target_runs_single_probe() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let probe = helper_binary(&repo_root, "probe");
    let mut cmd = Command::new(&probe);
    cmd.arg("--target")
        .arg("--probe")
        .arg(fixture.probe_id())
        .arg("--mode")
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1");
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
        "expected exactly one record for a single probe+mode"
    );
    let (record, _) = parse_boundary_object(lines[0].as_bytes())?;
    assert_eq!(record.probe.id, fixture.probe_id());
    assert_eq!(record.run.mode, "baseline");

    Ok(())
}

// Verifies --repeat fans out through probe-matrix and yields multiple boundary objects.
#[test]
fn probe_target_repeats_probe_runs() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let target_runner = helper_binary(&repo_root, "probe-target");
    let mut cmd = Command::new(&target_runner);
    cmd.arg("--probe")
        .arg(fixture.probe_id())
        .arg("--mode")
        .arg("baseline")
        .arg("--repeat")
        .arg("2")
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("repeat stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "--repeat 2 should emit two boundary objects"
    );
    for line in lines {
        let (record, _) = parse_boundary_object(line.as_bytes())?;
        assert_eq!(record.probe.id, fixture.probe_id());
        assert_eq!(record.run.mode, "baseline");
    }

    Ok(())
}

// Ensures capability selection resolves the bundled catalog and runs every probe in that slice.
#[test]
fn probe_target_runs_capability_subset() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let target_runner = helper_binary(&repo_root, "probe-target");
    let mut cmd = Command::new(&target_runner);
    cmd.arg("--cap")
        .arg("cap_fs_read_workspace_tree")
        .arg("--mode")
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("capability stdout utf-8")?;
    let lines: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    assert!(
        lines.len() >= 1,
        "capability selection should emit at least one boundary object"
    );
    let mut saw_fixture = false;
    for line in lines {
        let (record, _) = parse_boundary_object(line.as_bytes())?;
        if record.probe.id == fixture.probe_id() {
            saw_fixture = true;
        }
        assert_eq!(record.run.mode, "baseline");
    }
    assert!(
        saw_fixture,
        "capability selection should include the installed fixture probe"
    );

    Ok(())
}

// Smoke test for the compiled helper-backed probe.
#[test]
fn proc_paging_stress_probe_emits_expected_record() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let probe_run = helper_binary(&repo_root, "probe-exec");

    let mut cmd = Command::new(&probe_run);
    cmd.arg("baseline")
        .arg("proc_paging_stress")
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let (record, value) = parse_boundary_object(&output.stdout)?;
    assert_eq!(record.probe.id, "proc_paging_stress");
    assert_eq!(
        record.probe.primary_capability_id.0,
        "cap_proc_fork_and_child_spawn"
    );
    assert_eq!(record.result.observed_result, "success");
    assert_eq!(
        value
            .pointer("/payload/raw/helper_timeout")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(
        value
            .pointer("/operation/args/pattern")
            .and_then(Value::as_str),
        Some("random")
    );
    Ok(())
}

// Dry-run listing should summarize the plan without emitting JSON.
#[test]
fn probe_target_list_only_reports_plan() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let _fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let target_runner = helper_binary(&repo_root, "probe-target");
    let mut cmd = Command::new(&target_runner);
    cmd.arg("--cap")
        .arg("cap_fs_read_workspace_tree")
        .arg("--mode")
        .arg("baseline")
        .arg("--list-only")
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let stdout = String::from_utf8(output.stdout).context("list-only stdout utf-8")?;
    assert!(
        stdout.contains("probe target (dry-run)"),
        "list-only output should include the dry-run banner"
    );
    assert!(
        stdout.contains("modes: baseline"),
        "list-only output should echo the resolved modes"
    );
    assert!(
        stdout.contains("tests_fixture_probe"),
        "list-only output should mention the planned probe ids"
    );

    Ok(())
}

// Error handling: unknown probe id should surface a descriptive failure.
#[test]
fn probe_target_errors_on_unknown_probe() -> Result<()> {
    let repo_root = repo_root();
    let target_runner = helper_binary(&repo_root, "probe-target");
    let output = Command::new(&target_runner)
        .arg("--probe")
        .arg("does_not_exist")
        .arg("--mode")
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1")
        .output()
        .context("failed to execute probe-target unknown probe")?;
    assert!(
        !output.status.success(),
        "probe-target should fail for unknown probe ids"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Probe not found"),
        "stderr should explain the unknown probe; got: {stderr}"
    );
    Ok(())
}

// Error handling: unknown capability should be rejected before execution.
#[test]
fn probe_target_errors_on_unknown_capability() -> Result<()> {
    let repo_root = repo_root();
    let target_runner = helper_binary(&repo_root, "probe-target");
    let output = Command::new(&target_runner)
        .arg("--cap")
        .arg("cap_does_not_exist")
        .arg("--mode")
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1")
        .output()
        .context("failed to execute probe-target unknown capability")?;
    assert!(
        !output.status.success(),
        "probe-target should fail for unknown capabilities"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown capability"),
        "stderr should explain the missing capability; got: {stderr}"
    );
    Ok(())
}

// Validation: a selector is required, and providing both should error.
#[test]
fn probe_target_selector_validation() -> Result<()> {
    let repo_root = repo_root();
    let target_runner = helper_binary(&repo_root, "probe-target");

    let missing = Command::new(&target_runner)
        .arg("--mode")
        .arg("baseline")
        .env("TEST_PREFER_TARGET", "1")
        .output()
        .context("failed to execute probe-target without selector")?;
    assert!(
        !missing.status.success(),
        "--target should fail when --cap/--probe are both absent"
    );
    let missing_err = String::from_utf8_lossy(&missing.stderr);
    assert!(
        missing_err.contains("--cap or --probe"),
        "missing-selector error should mention required flags; stderr: {missing_err}"
    );

    let both = Command::new(&target_runner)
        .arg("--cap")
        .arg("cap_fs_read_workspace_tree")
        .arg("--probe")
        .arg("fs_read_workspace_readme")
        .env("TEST_PREFER_TARGET", "1")
        .output()
        .context("failed to execute probe-target with conflicting selectors")?;
    assert!(
        !both.status.success(),
        "--target should fail when both --cap and --probe are provided"
    );
    let both_err = String::from_utf8_lossy(&both.stderr);
    assert!(
        both_err.contains("exactly one"),
        "combined-selector error should mention exclusivity; stderr: {both_err}"
    );

    Ok(())
}

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
  --run-mode "${FENCE_RUN_MODE:-${FENCE_RUN_MODE:-baseline}}" \
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
        .arg("--modes")
        .arg("baseline")
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

// Validates json-extract helper semantics: pointer selection, type enforcement,
// defaults, and failure on type mismatch.
#[test]
fn json_extract_enforces_pointer_and_type() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "json-extract");
    let mut file = NamedTempFile::new().context("failed to create json fixture")?;
    writeln!(
        file,
        "{}",
        r#"{"nested":{"flag":true},"number":7,"text":"hello"}"#
    )?;

    // Happy path: extract nested flag as bool.
    let mut ok_cmd = Command::new(&helper);
    ok_cmd
        .arg("--file")
        .arg(file.path())
        .arg("--pointer")
        .arg("/nested/flag")
        .arg("--type")
        .arg("bool");
    let output = run_command(ok_cmd)?;
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value, Value::Bool(true));

    // Default applies when pointer missing.
    let mut default_cmd = Command::new(&helper);
    default_cmd
        .arg("--file")
        .arg(file.path())
        .arg("--pointer")
        .arg("/missing")
        .arg("--type")
        .arg("string")
        .arg("--default")
        .arg("\"fallback\"");
    let default_output = run_command(default_cmd)?;
    let default_value: Value = serde_json::from_slice(&default_output.stdout)?;
    assert_eq!(default_value, Value::String("fallback".to_string()));

    // Type mismatch should fail.
    let mut bad_type = Command::new(&helper);
    bad_type
        .arg("--file")
        .arg(file.path())
        .arg("--pointer")
        .arg("/number")
        .arg("--type")
        .arg("string");
    let bad_output = bad_type
        .output()
        .context("failed to run json-extract bad type")?;
    assert!(
        !bad_output.status.success(),
        "json-extract should fail on type mismatch"
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
    cmd.env("PROBE_CONTRACT_MODES", "baseline");
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
    cmd.env("PROBE_CONTRACT_MODES", "baseline");
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

// === Helper binary + library guard tests ===

#[test]
fn resolve_helper_prefers_release() -> Result<()> {
    let temp = TempRepo::new();
    let release_dir = temp.root.join("target/release");
    fs::create_dir_all(&release_dir)?;
    let helper = release_dir.join("probe-exec");
    fs::write(&helper, "#!/bin/sh\n")?;
    make_executable(&helper)?;
    let resolved = resolve_helper_binary(&temp.root, "probe-exec")?;
    assert_eq!(resolved, helper);
    Ok(())
}

#[test]
fn bin_helpers_match_manifest() -> Result<()> {
    let repo_root = repo_root();
    let manifest_path = repo_root.join("tools/helpers.manifest.json");
    let manifest: Vec<serde_json::Value> = serde_json::from_reader(File::open(&manifest_path)?)?;
    let mut registered: Vec<String> = manifest
        .into_iter()
        .filter_map(|v| {
            v.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    registered.sort();

    let mut present = Vec::new();
    for entry in fs::read_dir(repo_root.join("bin"))? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".gitkeep" {
            continue;
        }
        let name_str = name.to_string_lossy().to_string();
        if name_str == "probe-contract-gate" {
            continue;
        }
        present.push(name_str);
    }
    present.sort();

    assert_eq!(
        registered, present,
        "bin/ helpers must match tools/helpers.manifest.json"
    );
    Ok(())
}

#[test]
fn resolve_helper_falls_back_to_bin() -> Result<()> {
    let temp = TempRepo::new();
    let bin_dir = temp.root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let helper = bin_dir.join("emit-record");
    fs::write(&helper, "#!/bin/sh\n")?;
    make_executable(&helper)?;
    let resolved = resolve_helper_binary(&temp.root, "emit-record")?;
    assert_eq!(resolved, helper);
    Ok(())
}

#[test]
fn paging_stress_runs_small_workload() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "paging-stress");
    let mut cmd = Command::new(&helper);
    cmd.args([
        "--megabytes",
        "1",
        "--passes",
        "1",
        "--pattern",
        "sequential",
        "--max-seconds",
        "2",
    ])
    .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    assert!(
        output.stdout.is_empty(),
        "paging-stress should keep stdout empty"
    );
    Ok(())
}

#[test]
fn paging_stress_rejects_invalid_arguments() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "paging-stress");

    let mut cmd = Command::new(&helper);
    cmd.args(["--megabytes", "0"])
        .env("TEST_PREFER_TARGET", "1");
    let output = cmd
        .output()
        .context("failed to execute paging-stress with invalid args")?;
    assert!(!output.status.success(), "invalid argument run should fail");
    assert_eq!(output.status.code(), Some(1));
    Ok(())
}

#[test]
fn list_and_resolve_probes_share_semantics() -> Result<()> {
    let temp = TempRepo::new();
    let probes_dir = temp.root.join("probes");
    fs::create_dir_all(&probes_dir)?;
    let script = probes_dir.join("example.sh");
    fs::write(&script, "#!/usr/bin/env bash\nexit 0\n")?;
    make_executable(&script)?;

    let probes = list_probes(&temp.root)?;
    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].id, "example");

    let resolved = resolve_probe(&temp.root, "example")?;
    assert_eq!(resolved.path, fs::canonicalize(&script)?);
    let resolved_with_ext = resolve_probe(&temp.root, "example.sh")?;
    assert_eq!(resolved_with_ext.path, resolved.path);
    Ok(())
}

#[test]
fn boundary_object_round_trips_structs() -> Result<()> {
    let bo = sample_boundary_object();
    let value = serde_json::to_value(&bo)?;
    assert_eq!(
        value.get("schema_version").and_then(|v| v.as_str()),
        Some(boundary_schema_version().as_str())
    );
    let back: BoundaryObject = serde_json::from_value(value)?;
    assert_eq!(back.schema_version, boundary_schema_version());
    assert_eq!(back.run.mode, "baseline");
    assert_eq!(back.capability_context.primary.id.0, "cap_id");
    Ok(())
}

#[test]
fn capabilities_schema_version_serializes_in_json() -> Result<()> {
    let mut bo = sample_boundary_object();
    bo.capabilities_schema_version = Some(default_catalog_key());
    let value = serde_json::to_value(&bo)?;
    assert_eq!(
        value
            .get("capabilities_schema_version")
            .and_then(|v| v.as_str()),
        Some(default_catalog_key().0.as_str())
    );
    Ok(())
}

#[test]
fn repository_lookup_context_matches_capabilities() -> Result<()> {
    let catalog = load_catalog_from_path(&catalog_path())?;
    let key = catalog.catalog.key.clone();
    let primary = catalog.capabilities.first().expect("cap present");
    let secondary = catalog
        .capabilities
        .get(1)
        .map(|cap| vec![cap])
        .unwrap_or_default();
    let primary_id = primary.id.clone();
    let secondary_ids: Vec<_> = secondary.iter().map(|cap| cap.id.clone()).collect();
    let bo = sample_boundary_object().with_capabilities(key.clone(), primary, &secondary);
    let mut repo = CatalogRepository::default();
    repo.register(catalog);

    let (resolved_primary, resolved_secondary) = repo.lookup_context(&bo).expect("context");
    assert_eq!(resolved_primary.id, primary_id);
    if let Some(expected_secondary) = secondary_ids.first() {
        assert_eq!(resolved_secondary.first().unwrap().id, *expected_secondary);
    }
    Ok(())
}

#[test]
fn capability_snapshot_serializes_to_expected_shape() -> Result<()> {
    let snapshot = CapabilitySnapshot {
        id: CapabilityId("cap_test".to_string()),
        category: CapabilityCategory::Filesystem,
        layer: CapabilityLayer::OsSandbox,
    };
    let ctx = CapabilityContext {
        primary: snapshot.clone(),
        secondary: vec![snapshot.clone()],
    };
    let value = serde_json::to_value(&ctx)?;
    assert_eq!(
        value
            .get("primary")
            .and_then(|v| v.get("category"))
            .and_then(|v| v.as_str()),
        Some("filesystem")
    );
    assert_eq!(
        value
            .get("secondary")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len()),
        Some(1)
    );
    Ok(())
}

#[test]
fn category_round_trips_known_and_unknown() {
    let known = CapabilityCategory::SandboxProfile;
    let json = serde_json::to_string(&known).unwrap();
    assert_eq!(json.trim_matches('"'), "sandbox_profile");
    let back: CapabilityCategory = serde_json::from_str(&json).unwrap();
    assert_eq!(back, known);

    let custom_json = "\"custom_category\"";
    let parsed: CapabilityCategory = serde_json::from_str(custom_json).unwrap();
    assert_eq!(
        parsed,
        CapabilityCategory::Other("custom_category".to_string())
    );
    let serialized = serde_json::to_string(&parsed).unwrap();
    assert_eq!(serialized, custom_json);
}

#[test]
fn layer_round_trips_known_and_unknown() {
    let known = CapabilityLayer::AgentRuntime;
    let json = serde_json::to_string(&known).unwrap();
    assert_eq!(json.trim_matches('"'), "agent_runtime");
    let back: CapabilityLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back, known);

    let other_json = "\"custom_layer\"";
    let parsed: CapabilityLayer = serde_json::from_str(other_json).unwrap();
    assert_eq!(parsed, CapabilityLayer::Other("custom_layer".to_string()));
    let serialized = serde_json::to_string(&parsed).unwrap();
    assert_eq!(serialized, other_json);
}

#[test]
fn snapshot_serde_matches_schema() -> Result<()> {
    let snapshot = CapabilitySnapshot {
        id: CapabilityId("cap_example".into()),
        category: CapabilityCategory::Filesystem,
        layer: CapabilityLayer::OsSandbox,
    };
    let json = serde_json::to_value(&snapshot)?;
    assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("cap_example"));
    assert_eq!(
        json.get("category").and_then(|v| v.as_str()),
        Some("filesystem")
    );
    assert_eq!(
        json.get("layer").and_then(|v| v.as_str()),
        Some("os_sandbox")
    );

    let back: CapabilitySnapshot = serde_json::from_value(json)?;
    assert_eq!(back.id.0, "cap_example");
    assert!(matches!(back.category, CapabilityCategory::Filesystem));
    assert!(matches!(back.layer, CapabilityLayer::OsSandbox));
    Ok(())
}

#[test]
fn catalog_key_and_id_round_trip() {
    let key = default_catalog_key();
    let serialized = serde_json::to_string(&key).unwrap();
    assert_eq!(serialized, format!("\"{}\"", key.0));
    let parsed: CatalogKey = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed, key);

    let id = CapabilityId("cap_fs_read_workspace_tree".to_string());
    let serialized_id = serde_json::to_string(&id).unwrap();
    assert_eq!(serialized_id, "\"cap_fs_read_workspace_tree\"");
    let parsed_id: CapabilityId = serde_json::from_str(&serialized_id).unwrap();
    assert_eq!(parsed_id, id);
}

#[test]
fn load_real_catalog_smoke() -> Result<()> {
    let catalog = load_catalog_from_path(&catalog_path())?;
    assert!(!catalog.catalog.key.0.is_empty());
    assert!(!catalog.capabilities.is_empty());
    for cap in catalog.capabilities {
        assert!(!cap.id.0.is_empty());
        assert!(
            !matches!(cap.category, CapabilityCategory::Other(ref v) if v.is_empty()),
            "category should not be empty"
        );
        assert!(
            !matches!(cap.layer, CapabilityLayer::Other(ref v) if v.is_empty()),
            "layer should not be empty"
        );
    }
    Ok(())
}

#[test]
fn finds_capability_in_registered_catalog() -> Result<()> {
    let catalog = load_catalog_from_path(&catalog_path())?;
    let key = catalog.catalog.key.clone();
    let known_capability = catalog
        .capabilities
        .first()
        .expect("catalog should have capabilities")
        .id
        .clone();

    let mut repo = CatalogRepository::default();
    repo.register(catalog);

    let resolved = repo.find_capability(&key, &known_capability);
    assert!(resolved.is_some());
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
        CapabilityId(" cap_a ".to_string()),
        CapabilityId("cap_b".to_string()),
        CapabilityId("".to_string()),
        CapabilityId("cap_a".to_string()),
    ];
    let normalized = normalize_secondary_ids(&caps, &input)?;
    assert_eq!(
        normalized,
        vec![
            CapabilityId("cap_a".to_string()),
            CapabilityId("cap_b".to_string())
        ]
    );
    Ok(())
}

#[test]
fn normalize_secondary_rejects_unknown() -> Result<()> {
    let caps = sample_capability_index(&[("cap_a", "filesystem", "os_sandbox")])?;
    let input = vec![
        CapabilityId("cap_a".to_string()),
        CapabilityId("cap_missing".to_string()),
    ];
    assert!(normalize_secondary_ids(&caps, &input).is_err());
    Ok(())
}

#[test]
fn capability_index_enforces_schema_version() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "unexpected",
            "scope": {"description": "test", "policy_layers": [], "categories": {}},
            "docs": {},
            "capabilities": []
        }),
    )?;
    assert!(CapabilityIndex::load(file.path()).is_err());
    Ok(())
}

#[test]
fn capability_index_accepts_allowed_schema_version_override() -> Result<()> {
    // Custom schema versions are no longer allowed; ensure rejection path is covered.
    let mut temp = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut temp,
        &json!({
            "schema_version": "custom_catalog_v1",
            "catalog": {"key": "custom_catalog_v1", "title": "custom catalog"},
            "scope": {
                "description": "test",
                "policy_layers": [{"id": "os_sandbox", "description": "fixture layer"}],
                "categories": {"filesystem": "fixture"}
            },
            "docs": {},
            "capabilities": [{
                "id": "cap_fs_custom",
                "category": "filesystem",
                "layer": "os_sandbox",
                "description": "cap fs",
                "operations": {"allow": [], "deny": []}
            }]
        }),
    )?;

    assert!(
        CapabilityIndex::load(temp.path()).is_err(),
        "custom catalog schema_version should be rejected"
    );
    Ok(())
}

#[test]
fn emit_record_requires_primary_capability() -> Result<()> {
    let repo_root = repo_root();
    let emit_record = helper_binary(&repo_root, "emit-record");
    let output = Command::new(&emit_record)
        .arg("--run-mode")
        .arg("baseline")
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
        .arg("--run-mode")
        .arg("baseline")
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
        .arg("--run-mode")
        .arg("baseline")
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

#[test]
fn json_object_builder_overrides_fields() -> Result<()> {
    let mut builder = JsonObjectBuilder::default();
    builder.merge_json_string(r#"{"a":1,"b":2}"#, "object")?;
    builder.insert_string("b".to_string(), "override".to_string());
    builder.insert_list(
        "c".to_string(),
        vec!["first".to_string(), "second".to_string()],
    );
    builder.insert_json_value("d".to_string(), "true".to_string(), "object")?;
    let value = builder.build("test object")?;
    let obj = value.as_object().expect("object shape");
    assert_eq!(obj.get("a").and_then(Value::as_i64), Some(1));
    assert_eq!(obj.get("b").and_then(Value::as_str), Some("override"));
    assert_eq!(
        obj.get("c").and_then(Value::as_array).map(|arr| arr.len()),
        Some(2)
    );
    assert_eq!(obj.get("d").and_then(Value::as_bool), Some(true));
    Ok(())
}

#[test]
fn payload_builder_accepts_inline_snippets() -> Result<()> {
    let mut payload = PayloadArgs::default();
    payload.set_stdout(TextSource::Inline("hello".to_string()))?;
    payload.set_stderr(TextSource::Inline("stderr".to_string()))?;
    payload.raw_mut().insert_null("raw_key".to_string());
    let built = payload.build()?;
    assert_eq!(
        built.pointer("/stdout_snippet").and_then(Value::as_str),
        Some("hello")
    );
    assert_eq!(
        built.pointer("/stderr_snippet").and_then(Value::as_str),
        Some("stderr")
    );
    assert!(
        built
            .pointer("/raw/raw_key")
            .map(|v| v.is_null())
            .unwrap_or(false)
    );
    Ok(())
}

// === probe-exec workspace helpers ===

#[test]
fn resolve_probe_prefers_probes_dir() -> Result<()> {
    let workspace = TempWorkspace::new();
    let probes = workspace.root.join("probes");
    fs::create_dir_all(&probes)?;
    let script = probes.join("example.sh");
    fs::write(&script, "#!/usr/bin/env bash\nexit 0\n")?;
    make_executable(&script)?;
    let resolved = resolve_probe(&workspace.root, "example")?;
    assert!(resolved.path.ends_with("example.sh"));
    Ok(())
}

#[test]
fn workspace_override_skip_export() {
    let plan = workspace_plan_from_override(WorkspaceOverride::SkipExport);
    assert!(plan.export_value.is_none());
}

#[test]
fn workspace_override_canonicalizes_path() -> Result<()> {
    let workspace = TempWorkspace::new();
    let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
        workspace.root.join("probes").into_os_string(),
    ));
    assert!(
        plan.export_value
            .unwrap()
            .to_string_lossy()
            .contains("probes")
    );
    Ok(())
}

#[test]
fn workspace_tmpdir_prefers_workspace_tree() -> Result<()> {
    let workspace = TempWorkspace::new();
    let canonical_root = canonicalize_path(&workspace.root);
    let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
        canonical_root.clone().into_os_string(),
    ));
    let tmpdir_plan = workspace_tmpdir_plan(&plan, &canonical_root);
    let tmpdir = tmpdir_plan.path.expect("tmpdir");
    assert!(tmpdir.starts_with(&canonical_root));
    assert!(tmpdir.ends_with("tmp"));
    assert!(tmpdir.is_dir());
    Ok(())
}

#[test]
fn workspace_tmpdir_uses_override_when_present() -> Result<()> {
    let workspace = TempWorkspace::new();
    let override_root = workspace.root.join("custom_workspace");
    fs::create_dir_all(&override_root)?;
    let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
        override_root.clone().into_os_string(),
    ));
    let tmpdir_plan = workspace_tmpdir_plan(&plan, &workspace.root);
    let tmpdir = tmpdir_plan.path.expect("tmpdir");
    let override_canonical = canonicalize_path(&override_root);
    assert!(tmpdir.starts_with(&override_canonical));
    Ok(())
}

#[test]
fn workspace_tmpdir_records_error_when_all_candidates_fail() -> Result<()> {
    let workspace = TempWorkspace::new();
    let override_file = workspace.root.join("override_marker");
    fs::write(&override_file, "marker")?;
    let plan =
        workspace_plan_from_override(WorkspaceOverride::UsePath(override_file.into_os_string()));
    let tmpdir_plan = workspace_tmpdir_plan(&plan, &workspace.root);
    assert!(tmpdir_plan.path.is_none());
    let (attempted, message) = tmpdir_plan.last_error.expect("missing error");
    assert!(!message.is_empty());
    assert!(attempted.ends_with("tmp"));
    Ok(())
}

#[test]
fn resolve_probe_metadata_prefers_script_values() -> Result<()> {
    let workspace = TempWorkspace::new();
    let probes = workspace.root.join("probes");
    fs::create_dir_all(&probes)?;
    let script = probes.join("meta.sh");
    fs::write(
        &script,
        r#"#!/usr/bin/env bash
probe_name="custom_probe"
probe_version="2"
primary_capability_id="cap_fs_read_workspace_tree"
        "#,
    )?;
    make_executable(&script)?;
    let parsed = ProbeMetadata::from_script(&script)?;
    let probe = Probe {
        id: "meta".to_string(),
        path: fs::canonicalize(&script)?,
    };
    let resolved = resolve_probe_metadata(&probe, parsed)?;
    assert_eq!(resolved.id, "custom_probe");
    assert_eq!(resolved.version, "2");
    assert_eq!(resolved.primary_capability.0, "cap_fs_read_workspace_tree");
    Ok(())
}

// === CLI helper smoke tests (former bin_smoke) ===

#[test]
fn probe_prefers_repo_helper() -> Result<()> {
    let repo_root = repo_root();
    let temp_repo = TempDir::new().context("failed to allocate temp repo")?;
    let repo = temp_repo.path();
    let bin_dir = repo.join("bin");
    fs::create_dir_all(&bin_dir)?;
    fs::write(bin_dir.join(".gitkeep"), "")?;
    fs::write(repo.join("Makefile"), "all:\n\t@true\n")?;

    let marker = repo.join("helper_invoked");
    let helper_path = bin_dir.join("probe-matrix");
    fs::write(
        &helper_path,
        "#!/bin/sh\n[ -n \"$MARK_FILE\" ] && echo invoked > \"$MARK_FILE\"\n",
    )?;
    make_executable(&helper_path)?;

    let probe = helper_binary(&repo_root, "probe");
    let output = Command::new(probe)
        .arg("--matrix")
        .env("FENCE_ROOT", repo)
        .env("PATH", "")
        .env("MARK_FILE", &marker)
        .output()
        .context("failed to run probe stub")?;

    assert!(output.status.success());
    assert!(marker.is_file());
    Ok(())
}

#[test]
fn probe_falls_back_to_path() -> Result<()> {
    let repo_root = repo_root();
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let helper_dir = temp.path();
    let marker = helper_dir.join("path_helper_invoked");
    let helper_path = helper_dir.join("probe-listen");
    fs::write(
        &helper_path,
        "#!/bin/sh\n[ -n \"$MARK_FILE\" ] && echo listen > \"$MARK_FILE\"\n",
    )?;
    make_executable(&helper_path)?;

    let source = helper_binary(&repo_root, "probe");
    let runner = helper_dir.join("probe");
    fs::copy(&source, &runner)?;
    make_executable(&runner)?;

    let output = Command::new(&runner)
        .arg("--listen")
        .env("PATH", helper_dir)
        .env_remove("FENCE_ROOT")
        .env("MARK_FILE", &marker)
        .current_dir(helper_dir)
        .output()
        .context("failed to run probe path test")?;

    assert!(output.status.success());
    assert!(marker.is_file());
    Ok(())
}

#[test]
fn probe_exports_root_to_helpers() -> Result<()> {
    let repo_root = repo_root();
    let temp_repo = TempDir::new().context("failed to allocate temp repo")?;
    let repo = temp_repo.path();
    let bin_dir = repo.join("bin");
    fs::create_dir_all(&bin_dir)?;
    fs::write(bin_dir.join(".gitkeep"), "")?;
    fs::write(repo.join("Makefile"), "all:\n\t@true\n")?;

    let marker = repo.join("root_seen");
    let helper_path = bin_dir.join("probe-matrix");
    fs::write(
        &helper_path,
        "#!/bin/sh\n[ -n \"$FENCE_ROOT\" ] && echo \"$FENCE_ROOT\" > \"$MARK_FILE\"\n",
    )?;
    make_executable(&helper_path)?;

    let probe = helper_binary(&repo_root, "probe");
    let output = Command::new(probe)
        .arg("--matrix")
        .env("FENCE_ROOT", repo)
        .env("PATH", "")
        .env("MARK_FILE", &marker)
        .output()
        .context("failed to run probe env propagation test")?;

    assert!(output.status.success());
    let recorded = fs::read_to_string(&marker).context("marker missing")?;
    assert_eq!(fs::canonicalize(recorded.trim())?, fs::canonicalize(repo)?);
    Ok(())
}

#[test]
fn detect_stack_reports_expected_sandbox_modes() -> Result<()> {
    let repo_root = repo_root();
    let detect_stack = helper_binary(&repo_root, "detect-stack");

    let mut baseline_cmd = Command::new(&detect_stack);
    baseline_cmd.arg("baseline");
    let baseline = run_command(baseline_cmd)?;
    let baseline_json: Value = serde_json::from_slice(&baseline.stdout)?;
    assert!(
        baseline_json
            .get("sandbox_mode")
            .map(|v| v.is_null())
            .unwrap_or(true)
    );

    let override_val = "custom-mode";
    let mut override_cmd = Command::new(&detect_stack);
    override_cmd
        .arg("baseline")
        .env("FENCE_SANDBOX_MODE", override_val);
    let full = run_command(override_cmd)?;
    let full_json: Value = serde_json::from_slice(&full.stdout)?;
    assert_eq!(
        full_json
            .get("sandbox_mode")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        override_val
    );
    Ok(())
}

#[test]
fn json_extract_applies_default_value() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "json-extract");
    let temp = TempDir::new().context("failed to allocate json fixture dir")?;
    let json_path = temp.path().join("input.json");
    fs::write(&json_path, br#"{"present":true}"#)?;

    let mut cmd = Command::new(helper);
    cmd.arg("--file")
        .arg(&json_path)
        .arg("--pointer")
        .arg("/missing")
        .arg("--type")
        .arg("bool")
        .arg("--default")
        .arg("false");
    let output = run_command(cmd)?;
    let value: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(value, Value::Bool(false));
    Ok(())
}

#[test]
fn json_extract_rejects_unknown_type() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "json-extract");
    let output = Command::new(helper)
        .arg("--stdin")
        .arg("--type")
        .arg("unknown")
        .stdin(std::process::Stdio::piped())
        .output()
        .context("failed to spawn json-extract for error case")?;
    assert!(!output.status.success());
    Ok(())
}

#[test]
fn portable_path_relpath_matches_basics() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "portable-path");
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let base = temp.path().join("base");
    let target = base.join("nested/child");
    fs::create_dir_all(&target)?;

    let mut cmd = Command::new(helper);
    cmd.arg("relpath").arg(&target).arg(&base);
    let output = run_command(cmd)?;
    let relpath = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(relpath, "nested/child");
    Ok(())
}

#[test]
fn portable_path_relpath_handles_parent() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "portable-path");
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let base = temp.path().join("base/child");
    let target = temp.path().join("base/sibling/file.txt");
    fs::create_dir_all(target.parent().unwrap())?;
    fs::create_dir_all(&base)?;
    fs::write(&target, "content")?;

    let mut cmd = Command::new(helper);
    cmd.arg("relpath").arg(&target).arg(&base);
    let output = run_command(cmd)?;
    let relpath = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(relpath, "../sibling/file.txt");
    Ok(())
}

#[test]
fn portable_path_relpath_identical_path() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "portable-path");
    let temp = TempDir::new().context("failed to allocate temp dir")?;
    let base = temp.path().join("base");
    fs::create_dir_all(&base)?;

    let mut cmd = Command::new(helper);
    cmd.arg("relpath").arg(&base).arg(&base);
    let output = run_command(cmd)?;
    let relpath = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert_eq!(relpath, ".");
    Ok(())
}

#[test]
fn portable_path_realpath_nonexistent_is_blank() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "portable-path");
    let missing = TempDir::new()?.path().join("nope");
    let output = Command::new(helper)
        .arg("realpath")
        .arg(&missing)
        .output()
        .context("failed to run portable-path realpath")?;
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    Ok(())
}

// Helper for installing temporary probe mocks under probes/ and cleaning them
// up after each test.
struct FixtureProbe {
    path: PathBuf,
    name: String,
}

impl FixtureProbe {
    fn install(repo_root: &Path, name: &str) -> Result<Self> {
        let source = repo_root.join("tests/mocks/minimal_probe.sh");
        let dest = repo_root.join("probes").join(format!("{name}.sh"));
        if dest.exists() {
            bail!("fixture already exists at {}", dest.display());
        }
        fs::copy(&source, &dest)
            .with_context(|| format!("failed to copy fixture to {}", dest.display()))?;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
        Ok(Self {
            path: dest,
            name: name.to_string(),
        })
    }

    fn install_from_contents(repo_root: &Path, name: &str, contents: &str) -> Result<Self> {
        let dest = repo_root.join("probes").join(format!("{name}.sh"));
        if dest.exists() {
            bail!("fixture already exists at {}", dest.display());
        }
        fs::write(&dest, contents)
            .with_context(|| format!("failed to write fixture at {}", dest.display()))?;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
        Ok(Self {
            path: dest,
            name: name.to_string(),
        })
    }

    fn probe_id(&self) -> &str {
        &self.name
    }
}

impl Drop for FixtureProbe {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// Removes the referenced file on drop so tests can create temporary symlinks.
struct FileGuard {
    path: PathBuf,
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// Serializes repository-mutating tests so fixture installs do not conflict.
struct RepoGuard {
    _guard: MutexGuard<'static, ()>,
}

fn repo_guard() -> RepoGuard {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let mutex = LOCK.get_or_init(|| Mutex::new(()));
    let guard = mutex.lock().unwrap_or_else(|err| err.into_inner());
    RepoGuard { _guard: guard }
}

fn parse_boundary_object(bytes: &[u8]) -> Result<(BoundaryObject, Value)> {
    let value: Value = serde_json::from_slice(bytes)?;
    let record: BoundaryObject = serde_json::from_value(value.clone())?;
    Ok((record, value))
}

// Keep a helper for future assertions; suppress unused warnings for now.
#[allow(dead_code)]
fn relative_to_repo(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let mut dir = env::temp_dir();
        dir.push(format!(
            "probe-helper-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).expect("failed to create temp repo");
        Self { root: dir }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let mut base = env::temp_dir();
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        base.push(format!(
            "probe-workspace-test-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&base).expect("failed to create temp workspace");
        Self { root: base }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn sample_capability_index(entries: &[(&str, &str, &str)]) -> Result<CapabilityIndex> {
    let mut file = NamedTempFile::new()?;
    let capabilities: Vec<Value> = entries
        .iter()
        .map(|(id, category, layer)| {
            json!({
                "id": id,
                "category": category,
                "layer": layer,
                "description": format!("cap {id}"),
                "operations": {"allow": [], "deny": []}
            })
        })
        .collect();

    let mut categories = BTreeMap::new();
    let mut layers = BTreeSet::new();
    for (_, category, layer) in entries {
        categories
            .entry(category.to_string())
            .or_insert_with(|| "fixture".to_string());
        layers.insert(layer.to_string());
    }
    let policy_layers: Vec<Value> = layers
        .into_iter()
        .map(|layer| json!({"id": layer, "description": "fixture layer"}))
        .collect();

    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "sandbox_catalog_v1",
            "catalog": {"key": "sample_catalog_v1", "title": "sample catalog"},
            "scope": {"description": "test", "policy_layers": policy_layers, "categories": categories},
            "docs": {},
            "capabilities": capabilities
        }),
    )?;
    CapabilityIndex::load(file.path())
        .with_context(|| "failed to load sample capability index".to_string())
}

fn catalog_path() -> PathBuf {
    default_catalog_path(&repo_root())
}

fn default_catalog_key() -> CatalogKey {
    static KEY: OnceLock<CatalogKey> = OnceLock::new();
    KEY.get_or_init(|| {
        load_catalog_from_path(&catalog_path())
            .expect("load catalog")
            .catalog
            .key
            .clone()
    })
    .clone()
}

fn boundary_schema_version() -> String {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let path = resolve_boundary_schema_path(&repo_root(), None)
                .expect("resolve boundary schema path");
            BoundarySchema::load(&path)
                .expect("load boundary schema")
                .schema_version()
                .to_string()
        })
        .clone()
}

fn boundary_schema_key() -> Option<String> {
    static KEY: OnceLock<Option<String>> = OnceLock::new();
    KEY.get_or_init(|| {
        let path =
            resolve_boundary_schema_path(&repo_root(), None).expect("resolve boundary schema path");
        BoundarySchema::load(&path)
            .expect("load boundary schema")
            .schema_key()
            .map(str::to_string)
    })
    .clone()
}

fn empty_json_object() -> Value {
    Value::Object(Default::default())
}

fn sample_boundary_object() -> BoundaryObject {
    BoundaryObject {
        schema_version: boundary_schema_version(),
        schema_key: boundary_schema_key(),
        capabilities_schema_version: Some(default_catalog_key()),
        stack: StackInfo {
            sandbox_mode: Some("workspace-write".to_string()),
            os: "Darwin".to_string(),
        },
        probe: ProbeInfo {
            id: "probe".to_string(),
            version: "1".to_string(),
            primary_capability_id: CapabilityId("cap_id".to_string()),
            secondary_capability_ids: vec![],
        },
        run: RunInfo {
            mode: "baseline".to_string(),
            workspace_root: Some("/tmp".to_string()),
            command: "echo test".to_string(),
        },
        operation: OperationInfo {
            category: "fs".to_string(),
            verb: "read".to_string(),
            target: "/dev/null".to_string(),
            args: empty_json_object(),
        },
        result: ResultInfo {
            observed_result: "success".to_string(),
            raw_exit_code: Some(0),
            errno: None,
            message: None,
            error_detail: None,
        },
        payload: Payload {
            stdout_snippet: None,
            stderr_snippet: None,
            raw: empty_json_object(),
        },
        capability_context: CapabilityContext {
            primary: CapabilitySnapshot {
                id: CapabilityId("cap_id".to_string()),
                category: CapabilityCategory::Other("cat".to_string()),
                layer: CapabilityLayer::Other("layer".to_string()),
            },
            secondary: Vec::new(),
        },
    }
}
