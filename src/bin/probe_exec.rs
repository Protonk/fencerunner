//! Executes a probe while enforcing workspace rules.
//!
//! Responsibilities:
//! - resolve probes strictly within `probes/`
//! - export probe-facing environment expected by probe scripts and `emit-record`
//! - honor workspace overrides without silently falling back to host defaults

use anyhow::{Context, Result, bail};
use fencerunner::connectors::{CommandSpec, RunMode, plan_for_mode};
use fencerunner::fence_run_support::{
    WorkspaceOverride, WorkspacePlan, canonicalize_path, resolve_probe_metadata,
    workspace_plan_from_override, workspace_tmpdir_plan,
};
use fencerunner::{
    ProbeMetadata, find_repo_root, resolve_boundary_schema_path, resolve_catalog_path,
    resolve_probe,
};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = CliArgs::parse()?;
    let repo_root = find_repo_root()?;
    let catalog_path = resolve_catalog_path(&repo_root, args.catalog_path.as_deref());
    let boundary_path = resolve_boundary_schema_path(&repo_root, args.boundary_path.as_deref())?;
    let workspace_root = canonicalize_path(&repo_root);
    let workspace_plan = determine_workspace_plan(&workspace_root, args.workspace_override)?;
    let resolved_probe = resolve_probe(&workspace_root, &args.probe_name)?;
    let parsed_metadata = ProbeMetadata::from_script(&resolved_probe.path)?;
    let _resolved_metadata = resolve_probe_metadata(&resolved_probe, parsed_metadata)?;
    ensure_probe_executable(&resolved_probe.path)?;
    let workspace_tmpdir = workspace_tmpdir_plan(&workspace_plan, &workspace_root);
    let command_cwd = command_cwd_for(&workspace_plan, &workspace_root);

    let platform = detect_platform().unwrap_or_else(|| env::consts::OS.to_string());
    let mode_plan = plan_for_mode(&args.run_mode, &platform, &resolved_probe.path, None)?;

    run_command(
        mode_plan.command,
        &mode_plan.run_mode,
        &mode_plan.sandbox_env,
        &workspace_plan,
        workspace_tmpdir.path.as_deref(),
        &command_cwd,
        &catalog_path,
        &boundary_path,
    )?;
    Ok(())
}

struct CliArgs {
    workspace_override: Option<WorkspaceOverride>,
    catalog_path: Option<PathBuf>,
    boundary_path: Option<PathBuf>,
    run_mode: String,
    probe_name: String,
}

impl CliArgs {
    fn parse() -> Result<Self> {
        let mut args_iter = env::args().skip(1);
        let mut workspace_override = None;
        let mut catalog_path = None;
        let mut boundary_path = None;
        let mut positionals = Vec::new();

        while let Some(arg) = args_iter.next() {
            if arg.starts_with("--workspace-root=") {
                let value = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                workspace_override = Some(parse_workspace_override(value));
                continue;
            }
            if arg.starts_with("--catalog=") {
                let value = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                catalog_path = Some(PathBuf::from(value));
                continue;
            }
            if arg.starts_with("--boundary=") {
                let value = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                boundary_path = Some(PathBuf::from(value));
                continue;
            }

            match arg.as_str() {
                "--workspace-root" => {
                    let value = args_iter.next().unwrap_or_else(|| {
                        eprintln!("Missing path for --workspace-root");
                        usage();
                    });
                    workspace_override = Some(parse_workspace_override(&value));
                }
                "--catalog" => {
                    let value = args_iter.next().unwrap_or_else(|| {
                        eprintln!("Missing path for --catalog");
                        usage();
                    });
                    catalog_path = Some(PathBuf::from(value));
                }
                "--boundary" => {
                    let value = args_iter.next().unwrap_or_else(|| {
                        eprintln!("Missing path for --boundary");
                        usage();
                    });
                    boundary_path = Some(PathBuf::from(value));
                }
                "-h" | "--help" => usage(),
                _ if arg.starts_with("--") => {
                    eprintln!("Unknown option: {arg}");
                    usage();
                }
                _ => {
                    positionals.push(arg);
                    positionals.extend(args_iter);
                    break;
                }
            }
        }

        if positionals.len() != 2 {
            usage();
        }

        Ok(Self {
            workspace_override,
            catalog_path,
            boundary_path,
            run_mode: positionals[0].clone(),
            probe_name: positionals[1].clone(),
        })
    }
}

fn usage() -> ! {
    eprintln!(
        "Usage: probe-exec [--workspace-root PATH] [--catalog PATH] [--boundary PATH] MODE PROBE_NAME\n\nOverrides:\n  --workspace-root PATH     Export PATH via FENCE_WORKSPACE_ROOT (defaults to repo root).\n                            Pass an empty string to defer to emit-record's git/pwd fallback.\n  --catalog PATH            Override capability catalog path (or set CATALOG_PATH).\n  --boundary PATH           Override boundary-object schema path (or set BOUNDARY_PATH).\n\nEnvironment:\n  FENCE_WORKSPACE_ROOT      When set, takes precedence over the default repo root export."
    );
    std::process::exit(1);
}

fn parse_workspace_override(value: &str) -> WorkspaceOverride {
    if value.is_empty() {
        WorkspaceOverride::SkipExport
    } else {
        WorkspaceOverride::UsePath(OsString::from(value))
    }
}

fn determine_workspace_plan(
    default_root: &Path,
    cli_override: Option<WorkspaceOverride>,
) -> Result<WorkspacePlan> {
    // CLI override wins; otherwise honor FENCE_WORKSPACE_ROOT if set, and only
    // then fall back to the repo root.
    if let Some(override_value) = cli_override {
        return Ok(workspace_plan_from_override(override_value));
    }

    let env_override = ["FENCE_WORKSPACE_ROOT"]
        .iter()
        .find_map(|key| match env::var_os(key) {
            Some(value) if value.is_empty() => Some(WorkspaceOverride::SkipExport),
            Some(value) => Some(WorkspaceOverride::UsePath(value)),
            None => None,
        });

    if let Some(value) = env_override {
        return Ok(workspace_plan_from_override(value));
    }

    Ok(WorkspacePlan {
        export_value: Some(default_root.as_os_str().to_os_string()),
    })
}

/// Pick the working directory for probe execution. Prefer the exported workspace
/// root so external sandbox profiles align with the trusted tree, otherwise fall
/// back to the repository root.
fn command_cwd_for(plan: &WorkspacePlan, default_root: &Path) -> PathBuf {
    if let Some(value) = plan.export_value.as_ref() {
        return PathBuf::from(value);
    }
    env::current_dir().unwrap_or_else(|_| default_root.to_path_buf())
}

fn ensure_probe_executable(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Probe not found or not executable: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("Probe not found or not executable: {}", path.display());
    }
    if !has_execute_bit(&metadata) {
        bail!("Probe is not executable: {}", path.display());
    }
    Ok(())
}

fn has_execute_bit(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

fn detect_platform() -> Option<String> {
    let output = Command::new("uname")
        .arg("-s")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn run_command(
    spec: CommandSpec,
    run_mode: &RunMode,
    sandbox_mode: &OsString,
    workspace_plan: &WorkspacePlan,
    workspace_tmpdir: Option<&Path>,
    command_cwd: &Path,
    catalog_path: &Path,
    boundary_path: &Path,
) -> Result<()> {
    let mut command = Command::new(&spec.program);
    for arg in &spec.args {
        command.arg(arg);
    }
    command.current_dir(command_cwd);
    command.env("FENCE_RUN_MODE", run_mode.as_str());
    command.env("FENCE_SANDBOX_MODE", sandbox_mode);
    command.env("CATALOG_PATH", catalog_path);
    command.env("BOUNDARY_PATH", boundary_path);
    if let Some(value) = workspace_plan.export_value.as_ref() {
        command.env("FENCE_WORKSPACE_ROOT", value);
    }
    if let Some(tmpdir) = workspace_tmpdir {
        command.env("TMPDIR", tmpdir);
    }

    let status = command
        .status()
        .with_context(|| format!("Failed to execute {}", spec.program.to_string_lossy()))?;
    if !status.success() {
        if let Some(code) = status.code() {
            std::process::exit(code);
        } else {
            bail!("Probe terminated by signal");
        }
    }
    Ok(())
}
