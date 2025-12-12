//! Shared JSON Schema loader with optional descriptor/canonical enforcement.
//!
//! This keeps boundary and catalog schema handling aligned: callers can
//! validate descriptor wrappers (inline or via `schema_path`), enforce canonical copies, patch
//! `schema_version` consts, and compile a JSONSchema validator from the
//! resulting schema payload.

use anyhow::{Context, Result, anyhow, bail};
use jsonschema::JSONSchema;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// Result of loading and compiling a JSON Schema.
pub(crate) struct SchemaLoadResult {
    pub schema_version: String,
    pub compiled: JSONSchema,
    pub raw: Arc<Value>,
}

/// Controls how schemas are loaded and normalized before compilation.
pub(crate) struct SchemaLoadOptions<'a> {
    /// Optional descriptor contract path; when present, descriptors are
    /// validated and unwrapped. If validation fails and `allow_plain_schema` is
    /// false, loading fails.
    pub descriptor_schema_path: Option<&'a Path>,
    /// Optional JSON pointer used to extract the expected schema_version from a
    /// descriptor. When set, the pointed-to value is used in place of the
    /// schema's embedded const.
    pub descriptor_expected_version_pointer: Option<&'a str>,
    /// Optional canonical schema path to compare before patching.
    pub canonical_schema_path: Option<&'a Path>,
    /// Where to find the schema_version const inside the schema payload.
    pub schema_version_pointer: &'a str,
    /// Override schema_version when provided (used to align consts).
    pub expected_version: Option<&'a str>,
    /// Allowed schema_version values; enforced when present.
    pub allowed_versions: Option<&'a BTreeSet<String>>,
    /// When true, falls back to treating the file as a plain schema if
    /// descriptor validation fails.
    pub allow_plain_schema: bool,
    /// Patch the schema_version const in the schema payload to match
    /// `expected_version` (or the extracted version when no override is set).
    pub patch_schema_version_const: bool,
}

impl<'a> Default for SchemaLoadOptions<'a> {
    fn default() -> Self {
        Self {
            descriptor_schema_path: None,
            descriptor_expected_version_pointer: None,
            canonical_schema_path: None,
            schema_version_pointer: "/properties/schema_version/const",
            expected_version: None,
            allowed_versions: None,
            allow_plain_schema: true,
            patch_schema_version_const: false,
        }
    }
}

pub(crate) fn load_json_schema(
    path: &Path,
    mut options: SchemaLoadOptions<'_>,
) -> Result<SchemaLoadResult> {
    let descriptor_or_schema: Value = serde_json::from_reader(
        File::open(path).with_context(|| format!("opening schema {}", path.display()))?,
    )
    .with_context(|| format!("parsing schema {}", path.display()))?;

    let mut schema_value = descriptor_or_schema.clone();
    let descriptor_has_schema = descriptor_or_schema.get("schema").is_some()
        || descriptor_or_schema.get("schema_path").is_some();
    let mut descriptor_valid = false;

    if let Some(descriptor_schema_path) = options.descriptor_schema_path.take() {
        if descriptor_has_schema {
            if descriptor_schema_path.exists() {
                let descriptor_contract: Value = serde_json::from_reader(
                    File::open(descriptor_schema_path).with_context(|| {
                        format!(
                            "opening descriptor contract {}",
                            descriptor_schema_path.display()
                        )
                    })?,
                )
                .with_context(|| {
                    format!(
                        "parsing descriptor contract {}",
                        descriptor_schema_path.display()
                    )
                })?;
                let contract_arc = Arc::new(descriptor_contract);
                let contract_static: &'static Value = unsafe { &*(Arc::as_ptr(&contract_arc)) };
                let compiled_descriptor =
                    JSONSchema::compile(contract_static).with_context(|| {
                        format!(
                            "compiling descriptor contract {}",
                            descriptor_schema_path.display()
                        )
                    })?;
                if let Err(errors) = compiled_descriptor.validate(&descriptor_or_schema) {
                    if !options.allow_plain_schema {
                        let details = errors
                            .map(|err| err.to_string())
                            .collect::<Vec<_>>()
                            .join("\n");
                        bail!(
                            "schema descriptor {} failed validation:\n{}",
                            path.display(),
                            details
                        );
                    }
                } else {
                    descriptor_valid = true;
                }
            } else if !options.allow_plain_schema {
                bail!(
                    "schema descriptor {} missing 'schema' field",
                    path.display()
                );
            }
        } else if !options.allow_plain_schema {
            bail!(
                "schema descriptor {} missing 'schema' field",
                path.display()
            );
        }
    }

    if descriptor_has_schema && (descriptor_valid || options.allow_plain_schema) {
        if let Some(schema_path) = descriptor_or_schema
            .get("schema_path")
            .and_then(Value::as_str)
        {
            let resolved = if Path::new(schema_path).is_absolute() {
                Path::new(schema_path).to_path_buf()
            } else if let Some(base) = path.parent() {
                base.join(schema_path)
            } else {
                Path::new(schema_path).to_path_buf()
            };
            let nested_path = resolved
                .canonicalize()
                .unwrap_or_else(|_| resolved.to_path_buf());
            let schema_file = File::open(&nested_path).with_context(|| {
                format!(
                    "opening schema {} referenced by {}",
                    nested_path.display(),
                    path.display()
                )
            })?;
            schema_value = serde_json::from_reader(schema_file).with_context(|| {
                format!(
                    "parsing schema {} referenced by {}",
                    nested_path.display(),
                    path.display()
                )
            })?;
        } else if let Some(inline_schema) = descriptor_or_schema.get("schema") {
            schema_value = inline_schema.clone();
        } else if !options.allow_plain_schema {
            bail!(
                "schema descriptor {} missing 'schema' or 'schema_path' field",
                path.display()
            );
        }
    }

    let original_schema = schema_value.clone();

    let schema_version = if descriptor_has_schema {
        if let Some(pointer) = options.descriptor_expected_version_pointer {
            descriptor_or_schema
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(|v| v.to_string())
                .ok_or_else(|| anyhow!("schema descriptor missing version at pointer {pointer}"))?
        } else if let Some(version) = options.expected_version {
            version.to_string()
        } else {
            extract_schema_version(&schema_value, options.schema_version_pointer)
                .ok_or_else(|| anyhow!("schema missing schema_version const"))?
        }
    } else if let Some(version) = options.expected_version {
        version.to_string()
    } else {
        extract_schema_version(&schema_value, options.schema_version_pointer)
            .ok_or_else(|| anyhow!("schema missing schema_version const"))?
    };

    if let Some(allowed) = options.allowed_versions {
        if !allowed.contains(&schema_version) {
            bail!(
                "schema_version '{}' not in allowed set {:?}",
                schema_version,
                allowed
            );
        }
    }

    if let Some(canonical_path) = options.canonical_schema_path {
        if canonical_path.exists() {
            let canonical_value: Value =
                serde_json::from_reader(File::open(canonical_path).with_context(|| {
                    format!("opening canonical schema {}", canonical_path.display())
                })?)
                .with_context(|| {
                    format!("parsing canonical schema {}", canonical_path.display())
                })?;
            if canonical_value != original_schema {
                bail!(
                    "schema {} does not match canonical schema {}",
                    path.display(),
                    canonical_path.display()
                );
            }
        }
    }

    let mut schema_for_compile = schema_value;
    if options.patch_schema_version_const {
        let target = schema_for_compile
            .pointer_mut(options.schema_version_pointer)
            .ok_or_else(|| {
                anyhow!(
                    "schema missing pointer {} for schema_version const",
                    options.schema_version_pointer
                )
            })?;
        *target = Value::String(schema_version.clone());
    }

    let raw = Arc::new(schema_for_compile);
    let raw_static: &'static Value = unsafe { &*(Arc::as_ptr(&raw)) };
    let compiled = JSONSchema::compile(raw_static)
        .with_context(|| format!("compiling schema {}", path.display()))?;

    Ok(SchemaLoadResult {
        schema_version,
        compiled,
        raw,
    })
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
