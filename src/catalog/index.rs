//! Indexed view of the capability catalog.
//!
//! The index enforces the expected catalog schema key and provides fast lookup
//! by capability id. It is intentionally strict about duplicates and unknown
//! schema versions so helper binaries cannot silently consume mismatched
//! catalogs.

use crate::catalog::load_catalog_from_path;
use crate::catalog::{Capability, CapabilityCatalog, CapabilityId, CatalogKey};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;

// The harness currently ships a single catalog; reject unexpected versions
// rather than risk emitting records with mismatched metadata.
const EXPECTED_SCHEMA_VERSION: &str = "macOS_codex_v1";

#[derive(Debug)]
/// Capability catalog plus a derived index keyed by capability id.
pub struct CapabilityIndex {
    catalog_key: CatalogKey,
    catalog: CapabilityCatalog,
    by_id: BTreeMap<CapabilityId, Capability>,
}

impl CapabilityIndex {
    /// Load and validate the catalog from disk.
    ///
    /// Validates the schema key, ensures capability ids are unique, and builds
    /// a deterministic BTreeMap for fast lookups.
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

    /// The catalog key declared in the loaded file.
    pub fn key(&self) -> &CatalogKey {
        &self.catalog_key
    }

    /// Resolve a capability by id.
    ///
    /// Returns `None` instead of erroring; callers surface errors with the CLI
    /// context that referenced the missing id.
    pub fn capability(&self, id: &CapabilityId) -> Option<&Capability> {
        self.by_id.get(id)
    }

    /// Iterates capability ids in stable order.
    pub fn ids(&self) -> impl Iterator<Item = &CapabilityId> {
        self.by_id.keys()
    }

    /// Access the underlying catalog (categories, docs, etc.).
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
