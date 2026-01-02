//! Resolve and enumerate probe scripts under a run directory.
//!
//! Probe resolution is intentionally strict: anything outside the run dir is
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

/// Returns the canonical run directory.
pub fn canonical_run_dir(run_dir: &Path) -> Result<PathBuf> {
    let canonical = fs::canonicalize(run_dir)
        .with_context(|| format!("Unable to canonicalize run dir at {}", run_dir.display()))?;
    if !canonical.is_dir() {
        bail!("Run dir is not a directory: {}", canonical.display());
    }
    Ok(canonical)
}

/// Resolve a probe identifier to a script under `run_dir`.
///
/// The resolver enforces the workspace boundary by canonicalizing each
/// candidate and rejecting anything outside `run_dir`, guarding against
/// symlinks or relative paths that would escape the contract in
/// `docs/gates.md`.
pub fn resolve_probe(run_dir: &Path, identifier: &str) -> Result<Probe> {
    let run_root = canonical_run_dir(run_dir)?;
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        bail!("Empty probe identifier requested");
    }
    let trimmed = trimmed.strip_prefix("./").unwrap_or(trimmed);

    // Build a small set of path candidates so users can pass "id", "id.sh", or
    // an explicit path under the run dir.
    let mut attempts = Vec::new();
    let input_path = PathBuf::from(trimmed);
    if input_path.is_absolute() {
        attempts.push(input_path.clone());
    } else {
        attempts.push(run_root.join(&input_path));
        if input_path.extension().is_none() {
            attempts.push(run_root.join(format!("{trimmed}.sh")));
        }
    }

    for candidate in attempts {
        if candidate.is_file() {
            if let Ok(canonical) = fs::canonicalize(&candidate) {
                if canonical.starts_with(&run_root) {
                    ensure_probe_executable(&canonical)?;
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

/// List all probe scripts under `run_dir`.
///
/// Only `.sh` files are considered, and the file stem becomes the probe id.
/// Missing probes are treated as an error because downstream tooling expects at
/// least the fixtures to exist.
pub fn list_probes(run_dir: &Path) -> Result<Vec<Probe>> {
    let run_root = canonical_run_dir(run_dir)?;
    // BTreeMap keeps ordering stable across platforms and filesystems.
    let mut results: BTreeMap<String, Probe> = BTreeMap::new();
    for entry in fs::read_dir(&run_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("sh") {
            continue;
        }
        let canonical = fs::canonicalize(&path)?;
        if !canonical.starts_with(&run_root) {
            bail!(
                "Probe path escapes run dir: {} -> {}",
                path.display(),
                canonical.display()
            );
        }
        ensure_probe_executable(&canonical)?;
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
        bail!("No probes found under {}", run_root.to_string_lossy());
    }

    Ok(results.into_values().collect())
}

fn ensure_probe_executable(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("probe not found or not executable: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("probe not found or not executable: {}", path.display());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("probe is not executable: {}", path.display());
        }
    }

    Ok(())
}
