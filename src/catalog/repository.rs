//! Holds one or more capability catalogs for lookup by key.
//!
//! The repository lets callers resolve capability metadata using the catalog
//! key stored in boundary objects, keeping catalog selection explicit even when
//! multiple versions are loaded.

use crate::catalog::identity::{CapabilityId, CatalogKey};
use crate::catalog::model::{Capability, CapabilityCatalog};
use std::collections::BTreeMap;

#[derive(Default)]
/// In-memory store for capability catalogs keyed by `CatalogKey`.
pub struct CatalogRepository {
    catalogs: BTreeMap<CatalogKey, CapabilityCatalog>,
}

impl CatalogRepository {
    /// Register a catalog for later lookup.
    pub fn register(&mut self, catalog: CapabilityCatalog) {
        self.catalogs.insert(catalog.key.clone(), catalog);
    }

    /// Fetch a catalog by key, if present.
    pub fn get(&self, key: &CatalogKey) -> Option<&CapabilityCatalog> {
        self.catalogs.get(key)
    }

    /// Resolve a capability entry inside a registered catalog.
    pub fn find_capability(&self, key: &CatalogKey, id: &CapabilityId) -> Option<&Capability> {
        self.get(key)?.capabilities.iter().find(|cap| &cap.id == id)
    }
}
