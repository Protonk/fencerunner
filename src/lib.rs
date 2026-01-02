//! Shared library for the probe harness.
//!
//! This crate is intentionally small and repetitive to make the layering
//! obvious. The root is only a module index: it declares submodules and leaves
//! their APIs unflattened so callers import from `boundary`, `commitments`,
//! `gates`, `harness`, `probes`, `schema`, or `repo_tools`
//! directly. Treat those module
//! surfaces as contracts and keep behavior aligned with README.md plus the
//! narrative docs under docs/ and the meta-schemas under schema/.

pub mod boundary;
pub mod commands;
pub mod commitments;
pub mod gates;
pub mod harness;
pub mod probes;
pub mod repo_tools;
pub mod schema;
// Intentionally avoid flattening submodule APIs; callers import from the
// module that owns the behavior so the structure stays visible.
