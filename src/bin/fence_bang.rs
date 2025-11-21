//! Runs a probe/mode matrix and streams boundary objects as NDJSON.
//!
//! This binary is the scripted equivalent of `make matrix`: it discovers probes
//! (or honors `PROBES`/`PROBES_RAW`), selects modes (`MODES` or defaults based
//! on Codex availability), executes each probe via `fence-run`, and prints each
//! emitted JSON object on its own line.

use anyhow::{Context, Result, bail};
use codex_fence::{
    Probe, codex_present, find_repo_root, list_probes, resolve_helper_binary, resolve_probe,
    split_list,
};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    env,
    path::Path,
    process::{Command, Stdio},
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let repo_root = find_repo_root()?;
    let probes = resolve_probes(&repo_root)?;
    let modes = resolve_modes()?;

    for mode in modes {
        for probe in &probes {
            run_probe(&repo_root, probe, &mode)?;
        }
    }
    Ok(())
}

fn resolve_modes() -> Result<Vec<String>> {
    let modes = env::var("MODES")
        .ok()
        .and_then(|raw| {
            let parsed = split_list(&raw);
            if parsed.is_empty() {
                None
            } else {
                Some(parsed)
            }
        })
        .unwrap_or_else(|| {
            if codex_present() {
                vec![
                    "baseline".to_string(),
                    "codex-sandbox".to_string(),
                    "codex-full".to_string(),
                ]
            } else {
                vec!["baseline".to_string()]
            }
        });

    let allowed: BTreeSet<&'static str> = ["baseline", "codex-sandbox", "codex-full"]
        .into_iter()
        .collect();
    if let Some(bad) = modes.iter().find(|mode| !allowed.contains(mode.as_str())) {
        bail!("Unsupported mode requested: {bad}");
    }

    if modes.is_empty() {
        bail!("No modes resolved; check MODES env var");
    }

    Ok(modes)
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

fn run_probe(repo_root: &Path, probe: &Probe, mode: &str) -> Result<()> {
    let runner = resolve_helper_binary(repo_root, "fence-run")?;
    let output = Command::new(&runner)
        .arg(mode)
        .arg(&probe.path)
        .current_dir(repo_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("Failed to execute {}", runner.display()))?;

    if !output.status.success() {
        // Gracefully skip codex modes when the host blocks sandbox application.
        if mode.starts_with("codex")
            && (output.status.code() == Some(71)
                || String::from_utf8_lossy(&output.stderr).contains("sandbox_apply"))
        {
            eprintln!(
                "fence-bang: skipping mode {mode} for probe {}: codex sandbox unavailable",
                probe.id
            );
            return Ok(());
        }
        let code = output.status.code().unwrap_or(-1);
        bail!(
            "Probe {} in mode {} returned non-zero exit code {code}",
            probe.id,
            mode
        );
    }

    let json_value: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "Failed to parse boundary object for probe {} in mode {}",
            probe.id, mode
        )
    })?;
    let compact = serde_json::to_string(&json_value)?;
    println!("{compact}");
    Ok(())
}
