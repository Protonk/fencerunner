//! Catalog snapshot helpers tied to boundary objects.
//!
//! Attaches catalog snapshots to boundary records and resolves them back
//! through CatalogRepository for downstream lookups.

use crate::catalog::{Capability, CatalogKey, CatalogRepository};

use super::types::{BoundaryObject, CapabilityContext, ContextInfo};

impl BoundaryObject {
    /// Attach capability snapshots from the current catalog to the boundary
    /// object.
    ///
    /// Callers set the catalog version and snapshot fields before emitting the
    /// record so consumers can resolve metadata without reloading a catalog.
    pub fn with_capabilities(
        mut self,
        catalog_key: CatalogKey,
        primary: &Capability,
        secondary: &[&Capability],
    ) -> Self {
        // Mutate in place because the emitter typically constructs the record
        // first and then attaches catalog snapshots before serialization.
        let context = self.ensure_context();
        context.capabilities_schema_version = Some(catalog_key);
        context.capability_context = Some(CapabilityContext {
            primary: primary.snapshot(),
            secondary: secondary.iter().map(|c| c.snapshot()).collect(),
        });
        self
    }

    fn ensure_context(&mut self) -> &mut ContextInfo {
        if self.context.is_none() {
            self.context = Some(ContextInfo::default());
        }
        self.context.as_mut().expect("context exists")
    }
}

impl CatalogRepository {
    /// Resolve the capability metadata referenced by a boundary object against
    /// the registered catalogs.
    ///
    /// Returns `None` when the record references an unknown catalog key or
    /// capability id. This lookup intentionally trusts the
    /// `capabilities_schema_version` carried in the record so mismatches surface
    /// as empty lookups rather than cross-catalog ambiguities.
    pub fn lookup_context<'a>(
        &'a self,
        bo: &BoundaryObject,
    ) -> Option<(&'a Capability, Vec<&'a Capability>)> {
        // Use the catalog key embedded in the record. This keeps lookups
        // explicit even if multiple catalogs are loaded in memory.
        let context = bo.context.as_ref()?;
        let catalog_key = context.capabilities_schema_version.as_ref()?;
        let snapshot = context.capability_context.as_ref()?;
        let catalog = self.get(catalog_key)?;
        let primary = catalog
            .capabilities
            .iter()
            .find(|c| c.id == snapshot.primary.id)?;

        let secondary = snapshot
            .secondary
            .iter()
            .filter_map(|snap| catalog.capabilities.iter().find(|c| c.id == snap.id))
            .collect();

        Some((primary, secondary))
    }
}
