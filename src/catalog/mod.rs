//! Capability catalog wiring.
//!
//! This module wraps the JSON catalog under `schema/capabilities.json` so
//! helpers can load a validated snapshot and expose consistent identifiers.
//! Types here mirror the schema fields; callers use `CapabilityIndex` for fast
//! lookups and `CatalogRepository` when multiple catalogs are registered.

pub mod identity;
pub mod index;
pub mod model;
pub mod repository;

pub use identity::{
    CapabilityCategory, CapabilityId, CapabilityLayer, CapabilitySnapshot, CatalogKey,
};
pub use index::CapabilityIndex;
pub use model::{Capability, CapabilityCatalog, CapabilitySource, DocRef, Operations, Scope};
pub use repository::CatalogRepository;

pub use model::load_catalog_from_path;
