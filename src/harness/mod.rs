//! Harness-level helpers for locating and running probe-related binaries.
//!
//! Groups helper resolution, payload building, and contract enforcement so
//! binaries reuse a single source of truth.

pub mod payload;
pub mod run_dir_plan;
pub mod runner_root;
