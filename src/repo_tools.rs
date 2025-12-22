//! Repository discovery and repo-relative path resolution.
//!
//! Centralizes FENCE_ROOT detection, default catalog/schema paths, helper
//! binary resolution, and small parsing helpers used by CLIs.

use anyhow::{Context, Result, bail};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::boundary::BoundarySchema;
use crate::catalog::DEFAULT_CATALOG_PATH;
use crate::harness::binaries;

// A tiny file under bin/ that ships with the repo. We use it as a cheap "yes,
// this is the root" marker instead of trusting cwd alone.
const ROOT_SENTINEL: &str = "bin/.gitkeep";
const MAKEFILE: &str = "Makefile";
const ENV_CATALOG_PATH: &str = "CATALOG_PATH";

pub const DEFAULT_BOUNDARY_SCHEMA_PATH: &str = "boundary/boundary_object_schema.json";
pub const CANONICAL_BOUNDARY_SCHEMA_PATH: &str = "boundary/boundary_object_schema.json";

/// Default paths for catalog and boundary schemas, resolved relative to a repo root.
#[derive(Debug, Clone)]
pub struct DefaultSchemaPaths {
    /// Absolute path to the capability catalog JSON file.
    pub catalog: PathBuf,
    /// Absolute path to the boundary-object schema JSON file.
    pub boundary: PathBuf,
}

/// Returns true when `candidate` looks like the repository root.
///
/// The root detection is intentionally strict—helpers rely on the sentinel
/// files to avoid walking past the workspace boundary described in the
/// harness docs.
fn is_repo_root(candidate: &Path) -> bool {
    candidate.join(ROOT_SENTINEL).is_file() && candidate.join(MAKEFILE).is_file()
}

/// Verifies that an explicit `FENCE_ROOT` hint points at a valid repo.
fn repo_root_from_hint(hint: &str) -> Option<PathBuf> {
    if hint.is_empty() {
        return None;
    }
    let hint_path = PathBuf::from(hint);
    if !hint_path.exists() || !is_repo_root(&hint_path) {
        return None;
    }
    fs::canonicalize(hint_path).ok()
}

/// Walk upward from `start` to find the first directory that looks like a repo.
fn search_upwards(start: &Path) -> Option<PathBuf> {
    let mut dir = fs::canonicalize(start).ok()?;
    loop {
        if is_repo_root(&dir) {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

/// Locate the repository root using the harness contract.
///
/// Search order matches README expectations: explicit env hint, the current
/// executable location, then the build-time hint. Callers can treat failure as
/// fatal because binaries cannot run without the repo layout.
pub fn find_repo_root() -> Result<PathBuf> {
    if let Ok(env_root) = env::var("FENCE_ROOT") {
        if let Some(root) = repo_root_from_hint(&env_root) {
            return Ok(root);
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            if let Some(root) = search_upwards(exe_dir) {
                return Ok(root);
            }
        }
    }

    bail!("Unable to locate probe repository root. Set FENCE_ROOT to the cloned repository.")
}

/// Resolve the capability catalog path using CLI/env overrides or the default.
pub fn resolve_catalog_path(repo_root: &Path, cli_override: Option<&Path>) -> PathBuf {
    let default_catalog = default_catalog_path(repo_root);
    resolve_repo_data_path(repo_root, cli_override, ENV_CATALOG_PATH, &default_catalog)
}

/// Resolve the boundary schema path using the repo default.
pub fn resolve_boundary_schema_path(repo_root: &Path) -> Result<PathBuf> {
    let default_boundary = default_boundary_schema_path(repo_root);
    BoundarySchema::load(&default_boundary)
        .with_context(|| format!("loading boundary schema {}", default_boundary.display()))?;
    Ok(default_boundary)
}

fn resolve_repo_data_path(
    repo_root: &Path,
    cli_override: Option<&Path>,
    env_key: &str,
    default_path: &Path,
) -> PathBuf {
    // Order matters: explicit CLI path wins, then env var, then repo default.
    if let Some(path) = cli_override {
        return repo_relative(repo_root, path);
    }
    if let Ok(env_path) = env::var(env_key) {
        if !env_path.is_empty() {
            return repo_relative(repo_root, Path::new(&env_path));
        }
    }
    repo_relative(repo_root, default_path)
}

fn repo_relative(repo_root: &Path, candidate: &Path) -> PathBuf {
    // Absolute paths pass through unchanged; relative paths are anchored to the repo.
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        repo_root.join(candidate)
    }
}

/// Return the default capability catalog path.
pub fn default_catalog_path(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_CATALOG_PATH)
}

/// Return the default boundary schema.
pub fn default_boundary_schema_path(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_BOUNDARY_SCHEMA_PATH)
}

/// Resolve default schema paths from baked-in locations.
pub fn default_schema_paths(repo_root: &Path) -> DefaultSchemaPaths {
    DefaultSchemaPaths {
        catalog: repo_root.join(DEFAULT_CATALOG_PATH),
        boundary: repo_root.join(DEFAULT_BOUNDARY_SCHEMA_PATH),
    }
}

/// Resolve another helper binary within the same repo.
///
/// Prefers the synced `bin/` artifacts (kept up to date by `make build`),
/// then falls back to Cargo build outputs. Every binary should go through this
/// helper so the search order stays consistent.
pub fn resolve_helper_binary(repo_root: &Path, name: &str) -> Result<PathBuf> {
    let prefer_target = binaries::prefer_target_builds();
    if let Some(found) = binaries::resolve_repo_helper(repo_root, name, prefer_target) {
        return Ok(found);
    }

    bail!(
        "Unable to locate helper '{name}' under {}. Run 'make build' to sync the Rust binaries.",
        repo_root.display()
    );
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
