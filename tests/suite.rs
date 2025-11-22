#![cfg(unix)]

// Centralized integration suite for the fence harness; exercises schema validation,
// probe resolution rules, and helper utilities so changes surface in one place.

use anyhow::{Context, Result, bail};
use codex_fence::{BoundaryObject, find_repo_root};
use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::{NamedTempFile, TempDir};

// Ensures boundary objects emitted via emit-record satisfy the cfbo-v1 schema and
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
    emit_cmd.env("CODEX_FENCE_PREFER_TARGET", "1");
    let output = run_command(emit_cmd)?;

    let (record, value) = parse_boundary_object(&output.stdout)?;

    assert_eq!(record.schema_version, "cfbo-v1");
    assert!(value.get("capabilities_schema_version").is_some());
    if let Some(cap_schema) = value.get("capabilities_schema_version") {
        if let Some(cap_schema_str) = cap_schema.as_str() {
            assert!(
                cap_schema_str
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')),
                "capabilities_schema_version must match ^[A-Za-z0-9_.-]+$"
            );
        } else {
            assert!(cap_schema.is_null());
        }
    }

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

    assert!(matches!(
        record.run.mode.as_str(),
        "baseline" | "codex-sandbox" | "codex-full"
    ));
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
    for key in [
        "raw_exit_code",
        "errno",
        "message",
        "duration_ms",
        "error_detail",
    ] {
        assert!(result_obj.get(key).is_some(), "result missing {key}");
    }

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

    static BOUNDARY_OBJECT_SCHEMA: OnceLock<Value> = OnceLock::new();
    let schema = if let Some(existing) = BOUNDARY_OBJECT_SCHEMA.get() {
        existing
    } else {
        let schema_path = repo_root.join("schema/boundary_object.json");
        let schema_value: Value = serde_json::from_reader(File::open(&schema_path)?)?;
        BOUNDARY_OBJECT_SCHEMA.get_or_init(move || schema_value)
    };
    let compiled = JSONSchema::compile(schema)?;
    if let Err(errors) = compiled.validate(&value) {
        let details = errors
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        bail!("boundary object failed schema validation:\n{details}");
    }

    Ok(())
}

// Runs the minimal fixture probe through fence-run baseline to confirm the
// generated record reflects success and payload propagation.
#[test]
fn harness_smoke_probe_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut baseline_cmd = Command::new(helper_binary(&repo_root, "fence-run"));
    baseline_cmd
        .env("CODEX_FENCE_PREFER_TARGET", "1")
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

// Verifies baseline mode succeeds without codex binaries while sandbox mode
// fails when codex is absent from PATH, preventing false positives.
#[test]
fn baseline_no_codex_smoke() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let sanitized_path = sanitized_path_without_codex()?;

    let fence_run = helper_binary(&repo_root, "fence-run");
    let mut baseline_cmd = Command::new(&fence_run);
    baseline_cmd
        .env("PATH", &sanitized_path)
        .env("CODEX_FENCE_PREFER_TARGET", "1")
        .arg("baseline")
        .arg(fixture.probe_id());
    let baseline_output = run_command(baseline_cmd)?;
    let (record, _) = parse_boundary_object(&baseline_output.stdout)?;
    assert_eq!(record.probe.id, fixture.probe_id());
    assert_eq!(record.result.observed_result, "success");

    let sandbox_result = Command::new(&fence_run)
        .env("PATH", &sanitized_path)
        .env("CODEX_FENCE_PREFER_TARGET", "1")
        .arg("codex-sandbox")
        .arg(fixture.probe_id())
        .output()
        .context("failed to execute fence-run codex-sandbox")?;
    assert!(
        !sandbox_result.status.success(),
        "codex-sandbox unexpectedly succeeded without codex (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&sandbox_result.stdout),
        String::from_utf8_lossy(&sandbox_result.stderr)
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

    let mut fallback_cmd = Command::new(helper_binary(&repo_root, "fence-run"));
    fallback_cmd
        .current_dir(temp_run_dir.path())
        .env("FENCE_WORKSPACE_ROOT", "")
        .env("CODEX_FENCE_PREFER_TARGET", "1")
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

    let abs_result = Command::new(helper_binary(&repo_root, "fence-run"))
        .arg("baseline")
        .env("CODEX_FENCE_PREFER_TARGET", "1")
        .arg(&outside_script)
        .output()
        .context("failed to execute fence-run outside script")?;
    assert!(
        !abs_result.status.success(),
        "fence-run executed script outside probes/ (stdout: {}, stderr: {})",
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

    let symlink_result = Command::new(helper_binary(&repo_root, "fence-run"))
        .arg("baseline")
        .env("CODEX_FENCE_PREFER_TARGET", "1")
        .arg("tests_probe_resolution_symlink")
        .output()
        .context("failed to execute fence-run via symlink")?;
    assert!(
        !symlink_result.status.success(),
        "fence-run followed a symlink that escapes probes/ (stdout: {}, stderr: {})",
        String::from_utf8_lossy(&symlink_result.stdout),
        String::from_utf8_lossy(&symlink_result.stderr)
    );

    Ok(())
}

// Ensures fence-bang surfaces malformed probe output without blocking the
// remaining probes from running.
#[test]
fn fence_bang_continues_after_malformed_probe() -> Result<()> {
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

    let mut cmd = Command::new(helper_binary(&repo_root, "fence-bang"));
    cmd.env(
        "PROBES",
        format!("{},{}", broken.probe_id(), good.probe_id()),
    )
    .env("MODES", "baseline")
    .env("CODEX_FENCE_PREFER_TARGET", "1");
    let output = cmd
        .output()
        .context("failed to execute fence-bang with malformed probe")?;

    assert!(
        !output.status.success(),
        "fence-bang should fail when a probe emits invalid JSON"
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

// Confirms the static contract gate accepts the canonical fixture probe.
#[test]
fn static_probe_contract_accepts_fixture() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture = FixtureProbe::install(&repo_root, "tests_fixture_probe")?;

    let mut cmd = Command::new(repo_root.join("tools/validate_contract_gate.sh"));
    cmd.arg("--probe").arg(fixture.probe_id()).arg("--static-only");
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
  --run-mode "${FENCE_RUN_MODE:-baseline}" \
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
    cmd.arg("--probe").arg(broken.probe_id()).arg("--static-only");
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
        .env("CODEX_FENCE_PREFER_TARGET", "1");
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
    writeln!(file, "{}", r#"{"nested":{"flag":true},"number":7,"text":"hello"}"#)?;

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
    let broken =
        FixtureProbe::install_from_contents(&repo_root, "tests_contract_gate_static_violation", contents)?;

    let mut cmd = Command::new(repo_root.join("bin/probe-contract-gate"));
    cmd.arg(broken.probe_id());
    let output = cmd.output().context("failed to execute probe-contract-gate")?;
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

// Helper for installing temporary probe shims under probes/ and cleaning them
// up after each test.
struct FixtureProbe {
    path: PathBuf,
    name: String,
}

impl FixtureProbe {
    fn install(repo_root: &Path, name: &str) -> Result<Self> {
        let source = repo_root.join("tests/shims/minimal_probe.sh");
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

fn repo_root() -> PathBuf {
    find_repo_root().expect("tests require repository root")
}

fn parse_boundary_object(bytes: &[u8]) -> Result<(BoundaryObject, Value)> {
    let value: Value = serde_json::from_slice(bytes)?;
    let record: BoundaryObject = serde_json::from_value(value.clone())?;
    Ok((record, value))
}

fn run_command(cmd: Command) -> Result<Output> {
    let mut cmd = cmd;
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

fn helper_binary(repo_root: &Path, name: &str) -> PathBuf {
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

fn sanitized_path_without_codex() -> Result<OsString> {
    let original = env::var_os("PATH").unwrap_or_default();
    let mut entries: Vec<PathBuf> = Vec::new();
    let codex_dir = find_in_path("codex").and_then(|path| path.parent().map(PathBuf::from));
    for entry in env::split_paths(&original) {
        if let Some(dir) = &codex_dir {
            if same_path(&entry, dir) {
                continue;
            }
        }
        entries.push(entry);
    }
    Ok(env::join_paths(entries)?)
}

fn same_path(a: &Path, b: &Path) -> bool {
    if let (Ok(a_real), Ok(b_real)) = (fs::canonicalize(a), fs::canonicalize(b)) {
        return a_real == b_real;
    }
    a == b
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for entry in env::split_paths(&path) {
        let candidate = entry.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

// Keep a helper for future assertions; suppress unused warnings for now.
#[allow(dead_code)]
fn relative_to_repo(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
