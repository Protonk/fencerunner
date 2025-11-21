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

fn is_repo_root(candidate: &Path) -> bool {
    candidate.join(ROOT_SENTINEL).is_file() && candidate.join(MAKEFILE).is_file()
}

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

pub fn resolve_helper_binary(repo_root: &Path, name: &str) -> Result<PathBuf> {
    let synced = repo_root.join(SYNCED_BIN_DIR).join(name);
    if helper_is_executable(&synced) {
        return Ok(synced);
    }

    let target_release = repo_root.join("target").join("release").join(name);
    if helper_is_executable(&target_release) {
        return Ok(target_release);
    }

    let target_debug = repo_root.join("target").join("debug").join(name);
    if helper_is_executable(&target_debug) {
        return Ok(target_debug);
    }

    bail!(
        "Unable to locate helper '{name}' under {}. Run 'make build-bin' to sync the Rust binaries.",
        repo_root.display()
    )
}

pub fn codex_present() -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| helper_is_executable(&dir.join("codex"))))
        .unwrap_or(false)
}

pub fn split_list(value: &str) -> Vec<String> {
    value
        .replace(',', " ")
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

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

pub fn canonical_probes_root(repo_root: &Path) -> Result<PathBuf> {
    let probes_root = repo_root.join("probes");
    fs::canonicalize(&probes_root).with_context(|| {
        format!(
            "Unable to canonicalize probes dir at {}",
            probes_root.display()
        )
    })
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn resolve_helper_prefers_release() {
        let temp = TempRepo::new();
        let release_dir = temp.root.join("target/release");
        fs::create_dir_all(&release_dir).unwrap();
        let helper = release_dir.join("fence-run");
        fs::write(&helper, "#!/bin/sh\n").unwrap();
        make_executable(&helper);
        let resolved = resolve_helper_binary(&temp.root, "fence-run").unwrap();
        assert_eq!(resolved, helper);
    }

    #[test]
    fn resolve_helper_falls_back_to_bin() {
        let temp = TempRepo::new();
        let bin_dir = temp.root.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let helper = bin_dir.join("emit-record");
        fs::write(&helper, "#!/bin/sh\n").unwrap();
        make_executable(&helper);
        let resolved = resolve_helper_binary(&temp.root, "emit-record").unwrap();
        assert_eq!(resolved, helper);
    }

    #[test]
    #[cfg(unix)]
    fn codex_present_requires_executable() {
        let temp = TempRepo::new();
        let codex = temp.root.join("codex");
        fs::write(&codex, "#!/bin/sh\nexit 0\n").unwrap();

        let _guard = PathGuard::set(&temp.root);

        // No execute bit → should not be considered present.
        assert!(!codex_present());

        make_executable(&codex);
        assert!(codex_present());
    }

    #[test]
    fn list_and_resolve_probes_share_semantics() {
        let temp = TempRepo::new();
        let probes_dir = temp.root.join("probes");
        fs::create_dir_all(&probes_dir).unwrap();
        let script = probes_dir.join("example.sh");
        fs::write(&script, "#!/usr/bin/env bash\nexit 0\n").unwrap();
        make_executable(&script);

        let probes = list_probes(&temp.root).unwrap();
        assert_eq!(probes.len(), 1);
        assert_eq!(probes[0].id, "example");

        let resolved = resolve_probe(&temp.root, "example").unwrap();
        assert_eq!(resolved.path, fs::canonicalize(&script).unwrap());

        let resolved_with_ext = resolve_probe(&temp.root, "example.sh").unwrap();
        assert_eq!(resolved_with_ext.path, resolved.path);
    }

    struct TempRepo {
        root: PathBuf,
    }

    impl TempRepo {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let mut dir = env::temp_dir();
            dir.push(format!(
                "codex-fence-helper-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir_all(&dir).unwrap();
            Self { root: dir }
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    struct PathGuard {
        original: Option<OsString>,
    }

    impl PathGuard {
        fn set(value: &Path) -> Self {
            let original = env::var_os("PATH");
            // std::env mutation is marked unsafe on this toolchain.
            unsafe {
                env::set_var("PATH", value);
            }
            Self { original }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            unsafe {
                match self.original.take() {
                    Some(val) => env::set_var("PATH", val),
                    None => env::remove_var("PATH"),
                }
            }
        }
    }
}
