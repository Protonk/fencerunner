//! Shared library for the codex-fence harness.
//!
//! The crate exposes common types (boundary objects, capability catalogs) and
//! utilities used by the Rust helper binaries. Public functions here form the
//! contract that the binaries depend on: repository discovery, helper binary
//! resolution, probe lookup, and JSON parsing helpers that mirror the harness
//! expectations documented in README.md and docs/boundary_object.md.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub mod boundary;
pub mod catalog;
pub mod coverage;
pub mod metadata_validation;
pub mod probe_metadata;
pub mod emit_support;
pub mod fence_run_support;

pub use boundary::{
    BoundaryObject, CapabilityContext, OperationInfo, Payload, ProbeInfo, ResultInfo, RunInfo,
    StackInfo,
};
pub use catalog::{
    Capability, CapabilityCatalog, CapabilityCategory, CapabilityId, CapabilityIndex,
    CapabilityLayer, CapabilitySnapshot, CatalogKey, CatalogRepository, load_catalog_from_path,
};
pub use coverage::{CoverageEntry, build_probe_coverage_map, filter_coverage_probes};
pub use metadata_validation::{validate_boundary_objects, validate_probe_capabilities};
pub use probe_metadata::{ProbeMetadata, collect_probe_scripts};

const ROOT_SENTINEL: &str = "bin/.gitkeep";
const SYNCED_BIN_DIR: &str = "bin";
const MAKEFILE: &str = "Makefile";

/// Returns true when `candidate` looks like the repository root.
///
/// The root detection is intentionally strict—helpers rely on the sentinel
/// files to avoid walking past the workspace boundary described in the
/// harness docs.
fn is_repo_root(candidate: &Path) -> bool {
    candidate.join(ROOT_SENTINEL).is_file() && candidate.join(MAKEFILE).is_file()
}

/// Verifies that an explicit `CODEX_FENCE_ROOT` hint points at a valid repo.
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
/// Search order matches README expectations: honor `CODEX_FENCE_ROOT` if it
/// points at a real repo, fall back to climbing up from the current executable,
/// then use the build-time hint. Callers can treat failure as fatal because the
/// binaries cannot run without the repo layout.
pub fn find_repo_root() -> Result<PathBuf> {
    if let Ok(env_root) = env::var("CODEX_FENCE_ROOT") {
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

    if let Some(hint) = option_env!("CODEX_FENCE_ROOT_HINT") {
        if let Some(root) = repo_root_from_hint(hint) {
            return Ok(root);
        }
    }

    bail!(
        "Unable to locate codex-fence repository root. Set CODEX_FENCE_ROOT to the cloned repository."
    );
}

/// Resolve another helper binary within the same repo.
///
/// Prefers the synced `bin/` artifacts (kept up to date by `make build-bin`),
/// then falls back to Cargo build outputs. This keeps shell entry points on the
/// compiled helpers rather than stale scripts.
pub fn resolve_helper_binary(repo_root: &Path, name: &str) -> Result<PathBuf> {
    let prefer_target = env::var("CODEX_FENCE_PREFER_TARGET")
        .ok()
        .map(|v| !v.trim().is_empty() && v != "0")
        .unwrap_or(false);

    let target_release = repo_root.join("target").join("release").join(name);
    let target_debug = repo_root.join("target").join("debug").join(name);
    let synced = repo_root.join(SYNCED_BIN_DIR).join(name);

    let mut candidates: Vec<PathBuf> = if prefer_target {
        vec![target_release.clone(), target_debug.clone(), synced.clone()]
    } else {
        vec![synced.clone(), target_release.clone(), target_debug.clone()]
    };

    // Always include the remaining fallbacks to avoid missing an executable when
    // env-based ordering changes.
    candidates.push(target_release);
    candidates.push(target_debug);
    candidates.push(synced);

    for candidate in candidates {
        if helper_is_executable(&candidate) {
            return Ok(candidate);
        }
    }

    bail!(
        "Unable to locate helper '{name}' under {}. Run 'make build-bin' to sync the Rust binaries.",
        repo_root.display()
    )
}

/// Returns true when an executable named `codex` exists somewhere on PATH.
pub fn codex_present() -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| helper_is_executable(&dir.join("codex"))))
        .unwrap_or(false)
}

/// Split comma- or whitespace-delimited configuration lists into tokens.
pub fn split_list(value: &str) -> Vec<String> {
    value
        .replace(',', " ")
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a cfbo stream from stdin, accepting either NDJSON or a JSON array.
///
/// The parser mirrors the listener contract: empty input is an error, single
/// boundary objects or arrays are accepted, and NDJSON streams are parsed
/// line-by-line so partial writes do not break the whole run.
pub fn parse_json_stream(input: &str) -> Result<Vec<BoundaryObject>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("No input provided on stdin");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return match value {
            Value::Array(items) => items
                .into_iter()
                .map(serde_json::from_value)
                .collect::<Result<Vec<_>, _>>()
                .context("Unable to parse JSON array of boundary objects"),
            Value::Object(_) => serde_json::from_value(value)
                .map(|obj| vec![obj])
                .context("Unable to parse boundary object"),
            _ => bail!("Unsupported JSON input; expected object or array"),
        };
    }

    let mut records = Vec::new();
    for (idx, line) in trimmed.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let obj: BoundaryObject = serde_json::from_str(line)
            .with_context(|| format!("Unable to parse boundary object from line {}", idx + 1))?;
        records.push(obj);
    }

    if records.is_empty() {
        bail!("No boundary objects found in input stream");
    }

    Ok(records)
}

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

fn helper_is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(path) {
            return meta.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}
