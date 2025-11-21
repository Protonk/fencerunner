use crate::catalog::CapabilityIndex;
use crate::probe_metadata::ProbeMetadata;
use anyhow::{Result, anyhow, bail};
use serde::Serialize;
use std::collections::BTreeMap;

const IGNORED_PROBE_IDS: &[&str] = &["tests_fixture_probe", "tests_static_contract_broken"];

#[derive(Debug, Clone, Serialize)]
pub struct CoverageEntry {
    pub has_probe: bool,
    pub probe_ids: Vec<String>,
}

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
