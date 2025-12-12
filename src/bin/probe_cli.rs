//! Top-level CLI wrapper that delegates to the synced helper binaries.
//!
//! The binary keeps the public `probe --matrix/--listen/--target` interface
//! stable while resolving the real helper paths (preferring the synced `bin/`
//! artifacts). It also injects `FENCE_ROOT` when possible so helpers can
//! locate probes and fixtures even when invoked from an installed location.

use anyhow::{Context, Result, bail};
use fencerunner::{
    find_repo_root, resolve_helper_binary,
    runtime::{find_on_path, helper_is_executable},
};
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse()?;
    let repo_root = find_repo_root().ok();

    run_helper(&cli, repo_root.as_deref())
}

struct Cli {
    command: CommandTarget,
    trailing_args: Vec<OsString>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandTarget {
    Matrix,
    Listen,
    Target,
}

impl CommandTarget {
    fn helper_name(self) -> &'static str {
        match self {
            CommandTarget::Matrix => "probe-matrix",
            CommandTarget::Listen => "probe-listen",
            CommandTarget::Target => "probe-target",
        }
    }
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();

        let Some(flag) = args.next() else {
            usage(1);
        };

        let flag_str = flag
            .to_str()
            .with_context(|| "Invalid UTF-8 in command flag")?;

        let command = match flag_str {
            "--matrix" | "-m" => CommandTarget::Matrix,
            "--listen" | "-l" => CommandTarget::Listen,
            "--target" | "-t" => CommandTarget::Target,
            "--help" | "-h" => usage(0),
            _ => usage(1),
        };

        let trailing_args = args.collect();
        Ok(Self {
            command,
            trailing_args,
        })
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: probe (--matrix | --listen | --target) [args]\n\nCommands:\n  --matrix, -m   Run the full probe matrix once and emit boundary records (NDJSON).\n  --listen, -l   Read boundary-object JSON from stdin and print a human summary.\n  --target, -t   Run a targeted probe subset (see probe-target --help).\n\nExamples:\n  probe --matrix | probe --listen\n  probe --target --probe fs_read_workspace_readme --mode baseline"
    );
    std::process::exit(code);
}

/// Locate the requested helper, preferring the repo-synced binaries.
///
/// The search order mirrors the harness contract: repo root, sibling directory
/// to the current executable (useful for installed binaries), then PATH.
fn resolve_helper(name: &str, repo_root: Option<&Path>) -> Result<PathBuf> {
    if let Some(root) = repo_root {
        if let Ok(path) = resolve_helper_binary(root, name) {
            return Ok(path);
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let candidate = dir.join(name);
            if helper_is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }

    if let Some(path) = find_on_path(name) {
        return Ok(path);
    }

    bail!(
        "Unable to locate helper '{name}'. Run 'make build' (or tools/sync_bin_helpers.sh) or set FENCE_ROOT."
    )
}

/// Execute the resolved helper, wiring FENCE_ROOT when available.
fn run_helper(cli: &Cli, repo_root: Option<&Path>) -> Result<()> {
    let helper_path = resolve_helper(cli.command.helper_name(), repo_root)?;
    let mut command = Command::new(&helper_path);
    command.args(&cli.trailing_args);

    if let Some(root) = repo_root {
        if env::var_os("FENCE_ROOT").is_none() {
            command.env("FENCE_ROOT", root);
        }
    }

    let status = command
        .status()
        .with_context(|| format!("Failed to execute {}", helper_path.display()))?;

    if status.success() {
        return Ok(());
    }

    if let Some(code) = status.code() {
        std::process::exit(code);
    }

    bail!("Helper terminated by signal")
}
