//! Entrypoint for the static probe contract gate.
//!
//! Invokes `tools/validate_contract_gate.sh` from the detected repo root and
//! proxies its exit status so CI and local workflows can rely on a single Rust
//! binary instead of the shell shim.

use anyhow::{Context, Result};
use codex_fence::find_repo_root;
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let repo_root = find_repo_root()?;
    let script = repo_root.join("tools/validate_contract_gate.sh");
    let status = Command::new(&script)
        .current_dir(&repo_root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to execute {}", script.display()))?;

    match status.code() {
        Some(0) => Ok(()),
        Some(code) => std::process::exit(code),
        None => {
            eprintln!("static probe contract terminated by signal");
            std::process::exit(1);
        }
    }
}
