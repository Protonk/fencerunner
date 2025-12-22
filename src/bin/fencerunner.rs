//! Top-level CLI wrapper that delegates to the synced helper binaries.
//!
//! The binary keeps the public `fencerunner --bang/--bundle/--probe/--listen`
//! interface stable while resolving the real helper paths (preferring the
//! synced `bin/` artifacts). It also injects `FENCE_ROOT` when possible so
//! helpers can locate probes and fixtures even when invoked from an installed
//! location.

use anyhow::{Context, Result, anyhow, bail};
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
    RunMatrix,
    Listen,
}

impl CommandTarget {
    fn helper_name(self) -> &'static str {
        match self {
            CommandTarget::RunMatrix => "probe-matrix",
            CommandTarget::Listen => "probe-listen",
        }
    }
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args_os();
        let _program = args.next();
    let mut command: Option<CommandTarget> = None;
    let mut trailing_args: Vec<OsString> = Vec::new();

    while let Some(arg) = args.next() {
        let arg_str = arg
            .to_str()
            .ok_or_else(|| anyhow!("Invalid UTF-8 in argument"))?;
        match arg_str {
            "--bang" => {
                command.get_or_insert(CommandTarget::RunMatrix);
                trailing_args.push(arg);
            }
            "--bundle" | "--probe" | "--catalog" => {
                command.get_or_insert(CommandTarget::RunMatrix);
                trailing_args.push(arg.clone());
                let Some(value) = args.next() else {
                    bail!("{arg_str} requires a value");
                };
                trailing_args.push(value);
            }
            "--listen" | "-l" => {
                if command.is_some() {
                    bail!("--listen cannot be combined with other commands or flags");
                }
                command = Some(CommandTarget::Listen);
                trailing_args.push(arg);
                // Consume any trailing args to surface a clear error below.
                while let Some(extra) = args.next() {
                    bail!("--listen takes no additional flags or arguments (saw {extra:?})");
                }
            }
            "--help" | "-h" => usage(0),
            other => bail!("unknown argument: {other}"),
        }
    }

        let command = command.unwrap_or_else(|| usage(1));
        Ok(Self {
            command,
            trailing_args,
        })
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: fencerunner (--bang | --bundle <capability-id> | --probe <probe-id> | --listen) [args]\n\nCommands:\n  --bang               Run every probe once and emit boundary records (NDJSON).\n  --bundle <cap-id>    Run probes whose primary capability matches <cap-id>.\n  --probe <probe-id>   Run a single probe by id.\n  --listen, -l         Read boundary-object JSON from stdin and print a human summary.\n\nOptions:\n  --catalog <path>     Override capability catalog path (or set CATALOG_PATH).\n\nExamples:\n  fencerunner --bang | fencerunner --listen\n  fencerunner --probe fs_read_workspace_readme\n  fencerunner --bundle cap_fs_read_workspace_tree"
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
