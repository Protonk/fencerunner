use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const RUNNER_RELATIVE: &str = "bin/fence-run";
const MAKEFILE: &str = "Makefile";

#[derive(Debug, Deserialize, Clone)]
pub struct CapabilitySnapshot {
    pub id: String,
    pub category: String,
    pub layer: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CapabilityContext {
    pub primary: CapabilitySnapshot,
    #[serde(default)]
    pub secondary: Vec<CapabilitySnapshot>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProbeInfo {
    pub id: String,
    pub version: String,
    pub primary_capability_id: String,
    #[serde(default)]
    pub secondary_capability_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunInfo {
    pub mode: String,
    #[serde(default)]
    pub workspace_root: Option<String>,
    pub command: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OperationInfo {
    pub category: String,
    pub verb: String,
    pub target: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ResultInfo {
    pub observed_result: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BoundaryObject {
    pub schema_version: String,
    #[serde(default)]
    pub capabilities_schema_version: Option<String>,
    pub probe: ProbeInfo,
    pub run: RunInfo,
    pub result: ResultInfo,
    pub operation: OperationInfo,
    #[serde(default)]
    pub capability_context: Option<CapabilityContext>,
}

impl BoundaryObject {
    pub fn primary_capability_id(&self) -> Option<&str> {
        if let Some(ctx) = &self.capability_context {
            return Some(ctx.primary.id.as_str());
        }
        Some(self.probe.primary_capability_id.as_str())
    }
}

fn is_repo_root(candidate: &Path) -> bool {
    candidate.join(RUNNER_RELATIVE).is_file() && candidate.join(MAKEFILE).is_file()
}

fn repo_root_from_hint(hint: &str) -> Option<PathBuf> {
    if hint.is_empty() {
        return None;
    }
    let hint_path = PathBuf::from(hint);
    if !hint_path.exists() || !is_repo_root(&hint_path) {
        return None;
    }
    fs::canonicalize(hint_path).ok()
}

fn search_upwards(start: &Path) -> Option<PathBuf> {
    let mut dir = fs::canonicalize(start).ok()?;
    loop {
        if is_repo_root(&dir) {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

pub fn find_repo_root() -> Result<PathBuf> {
    if let Ok(env_root) = env::var("CODEX_FENCE_ROOT") {
        if let Some(root) = repo_root_from_hint(&env_root) {
            return Ok(root);
        }
    }

    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            if let Some(root) = search_upwards(exe_dir) {
                return Ok(root);
            }
        }
    }

    if let Some(hint) = option_env!("CODEX_FENCE_ROOT_HINT") {
        if let Some(root) = repo_root_from_hint(hint) {
            return Ok(root);
        }
    }

    bail!(
        "Unable to locate codex-fence repository root. Set CODEX_FENCE_ROOT to the cloned repository."
    );
}

pub fn codex_present() -> bool {
    env::var_os("PATH")
        .map(|paths| {
            env::split_paths(&paths).any(|dir| {
                let candidate = dir.join("codex");
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

pub fn split_list(value: &str) -> Vec<String> {
    value
        .replace(',', " ")
        .split_whitespace()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn parse_json_stream(input: &str) -> Result<Vec<BoundaryObject>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("No input provided on stdin");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return match value {
            Value::Array(items) => items
                .into_iter()
                .map(serde_json::from_value)
                .collect::<Result<Vec<_>, _>>()
                .context("Unable to parse JSON array of boundary objects"),
            Value::Object(_) => serde_json::from_value(value)
                .map(|obj| vec![obj])
                .context("Unable to parse boundary object"),
            _ => bail!("Unsupported JSON input; expected object or array"),
        };
    }

    let mut records = Vec::new();
    for (idx, line) in trimmed.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let obj: BoundaryObject = serde_json::from_str(line)
            .with_context(|| format!("Unable to parse boundary object from line {}", idx + 1))?;
        records.push(obj);
    }

    if records.is_empty() {
        bail!("No boundary objects found in input stream");
    }

    Ok(records)
}
