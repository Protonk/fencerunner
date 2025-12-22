//! Helper utilities shared by probe-exec/probe-matrix.
//!
//! This module centralizes workspace planning, preflight classification, and
//! probe metadata resolution so the CLI binaries do not drift. The goal is
//! defensive consistency: if the probe contract changes, the rule lives in one
//! place that all callers reuse.

use crate::catalog::CapabilityId;
use crate::probes::discovery::Probe;
use crate::probes::metadata::ProbeMetadata;
use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub enum WorkspaceOverride {
    /// Export the provided path (after canonicalization) as the workspace root.
    UsePath(OsString),
    /// Explicitly skip exporting a workspace root, forcing fallback logic.
    SkipExport,
}

pub struct WorkspacePlan {
    /// The value to export in FENCE_WORKSPACE_ROOT, if any.
    pub export_value: Option<OsString>,
}

/// Decide how the workspace root should be exported to probes.
pub fn workspace_plan_from_override(value: WorkspaceOverride) -> WorkspacePlan {
    match value {
        WorkspaceOverride::SkipExport => WorkspacePlan { export_value: None },
        WorkspaceOverride::UsePath(path) => WorkspacePlan {
            export_value: Some(canonicalize_os_string(&path)),
        },
    }
}

/// Canonicalize a path for logging/exports; fall back to the original path if
/// canonicalization fails (for example, missing directories).
pub fn canonicalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Canonicalize an OsString path without losing non-UTF8 bytes.
pub fn canonicalize_os_string(value: &OsString) -> OsString {
    let candidate = PathBuf::from(value);
    fs::canonicalize(&candidate)
        .unwrap_or(candidate)
        .into_os_string()
}

pub struct TmpdirPlan {
    /// The TMPDIR to export for the run, if we could create one.
    pub path: Option<PathBuf>,
    /// Last error we saw when creating TMPDIR candidates, for diagnostics.
    pub last_error: Option<(PathBuf, String)>,
}

/// Decide where TMPDIR should point for a run and capture the last failure so
/// the caller can emit a descriptive preflight record.
pub fn workspace_tmpdir_plan(workspace_plan: &WorkspacePlan, repo_root: &Path) -> TmpdirPlan {
    let mut candidates = Vec::new();
    if let Some(value) = workspace_plan.export_value.as_ref() {
        candidates.push(PathBuf::from(value).join("tmp"));
    }
    if workspace_plan.export_value.is_none() {
        candidates.push(repo_root.join("tmp"));
    }

    let mut last_error = None;
    for candidate in candidates {
        match fs::create_dir_all(&candidate) {
            Ok(()) => {
                return TmpdirPlan {
                    path: Some(canonicalize_path(&candidate)),
                    last_error: None,
                };
            }
            Err(err) => last_error = Some((candidate, err.to_string())),
        }
    }

    TmpdirPlan {
        path: None,
        last_error,
    }
}

pub struct ResolvedProbeMetadata {
    /// Probe id to report in emitted boundary objects.
    pub id: String,
    /// Capability id recorded as the primary capability for the probe.
    pub primary_capability: CapabilityId,
}

pub fn resolve_probe_metadata(
    probe: &Probe,
    parsed: ProbeMetadata,
) -> Result<ResolvedProbeMetadata> {
    // Keep resolution strict: probes must name a primary capability, and
    // defaulting to implicit ids is a last resort to preserve backward
    // compatibility.
    let primary_capability = parsed.primary_capability.ok_or_else(|| {
        anyhow!(
            "probe {} is missing primary_capability_id",
            probe.path.display()
        )
    })?;
    Ok(ResolvedProbeMetadata {
        id: parsed.probe_name.unwrap_or_else(|| probe.id.clone()),
        primary_capability,
    })
}
