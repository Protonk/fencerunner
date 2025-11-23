//! Targeted probe runner backing `codex-fence --rattle`.
//!
//! The CLI selects a subset of probes by capability id or explicit probe id,
//! fans out across the requested modes, and shells out to `fence-bang` so the
//! existing execution pipeline (fence-run → emit-record) remains untouched.

use anyhow::{Context, Result, anyhow, bail};
use codex_fence::{
    CapabilityId, CapabilityIndex, Probe, ProbeMetadata, codex_present, find_repo_root,
    list_probes, resolve_helper_binary, resolve_probe,
};
use std::collections::BTreeSet;
use std::env;
use std::path::Path;
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse()?;
    let repo_root = find_repo_root()?;
    let modes = resolve_modes(&cli.modes)?;
    let plan = resolve_selection(&repo_root, &cli.selection)?;

    if cli.list_only {
        print_dry_run(&plan, &modes, cli.repeat);
        return Ok(());
    }

    run_matrix(&repo_root, &plan.probes, &modes, cli.repeat)
}

fn print_dry_run(plan: &SelectionPlan, modes: &[String], repeat: u32) {
    println!("codex-fence rattle (dry-run)");
    match &plan.selection {
        SelectionDescription::Capability(id) => println!("capability: {}", id.0),
        SelectionDescription::Probes(ids) => println!("probes: {}", ids.join(", ")),
    }
    println!("modes: {}", modes.join(", "));
    if repeat > 1 {
        println!("repeat: {repeat}");
    }
    println!("probes to run:");
    for probe in &plan.probes {
        println!("  - {}", probe.id);
    }
}

fn run_matrix(repo_root: &Path, probes: &[Probe], modes: &[String], repeat: u32) -> Result<()> {
    if probes.is_empty() {
        bail!("no probes resolved for rattle run");
    }
    let helper = resolve_helper_binary(repo_root, "fence-bang")?;
    let probes_arg = probes
        .iter()
        .map(|probe| probe.id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let modes_arg = modes.join(" ");

    for attempt in 0..repeat {
        let mut cmd = Command::new(&helper);
        cmd.current_dir(repo_root)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .env("PROBES", &probes_arg)
            .env("MODES", &modes_arg);
        if env::var_os("CODEX_FENCE_ROOT").is_none() {
            cmd.env("CODEX_FENCE_ROOT", repo_root);
        }

        let status = cmd
            .status()
            .with_context(|| format!("failed to execute {}", helper.display()))?;
        if !status.success() {
            let prefix = if repeat > 1 {
                format!("repeat {} failed", attempt + 1)
            } else {
                "rattle run failed".to_string()
            };
            if let Some(code) = status.code() {
                bail!("{prefix} with exit code {code}");
            }
            bail!("{prefix}: helper terminated by signal");
        }
    }

    Ok(())
}

fn resolve_modes(requested: &[String]) -> Result<Vec<String>> {
    let modes = if requested.is_empty() {
        default_modes()
    } else {
        requested.to_vec()
    };

    if modes.is_empty() {
        bail!("no execution modes resolved");
    }

    let allowed: BTreeSet<&'static str> = ["baseline", "codex-sandbox", "codex-full"]
        .into_iter()
        .collect();
    if let Some(invalid) = modes.iter().find(|mode| !allowed.contains(mode.as_str())) {
        bail!("unsupported mode requested: {invalid}");
    }

    Ok(modes)
}

fn default_modes() -> Vec<String> {
    if codex_present() {
        vec![
            "baseline".to_string(),
            "codex-sandbox".to_string(),
            "codex-full".to_string(),
        ]
    } else {
        vec!["baseline".to_string()]
    }
}

fn resolve_selection(repo_root: &Path, selection: &Selection) -> Result<SelectionPlan> {
    match selection {
        Selection::Capability(id) => resolve_capability_selection(repo_root, id),
        Selection::Probes(ids) => resolve_probe_selection(repo_root, ids),
    }
}

fn resolve_capability_selection(repo_root: &Path, id: &CapabilityId) -> Result<SelectionPlan> {
    let catalog_path = repo_root.join("schema").join("capabilities.json");
    let index = CapabilityIndex::load(&catalog_path)?;
    if index.capability(id).is_none() {
        bail!(
            "unknown capability '{}' (not present in bundled catalog)",
            id.0
        );
    }

    let probes = probes_for_capability(repo_root, id)?;
    if probes.is_empty() {
        bail!("capability '{}' has no probes in this workspace", id.0);
    }

    Ok(SelectionPlan {
        selection: SelectionDescription::Capability(id.clone()),
        probes,
    })
}

fn probes_for_capability(repo_root: &Path, capability: &CapabilityId) -> Result<Vec<Probe>> {
    let mut matches = Vec::new();
    for probe in list_probes(repo_root)? {
        let metadata = ProbeMetadata::from_script(&probe.path)?;
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
    Ok(matches)
}

fn resolve_probe_selection(repo_root: &Path, requested: &[String]) -> Result<SelectionPlan> {
    if requested.is_empty() {
        bail!("--probe must be provided at least once when --cap is omitted");
    }

    let mut probes = Vec::new();
    let mut seen = BTreeSet::new();
    for raw in requested {
        let resolved = resolve_probe(repo_root, raw)?;
        if seen.insert(resolved.id.clone()) {
            probes.push(resolved);
        }
    }

    Ok(SelectionPlan {
        selection: SelectionDescription::Probes(probes.iter().map(|p| p.id.clone()).collect()),
        probes,
    })
}

struct SelectionPlan {
    selection: SelectionDescription,
    probes: Vec<Probe>,
}

enum SelectionDescription {
    Capability(CapabilityId),
    Probes(Vec<String>),
}

#[derive(Clone)]
enum Selection {
    Capability(CapabilityId),
    Probes(Vec<String>),
}

struct Cli {
    selection: Selection,
    modes: Vec<String>,
    repeat: u32,
    list_only: bool,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();

        let mut cap: Option<String> = None;
        let mut probes: Vec<String> = Vec::new();
        let mut modes: Vec<String> = Vec::new();
        let mut repeat: u32 = 1;
        let mut list_only = false;

        while let Some(arg) = args.next() {
            let arg_str = arg
                .to_str()
                .ok_or_else(|| anyhow!("invalid UTF-8 in argument"))?;
            match arg_str {
                "--cap" => {
                    let value = next_value("--cap", &mut args)?;
                    if cap.is_some() {
                        bail!("--cap may only be specified once");
                    }
                    cap = Some(normalize_token(value, "--cap")?);
                }
                "--probe" => {
                    let value = next_value("--probe", &mut args)?;
                    probes.push(normalize_token(value, "--probe")?);
                }
                "--mode" => {
                    let value = next_value("--mode", &mut args)?;
                    modes.push(normalize_token(value, "--mode")?);
                }
                "--repeat" => {
                    let value = next_value("--repeat", &mut args)?;
                    repeat = value.parse().context("--repeat must be >= 1")?;
                    if repeat == 0 {
                        bail!("--repeat must be >= 1");
                    }
                }
                "--list-only" => list_only = true,
                "--help" | "-h" => usage(0),
                other => {
                    bail!("unknown argument: {other}");
                }
            }
        }

        let selection = match (cap, probes.is_empty()) {
            (Some(cap_id), true) => Selection::Capability(CapabilityId(cap_id)),
            (None, false) => Selection::Probes(probes),
            (Some(_), false) => {
                bail!("Specify exactly one of --cap or --probe");
            }
            (None, true) => {
                bail!("--cap or --probe is required for --rattle");
            }
        };

        Ok(Self {
            selection,
            modes,
            repeat,
            list_only,
        })
    }
}

fn next_value(flag: &str, args: &mut env::ArgsOs) -> Result<String> {
    let value = args
        .next()
        .ok_or_else(|| anyhow!("{flag} requires a value"))?;
    value
        .into_string()
        .map_err(|_| anyhow!("{flag} value must be valid UTF-8"))
}

fn normalize_token(raw: String, flag: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("{flag} value must not be empty");
    }
    Ok(trimmed.to_string())
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: fence-rattle (--cap <capability-id> | --probe <probe-id>) [options]\n\nOptions:\n      --cap <id>        Run every probe whose primary capability matches <id>.\n      --probe <id>      Run a specific probe (repeatable).\n      --mode <mode>     Restrict modes (baseline, codex-sandbox, codex-full).\n      --repeat <n>      Rerun the selection n times (default: 1).\n      --list-only       Print the plan without executing probes.\n      --help            Show this help text.\n"
    );
    std::process::exit(code);
}
