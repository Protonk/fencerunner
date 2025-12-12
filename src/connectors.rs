//! Connector registry for run modes.
//!
//! This module centralizes how run modes map to connectors (ambient only),
//! sandbox defaults, and command planning. Binaries should rely on this
//! registry instead of hard-coding mode strings so new connectors can be added
//! in one place without changing public CLI flags or drifting from the
//! boundary-object schema and `docs/probes.md`.

use anyhow::{Result, bail};
use std::ffi::OsString;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectorKind {
    Ambient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    Baseline,
}

impl RunMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunMode::Baseline => "baseline",
        }
    }

    pub fn connector(&self) -> ConnectorKind {
        match self {
            RunMode::Baseline => ConnectorKind::Ambient,
        }
    }

    pub fn sandbox_env(&self, _override_value: Option<String>) -> OsString {
        match self {
            RunMode::Baseline => OsString::from(""),
        }
    }

    fn ensure_connector_present(&self) -> Result<()> {
        Ok(())
    }

    fn command_spec(
        &self,
        _platform: Option<&str>,
        probe_path: &Path,
    ) -> Result<CommandSpec> {
        let probe_arg = probe_path.as_os_str().to_os_string();
        match self {
            RunMode::Baseline => Ok(CommandSpec {
                program: probe_arg,
                args: Vec::new(),
            }),
        }
    }

}

impl TryFrom<&str> for RunMode {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "baseline" => Ok(RunMode::Baseline),
            other => bail!("Unknown mode: {other}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModePlan {
    pub run_mode: RunMode,
    pub connector: ConnectorKind,
    pub sandbox_env: OsString,
    pub command: CommandSpec,
}

pub fn plan_for_mode(
    requested_mode: &str,
    _platform: &str,
    probe_path: &Path,
    sandbox_override: Option<String>,
) -> Result<ModePlan> {
    let run_mode = RunMode::try_from(requested_mode)?;
    run_mode.ensure_connector_present()?;
    let sandbox_env = run_mode.sandbox_env(sandbox_override);
    let command = run_mode.command_spec(None, probe_path)?;

    Ok(ModePlan {
        run_mode,
        connector: run_mode.connector(),
        sandbox_env,
        command,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: OsString,
    pub args: Vec<OsString>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Availability;

impl Availability {
    pub fn for_host() -> Self {
        Availability
    }
}

pub fn default_mode_names(availability: Availability) -> Vec<String> {
    MODE_SPECS
        .iter()
        .filter(|spec| (spec.default_gate)(&availability))
        .map(|spec| spec.run_mode.as_str().to_string())
        .collect()
}

pub fn parse_modes(modes: &[String]) -> Result<Vec<RunMode>> {
    modes
        .iter()
        .map(|mode| RunMode::try_from(mode.as_str()))
        .collect()
}

pub fn allowed_mode_names() -> Vec<&'static str> {
    MODE_SPECS
        .iter()
        .map(|spec| spec.run_mode.as_str())
        .collect()
}

fn always_available(_: &Availability) -> bool {
    true
}

struct ModeSpec {
    run_mode: RunMode,
    default_gate: fn(&Availability) -> bool,
}

const MODE_SPECS: &[ModeSpec] = &[ModeSpec {
    run_mode: RunMode::Baseline,
    default_gate: always_available,
}];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn run_mode_parse_and_strings_round_trip() {
        let baseline = RunMode::try_from("baseline").expect("baseline parses");
        assert_eq!(baseline.as_str(), "baseline");
        assert!(RunMode::try_from("unknown-mode").is_err());
    }

    #[test]
    fn baseline_plan_uses_direct_execution() {
        let plan = plan_for_mode(
            "baseline",
            "Darwin",
            PathBuf::from("/tmp/probe.sh").as_path(),
            None,
        )
        .expect("baseline plan");

        assert_eq!(plan.run_mode, RunMode::Baseline);
        assert_eq!(plan.connector, ConnectorKind::Ambient);
        assert_eq!(plan.command.args.len(), 0);
        assert_eq!(plan.command.program, OsString::from("/tmp/probe.sh"));
    }
}
