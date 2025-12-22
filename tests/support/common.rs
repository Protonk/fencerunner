#![cfg(unix)]
#![allow(dead_code)]

use anyhow::{Context, Result, bail};
use fencerunner::{
    BoundaryObject, BoundarySchema, CapabilityCategory, CapabilityContext, CapabilityId,
    CapabilityIndex, CapabilityLayer, CapabilitySnapshot, CatalogKey, OperationInfo, Payload,
    ProbeInfo, ResultInfo, RunInfo, StackInfo, default_catalog_path, load_catalog_from_path,
    resolve_boundary_schema_path,
};
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

// Helper for installing temporary probe mocks under probes/ and cleaning them
// up after each test.
pub struct FixtureProbe {
    path: PathBuf,
    name: String,
}

impl FixtureProbe {
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

    pub fn probe_id(&self) -> &str {
        &self.name
    }
}

impl Drop for FixtureProbe {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// Removes the referenced file on drop so tests can create temporary symlinks.
pub struct FileGuard {
    pub path: PathBuf,
}

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

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

pub fn parse_boundary_object(bytes: &[u8]) -> Result<(BoundaryObject, Value)> {
    let value: Value = serde_json::from_slice(bytes)?;
    let record: BoundaryObject = serde_json::from_value(value.clone())?;
    Ok((record, value))
}

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
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub struct TempWorkspace {
    pub root: PathBuf,
}

impl TempWorkspace {
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
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

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
    let policy_layers: Vec<Value> = layers
        .into_iter()
        .map(|layer| json!({"id": layer, "description": "fixture layer"}))
        .collect();

    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "sandbox_catalog_v1",
            "catalog": {"key": "sample_catalog_v1", "title": "sample catalog"},
            "scope": {"description": "test", "policy_layers": policy_layers, "categories": categories},
            "docs": {},
            "capabilities": capabilities
        }),
    )?;
    CapabilityIndex::load(file.path())
        .with_context(|| "failed to load sample capability index".to_string())
}

pub fn catalog_path() -> PathBuf {
    default_catalog_path(&repo_root())
}

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

pub fn boundary_schema_version() -> String {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let path = resolve_boundary_schema_path(&repo_root(), None)
                .expect("resolve boundary schema path");
            BoundarySchema::load(&path)
                .expect("load boundary schema")
                .schema_version()
                .to_string()
        })
        .clone()
}

pub fn boundary_schema_key() -> Option<String> {
    static KEY: OnceLock<Option<String>> = OnceLock::new();
    KEY.get_or_init(|| {
        let path =
            resolve_boundary_schema_path(&repo_root(), None).expect("resolve boundary schema path");
        BoundarySchema::load(&path)
            .expect("load boundary schema")
            .schema_key()
            .map(str::to_string)
    })
    .clone()
}

pub fn empty_json_object() -> Value {
    Value::Object(Default::default())
}

pub fn sample_boundary_object() -> BoundaryObject {
    BoundaryObject {
        schema_version: boundary_schema_version(),
        schema_key: boundary_schema_key(),
        capabilities_schema_version: Some(default_catalog_key()),
        stack: StackInfo {
            sandbox_mode: Some("workspace-write".to_string()),
            os: "Darwin".to_string(),
        },
        probe: ProbeInfo {
            id: "probe".to_string(),
            version: "1".to_string(),
            primary_capability_id: CapabilityId("cap_id".to_string()),
            secondary_capability_ids: vec![],
        },
        run: RunInfo {
            workspace_root: Some("/tmp".to_string()),
            command: "echo test".to_string(),
        },
        operation: OperationInfo {
            category: "fs".to_string(),
            verb: "read".to_string(),
            target: "/dev/null".to_string(),
            args: empty_json_object(),
        },
        result: ResultInfo {
            observed_result: "success".to_string(),
            raw_exit_code: Some(0),
            errno: None,
            message: None,
            error_detail: None,
        },
        payload: Payload {
            stdout_snippet: None,
            stderr_snippet: None,
            raw: empty_json_object(),
        },
        capability_context: CapabilityContext {
            primary: CapabilitySnapshot {
                id: CapabilityId("cap_id".to_string()),
                category: CapabilityCategory::Other("cat".to_string()),
                layer: CapabilityLayer::Other("layer".to_string()),
            },
            secondary: Vec::new(),
        },
    }
}
