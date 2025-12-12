//! Runs a probe/mode matrix and streams boundary objects as NDJSON.
//!
//! This binary underpins `probe --matrix`: it discovers probes
//! (or honors `PROBES`/`PROBES_RAW`), selects modes (`MODES` or defaults based
//! on connector availability), executes each probe via `probe-exec`, and
//! prints each emitted JSON object on its own line.

use anyhow::{Context, Result, anyhow, bail};
use fencerunner::connectors::{
    Availability, RunMode, allowed_mode_names, default_mode_names, parse_modes,
};
use fencerunner::{
    Probe, find_repo_root, list_probes, resolve_boundary_schema_path, resolve_catalog_path,
    resolve_helper_binary, resolve_probe, split_list,
};
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
    let boundary_schema_path =
        resolve_boundary_schema_path(&repo_root, cli.boundary_path.as_deref())?;
    let probes = resolve_probes(&repo_root)?;
    let modes = resolve_modes()?;

    let mut errors: Vec<String> = Vec::new();
    for mode in modes {
        for probe in &probes {
            if let Err(err) = run_probe(
                &repo_root,
                probe,
                mode,
                &catalog_path,
                &boundary_schema_path,
            ) {
                let message = format!(
                    "probe {} in mode {} failed: {err:#}",
                    probe.id,
                    mode.as_str()
                );
                eprintln!("probe-matrix: {message}");
                errors.push(message);
            }
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

fn resolve_modes() -> Result<Vec<RunMode>> {
    let requested = env::var("MODES")
        .ok()
        .and_then(|raw| {
            let parsed = split_list(&raw);
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        })
        .unwrap_or_else(|| default_mode_names(Availability::for_host()));

    if requested.is_empty() {
        bail!("No modes resolved; check MODES env var");
    }

    let allowed = allowed_mode_names();
    if let Some(bad) = requested
        .iter()
        .find(|mode| !allowed.contains(&mode.as_str()))
    {
        bail!("Unsupported mode requested: {bad}");
    }

    parse_modes(&requested)
}

fn resolve_probes(repo_root: &Path) -> Result<Vec<Probe>> {
    let requested = env::var("PROBES")
        .or_else(|_| env::var("PROBES_RAW"))
        .ok()
        .map(|raw| split_list(&raw))
        .unwrap_or_default();

    if requested.is_empty() {
        return list_probes(repo_root);
    }

    let mut probes = Vec::new();
    for raw in requested {
        probes.push(resolve_probe(repo_root, &raw)?);
    }
    Ok(probes)
}

fn run_probe(
    repo_root: &Path,
    probe: &Probe,
    mode: RunMode,
    catalog_path: &Path,
    boundary_path: &Path,
) -> Result<()> {
    let runner = resolve_helper_binary(repo_root, "probe-exec")?;
    let output = Command::new(&runner)
        .arg(mode.as_str())
        .arg(&probe.path)
        .current_dir(repo_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .env("CATALOG_PATH", catalog_path)
        .env("BOUNDARY_PATH", boundary_path)
        .output()
        .with_context(|| format!("Failed to execute {}", runner.display()))?;

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!(
            "Probe {} in mode {} returned non-zero exit code {code}",
            probe.id,
            mode.as_str()
        );
    }

    let json_value: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "Failed to parse boundary object for probe {} in mode {}",
            probe.id,
            mode.as_str()
        )
    })?;
    let compact = serde_json::to_string(&json_value)?;
    println!("{compact}");
    Ok(())
}

struct Cli {
    catalog_path: Option<PathBuf>,
    boundary_path: Option<PathBuf>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();
        let mut catalog_path = None;
        let mut boundary_path = None;

        while let Some(arg) = args.next() {
            let arg_str = arg
                .to_str()
                .ok_or_else(|| anyhow!("invalid UTF-8 in argument"))?;
            match arg_str {
                "--catalog" => catalog_path = Some(next_path("--catalog", &mut args)?),
                "--boundary" => boundary_path = Some(next_path("--boundary", &mut args)?),
                "--help" | "-h" => usage(0),
                other => bail!("unknown argument: {other}"),
            }
        }

        Ok(Self {
            catalog_path,
            boundary_path,
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

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: probe-matrix [--catalog PATH] [--boundary PATH]\n\nOptions:\n  --catalog PATH            Override capability catalog path (or set CATALOG_PATH).\n  --boundary PATH           Override boundary-object schema path (or set BOUNDARY_PATH).\n  --help                    Show this help text."
    );
    std::process::exit(code);
}
