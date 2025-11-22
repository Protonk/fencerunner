//! Deserializable representation of `schema/capabilities.json`.
//!
//! The types mirror the catalog schema so helpers and tests can reason about
//! capability metadata without ad-hoc JSON handling. Use `CapabilityIndex` for
//! validation and id lookup; use these structs when the full catalog surface is
//! required (docs, categories, policy layers).

use crate::catalog::identity::{
    CapabilityCategory, CapabilityId, CapabilityLayer, CapabilitySnapshot, CatalogKey,
};
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
/// Full capability catalog as stored on disk.
pub struct CapabilityCatalog {
    #[serde(rename = "schema_version")]
    pub key: CatalogKey,
    pub scope: Scope,
    pub docs: BTreeMap<String, DocRef>,
    pub capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize)]
/// Top-level catalog scope: which system, layers, and categories this snapshot covers.
pub struct Scope {
    pub description: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub policy_layers: Vec<PolicyLayer>,
    pub categories: BTreeMap<String, String>,
    #[serde(default)]
    pub limitations: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
/// Short description of a policy layer referenced in the catalog.
pub struct PolicyLayer {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize)]
/// Document reference pulled into the catalog for traceability.
pub struct DocRef {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub url_hint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
/// Core capability entry describing one observable policy surface.
pub struct Capability {
    pub id: CapabilityId,
    pub category: CapabilityCategory,
    pub layer: CapabilityLayer,
    pub description: String,
    pub operations: Operations,
    #[serde(default)]
    pub meta_ops: Vec<String>,
    #[serde(default)]
    pub agent_controls: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub sources: Vec<CapabilitySource>,
}

#[derive(Clone, Debug, Deserialize)]
/// Allowed/denied operations associated with a capability.
pub struct Operations {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
/// Source citations for a capability.
pub struct CapabilitySource {
    pub doc: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub url_hint: Option<String>,
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
    let data = fs::read_to_string(path)?;
    let catalog: CapabilityCatalog = serde_json::from_str(&data)?;
    Ok(catalog)
}
