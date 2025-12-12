use crate::{CapabilityId, Probe, ProbeMetadata};
use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

// Helper utilities shared by probe-exec/probe-matrix: workspace planning,
// preflight classification, and probe metadata resolution. Keeping these in one
// place makes the defense-in-depth rules visible instead of buried in each CLI
// and keeps behavior aligned with the probe/docs contracts.

#[derive(Clone)]
pub enum WorkspaceOverride {
    UsePath(OsString),
    SkipExport,
}

pub struct WorkspacePlan {
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

pub fn canonicalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn canonicalize_os_string(value: &OsString) -> OsString {
    let candidate = PathBuf::from(value);
    fs::canonicalize(&candidate)
        .unwrap_or(candidate)
        .into_os_string()
}

pub struct TmpdirPlan {
    pub path: Option<PathBuf>,
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
    pub id: String,
    pub version: String,
    pub primary_capability: CapabilityId,
}

pub fn resolve_probe_metadata(
    probe: &Probe,
    parsed: ProbeMetadata,
) -> Result<ResolvedProbeMetadata> {
    // Keep resolution strict: probes must name a primary capability, and
    // defaulting to implicit ids/versions is a last resort to preserve
    // backward compatibility.
    let primary_capability = parsed.primary_capability.ok_or_else(|| {
        anyhow!(
            "probe {} is missing primary_capability_id",
            probe.path.display()
        )
    })?;
    Ok(ResolvedProbeMetadata {
        id: parsed.probe_name.unwrap_or_else(|| probe.id.clone()),
        version: parsed.probe_version.unwrap_or_else(|| "1".to_string()),
        primary_capability,
    })
}
