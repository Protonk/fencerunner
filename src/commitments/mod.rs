//! Commitment registry types and indexing.
//!
//! Commitments are declared dependencies a script enrolls in via `commit_help_me`.
//! A run directory contains a `commitments.json` registry that is validated
//! before scripts execute.

pub mod index;
pub mod model;
