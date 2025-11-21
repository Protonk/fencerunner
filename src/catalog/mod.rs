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
