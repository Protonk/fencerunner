//! Commitment registry types and indexing.
//!
//! Commitments are declared dependencies a probe enrolls in via `commit_help_me`.
//! A run directory contains a `commitments.json` registry that is validated
//! before probes execute.

pub mod index;
pub mod model;
