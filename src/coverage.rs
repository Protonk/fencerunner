//! Coverage accounting between the capability catalog and authored probes.
//!
//! Helpers here build a capability→probe mapping used by listeners and tests to
//! identify gaps. Coverage intentionally ignores fixtures and known broken
//! probes so only actionable entries surface.

use crate::catalog::CapabilityIndex;
use crate::probe_metadata::ProbeMetadata;
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use std::collections::BTreeMap;

// Probes used by tests or contract fixtures should not count toward coverage.
const IGNORED_PROBE_IDS: &[&str] = &["tests_fixture_probe", "tests_static_contract_broken"];

#[derive(Debug, Clone, Serialize)]
/// Whether a capability has one or more probes plus the list of those ids.
pub struct CoverageEntry {
    pub has_probe: bool,
    pub probe_ids: Vec<String>,
}

/// Build a mapping of capability id to probe coverage.
///
/// Rejects unknown capability ids so regressions in probe metadata surface
/// immediately. Duplicate probe ids per capability are deduplicated but kept
/// stable for deterministic output.
pub fn build_probe_coverage_map(
    capabilities: &CapabilityIndex,
    probes: &[ProbeMetadata],
) -> Result<BTreeMap<String, CoverageEntry>> {
    let mut map: BTreeMap<String, CoverageEntry> = capabilities
        .ids()
        .map(|id| {
            (
                id.0.clone(),
                CoverageEntry {
                    has_probe: false,
                    probe_ids: Vec::new(),
                },
            )
        })
        .collect();

    for probe in probes {
        let path_display = probe.script.display();
        let probe_name = probe
            .probe_name
            .as_deref()
            .ok_or_else(|| anyhow!("{path_display} is missing probe_name"))?;
        let primary = probe
            .primary_capability
            .as_ref()
            .ok_or_else(|| anyhow!("{path_display} is missing primary_capability_id"))?;
        let entry = map.get_mut(&primary.0).ok_or_else(|| {
            anyhow!(
                "{path_display} references unknown capability '{}'",
                primary.0
            )
        })?;

        entry.has_probe = true;
        if !entry.probe_ids.contains(&probe_name.to_string()) {
            entry.probe_ids.push(probe_name.to_string());
            entry.probe_ids.sort();
        }
    }

    Ok(map)
}

/// Sanity-check that the coverage map contains every capability in the index.
pub fn validate_coverage_against_map(
    coverage: &BTreeMap<String, CoverageEntry>,
    capabilities: &CapabilityIndex,
) -> Result<()> {
    for id in capabilities.ids() {
        if !coverage.contains_key(&id.0) {
            bail!("coverage map missing entry for '{}'", id.0);
        }
    }
    Ok(())
}

/// Filter out probes that should not affect coverage reporting.
pub fn filter_coverage_probes(probes: &[ProbeMetadata]) -> Vec<ProbeMetadata> {
    probes
        .iter()
        .cloned()
        .filter(|probe| match &probe.probe_name {
            Some(name) => !IGNORED_PROBE_IDS.contains(&name.as_str()),
            None => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapabilityId;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    #[test]
    fn build_probe_coverage_map_rejects_unknown_capability() {
        let caps = sample_index().expect("load sample index");
        let probe = ProbeMetadata {
            script: PathBuf::from("probe.sh"),
            probe_name: Some("probe".to_string()),
            probe_version: Some("1".to_string()),
            primary_capability: Some(CapabilityId("cap_missing".to_string())),
            secondary_capabilities: Vec::new(),
        };
        let err = build_probe_coverage_map(&caps, &[probe]).expect_err("unknown cap should fail");
        assert!(
            err.to_string().contains("cap_missing"),
            "error should mention missing capability"
        );
    }

    #[test]
    fn filter_coverage_probes_ignores_fixtures() {
        let probes = vec![
            ProbeMetadata {
                script: PathBuf::from("probe.sh"),
                probe_name: Some("tests_fixture_probe".to_string()),
                probe_version: None,
                primary_capability: None,
                secondary_capabilities: Vec::new(),
            },
            ProbeMetadata {
                script: PathBuf::from("probe2.sh"),
                probe_name: Some("real_probe".to_string()),
                probe_version: None,
                primary_capability: None,
                secondary_capabilities: Vec::new(),
            },
        ];
        let filtered = filter_coverage_probes(&probes);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].probe_name.as_deref(), Some("real_probe"));
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
