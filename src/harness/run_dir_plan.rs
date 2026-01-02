//! Run-dir planning shared by the probe runners.
//!
//! A run dir is a flat directory containing:
//! - `gates.json`
//! - `commitments.json`
//! - `boundaries.json`
//! - one or more executable `*.sh` probe scripts
//!
//! This module centralizes the preflight and probe-discovery logic so runner
//! binaries (currently `fencerunner`) do not drift.

use crate::boundary::BoundaryContractIndex;
use crate::commitments::index::CommitmentIndex;
use crate::gates::contract::GatesContractIndex;
use crate::probes::discovery::{Probe, canonical_run_dir, list_probes};
use crate::repo_tools::{boundaries_contract_path, commitments_registry_path, gates_contract_path};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RunDirPlan {
    pub path: PathBuf,
    pub boundary: BoundaryContractIndex,
    pub enforce_stderr_empty: bool,
}

/// Load and validate each run dir, returning plans in the same order as `raw`.
///
/// Failure is a hard error: runners should abort early when any run dir is
/// missing required contract files or they do not validate.
pub fn preflight_run_dirs(raw: &[PathBuf]) -> Result<Vec<RunDirPlan>> {
    let mut run_dirs: Vec<RunDirPlan> = Vec::new();
    for candidate in raw {
        let run_dir = canonical_run_dir(candidate)
            .with_context(|| format!("invalid run dir {}", candidate.display()))?;

        let gates_contract_path = gates_contract_path(&run_dir);
        let gates_contract = GatesContractIndex::load(&gates_contract_path)
            .with_context(|| format!("loading {}", gates_contract_path.display()))?;
        let enforce_stderr_empty = gates_contract.enforces_stderr_empty();

        let registry_path = commitments_registry_path(&run_dir);
        CommitmentIndex::load(&registry_path)
            .with_context(|| format!("loading {}", registry_path.display()))?;

        let boundaries_path = boundaries_contract_path(&run_dir);
        let boundary = BoundaryContractIndex::load(&boundaries_path)
            .with_context(|| format!("loading {}", boundaries_path.display()))?;

        run_dirs.push(RunDirPlan {
            path: run_dir,
            boundary,
            enforce_stderr_empty,
        });
    }
    Ok(run_dirs)
}

/// List every probe across all run dirs and reject id collisions.
///
/// The returned plan is stable: within each run dir probes are sorted by id,
/// and run dirs preserve the caller-provided ordering.
pub fn plan_probes(run_dirs: &[RunDirPlan]) -> Result<Vec<(usize, Probe)>> {
    let mut seen_probe_ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut execution_plan: Vec<(usize, Probe)> = Vec::new();

    for (idx, run_dir) in run_dirs.iter().enumerate() {
        let probes = list_probes(&run_dir.path)
            .with_context(|| format!("listing probes under {}", run_dir.path.display()))?;
        for probe in probes {
            if let Some(existing) = seen_probe_ids.get(&probe.id) {
                bail!(
                    "Duplicate probe id '{}' found in {} and {}",
                    probe.id,
                    existing.display(),
                    probe.path.display()
                );
            }
            seen_probe_ids.insert(probe.id.clone(), probe.path.clone());
            execution_plan.push((idx, probe));
        }
    }

    Ok(execution_plan)
}
