//! Capability catalog wiring.
//!
//! This module wraps capability catalogs on disk (for example
//! `catalogs/macos_codex_v1.json`) so helpers can load a validated snapshot and
//! expose consistent identifiers. Types here mirror the schema fields; callers
//! use `CapabilityIndex` for fast lookups and `CatalogRepository` when multiple
//! catalogs are registered.

pub mod identity;
pub mod index;
pub mod model;
pub mod repository;

pub use identity::{
    CapabilityCategory, CapabilityId, CapabilityLayer, CapabilitySnapshot, CatalogKey,
};
pub use index::CapabilityIndex;
pub use model::{
    Capability, CapabilityCatalog, CapabilitySource, CatalogMetadata, Operations, Scope,
    SourceRef,
};
pub use repository::CatalogRepository;

pub use model::load_catalog_from_path;

/// Default relative path to the bundled capability catalog.
pub const DEFAULT_CATALOG_PATH: &str = "catalogs/macos_codex_v1.json";
