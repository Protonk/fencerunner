//! Shared JSON Schema loader with optional schema-version enforcement.
//!
//! Loads a JSON schema from disk, optionally validates its `schema_version`
//! const, and compiles a JSONSchema validator.

use anyhow::{Context, Result, anyhow, bail};
use jsonschema::JSONSchema;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// Result of loading and compiling a JSON Schema.
pub(crate) struct SchemaLoadResult {
    pub compiled: JSONSchema,
}

/// Controls how schemas are loaded and normalized before compilation.
pub(crate) struct SchemaLoadOptions<'a> {
    /// Allowed schema_version values; enforced when present.
    pub allowed_versions: Option<&'a BTreeSet<String>>,
}

impl<'a> Default for SchemaLoadOptions<'a> {
    fn default() -> Self {
        Self { allowed_versions: None }
    }
}

pub(crate) fn load_json_schema(
    path: &Path,
    options: SchemaLoadOptions<'_>,
) -> Result<SchemaLoadResult> {
    let schema_value: Value = serde_json::from_reader(
        File::open(path).with_context(|| format!("opening schema {}", path.display()))?,
    )
    .with_context(|| format!("parsing schema {}", path.display()))?;

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

    let raw = Arc::new(schema_value);
    let raw_static: &'static Value = unsafe { &*(Arc::as_ptr(&raw)) };
    let compiled = JSONSchema::compile(raw_static)
        .with_context(|| format!("compiling schema {}", path.display()))?;

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
