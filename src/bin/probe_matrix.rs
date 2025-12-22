//! Runs probes and streams boundary objects as NDJSON.
//!
//! This binary underpins `fencerunner --bang/--bundle/--probe`: it discovers
//! probes, selects the requested slice (all probes, a capability bundle, or a
//! single probe), executes each probe via `probe-exec`, and prints each emitted
//! JSON object on its own line.

use anyhow::{Context, Result, anyhow, bail};
use fencerunner::catalog::{CapabilityId, CapabilityIndex};
use fencerunner::repo_tools::split_list;
use fencerunner::probes::discovery::{Probe, list_probes, resolve_probe};
use fencerunner::repo_tools::{find_repo_root, resolve_catalog_path, resolve_helper_binary};
use serde_json::Value;
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse()?;
    let repo_root = find_repo_root()?;
    let catalog_path = resolve_catalog_path(&repo_root, cli.catalog_path.as_deref());
    let probes = resolve_probes(&repo_root, &catalog_path, &cli)?;
    let mut errors: Vec<String> = Vec::new();
    for probe in &probes {
        if let Err(err) = run_probe(&repo_root, probe, &catalog_path) {
            let message = format!("probe {} failed: {err:#}", probe.id);
            eprintln!("probe-matrix: {message}");
            errors.push(message);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(
            "{} probe(s) failed; see stderr for details:\n{}",
            errors.len(),
            errors.join("\n")
        )
    }
}

fn resolve_probes(repo_root: &Path, catalog_path: &Path, cli: &Cli) -> Result<Vec<Probe>> {
    match cli.selection {
        Selection::All { explicit } => {
            if explicit {
                return list_probes(repo_root);
            }
            // PROBES/PROBES_RAW allow a smaller slice without changing CLI args.
            let requested = env::var("PROBES")
                .or_else(|_| env::var("PROBES_RAW"))
                .ok()
                .map(|raw| split_list(&raw))
                .unwrap_or_default();

            if requested.is_empty() {
                list_probes(repo_root)
            } else {
                let mut probes = Vec::new();
                for raw in requested {
                    probes.push(resolve_probe(repo_root, &raw)?);
                }
                Ok(probes)
            }
        }
        Selection::Probe(ref id) => Ok(vec![resolve_probe(repo_root, id)?]),
        Selection::Bundle(ref id) => {
            let index = CapabilityIndex::load(catalog_path)?;
            let capability = CapabilityId(id.clone());
            if index.capability(&capability).is_none() {
                bail!("unknown capability '{}'", id);
            }
            probes_for_capability(repo_root, &capability)
        }
    }
}

fn run_probe(
    repo_root: &Path,
    probe: &Probe,
    catalog_path: &Path,
) -> Result<()> {
    // probe-exec enforces the probe contract and exports FENCE_* metadata.
    let runner = resolve_helper_binary(repo_root, "probe-exec")?;
    let output = Command::new(&runner)
        .arg(&probe.path)
        .current_dir(repo_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .env("CATALOG_PATH", catalog_path)
        .output()
        .with_context(|| format!("Failed to execute {}", runner.display()))?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!("Probe {} returned non-zero exit code {code}", probe.id);
    }

    // Keep the output compact and single-line so downstream consumers can
    // stream records as NDJSON.
    let json_value: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!("Failed to parse boundary object for probe {}", probe.id)
    })?;
    let compact = serde_json::to_string(&json_value)?;
    println!("{compact}");
    Ok(())
}

fn probes_for_capability(repo_root: &Path, capability: &CapabilityId) -> Result<Vec<Probe>> {
    let mut matches = Vec::new();
    for probe in list_probes(repo_root)? {
        let metadata = fencerunner::probes::metadata::ProbeMetadata::from_script(&probe.path)?;
        if metadata
            .primary_capability
            .as_ref()
            .map(|id| id == capability)
            .unwrap_or(false)
        {
            matches.push(probe);
        }
    }
    matches.sort_by(|a, b| a.id.cmp(&b.id));
    if matches.is_empty() {
        bail!(
            "capability '{}' has no probes in this workspace",
            capability.0
        );
    }
    Ok(matches)
}

struct Cli {
    selection: Selection,
    catalog_path: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();
        let mut selection: Option<Selection> = None;
        let mut catalog_path = None;

        while let Some(arg) = args.next() {
            let arg_str = arg
                .to_str()
                .ok_or_else(|| anyhow!("invalid UTF-8 in argument"))?;
            match arg_str {
                "--bang" => {
                    selection = Some(Selection::All { explicit: true });
                }
                "--bundle" => {
                    let value = next_value("--bundle", &mut args)?;
                    selection = set_selection(selection, Selection::Bundle(value))?;
                }
                "--probe" => {
                    let value = next_value("--probe", &mut args)?;
                    selection = set_selection(selection, Selection::Probe(value))?;
                }
                "--catalog" => catalog_path = Some(next_path("--catalog", &mut args)?),
                "--help" | "-h" => usage(0),
                other => bail!("unknown argument: {other}"),
            }
        }

        Ok(Self {
            selection: selection.unwrap_or(Selection::All { explicit: false }),
            catalog_path,
        })
    }
}

fn next_path(flag: &str, args: &mut env::ArgsOs) -> Result<PathBuf> {
    let value = args
        .next()
        .ok_or_else(|| anyhow!("{flag} requires a value"))?;
    let path = PathBuf::from(
        value
            .into_string()
            .map_err(|_| anyhow!("{flag} must be valid UTF-8"))?,
    );
    if path.as_os_str().is_empty() {
        bail!("{flag} must not be empty");
    }
    Ok(path)
}

fn next_value(flag: &str, args: &mut env::ArgsOs) -> Result<String> {
    let value = args
        .next()
        .ok_or_else(|| anyhow!("{flag} requires a value"))?;
    value
        .into_string()
        .map_err(|_| anyhow!("{flag} value must be valid UTF-8"))
        .and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                bail!("{flag} value must not be empty");
            }
            Ok(trimmed.to_string())
        })
}

fn set_selection(current: Option<Selection>, new_sel: Selection) -> Result<Option<Selection>> {
    match current {
        None => Ok(Some(new_sel)),
        Some(Selection::All { explicit: false }) => Ok(Some(new_sel)),
        Some(_) => bail!("select exactly one of --bang, --bundle, or --probe"),
    }
}

#[derive(Clone)]
enum Selection {
    All { explicit: bool },
    Bundle(String),
    Probe(String),
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: probe-matrix (--bang | --bundle <capability-id> | --probe <probe-id>) [--catalog PATH]\n\nCommands:\n  --bang                 Run every probe once.\n  --bundle <capability>  Run probes whose primary capability matches <capability>.\n  --probe <id>           Run a single probe by id.\n\nOptions:\n  --catalog PATH         Override capability catalog path (or set CATALOG_PATH).\n  --help                 Show this help text."
    );
    std::process::exit(code);
}
