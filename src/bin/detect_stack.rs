//! Collects host/sandbox metadata for inclusion in boundary objects.
//!
//! The binary is intentionally dependency-free and lightweight because probes
//! invoke it for every record. It reflects the current run mode (from CLI or
//! env), infers sandbox/codex details, and emits a JSON `StackInfo` snapshot.

use anyhow::{Result, bail};
use serde::Serialize;
use std::env;
use std::process::Command;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli_run_mode = parse_cli_run_mode();
    let run_mode = match cli_run_mode {
        Some(mode) => mode,
        None => env_non_empty("FENCE_RUN_MODE").unwrap_or_else(|| usage_and_exit()),
    };

    let sandbox_mode = determine_sandbox_mode(&run_mode, env_non_empty("FENCE_SANDBOX_MODE"))?;
    let codex_cli_version = detect_codex_cli_version();
    let codex_profile = env_non_empty("FENCE_CODEX_PROFILE");
    let codex_model = env_non_empty("FENCE_CODEX_MODEL");
    let os_info = detect_uname(&["-srm"]).unwrap_or_else(|| fallback_os_info());
    let os_name = detect_uname(&["-s"]).unwrap_or_else(|| fallback_os_name());
    let container_tag = resolve_container_tag(&os_name, env_non_empty("FENCE_CONTAINER_TAG"));

    let info = StackInfo {
        codex_cli_version,
        codex_profile,
        codex_model,
        sandbox_mode,
        os: os_info,
        container_tag,
    };

    println!("{}", serde_json::to_string(&info)?);
    Ok(())
}

#[derive(Serialize)]
struct StackInfo {
    codex_cli_version: Option<String>,
    codex_profile: Option<String>,
    codex_model: Option<String>,
    sandbox_mode: Option<String>,
    os: String,
    container_tag: String,
}

fn parse_cli_run_mode() -> Option<String> {
    let mut args = env::args().skip(1);
    let first = args.next()?;
    if matches!(first.as_str(), "-h" | "--help") {
        usage_and_exit();
    }
    if args.next().is_some() {
        usage_and_exit();
    }
    Some(first)
}

fn determine_sandbox_mode(run_mode: &str, sandbox_env: Option<String>) -> Result<Option<String>> {
    match run_mode {
        "baseline" => Ok(None),
        "codex-sandbox" => Ok(Some(
            sandbox_env.unwrap_or_else(|| "workspace-write".to_string()),
        )),
        "codex-full" => Ok(Some(
            sandbox_env.unwrap_or_else(|| "danger-full-access".to_string()),
        )),
        other => bail!("Unknown run mode: {other}"),
    }
}

fn detect_codex_cli_version() -> Option<String> {
    let output = Command::new("codex").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
}

fn detect_uname(args: &[&str]) -> Option<String> {
    let output = Command::new("uname").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

fn fallback_os_info() -> String {
    format!("{} {}", env::consts::OS, env::consts::ARCH)
}

fn fallback_os_name() -> String {
    match env::consts::OS {
        "macos" => "Darwin".to_string(),
        "linux" => "Linux".to_string(),
        other => other.to_string(),
    }
}

fn resolve_container_tag(os_name: &str, env_tag: Option<String>) -> String {
    if let Some(tag) = env_tag {
        return tag;
    }
    match os_name {
        "Darwin" => "local-macos".to_string(),
        "Linux" => "local-linux".to_string(),
        _ => "local-unknown".to_string(),
    }
}

fn env_non_empty(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn usage_and_exit() -> ! {
    eprintln!("Usage: detect-stack [RUN_MODE]");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_mode_baseline_is_none() {
        assert_eq!(determine_sandbox_mode("baseline", None).unwrap(), None);
    }

    #[test]
    fn sandbox_mode_codex_sandbox_defaults() {
        assert_eq!(
            determine_sandbox_mode("codex-sandbox", None).unwrap(),
            Some("workspace-write".to_string())
        );
    }

    #[test]
    fn sandbox_mode_codex_full_env_override() {
        let result =
            determine_sandbox_mode("codex-full", Some("custom-profile".to_string())).unwrap();
        assert_eq!(result, Some("custom-profile".to_string()));
    }

    #[test]
    fn container_tag_defaults_by_os() {
        assert_eq!(resolve_container_tag("Darwin", None), "local-macos");
        assert_eq!(resolve_container_tag("Linux", None), "local-linux");
        assert_eq!(resolve_container_tag("Other", None), "local-unknown");
    }

    #[test]
    fn container_tag_env_override() {
        assert_eq!(
            resolve_container_tag("Darwin", Some("custom".to_string())),
            "custom"
        );
    }
}
