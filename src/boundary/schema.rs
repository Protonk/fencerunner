//! Schema loading and validation for boundary objects.
//!
//! Wraps the jsonschema compiler so emit/listen can validate records against
//! the bundled boundary-object schema.

use anyhow::{Context, Result, bail};
use jsonschema::JSONSchema;
use serde_json::Value;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
/// Loaded boundary-object schema with a cached JSONSchema validator.
pub struct BoundarySchema {
    compiled: JSONSchema,
    #[allow(dead_code)]
    raw: Arc<Value>,
}

impl BoundarySchema {
    /// Load a boundary-object schema from disk and compile it.
    pub fn load(path: &Path) -> Result<Self> {
        let value: Value = serde_json::from_reader(
            File::open(path).with_context(|| format!("opening boundary schema {}", path.display()))?,
        )
        .with_context(|| format!("parsing boundary schema {}", path.display()))?;

        // Keep a strong reference to the raw schema alongside the compiled
        // validator so callers can inspect it for diagnostics.
        let raw = Arc::new(value);
        let raw_static: &'static Value = unsafe { &*(Arc::as_ptr(&raw)) };
        let compiled = JSONSchema::compile(raw_static)
            .with_context(|| format!("compiling boundary schema {}", path.display()))?;
        Ok(Self { compiled, raw })
    }

    /// Exposes the raw schema value backing the compiled validator.
    pub fn raw_schema(&self) -> &Value {
        &self.raw
    }

    /// Validate a JSON value against the compiled schema.
    pub fn validate(&self, value: &Value) -> Result<()> {
        if let Err(errors) = self.compiled.validate(value) {
            let mut details = Vec::new();
            for err in errors {
                details.push(err.to_string());
            }
            bail!(
                "boundary object failed schema validation:\n{}",
                details.join("\n")
            );
        }
        Ok(())
    }
}
