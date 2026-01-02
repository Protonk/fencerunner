#![cfg(unix)]
#![allow(dead_code)]

// Shared fixtures and builders for integration tests. These helpers centralize
// temp dirs, probe fixtures, and sample boundary objects so tests stay focused
// on contract behavior instead of setup details.
use anyhow::{Context, Result, bail};
use fencerunner::boundary::{
    BoundaryObject, ContextInfo, OperationInfo, ProbeInfo, ResultDetails, ResultInfo, RunInfo,
    StackInfo,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::TempDir;

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
        let source = repo_root.join("probes/minimal_example.sh");
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

    /// Install the minimal probe fixture with a new name under the provided run dir.
    pub fn install_in_run_dir(repo_root: &Path, run_dir: &Path, name: &str) -> Result<Self> {
        let source = repo_root.join("probes/minimal_example.sh");
        let dest = run_dir.join(format!("{name}.sh"));
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

    /// Write a custom fixture script to a run dir with executable permissions.
    pub fn install_from_contents_in_run_dir(
        run_dir: &Path,
        name: &str,
        contents: &str,
    ) -> Result<Self> {
        let dest = run_dir.join(format!("{name}.sh"));
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

/// Ephemeral probe run directory rooted under the repo's `tmp/`.
///
/// The run dir is flat and includes the required `gates.json`,
/// `commitments.json`, and `boundaries.json` files so fixture probes can be
/// executed via `fencerunner`.
pub struct FixtureRunDir {
    temp: TempDir,
}

impl FixtureRunDir {
    pub fn new(repo_root: &Path) -> Result<Self> {
        let base = repo_root.join("tmp");
        fs::create_dir_all(&base)?;
        let temp = tempfile::Builder::new()
            .prefix("tests-run-dir")
            .tempdir_in(&base)
            .context("failed to create temp run dir")?;

        fs::copy(
            repo_root.join("probes/commitments.json"),
            temp.path().join("commitments.json"),
        )
        .with_context(|| "copy commitments.json".to_string())?;
        fs::copy(
            repo_root.join("probes/boundaries.json"),
            temp.path().join("boundaries.json"),
        )
        .with_context(|| "copy boundaries.json".to_string())?;
        fs::copy(
            repo_root.join("probes/gates.json"),
            temp.path().join("gates.json"),
        )
        .with_context(|| "copy gates.json".to_string())?;

        Ok(Self { temp })
    }

    pub fn path(&self) -> &Path {
        self.temp.path()
    }
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
        context: ContextInfo {
            commitments: Vec::new(),
            run: Some(RunInfo {
                workspace_root: Some("/tmp".to_string()),
                command: "echo test".to_string(),
            }),
            stack: Some(StackInfo {
                os: "Darwin".to_string(),
            }),
            extra: BTreeMap::new(),
        },
        payload: Some(json!({
            "stdout_snippet": null,
            "stderr_snippet": null,
            "raw": {}
        })),
        extensions: None,
    }
}
