//! Entrypoint for the probe contract gate used by the test suite.
//!
//! Invokes `tools/validate_contract_gate.sh` from the detected repo root and
//! proxies its exit status so CI and local workflows can rely on a single Rust
//! binary instead of the shell shim. This keeps `cargo test` wired to the same
//! gate that `bin/probe-contract-gate` exposes for CLI callers.

use anyhow::{Context, Result, anyhow};
use fencerunner::repo_tools::find_repo_root;
use std::env;
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let repo_root = find_repo_root()?;
    let mut args: Vec<String> = env::args().skip(1).collect();
    let has_probe_flag = args.iter().any(|arg| arg == "--probe");
    let mut script_args: Vec<String> = Vec::new();

    if !args.is_empty() && !args[0].starts_with('-') {
        // Allow "probe-gate <id>" as a shorthand for "--probe <id>".
        if has_probe_flag {
            return Err(anyhow!(
                "cannot mix positional probe id with --probe flag; remove one"
            ));
        }
        let probe = args.remove(0);
        script_args.push("--probe".to_string());
        script_args.push(probe);
    }

    script_args.extend(args.into_iter());
    let script = repo_root.join("tools/validate_contract_gate.sh");
    let mut cmd = Command::new(&script);
    cmd.current_dir(&repo_root)
        // Force the script path so the gate logic is consistent with CI.
        .env("FENCE_TEST_FORCE_SCRIPT", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    cmd.args(&script_args);
    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {}", script.display()))?;

    match status.code() {
        Some(0) => Ok(()),
        Some(code) => std::process::exit(code),
        None => {
            eprintln!("probe contract gate terminated by signal");
            std::process::exit(1);
        }
    }
}
