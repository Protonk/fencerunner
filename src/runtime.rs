//! Runtime helpers shared across binaries.
//!
//! Centralizes executable detection, PATH resolution, and helper search order
//! so CLIs subscribe to the same behavior instead of re-implementing it.

use std::env;
use std::path::{Path, PathBuf};

/// Returns true when a file exists and has any execute bit set.
pub fn helper_is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return meta.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Whether callers requested preferring target/ builds over synced bin/.
pub fn prefer_target_builds() -> bool {
    env::var("CODEX_FENCE_PREFER_TARGET")
        .ok()
        .map(|v| !v.trim().is_empty() && v != "0")
        .unwrap_or(false)
}

/// Helper search order anchored at the repository root.
///
/// The candidates mirror how users build binaries: synced bin/ first when
/// present, then target/release and target/debug. Ordering is controlled by
/// `CODEX_FENCE_PREFER_TARGET` so tests can prefer local builds.
pub fn repo_helper_candidates(repo_root: &Path, name: &str, prefer_target: bool) -> Vec<PathBuf> {
    let target_release = repo_root.join("target").join("release").join(name);
    let target_debug = repo_root.join("target").join("debug").join(name);
    let synced = repo_root.join("bin").join(name);

    let mut candidates: Vec<PathBuf> = if prefer_target {
        vec![target_release.clone(), target_debug.clone(), synced.clone()]
    } else {
        vec![synced.clone(), target_release.clone(), target_debug.clone()]
    };

    // Always include fallbacks so callers find any existing build regardless of
    // the initial ordering.
    candidates.push(target_release);
    candidates.push(target_debug);
    candidates.push(synced);
    candidates
}

/// Resolve the first executable helper in the repo search order.
pub fn resolve_repo_helper(repo_root: &Path, name: &str, prefer_target: bool) -> Option<PathBuf> {
    for candidate in repo_helper_candidates(repo_root, name, prefer_target) {
        if helper_is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Find an executable by name somewhere on PATH.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        let candidate = dir.join(name);
        if helper_is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}
