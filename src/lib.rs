//! Shared library for the probe harness.
//!
//! This crate is intentionally small and repetitive to make the layering
//! obvious. The root is only a module index: it declares submodules and leaves
//! their APIs unflattened so callers import from `boundary`, `catalog`,
//! `harness`, `probes`, `schema`, or `repo_tools` directly. Treat those module
//! surfaces as contracts and keep behavior aligned with README.md plus the
//! domain guides under catalogs/, boundary/, and probes/.

pub mod boundary;
pub mod catalog;
pub mod harness;
pub mod probes;
pub mod schema;
pub mod repo_tools;
// Intentionally avoid flattening submodule APIs; callers import from the
// module that owns the behavior so the structure stays visible.
