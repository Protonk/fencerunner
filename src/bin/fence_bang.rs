use anyhow::{Context, Result, bail};
use codex_fence::{codex_present, find_repo_root, split_list};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone)]
struct Probe {
    id: String,
    path: PathBuf,
}

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
        return list_all_probes(repo_root);
    }

    let mut probes = Vec::new();
    for raw in requested {
        probes.push(resolve_probe_identifier(repo_root, &raw)?);
    }
    Ok(probes)
}

fn list_all_probes(repo_root: &Path) -> Result<Vec<Probe>> {
    let probes_dir = repo_root.join("probes");
    let mut results = BTreeMap::new();
    for entry in fs::read_dir(&probes_dir)
        .with_context(|| format!("Failed to read probes dir at {}", probes_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("sh") {
            continue;
        }
        let canonical = fs::canonicalize(&path)
            .with_context(|| format!("Failed to canonicalize probe path {}", path.display()))?;
        if let Some(stem) = canonical.file_stem().and_then(|s| s.to_str()) {
            results.insert(
                stem.to_string(),
                Probe {
                    id: stem.to_string(),
                    path: canonical,
                },
            );
        }
    }

    if results.is_empty() {
        bail!("No probes found under {}", probes_dir.to_string_lossy());
    }

    Ok(results.into_values().collect())
}

fn resolve_probe_identifier(repo_root: &Path, raw: &str) -> Result<Probe> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("Empty probe identifier requested");
    }

    let probes_root =
        fs::canonicalize(repo_root.join("probes")).context("Failed to resolve probes directory")?;
    let mut candidates = Vec::new();
    let input_path = PathBuf::from(trimmed);
    if input_path.is_absolute() {
        candidates.push(input_path.clone());
    } else {
        candidates.push(repo_root.join(&input_path));
        if input_path.extension().is_none() {
            candidates.push(repo_root.join(format!("{trimmed}.sh")));
        }
        candidates.push(repo_root.join("probes").join(&input_path));
        if input_path.extension().is_none() {
            candidates.push(repo_root.join("probes").join(format!("{trimmed}.sh")));
        }
    }

    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let canonical = fs::canonicalize(&candidate).with_context(|| {
            format!(
                "Failed to canonicalize candidate probe {}",
                candidate.display()
            )
        })?;
        if !canonical.starts_with(&probes_root) {
            continue;
        }
        if let Some(stem) = canonical.file_stem().and_then(|s| s.to_str()) {
            return Ok(Probe {
                id: stem.to_string(),
                path: canonical,
            });
        }
    }

    bail!("Probe not found or outside probes/: {trimmed}");
}

fn run_probe(repo_root: &Path, probe: &Probe, mode: &str) -> Result<()> {
    let runner = repo_root.join("bin/fence-run");
    let output = Command::new(&runner)
        .arg(mode)
        .arg(&probe.path)
        .current_dir(repo_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("Failed to execute {}", runner.display()))?;

    if !output.status.success() {
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
