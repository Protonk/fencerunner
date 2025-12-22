//! Resolve and enumerate probe scripts under the trusted probes/ tree.
//!
//! Probe resolution is intentionally strict: anything outside probes/ is
//! rejected to keep the harness contract enforceable and to avoid symlink
//! escapes.

use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Probe {
    pub id: String,
    pub path: PathBuf,
}

/// Returns the canonical `probes/` root for the current repository.
pub fn canonical_probes_root(repo_root: &Path) -> Result<PathBuf> {
    let probes_root = repo_root.join("probes");
    fs::canonicalize(&probes_root).with_context(|| {
        format!(
            "Unable to canonicalize probes dir at {}",
            probes_root.display()
        )
    })
}

/// Resolve a probe identifier to a script under `probes/`.
///
/// The resolver enforces the workspace boundary by canonicalizing each
/// candidate and rejecting anything outside `probes/`, guarding against
/// symlinks or relative paths that would escape the contract in
/// `probes/AGENTS.md`.
pub fn resolve_probe(repo_root: &Path, identifier: &str) -> Result<Probe> {
    let probes_root = canonical_probes_root(repo_root)?;
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        bail!("Empty probe identifier requested");
    }
    let trimmed = trimmed.strip_prefix("./").unwrap_or(trimmed);

    // Build a small set of path candidates so users can pass "id", "id.sh",
    // "probes/id.sh", or a repo-relative path.
    let mut attempts = Vec::new();
    let input_path = PathBuf::from(trimmed);
    if input_path.is_absolute() {
        attempts.push(input_path.clone());
    } else {
        attempts.push(repo_root.join(&input_path));
        if input_path.extension().is_none() {
            attempts.push(repo_root.join(format!("{trimmed}.sh")));
        }
        attempts.push(repo_root.join("probes").join(&input_path));
        if input_path.extension().is_none() {
            attempts.push(repo_root.join("probes").join(format!("{trimmed}.sh")));
        }
    }

    for candidate in attempts {
        if candidate.is_file() {
            if let Ok(canonical) = fs::canonicalize(&candidate) {
                if canonical.starts_with(&probes_root) {
                    if let Some(stem) = canonical.file_stem().and_then(|s| s.to_str()) {
                        return Ok(Probe {
                            id: stem.to_string(),
                            path: canonical,
                        });
                    }
                }
            }
        }
    }

    bail!("Probe not found: {identifier}")
}

/// List all probe scripts under `probes/`.
///
/// Only `.sh` files are considered, and the file stem becomes the probe id.
/// Missing probes are treated as an error because downstream tooling expects at
/// least the fixtures to exist.
pub fn list_probes(repo_root: &Path) -> Result<Vec<Probe>> {
    let probes_root = canonical_probes_root(repo_root)?;
    // BTreeMap keeps ordering stable across platforms and filesystems.
    let mut results: BTreeMap<String, Probe> = BTreeMap::new();
    for entry in fs::read_dir(&probes_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("sh") {
            continue;
        }
        let canonical = fs::canonicalize(&path)?;
        if let Some(stem) = canonical.file_stem().and_then(|s| s.to_str()) {
            results.insert(
                stem.to_string(),
                Probe {
                    id: stem.to_string(),
                    path: canonical,
                },
            );
        }
    }

    if results.is_empty() {
        bail!("No probes found under {}", probes_root.to_string_lossy());
    }

    Ok(results.into_values().collect())
}
