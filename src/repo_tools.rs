//! Small path helpers and parsing utilities.
//!
//! The installed runner and script-facing shims embed the assets they need and
//! do not require a checked out repository on disk.

use std::path::{Path, PathBuf};

pub const COMMITMENTS_REGISTRY_FILENAME: &str = "commitments.json";
pub const BOUNDARIES_CONTRACT_FILENAME: &str = "boundaries.json";
pub const GATES_CONTRACT_FILENAME: &str = "gates.json";

/// Return the commitments registry path for a run directory.
pub fn commitments_registry_path(run_dir: &Path) -> PathBuf {
    run_dir.join(COMMITMENTS_REGISTRY_FILENAME)
}

/// Return the boundaries contract path for a run directory.
pub fn boundaries_contract_path(run_dir: &Path) -> PathBuf {
    run_dir.join(BOUNDARIES_CONTRACT_FILENAME)
}

/// Return the gates contract path for a run directory.
pub fn gates_contract_path(run_dir: &Path) -> PathBuf {
    run_dir.join(GATES_CONTRACT_FILENAME)
}

// === Small parsing helpers ===
/// Split comma- or whitespace-delimited configuration lists into tokens.
pub fn split_list(value: &str) -> Vec<String> {
    // Simple normalization helper for env vars like "a,b c".
    value
        .replace(',', " ")
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
