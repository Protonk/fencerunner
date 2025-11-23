//! Validation helpers for cross-checking probes and emitted records.
//!
//! Used by guard-rail tests to ensure probe metadata only references known
//! capability IDs and that stored boundary objects remain in sync with the
//! current catalog snapshot.

use crate::catalog::{CapabilityId, CapabilityIndex};
use crate::probe_metadata::ProbeMetadata;
use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn validate_probe_capabilities(
    capabilities: &CapabilityIndex,
    probes: &[ProbeMetadata],
) -> Vec<String> {
    // Return a list of errors rather than short-circuiting so callers can
    // surface multiple probe issues at once.
    let mut errors = Vec::new();
    for probe in probes {
        let display = probe.script.display();
        let Some(primary) = &probe.primary_capability else {
            errors.push(format!("{display} is missing primary_capability_id"));
            continue;
        };
        if capabilities.capability(primary).is_none() {
            errors.push(format!(
                "{display} references unknown capability '{}'",
                primary.0
            ));
        }
        for secondary in &probe.secondary_capabilities {
            if capabilities.capability(secondary).is_none() {
                errors.push(format!(
                    "{display} references unknown secondary capability '{}'",
                    secondary.0
                ));
            }
        }
    }
    errors
}

pub fn validate_boundary_objects(
    capabilities: &CapabilityIndex,
    dirs: &[PathBuf],
) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    let json_files = find_json_files(dirs)?;
    for json_file in json_files {
        let data = match fs::read_to_string(&json_file) {
            Ok(data) => data,
            Err(err) => {
                errors.push(format!("{}: unable to read: {err}", json_file.display()));
                continue;
            }
        };

        let value: Value = match serde_json::from_str(&data) {
            Ok(val) => val,
            Err(err) => {
                errors.push(format!("{}: invalid JSON: {err}", json_file.display()));
                continue;
            }
        };

        let mut seen = BTreeSet::new();
        for cap_id in extract_capability_ids(&value) {
            // Avoid spamming the same missing capability multiple times when it
            // appears in both probe and context sections.
            if !seen.insert(cap_id.clone()) {
                continue;
            }
            if capabilities.capability(&cap_id).is_none() {
                errors.push(format!(
                    "{} references unknown capability '{}'",
                    json_file.display(),
                    cap_id.0
                ));
            }
        }
    }
    Ok(errors)
}

fn find_json_files(dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in dirs {
        collect_json(dir, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_json(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json(&path, acc)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
            acc.push(path);
        }
    }
    Ok(())
}

fn extract_capability_ids(value: &Value) -> Vec<CapabilityId> {
    let mut ids = Vec::new();
    if let Some(id) = value
        .pointer("/probe/primary_capability_id")
        .and_then(Value::as_str)
    {
        ids.push(CapabilityId(id.to_string()));
    }

    if let Some(secondary) = value
        .pointer("/probe/secondary_capability_ids")
        .and_then(Value::as_array)
    {
        ids.extend(
            secondary
                .iter()
                .filter_map(Value::as_str)
                .map(|s| CapabilityId(s.to_string())),
        );
    }

    if let Some(primary_ctx) = value
        .pointer("/capability_context/primary/id")
        .and_then(Value::as_str)
    {
        ids.push(CapabilityId(primary_ctx.to_string()));
    }

    if let Some(secondary_ctx) = value
        .pointer("/capability_context/secondary")
        .and_then(Value::as_array)
    {
        ids.extend(secondary_ctx.iter().filter_map(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(|s| CapabilityId(s.to_string()))
        }));
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn validate_probe_capabilities_flags_missing_ids() {
        let index = sample_index().expect("sample index loads");
        let probe = ProbeMetadata {
            script: PathBuf::from("probe.sh"),
            probe_name: Some("probe".to_string()),
            probe_version: Some("1".to_string()),
            primary_capability: Some(CapabilityId("cap_missing".to_string())),
            secondary_capabilities: vec![CapabilityId("cap_fs_read_workspace_tree".to_string())],
        };
        let errors = validate_probe_capabilities(&index, &[probe]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("cap_missing"));
    }

    #[test]
    fn validate_boundary_objects_reports_unknown_capabilities() {
        let index = sample_index().expect("sample index loads");
        let dir = tempfile::tempdir().expect("temp dir");
        let bo_path = dir.path().join("bo.json");
        let record = json!({
            "schema_version": "cfbo-v1",
            "capabilities_schema_version": "macOS_codex_v1",
            "stack": {"os": "Darwin"},
            "probe": {
                "id": "probe",
                "version": "1",
                "primary_capability_id": "cap_missing",
                "secondary_capability_ids": []
            },
            "run": {"mode": "baseline", "workspace_root": "/tmp", "command": "true"},
            "operation": {"category": "fs", "verb": "read", "target": "/tmp", "args": {}},
            "result": {"observed_result": "success", "raw_exit_code": 0, "errno": null, "message": null, "error_detail": null},
            "payload": {"stdout_snippet": null, "stderr_snippet": null, "raw": {}},
            "capability_context": {"primary": {"id": "cap_missing", "category": "filesystem", "layer": "os_sandbox"}, "secondary": []}
        });
        std::fs::write(&bo_path, serde_json::to_string(&record).unwrap()).unwrap();

        let errors = validate_boundary_objects(&index, &[dir.path().to_path_buf()])
            .expect("validation should run");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("cap_missing"));
    }

    #[test]
    fn validate_boundary_objects_recurses_nested_dirs() {
        let index = sample_index().expect("sample index loads");
        let root = tempfile::tempdir().expect("temp dir");
        let nested = root.path().join("nested/inner");
        std::fs::create_dir_all(&nested).unwrap();
        let bo_path = nested.join("record.json");
        let record = json!({
            "schema_version": "cfbo-v1",
            "capabilities_schema_version": "macOS_codex_v1",
            "stack": {"os": "Darwin"},
            "probe": {
                "id": "probe",
                "version": "1",
                "primary_capability_id": "cap_fs_read_workspace_tree",
                "secondary_capability_ids": []
            },
            "run": {"mode": "baseline", "workspace_root": "/tmp", "command": "true"},
            "operation": {"category": "fs", "verb": "read", "target": "/tmp", "args": {}},
            "result": {"observed_result": "success", "raw_exit_code": 0, "errno": null, "message": null, "error_detail": null},
            "payload": {"stdout_snippet": null, "stderr_snippet": null, "raw": {}},
            "capability_context": {"primary": {"id": "cap_fs_read_workspace_tree", "category": "filesystem", "layer": "os_sandbox"}, "secondary": []}
        });
        std::fs::write(&bo_path, serde_json::to_string(&record).unwrap()).unwrap();

        let errors = validate_boundary_objects(&index, &[root.path().to_path_buf()])
            .expect("validation should run");
        assert!(
            errors.is_empty(),
            "expected no validation errors, got {errors:?}"
        );
    }

    fn sample_index() -> Result<CapabilityIndex> {
        let mut file = NamedTempFile::new()?;
        serde_json::to_writer(
            &mut file,
            &json!({
                "schema_version": "macOS_codex_v1",
                "scope": {"description": "test", "policy_layers": [], "categories": {}},
                "docs": {},
                "capabilities": [{
                    "id": "cap_fs_read_workspace_tree",
                    "category": "filesystem",
                    "layer": "os_sandbox",
                    "description": "fixture",
                    "operations": {"allow": [], "deny": []}
                }]
            }),
        )?;
        CapabilityIndex::load(file.path())
    }
}
