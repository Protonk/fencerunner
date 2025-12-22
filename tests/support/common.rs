#![cfg(unix)]
#![allow(dead_code)]

// Shared fixtures and builders for integration tests. These helpers centralize
// temp dirs, probe fixtures, and sample boundary objects so tests stay focused
// on contract behavior instead of setup details.
use anyhow::{Context, Result, bail};
use fencerunner::boundary::{
    BoundaryObject, CapabilityContext, ContextInfo, OperationInfo, ProbeContext, ProbeInfo,
    ResultDetails, ResultInfo, RunInfo, StackInfo,
};
use fencerunner::catalog::{
    CapabilityCategory, CapabilityId, CapabilityIndex, CapabilityLayer, CapabilitySnapshot,
    CatalogKey, load_catalog_from_path,
};
use fencerunner::repo_tools::default_catalog_path;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::NamedTempFile;

use crate::support::repo_root;

// Fixture probes are installed into probes/ so we can exercise real probe
// discovery paths. Drop removes them to keep the repo clean between tests.
// Helper for installing temporary probe mocks under probes/ and cleaning them
// up after each test.
pub struct FixtureProbe {
    path: PathBuf,
    name: String,
}

impl FixtureProbe {
    /// Install the minimal probe fixture with a new name under probes/.
    pub fn install(repo_root: &Path, name: &str) -> Result<Self> {
        let source = repo_root.join("tests/mocks/minimal_probe.sh");
        let dest = repo_root.join("probes").join(format!("{name}.sh"));
        if dest.exists() {
            bail!("fixture already exists at {}", dest.display());
        }
        fs::copy(&source, &dest)
            .with_context(|| format!("failed to copy fixture to {}", dest.display()))?;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
        Ok(Self {
            path: dest,
            name: name.to_string(),
        })
    }

    /// Install a named fixture script from tests/mocks.
    pub fn install_from_fixture(repo_root: &Path, fixture: &str, name: &str) -> Result<Self> {
        let source = repo_root.join("tests/mocks").join(fixture);
        let dest = repo_root.join("probes").join(format!("{name}.sh"));
        if dest.exists() {
            bail!("fixture already exists at {}", dest.display());
        }
        fs::copy(&source, &dest).with_context(|| {
            format!(
                "failed to copy fixture {} to {}",
                source.display(),
                dest.display()
            )
        })?;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
        Ok(Self {
            path: dest,
            name: name.to_string(),
        })
    }

    /// Write a custom fixture script to probes/ with executable permissions.
    pub fn install_from_contents(repo_root: &Path, name: &str, contents: &str) -> Result<Self> {
        let dest = repo_root.join("probes").join(format!("{name}.sh"));
        if dest.exists() {
            bail!("fixture already exists at {}", dest.display());
        }
        fs::write(&dest, contents)
            .with_context(|| format!("failed to write fixture at {}", dest.display()))?;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
        Ok(Self {
            path: dest,
            name: name.to_string(),
        })
    }

    /// Return the probe id (derived from the filename stem).
    pub fn probe_id(&self) -> &str {
        &self.name
    }
}

impl Drop for FixtureProbe {
    // RAII cleanup makes probe fixtures safe in parallel test runs.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// A lightweight guard for temporary files/symlinks created in repo paths.
// Removes the referenced file on drop so tests can create temporary symlinks.
pub struct FileGuard {
    pub path: PathBuf,
}

impl Drop for FileGuard {
    // Drop is infallible by design; ignore removal errors to avoid masking test results.
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// RepoGuard serializes tests that mutate probes/ or other shared repo paths.
// Using a Mutex ensures concurrent tests don't stomp on each other's fixtures.
// Serializes repository-mutating tests so fixture installs do not conflict.
pub struct RepoGuard {
    _guard: MutexGuard<'static, ()>,
}

pub fn repo_guard() -> RepoGuard {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let mutex = LOCK.get_or_init(|| Mutex::new(()));
    let guard = mutex.lock().unwrap_or_else(|err| err.into_inner());
    RepoGuard { _guard: guard }
}

/// Parse a boundary object from raw bytes and keep both typed and raw views.
/// This is handy when we want schema shape assertions on the raw JSON.
pub fn parse_boundary_object(bytes: &[u8]) -> Result<(BoundaryObject, Value)> {
    let value: Value = serde_json::from_slice(bytes)?;
    let record: BoundaryObject = serde_json::from_value(value.clone())?;
    Ok((record, value))
}

/// Make a path printable relative to the repo root for diagnostics.
#[allow(dead_code)]
pub fn relative_to_repo(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub struct TempRepo {
    pub root: PathBuf,
}

impl TempRepo {
    /// Create a unique temp directory to stand in for a repo root.
    pub fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let mut dir = env::temp_dir();
        dir.push(format!(
            "probe-helper-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&dir).expect("failed to create temp repo");
        Self { root: dir }
    }
}

impl Drop for TempRepo {
    // Best-effort cleanup keeps temp dirs from piling up if tests abort.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub struct TempWorkspace {
    pub root: PathBuf,
}

impl TempWorkspace {
    /// Create a unique temp workspace root for workspace planning tests.
    pub fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let mut base = env::temp_dir();
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        base.push(format!(
            "probe-workspace-test-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&base).expect("failed to create temp workspace");
        Self { root: base }
    }
}

impl Drop for TempWorkspace {
    // Temp workspaces are disposable; ignore errors on cleanup.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Build a minimal catalog file and load it as a CapabilityIndex.
/// This keeps tests independent from the bundled catalog content.
pub fn sample_capability_index(entries: &[(&str, &str, &str)]) -> Result<CapabilityIndex> {
    let mut file = NamedTempFile::new()?;
    let capabilities: Vec<Value> = entries
        .iter()
        .map(|(id, category, layer)| {
            json!({
                "id": id,
                "category": category,
                "layer": layer,
                "description": format!("cap {id}"),
                "operations": {"allow": [], "deny": []}
            })
        })
        .collect();

    let mut categories = BTreeMap::new();
    let mut layers = BTreeSet::new();
    for (_, category, layer) in entries {
        categories
            .entry(category.to_string())
            .or_insert_with(|| "fixture".to_string());
        layers.insert(layer.to_string());
    }
    let policy_layers: BTreeMap<String, String> = layers
        .into_iter()
        .map(|layer| (layer, "fixture layer".to_string()))
        .collect();

    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "sandbox_catalog_v1",
            "catalog": {"key": "sample_catalog_v1", "title": "sample catalog"},
            "scope": {"description": "test", "policy_layers": policy_layers, "categories": categories},
            "sources": {},
            "capabilities": capabilities
        }),
    )?;
    CapabilityIndex::load(file.path())
        .with_context(|| "failed to load sample capability index".to_string())
}

/// Resolve the default catalog path using repo tooling so tests match production.
pub fn catalog_path() -> PathBuf {
    default_catalog_path(&repo_root())
}

/// Cache the default catalog key to avoid re-reading the catalog in each test.
pub fn default_catalog_key() -> CatalogKey {
    static KEY: OnceLock<CatalogKey> = OnceLock::new();
    KEY.get_or_init(|| {
        load_catalog_from_path(&catalog_path())
            .expect("load catalog")
            .catalog
            .key
            .clone()
    })
    .clone()
}

/// Convenience for building empty JSON object payloads.
pub fn empty_json_object() -> Value {
    Value::Object(Default::default())
}

/// A minimal but valid boundary object for serde round-trip tests.
/// This is not meant to represent a real probe; it only exercises struct wiring.
pub fn sample_boundary_object() -> BoundaryObject {
    BoundaryObject {
        probe: ProbeInfo {
            id: "probe".to_string(),
        },
        operation: OperationInfo {
            kind: "fs.read".to_string(),
            target: "/dev/null".to_string(),
            args: Some(empty_json_object()),
        },
        result: ResultInfo {
            outcome: "success".to_string(),
            details: Some(ResultDetails {
                exit_code: Some(0),
                ..ResultDetails::default()
            }),
        },
        context: Some(ContextInfo {
            run: Some(RunInfo {
                workspace_root: Some("/tmp".to_string()),
                command: "echo test".to_string(),
            }),
            stack: Some(StackInfo {
                os: "Darwin".to_string(),
            }),
            capabilities_schema_version: Some(default_catalog_key()),
            capability_context: Some(CapabilityContext {
                primary: CapabilitySnapshot {
                    id: CapabilityId("cap_id".to_string()),
                    category: CapabilityCategory::Other("cat".to_string()),
                    layer: CapabilityLayer::Other("layer".to_string()),
                },
                secondary: Vec::new(),
            }),
            probe: Some(ProbeContext {
                primary_capability_id: CapabilityId("cap_id".to_string()),
                secondary_capability_ids: Vec::new(),
            }),
            extra: BTreeMap::new(),
        }),
        payload: Some(json!({
            "stdout_snippet": null,
            "stderr_snippet": null,
            "raw": {}
        })),
        extensions: None,
    }
}
