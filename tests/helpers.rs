#![cfg(unix)]

// Helper utilities and compiled script guard rails.
#[path = "support/common.rs"]
mod common;
mod support;

use anyhow::Result;
use fencerunner::harness::payload::{JsonObjectBuilder, PayloadArgs, TextSource};
use fencerunner::scripts::discovery::{list_scripts, resolve_script};
use serde_json::Value;
use std::fs;
use support::make_executable;

use common::TempRepo;

// list_scripts and resolve_script should agree on how ids and extensions behave.
#[test]
fn list_and_resolve_scripts_share_semantics() -> Result<()> {
    let temp = TempRepo::new();
    let scripts_dir = temp.root.join("scripts");
    fs::create_dir_all(&scripts_dir)?;
    let script = scripts_dir.join("example.sh");
    fs::write(&script, "#!/usr/bin/env bash\nexit 0\n")?;
    make_executable(&script)?;

    let scripts = list_scripts(&scripts_dir)?;
    assert_eq!(scripts.len(), 1);
    assert_eq!(scripts[0].id, "example");

    let resolved = resolve_script(&scripts_dir, "example")?;
    assert_eq!(resolved.path, fs::canonicalize(&script)?);
    let resolved_with_ext = resolve_script(&scripts_dir, "example.sh")?;
    assert_eq!(resolved_with_ext.path, resolved.path);
    Ok(())
}

// === emit-record builders and payload helpers ===

// JsonObjectBuilder should allow later inserts to override earlier keys.
// This mirrors how CLI flags can override JSON file contents.
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

// PayloadArgs supports inline stdout/stderr text and raw payload fields.
#[test]
fn payload_builder_accepts_inline_snippets() -> Result<()> {
    let mut payload = PayloadArgs::default();
    // stdout/stderr snippets are required even when empty.
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

// Payload size limits protect downstream consumers from huge embedded blobs.
#[test]
fn payload_builder_rejects_large_payloads() -> Result<()> {
    let mut payload = PayloadArgs::default();
    payload.set_stdout(TextSource::Inline("".to_string()))?;
    payload.set_stderr(TextSource::Inline("".to_string()))?;
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
