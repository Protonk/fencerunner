#![cfg(unix)]

// Schema and serialization guard rails: boundary object shape, catalog schema,
// and serde round-trip coverage for boundary/capability types.
mod support;
#[path = "support/common.rs"]
mod common;

use anyhow::{Context, Result, bail};
use fencerunner::boundary::{BoundaryObject, BoundarySchema, CapabilityContext};
use fencerunner::catalog::{
    CapabilityCategory, CapabilityId, CapabilityLayer, CapabilitySnapshot, CatalogKey,
};
use fencerunner::repo_tools::{default_catalog_path, resolve_boundary_schema_path};
use jsonschema::JSONSchema;
use serde_json::{Value, json};
use std::fs::File;
use std::sync::OnceLock;
use support::{helper_binary, repo_root, run_command};
use tempfile::NamedTempFile;

use common::{default_catalog_key, parse_boundary_object, sample_boundary_object};

// Ensures emit-record produces schema-valid boundary objects with required context.
// We treat this as an integration test: use the real helper to catch flag or
// schema drift that pure Rust unit tests would miss.
#[test]
fn boundary_object_schema() -> Result<()> {
    let repo_root = repo_root();
    let emit_record = helper_binary(&repo_root, "emit-record");
    // Use a payload file to exercise the file-based payload path.
    let payload = json!({
        "stdout_snippet": "fixture-stdout",
        "stderr_snippet": "fixture-stderr",
        "raw": {"detail": "schema-test"}
    });

    let mut payload_file = NamedTempFile::new().context("failed to allocate payload file")?;
    serde_json::to_writer(&mut payload_file, &payload)?;

    let mut emit_cmd = std::process::Command::new(&emit_record);
    emit_cmd
        .arg("--probe-name")
        .arg("schema_test_fixture")
        .arg("--primary-capability-id")
        .arg("cap_fs_read_workspace_tree")
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
    // Prefer target builds during tests so the freshly built helper is used.
    emit_cmd.env("TEST_PREFER_TARGET", "1");
    let output = run_command(emit_cmd)?;

    // Parse both typed and raw JSON so we can assert on types and raw shape.
    let (record, value) = parse_boundary_object(&output.stdout)?;

    // Probe identity and core operation fields should match the CLI inputs.
    assert_eq!(record.probe.id, "schema_test_fixture");
    assert_eq!(record.operation.kind, "fs.read");
    assert_eq!(record.operation.target, "/dev/null");
    assert!(
        value
            .get("operation")
            .and_then(|op| op.get("args"))
            .map(|args| args.is_object())
            .unwrap_or(false)
    );

    // Outcome validation covers the enumerated values expected by the schema.
    assert!(matches!(
        record.result.outcome.as_str(),
        "success" | "denied" | "partial" | "error"
    ));
    let details = record.result.details.as_ref().expect("details present");
    assert_eq!(details.exit_code, Some(0));
    assert_eq!(details.message.as_deref(), Some("fixture"));
    assert!(details.errno.is_none());
    assert!(details.error_detail.is_none());

    // capabilities_schema_version is a catalog key; keep it URL-safe.
    let cap_schema = value
        .pointer("/context/capabilities_schema_version")
        .and_then(Value::as_str)
        .expect("capabilities_schema_version present");
    assert!(
        cap_schema
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')),
        "capabilities_schema_version must match ^[A-Za-z0-9_.-]+$"
    );
    assert!(
        value
            .pointer("/context/stack")
            .map(|s| s.is_object())
            .unwrap_or(false)
    );
    // The run.command field captures the probe command we passed to emit-record.
    assert!(
        value
            .pointer("/context/run/command")
            .and_then(Value::as_str)
            .is_some()
    );
    if let Some(ids) = value.pointer("/context/probe/secondary_capability_ids") {
        assert!(ids.is_array());
    }

    // Payload fields should round-trip with the values we provided.
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

    // Capability snapshots should be populated based on the catalog id.
    let capability_context = value
        .pointer("/context/capability_context")
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
    if let Some(secondary) = capability_context.get("secondary") {
        assert!(secondary.is_array());
    }

    // Cache schema parsing across tests; JSONSchema compilation is expensive.
    static BOUNDARY_OBJECT_SCHEMA: OnceLock<BoundarySchema> = OnceLock::new();
    let schema = BOUNDARY_OBJECT_SCHEMA.get_or_init(|| {
        let schema_path = resolve_boundary_schema_path(&repo_root).expect("resolve boundary schema");
        BoundarySchema::load(&schema_path).expect("load boundary schema")
    });
    // Full schema validation is the ultimate contract check.
    schema.validate(&value)?;

    Ok(())
}

// Confirms the bundled capability catalog satisfies the published JSON schema.
// This is a schema-level check; CatalogIndex adds stricter runtime validation.
#[test]
fn capability_catalog_schema() -> Result<()> {
    let repo_root = repo_root();
    let schema_path = repo_root.join("catalogs/capability_catalog.schema.json");
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

// Serde round-trip for BoundaryObject ensures the structs map 1:1 with JSON.
// Limitation: this uses a synthetic sample object, not the full emitted record.
#[test]
fn boundary_object_round_trips_structs() -> Result<()> {
    let bo = sample_boundary_object();
    let value = serde_json::to_value(&bo)?;
    let back: BoundaryObject = serde_json::from_value(value)?;
    assert_eq!(back.operation.kind, "fs.read");
    assert_eq!(back.result.outcome, "success");
    let run_command = back
        .context
        .as_ref()
        .and_then(|ctx| ctx.run.as_ref())
        .map(|run| run.command.as_str());
    assert_eq!(run_command, Some("echo test"));
    let primary_id = back
        .context
        .as_ref()
        .and_then(|ctx| ctx.capability_context.as_ref())
        .map(|ctx| ctx.primary.id.0.as_str());
    assert_eq!(primary_id, Some("cap_id"));
    Ok(())
}

// The catalog key is serialized as a string; this ensures we don't lose it.
#[test]
fn capabilities_schema_version_serializes_in_json() -> Result<()> {
    let bo = sample_boundary_object();
    let value = serde_json::to_value(&bo)?;
    assert_eq!(
        value
            .pointer("/context/capabilities_schema_version")
            .and_then(|v| v.as_str()),
        Some(default_catalog_key().0.as_str())
    );
    Ok(())
}

// Capability snapshots should serialize to the JSON shape the schema expects.
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

// Known enum values should round-trip, while unknown strings should map to `Other`.
// This keeps catalog evolution compatible with older code.
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

// Same round-trip behavior for policy layers.
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

// Schema-aligned fields should survive a serde round-trip intact.
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

// Catalog keys and capability ids are thin wrappers; serde should preserve values.
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
