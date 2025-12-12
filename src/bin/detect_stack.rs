//! Collects host/sandbox metadata for inclusion in boundary objects.
//!
//! The binary is intentionally dependency-free and lightweight because probes
//! invoke it for every record. It reflects the current run mode (from CLI or
//! env), captures any sandbox override, and emits a JSON `StackInfo` snapshot.

use anyhow::Result;
use fencerunner::connectors::RunMode;
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
    let run_mode_raw = match cli_run_mode {
        Some(mode) => mode,
        None => env_non_empty_any(&["FENCE_RUN_MODE"]).unwrap_or_else(|| usage_and_exit()),
    };

    let _run_mode = RunMode::try_from(run_mode_raw.as_str())?;
    let sandbox_mode = env_non_empty("FENCE_SANDBOX_MODE");
    let os_info = detect_uname(&["-srm"]).unwrap_or_else(|| fallback_os_info());

    let info = StackInfo {
        sandbox_mode,
        os: os_info,
    };

    println!("{}", serde_json::to_string(&info)?);
    Ok(())
}

#[derive(Serialize)]
struct StackInfo {
    sandbox_mode: Option<String>,
    os: String,
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

fn env_non_empty(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn env_non_empty_any(names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(value) = env_non_empty(name) {
            return Some(value);
        }
    }
    None
}

fn usage_and_exit() -> ! {
    eprintln!("Usage: detect-stack [RUN_MODE]");
    std::process::exit(1);
}
