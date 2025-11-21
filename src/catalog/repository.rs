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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::model::load_catalog_from_path;
    use std::path::PathBuf;

    fn catalog_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("schema")
            .join("capabilities.json")
    }

    #[test]
    fn finds_capability_in_registered_catalog() {
        let catalog = load_catalog_from_path(&catalog_path()).expect("loaded catalog");
        let key = catalog.key.clone();
        let known_capability = catalog
            .capabilities
            .first()
            .expect("catalog should have capabilities")
            .id
            .clone();

        let mut repo = CatalogRepository::default();
        repo.register(catalog);

        let resolved = repo.find_capability(&key, &known_capability);
        assert!(resolved.is_some());
    }
}
