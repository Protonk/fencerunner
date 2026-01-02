//! Shared JSON Schema loader with optional schema-version enforcement.
//!
//! Loads a JSON schema (usually bundled via `include_str!`), optionally
//! validates its `schema_version` const, and compiles a JSONSchema validator.

use anyhow::{Context, Result, anyhow, bail};
use jsonschema::JSONSchema;
use serde_json::Value;
use std::collections::BTreeSet;

/// Result of loading and compiling a JSON Schema.
pub(crate) struct SchemaLoadResult {
    pub compiled: JSONSchema,
}

/// Controls how schemas are loaded and normalized before compilation.
#[derive(Default)]
pub(crate) struct SchemaLoadOptions<'a> {
    /// Allowed schema_version values; enforced when present.
    pub allowed_versions: Option<&'a BTreeSet<String>>,
    /// Expected schema title; enforced when present.
    pub expected_title: Option<&'a str>,
    /// Expected root type; enforced when present.
    pub expected_type: Option<&'a str>,
    /// Required JSON pointers that must exist in the schema.
    pub required_pointers: Option<&'a [&'a str]>,
}

pub(crate) fn load_json_schema_str(
    label: &str,
    schema: &str,
    options: SchemaLoadOptions<'_>,
) -> Result<SchemaLoadResult> {
    let schema_value: Value =
        serde_json::from_str(schema).with_context(|| format!("parsing schema {label}"))?;
    load_json_schema_value(schema_value, label.to_string(), options)
}

fn load_json_schema_value(
    schema_value: Value,
    label: String,
    options: SchemaLoadOptions<'_>,
) -> Result<SchemaLoadResult> {
    if let Some(expected_title) = options.expected_title {
        let title = schema_value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if title != expected_title {
            bail!(
                "schema title '{}' does not match expected '{}'",
                title,
                expected_title
            );
        }
    }

    if let Some(expected_type) = options.expected_type {
        let schema_type = schema_value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if schema_type != expected_type {
            bail!(
                "schema type '{}' does not match expected '{}'",
                schema_type,
                expected_type
            );
        }
    }

    if let Some(pointers) = options.required_pointers {
        for pointer in pointers {
            if schema_value.pointer(pointer).is_none() {
                bail!("schema missing required pointer '{}'", pointer);
            }
        }
    }

    let schema_version = extract_schema_version(&schema_value, "/properties/schema_version/const")
        .ok_or_else(|| anyhow!("schema missing schema_version const"))?;

    if let Some(allowed) = options.allowed_versions {
        if !allowed.contains(&schema_version) {
            bail!(
                "schema_version '{}' not in allowed set {:?}",
                schema_version,
                allowed
            );
        }
    }

    let compiled = JSONSchema::compile(&schema_value)
        .map_err(|err| anyhow!("compiling schema {label}: {err}"))?;

    Ok(SchemaLoadResult { compiled })
}

fn extract_schema_version(schema: &Value, pointer: &str) -> Option<String> {
    let version = schema.pointer(pointer).and_then(Value::as_str)?;
    if version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        Some(version.to_string())
    } else {
        None
    }
}
