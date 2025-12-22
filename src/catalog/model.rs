//! Deserializable representation of the bundled capability catalog.
//!
//! The types mirror the catalog schema so helpers and tests can reason about
//! capability metadata without ad-hoc JSON handling. Use `CapabilityIndex` for
//! validation and id lookup; use these structs when the full catalog surface is
//! needed (sources, categories, policy layers).

use crate::catalog::identity::{
    CapabilityCategory, CapabilityId, CapabilityLayer, CapabilitySnapshot, CatalogKey,
};
use anyhow::Result;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
/// Full capability catalog as stored on disk.
pub struct CapabilityCatalog {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    pub catalog: CatalogMetadata,
    pub scope: Scope,
    #[serde(default)]
    pub sources: BTreeMap<String, SourceRef>,
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Describes the catalog instance (environment/client labels, human title, and key).
pub struct CatalogMetadata {
    pub key: CatalogKey,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Top-level catalog scope: which system, layers, and categories this snapshot covers.
pub struct Scope {
    pub description: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default, deserialize_with = "deserialize_policy_layers")]
    pub policy_layers: Vec<PolicyLayer>,
    pub categories: BTreeMap<String, String>,
    #[serde(default)]
    pub limitations: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Short description of a policy layer referenced in the catalog.
pub struct PolicyLayer {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Source reference pulled into the catalog for traceability.
pub struct SourceRef {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub url_hint: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Core capability entry describing one observable policy surface.
pub struct Capability {
    pub id: CapabilityId,
    pub category: CapabilityCategory,
    pub layer: CapabilityLayer,
    pub description: String,
    #[serde(default)]
    pub status: Option<String>,
    pub operations: Operations,
    #[serde(default)]
    pub meta_ops: Vec<String>,
    #[serde(default)]
    pub agent_controls: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default, deserialize_with = "deserialize_sources")]
    pub sources: Vec<CapabilitySource>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Allowed/denied operations associated with a capability.
pub struct Operations {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
/// Source citations for a capability.
pub struct CapabilitySource {
    pub doc: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub url_hint: Option<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum PolicyLayersInput {
    List(Vec<PolicyLayer>),
    Map(BTreeMap<String, String>),
}

fn deserialize_policy_layers<'de, D>(deserializer: D) -> Result<Vec<PolicyLayer>, D::Error>
where
    D: Deserializer<'de>,
{
    // Accept both the modern list form and the legacy map form for backward
    // compatibility with older catalogs.
    let input = Option::<PolicyLayersInput>::deserialize(deserializer)?;
    Ok(match input {
        None => Vec::new(),
        Some(PolicyLayersInput::List(list)) => list,
        Some(PolicyLayersInput::Map(map)) => map
            .into_iter()
            .map(|(id, description)| PolicyLayer {
                id,
                description,
                extensions: BTreeMap::new(),
            })
            .collect(),
    })
}

#[derive(Clone, Debug, Deserialize)]
struct SourceDetail {
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    url_hint: Option<String>,
    #[serde(default)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
enum SourcesInput {
    List(Vec<CapabilitySource>),
    Map(BTreeMap<String, SourceDetail>),
}

fn deserialize_sources<'de, D>(deserializer: D) -> Result<Vec<CapabilitySource>, D::Error>
where
    D: Deserializer<'de>,
{
    // Sources can appear as a list or a map keyed by doc id; normalize to a list.
    let input = Option::<SourcesInput>::deserialize(deserializer)?;
    Ok(match input {
        None => Vec::new(),
        Some(SourcesInput::List(list)) => list,
        Some(SourcesInput::Map(map)) => map
            .into_iter()
            .map(|(doc, detail)| CapabilitySource {
                doc,
                section: detail.section,
                url_hint: detail.url_hint,
                extensions: detail.extensions,
            })
            .collect(),
    })
}

impl Capability {
    /// Create the compact snapshot used in boundary objects.
    pub fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot {
            id: self.id.clone(),
            category: self.category.clone(),
            layer: self.layer.clone(),
        }
    }
}

/// Read and parse a capability catalog from disk without additional validation.
pub fn load_catalog_from_path(path: &Path) -> Result<CapabilityCatalog> {
    // Schema validation happens in CapabilityIndex; this is a raw parse helper.
    let data = fs::read_to_string(path)?;
    let catalog: CapabilityCatalog = serde_json::from_str(&data)?;
    Ok(catalog)
}
