//! Shared library for the probe harness.
//!
//! This crate is intentionally small and repetitive to make visible
//! how the layers stack. Each public function exists because a binary
//! depends on it; treat them as contracts and
//! keep behavior aligned with the narrative in README.md plus the domain
//! guides under catalogs/, boundaries/, and probes/.

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
pub mod emit_support;
pub mod fence_run_support;
pub mod metadata_validation;
pub mod probe_metadata;
pub mod runtime;
pub(crate) mod schema_loader;

pub use boundary::{
    BoundaryObject, BoundaryReadError, BoundarySchema, CapabilityContext, ContextInfo,
    OperationInfo, ProbeContext, ProbeInfo, ResultDetails, ResultInfo, RunInfo, StackInfo,
    read_boundary_objects,
};
pub use catalog::{
    Capability, CapabilityCatalog, CapabilityCategory, CapabilityId, CapabilityIndex,
    CapabilityLayer, CapabilitySnapshot, CatalogKey, CatalogRepository, DEFAULT_CATALOG_PATH,
    load_catalog_from_path,
};
pub use coverage::{CoverageEntry, build_probe_coverage_map, filter_coverage_probes};
pub use metadata_validation::{validate_boundary_objects, validate_probe_capabilities};
pub use probe_metadata::{ProbeMetadata, collect_probe_scripts};

// === Repository discovery and helper resolution ===
const ROOT_SENTINEL: &str = "bin/.gitkeep";
const MAKEFILE: &str = "Makefile";
const ENV_CATALOG_PATH: &str = "CATALOG_PATH";
const ENV_BOUNDARY_SCHEMA_PATH: &str = "BOUNDARY_PATH";
pub const DEFAULT_BOUNDARY_SCHEMA_PATH: &str = "schema/boundary_object_schema.json";
pub const CANONICAL_BOUNDARY_SCHEMA_PATH: &str = "schema/boundary_object_schema.json";

/// Default paths for catalog and boundary schemas, resolved relative to a repo root.
#[derive(Debug, Clone)]
pub struct DefaultSchemaPaths {
    pub catalog: PathBuf,
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

    bail!("Unable to locate probe repository root. Set FENCE_ROOT to the cloned repository.");
}

/// Resolve the capability catalog path using CLI/env overrides or the default.
pub fn resolve_catalog_path(repo_root: &Path, cli_override: Option<&Path>) -> PathBuf {
    let default_catalog = default_catalog_path(repo_root);
    resolve_repo_data_path(repo_root, cli_override, ENV_CATALOG_PATH, &default_catalog)
}

/// Resolve the boundary schema path using CLI/env overrides or the default.
pub fn resolve_boundary_schema_path(
    repo_root: &Path,
    cli_override: Option<&Path>,
) -> Result<PathBuf> {
    let default_boundary = default_boundary_schema_path(repo_root);
    let resolved = if let Some(path) = cli_override {
        repo_relative(repo_root, path)
    } else if let Ok(env_path) = env::var(ENV_BOUNDARY_SCHEMA_PATH) {
        if env_path.is_empty() {
            default_boundary.clone()
        } else {
            repo_relative(repo_root, Path::new(&env_path))
        }
    } else {
        default_boundary.clone()
    };

    BoundarySchema::load(&resolved)
        .with_context(|| format!("loading boundary schema {}", resolved.display()))?;

    Ok(resolved)
}

fn resolve_repo_data_path(
    repo_root: &Path,
    cli_override: Option<&Path>,
    env_key: &str,
    default_path: &Path,
) -> PathBuf {
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
    let prefer_target = runtime::prefer_target_builds();
    if let Some(found) = runtime::resolve_repo_helper(repo_root, name, prefer_target) {
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
    value
        .replace(',', " ")
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a boundary-object stream from stdin, accepting either NDJSON or a JSON array.
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

// === Probe discovery (trusted tree only) ===
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_json_stream_accepts_ndjson_and_array() {
        let record_json = sample_record_json();
        let serialized = serde_json::to_string(&record_json).expect("serialize sample record");

        let ndjson = format!("{0}\n{0}\n", serialized);
        let nd_records = parse_json_stream(&ndjson).expect("ndjson parses");
        assert_eq!(nd_records.len(), 2);
        assert_eq!(nd_records[0].probe.id, "probe_id");

        let array_input = format!("[{0},{0}]", serialized);
        let array_records = parse_json_stream(&array_input).expect("array parses");
        assert_eq!(array_records.len(), 2);
        let command = array_records[1]
            .context
            .as_ref()
            .and_then(|ctx| ctx.run.as_ref())
            .map(|run| run.command.as_str());
        assert_eq!(command, Some("/bin/true"));
    }

    #[test]
    fn parse_json_stream_rejects_non_objects() {
        assert!(parse_json_stream("").is_err(), "empty input should fail");
        assert!(
            parse_json_stream("42").is_err(),
            "non-object json should fail"
        );
    }

    fn sample_record_json() -> serde_json::Value {
        json!({
            "probe": {
                "id": "probe_id"
            },
            "operation": {
                "kind": "fs.read",
                "target": "/tmp",
                "args": {}
            },
            "result": {
                "outcome": "success",
                "details": {
                    "exit_code": 0
                }
            },
            "context": {
                "capabilities_schema_version": "example_catalog_key",
                "stack": {
                    "os": "Darwin"
                },
                "probe": {
                    "primary_capability_id": "cap_fs_read_workspace_tree",
                    "secondary_capability_ids": []
                },
                "run": {
                    "workspace_root": "/tmp",
                    "command": "/bin/true"
                },
                "capability_context": {
                    "primary": {
                        "id": "cap_fs_read_workspace_tree",
                        "category": "filesystem",
                        "layer": "os_sandbox"
                    },
                    "secondary": []
                }
            },
            "payload": {
                "stdout_snippet": null,
                "stderr_snippet": null,
                "raw": {}
            }
        })
    }
}
