//! Indexed view of a probe commitments registry.
//!
//! The index enforces the schema version, validates the registry against the
//! bundled JSON schema, and rejects duplicate commitment ids within a single
//! run dir.

use crate::commitments::model::{Commitment, CommitmentHelp, CommitmentRegistry};
use crate::schema::loader::{SchemaLoadOptions, load_json_schema_str};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

const DEFAULT_SCHEMA_VERSION: &str = "commitments_v1";
const REGISTRY_SCHEMA_TITLE: &str = "Probe commitments registry (v1)";
const REGISTRY_SCHEMA_REQUIRED_POINTERS: [&str; 3] = [
    "/properties/schema_version/const",
    "/properties/commitments",
    "/$defs/commitment",
];
const REGISTRY_META_SCHEMA_LABEL: &str = "schema/commitments.json";
const REGISTRY_META_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/commitments.json"
));

#[derive(Debug)]
pub struct CommitmentIndex {
    registry: CommitmentRegistry,
    by_id: BTreeMap<String, Commitment>,
}

impl CommitmentIndex {
    pub fn load(path: &Path) -> Result<Self> {
        validate_against_schema(path)?;
        let registry = load_registry_from_path(path)?;
        validate_schema_version(&registry.schema_version)?;
        let by_id = build_index(&registry)?;
        Ok(Self { registry, by_id })
    }

    pub fn commitment(&self, id: &str) -> Option<&Commitment> {
        self.by_id.get(id)
    }

    pub fn supports_help(&self, id: &str, help: CommitmentHelp) -> bool {
        self.commitment(id)
            .is_some_and(|cap| cap.helps.iter().any(|candidate| candidate == &help))
    }

    pub fn registry(&self) -> &CommitmentRegistry {
        &self.registry
    }
}

fn load_registry_from_path(path: &Path) -> Result<CommitmentRegistry> {
    let data = std::fs::read_to_string(path)?;
    let registry: CommitmentRegistry = serde_json::from_str(&data)?;
    Ok(registry)
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
    let value: Value = serde_json::from_str(REGISTRY_META_SCHEMA).ok()?;
    value
        .pointer("/properties/schema_version/const")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn build_index(registry: &CommitmentRegistry) -> Result<BTreeMap<String, Commitment>> {
    let mut map = BTreeMap::new();
    for commitment in &registry.commitments {
        if commitment.id.trim().is_empty() {
            bail!("encountered commitment with no id");
        }
        if map.contains_key(&commitment.id) {
            bail!("duplicate commitment id {}", commitment.id);
        }
        map.insert(commitment.id.clone(), commitment.clone());
    }
    Ok(map)
}

fn validate_against_schema(registry_path: &Path) -> Result<()> {
    let file = File::open(registry_path)
        .with_context(|| format!("opening commitments registry {}", registry_path.display()))?;
    let value: Value = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("parsing commitments registry {}", registry_path.display()))?;

    let allowed = allowed_schema_versions();
    let schema = load_json_schema_str(
        REGISTRY_META_SCHEMA_LABEL,
        REGISTRY_META_SCHEMA,
        SchemaLoadOptions {
            allowed_versions: Some(&allowed),
            expected_title: Some(REGISTRY_SCHEMA_TITLE),
            expected_type: Some("object"),
            required_pointers: Some(&REGISTRY_SCHEMA_REQUIRED_POINTERS),
        },
    )
    .context("loading commitments schema")?;

    if let Err(errors) = schema.compiled.validate(&value) {
        let details = errors
            .map(|err| err.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "commitments registry {} failed schema validation:\n{}",
            registry_path.display(),
            details
        );
    }
    Ok(())
}
