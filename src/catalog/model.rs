use crate::catalog::identity::{
    CapabilityCategory, CapabilityId, CapabilityLayer, CapabilitySnapshot, CatalogKey,
};
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
pub struct CapabilityCatalog {
    #[serde(rename = "schema_version")]
    pub key: CatalogKey,
    pub scope: Scope,
    pub docs: BTreeMap<String, DocRef>,
    pub capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Deserialize)]
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
pub struct PolicyLayer {
    pub id: String,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DocRef {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub url_hint: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
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
pub struct Operations {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CapabilitySource {
    pub doc: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub url_hint: Option<String>,
}

impl Capability {
    pub fn snapshot(&self) -> CapabilitySnapshot {
        CapabilitySnapshot {
            id: self.id.clone(),
            category: self.category.clone(),
            layer: self.layer.clone(),
        }
    }
}

pub fn load_catalog_from_path(path: &Path) -> Result<CapabilityCatalog> {
    let data = fs::read_to_string(path)?;
    let catalog: CapabilityCatalog = serde_json::from_str(&data)?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("schema")
            .join("capabilities.json")
    }

    #[test]
    fn load_real_catalog() {
        let catalog = load_catalog_from_path(&catalog_path()).expect("catalog loads");
        assert!(!catalog.key.0.is_empty());
        assert!(!catalog.capabilities.is_empty());
        for cap in catalog.capabilities {
            assert!(!cap.id.0.is_empty());
            assert!(
                !matches!(cap.category, CapabilityCategory::Other(ref v) if v.is_empty()),
                "category should not be empty"
            );
            assert!(
                !matches!(cap.layer, CapabilityLayer::Other(ref v) if v.is_empty()),
                "layer should not be empty"
            );
        }
    }
}
