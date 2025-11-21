use crate::catalog::load_catalog_from_path;
use crate::catalog::{Capability, CapabilityCatalog, CapabilityId, CatalogKey};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;

const EXPECTED_SCHEMA_VERSION: &str = "macOS_codex_v1";

#[derive(Debug)]
pub struct CapabilityIndex {
    catalog_key: CatalogKey,
    catalog: CapabilityCatalog,
    by_id: BTreeMap<CapabilityId, Capability>,
}

impl CapabilityIndex {
    pub fn load(path: &Path) -> Result<Self> {
        let catalog =
            load_catalog_from_path(path).with_context(|| format!("loading {}", path.display()))?;
        validate_schema_key(&catalog.key)?;
        let by_id = build_index(&catalog)?;
        Ok(Self {
            catalog_key: catalog.key.clone(),
            catalog,
            by_id,
        })
    }

    pub fn key(&self) -> &CatalogKey {
        &self.catalog_key
    }

    pub fn capability(&self, id: &CapabilityId) -> Option<&Capability> {
        self.by_id.get(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &CapabilityId> {
        self.by_id.keys()
    }

    pub fn catalog(&self) -> &CapabilityCatalog {
        &self.catalog
    }
}

fn validate_schema_key(key: &CatalogKey) -> Result<()> {
    if key.0.is_empty() {
        bail!("schema_version must not be empty");
    }

    if !key
        .0
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!("schema_version must match ^[A-Za-z0-9_.-]+$, got {}", key.0);
    }

    if key.0 != EXPECTED_SCHEMA_VERSION {
        bail!(
            "expected schema_version={}, got {}",
            EXPECTED_SCHEMA_VERSION,
            key.0
        );
    }

    Ok(())
}

fn build_index(catalog: &CapabilityCatalog) -> Result<BTreeMap<CapabilityId, Capability>> {
    let mut map = BTreeMap::new();
    for cap in &catalog.capabilities {
        if cap.id.0.trim().is_empty() {
            bail!("encountered capability with no id");
        }
        if map.contains_key(&cap.id) {
            bail!("duplicate capability id {}", cap.id.0);
        }
        map.insert(cap.id.clone(), cap.clone());
    }
    Ok(map)
}
