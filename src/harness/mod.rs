//! Harness-level helpers for locating and running probe-related binaries.
//!
//! Groups helper resolution, workspace planning, payload building, and contract
//! validation so binaries reuse a single source of truth.

pub mod binaries;
pub mod contract;
pub mod payload;
pub mod workspace;
