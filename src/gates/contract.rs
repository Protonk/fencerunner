//! Run-dir-local gate contract loader.
//!
//! Each run directory may provide a `gates.json` that declares run-dir-wide
//! gate enrollments for the contract gate.
//!
//! The contract is validated against the repo-shipped meta-schema at
//! `schema/gates.json`.

use crate::schema::loader::{SchemaLoadOptions, load_json_schema_str};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const DEFAULT_SCHEMA_VERSION: &str = "gates_v1";
const CONTRACT_SCHEMA_TITLE: &str = "Gates contract (v1)";
const CONTRACT_SCHEMA_REQUIRED_POINTERS: [&str; 3] = [
    "/properties/schema_version/const",
    "/properties/gates",
    "/properties/gates/properties/enforced_checks",
];
const CONTRACT_META_SCHEMA_LABEL: &str = "schema/gates.json";
const CONTRACT_META_SCHEMA: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/schema/gates.json"));

#[derive(Debug, Clone, Deserialize)]
pub struct GatesContract {
    pub schema_version: String,
    #[serde(default)]
    pub gates: GateSettings,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GateSettings {
    #[serde(default)]
    pub enforced_checks: Vec<String>,
}

#[derive(Debug)]
pub struct GatesContractIndex {
    contract: GatesContract,
}

impl GatesContractIndex {
    pub fn load(path: &Path) -> Result<Self> {
        validate_against_schema(path)?;
        let contract = load_contract_from_path(path)?;
        validate_schema_version(&contract.schema_version)?;
        Ok(Self { contract })
    }

    pub fn contract(&self) -> &GatesContract {
        &self.contract
    }

    pub fn enforces_stderr_empty(&self) -> bool {
        self.contract
            .gates
            .enforced_checks
            .iter()
            .any(|check| check == "stderr.empty")
    }
}

fn load_contract_from_path(path: &Path) -> Result<GatesContract> {
    let file =
        File::open(path).with_context(|| format!("opening gates contract {}", path.display()))?;
    let contract: GatesContract = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing gates contract {}", path.display()))?;
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
        .with_context(|| format!("opening gates contract {}", contract_path.display()))?;
    let value: Value = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing gates contract {}", contract_path.display()))?;

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
    .context("loading gates contract schema")?;

    if let Err(errors) = schema.compiled.validate(&value) {
        let details = errors
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "gates contract {} failed schema validation:\n{}",
            contract_path.display(),
            details
        );
    }
    Ok(())
}
