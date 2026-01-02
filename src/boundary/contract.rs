//! Run-dir local boundary output contract loader.
//!
//! Each run directory must provide a `boundaries.json` that declares:
//! - `schema_version`
//! - `stdout.format` (currently `ndjson`)
//! - `record_schema` (JSON Schema for each boundary record)
//!
//! The contract is validated against the repo-shipped meta-schema at
//! `schema/boundaries.json`, then the embedded `record_schema` is compiled
//! so runners can validate script output deterministically.

use crate::schema::loader::{SchemaLoadOptions, load_json_schema_str};
use anyhow::{Context, Result, anyhow, bail};
use jsonschema::JSONSchema;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const DEFAULT_SCHEMA_VERSION: &str = "boundaries_v1";
const CONTRACT_SCHEMA_TITLE: &str = "Boundaries contract (v1)";
const CONTRACT_SCHEMA_REQUIRED_POINTERS: [&str; 3] = [
    "/properties/schema_version/const",
    "/properties/stdout",
    "/properties/record_schema",
];
const CONTRACT_META_SCHEMA_LABEL: &str = "schema/boundaries.json";
const CONTRACT_META_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/boundaries.json"
));

#[derive(Debug, Clone, Deserialize)]
pub struct BoundaryContract {
    pub schema_version: String,
    pub stdout: BoundaryStdout,
    pub record_schema: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoundaryStdout {
    pub format: String,
}

#[derive(Debug)]
pub struct BoundaryContractIndex {
    contract: BoundaryContract,
    record_schema: JSONSchema,
}

impl BoundaryContractIndex {
    pub fn load(path: &Path) -> Result<Self> {
        validate_against_schema(path)?;
        let contract = load_contract_from_path(path)?;
        validate_schema_version(&contract.schema_version)?;
        if contract.stdout.format != "ndjson" {
            bail!(
                "unsupported stdout.format '{}' (expected 'ndjson')",
                contract.stdout.format
            );
        }
        let record_schema = JSONSchema::compile(&contract.record_schema)
            .map_err(|err| anyhow!("compiling record_schema in {}: {}", path.display(), err))?;
        Ok(Self {
            contract,
            record_schema,
        })
    }

    pub fn contract(&self) -> &BoundaryContract {
        &self.contract
    }

    pub fn validate_record(&self, record: &Value) -> Result<()> {
        if let Err(errors) = self.record_schema.validate(record) {
            let details = errors
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            bail!("boundary record failed run-dir schema validation:\n{details}");
        }
        Ok(())
    }
}

fn load_contract_from_path(path: &Path) -> Result<BoundaryContract> {
    let file = File::open(path)
        .with_context(|| format!("opening boundaries contract {}", path.display()))?;
    let contract: BoundaryContract = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing boundaries contract {}", path.display()))?;
    Ok(contract)
}

fn validate_schema_version(schema_version: &str) -> Result<()> {
    if schema_version.is_empty() {
        bail!("schema_version must not be empty");
    }
    if !schema_version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!(
            "schema_version must match ^[A-Za-z0-9_.-]+$, got {}",
            schema_version
        );
    }
    let allowed = allowed_schema_versions();
    if !allowed.contains(schema_version) {
        bail!(
            "schema_version '{}' not in allowed set {:?}",
            schema_version,
            allowed
        );
    }
    Ok(())
}

fn allowed_schema_versions() -> BTreeSet<String> {
    BTreeSet::from_iter([default_schema_version()])
}

fn default_schema_version() -> String {
    schema_version_from_bundled().unwrap_or_else(|| DEFAULT_SCHEMA_VERSION.to_string())
}

fn schema_version_from_bundled() -> Option<String> {
    let value: Value = serde_json::from_str(CONTRACT_META_SCHEMA).ok()?;
    value
        .pointer("/properties/schema_version/const")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn validate_against_schema(contract_path: &Path) -> Result<()> {
    let file = File::open(contract_path)
        .with_context(|| format!("opening boundaries contract {}", contract_path.display()))?;
    let value: Value = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing boundaries contract {}", contract_path.display()))?;

    let allowed = allowed_schema_versions();
    let schema = load_json_schema_str(
        CONTRACT_META_SCHEMA_LABEL,
        CONTRACT_META_SCHEMA,
        SchemaLoadOptions {
            allowed_versions: Some(&allowed),
            expected_title: Some(CONTRACT_SCHEMA_TITLE),
            expected_type: Some("object"),
            required_pointers: Some(&CONTRACT_SCHEMA_REQUIRED_POINTERS),
        },
    )
    .context("loading boundaries contract schema")?;

    if let Err(errors) = schema.compiled.validate(&value) {
        let details = errors
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "boundaries contract {} failed schema validation:\n{}",
            contract_path.display(),
            details
        );
    }
    Ok(())
}
