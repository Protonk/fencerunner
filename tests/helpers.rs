#![cfg(unix)]

// Helper utilities and compiled probe guard rails.
mod support;
#[path = "support/common.rs"]
mod common;

use anyhow::{Context, Result};
use fencerunner::emit_support::{JsonObjectBuilder, PayloadArgs, TextSource};
use fencerunner::{list_probes, resolve_probe};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::process::Command;
use support::{helper_binary, make_executable, repo_root, run_command};
use tempfile::TempDir;

use common::{FixtureProbe, TempRepo, parse_boundary_object, repo_guard};

#[test]
fn paging_stress_probe_emits_expected_record() -> Result<()> {
    let repo_root = repo_root();
    let _guard = repo_guard();
    let fixture =
        FixtureProbe::install_from_fixture(&repo_root, "paging_stress.sh", "tests_paging_stress")?;
    let probe_run = helper_binary(&repo_root, "probe-exec");

    let mut cmd = Command::new(&probe_run);
    cmd.arg(fixture.probe_id())
        .env("TEST_PREFER_TARGET", "1");
    let output = run_command(cmd)?;
    let (record, value) = parse_boundary_object(&output.stdout)?;
    assert_eq!(record.probe.id, fixture.probe_id());
    assert_eq!(
        record
            .context
            .as_ref()
            .and_then(|ctx| ctx.probe.as_ref())
            .map(|probe| probe.primary_capability_id.0.as_str()),
        Some("cap_proc_fork_and_child_spawn")
    );
    assert_eq!(record.result.outcome, "success");
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

// === json-extract helper semantics ===

#[test]
fn json_extract_enforces_pointer_and_type() -> Result<()> {
    let repo_root = repo_root();
    let helper = helper_binary(&repo_root, "json-extract");
    let mut file = tempfile::NamedTempFile::new().context("failed to create json fixture")?;
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

// === emit-record builders and payload helpers ===

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

#[test]
fn payload_builder_rejects_large_payloads() -> Result<()> {
    let mut payload = PayloadArgs::default();
    payload
        .raw_mut()
        .insert_string("big".to_string(), "a".repeat(5000));
    let err = payload
        .build()
        .expect_err("expected payload size enforcement to fail");
    assert!(
        err.to_string().contains("Payload exceeds"),
        "expected payload size error, got: {err}"
    );
    Ok(())
}

// === portable-path helper semantics ===

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
