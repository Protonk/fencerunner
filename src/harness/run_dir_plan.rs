//! Run-dir planning shared by the script runners.
//!
//! A run dir is a flat directory containing:
//! - `gates.json`
//! - `commitments.json`
//! - `boundaries.json`
//! - one or more executable `*.sh` scripts
//!
//! This module centralizes the preflight and script-discovery logic so runner
//! binaries (currently `fencerunner`) do not drift.

use crate::boundary::BoundaryContractIndex;
use crate::commitments::index::CommitmentIndex;
use crate::gates::contract::GatesContractIndex;
use crate::scripts::discovery::{Script, canonical_run_dir, list_scripts};
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
        // Commitments are validated at preflight even though enrollment is runtime-only.
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

/// List every script across all run dirs and reject id collisions.
///
/// The returned plan is stable: within each run dir scripts are sorted by id
/// (lexicographic), and run dirs preserve the caller-provided ordering.
pub fn plan_scripts(run_dirs: &[RunDirPlan]) -> Result<Vec<(usize, Script)>> {
    let mut seen_script_ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut execution_plan: Vec<(usize, Script)> = Vec::new();

    for (idx, run_dir) in run_dirs.iter().enumerate() {
        let scripts = list_scripts(&run_dir.path)
            .with_context(|| format!("listing scripts under {}", run_dir.path.display()))?;
        for script in scripts {
            if let Some(existing) = seen_script_ids.get(&script.id) {
                bail!(
                    "Duplicate script id '{}' found in {} and {}",
                    script.id,
                    existing.display(),
                    script.path.display()
                );
            }
            seen_script_ids.insert(script.id.clone(), script.path.clone());
            execution_plan.push((idx, script));
        }
    }

    Ok(execution_plan)
}
