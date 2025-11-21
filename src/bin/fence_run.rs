use anyhow::{Context, Result, bail};
use codex_fence::{codex_present, find_repo_root, resolve_probe};
use std::env;
use std::env::VarError;
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
    let workspace_root = canonicalize_path(&repo_root);
    let workspace_plan = determine_workspace_plan(&workspace_root, args.workspace_override)?;
    let resolved_probe = resolve_probe(&workspace_root, &args.probe_name)?;
    ensure_probe_executable(&resolved_probe.path)?;

    let sandbox_mode = sandbox_mode_for_mode(&args.run_mode)?;
    let platform = detect_platform().unwrap_or_else(|| env::consts::OS.to_string());
    let command_spec = build_command_spec(&args.run_mode, &platform, &resolved_probe.path)?;

    run_command(command_spec, &args.run_mode, &sandbox_mode, &workspace_plan)?;
    Ok(())
}

struct CliArgs {
    workspace_override: Option<WorkspaceOverride>,
    run_mode: String,
    probe_name: String,
}

#[derive(Clone)]
enum WorkspaceOverride {
    UsePath(OsString),
    SkipExport,
}

struct WorkspacePlan {
    export_value: Option<OsString>,
}

struct CommandSpec {
    program: OsString,
    args: Vec<OsString>,
}

impl CliArgs {
    fn parse() -> Result<Self> {
        let mut args_iter = env::args().skip(1);
        let mut workspace_override = None;
        let mut positionals = Vec::new();

        while let Some(arg) = args_iter.next() {
            if arg.starts_with("--workspace-root=") {
                let value = arg.split_once('=').map(|(_, v)| v).unwrap_or("");
                workspace_override = Some(parse_workspace_override(value));
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
            run_mode: positionals[0].clone(),
            probe_name: positionals[1].clone(),
        })
    }
}

fn usage() -> ! {
    eprintln!(
        "Usage: fence-run [--workspace-root PATH] MODE PROBE_NAME\n\nOverrides:\n  --workspace-root PATH   Export PATH via FENCE_WORKSPACE_ROOT (defaults to repo root).\n                          Pass an empty string to defer to emit-record's git/pwd fallback.\n\nEnvironment:\n  FENCE_WORKSPACE_ROOT    When set, takes precedence over the default repo root export."
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
    if let Some(override_value) = cli_override {
        return Ok(workspace_plan_from_override(override_value));
    }

    let env_override = match env::var_os("FENCE_WORKSPACE_ROOT") {
        Some(value) if value.is_empty() => Some(WorkspaceOverride::SkipExport),
        Some(value) => Some(WorkspaceOverride::UsePath(value)),
        None => None,
    };

    if let Some(value) = env_override {
        return Ok(workspace_plan_from_override(value));
    }

    Ok(WorkspacePlan {
        export_value: Some(default_root.as_os_str().to_os_string()),
    })
}

fn workspace_plan_from_override(value: WorkspaceOverride) -> WorkspacePlan {
    match value {
        WorkspaceOverride::SkipExport => WorkspacePlan { export_value: None },
        WorkspaceOverride::UsePath(path) => WorkspacePlan {
            export_value: Some(canonicalize_os_string(&path)),
        },
    }
}

fn canonicalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn canonicalize_os_string(value: &OsString) -> OsString {
    let candidate = PathBuf::from(value);
    fs::canonicalize(&candidate)
        .unwrap_or(candidate)
        .into_os_string()
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

fn sandbox_mode_for_mode(run_mode: &str) -> Result<OsString> {
    let env_value = match env::var("FENCE_SANDBOX_MODE") {
        Ok(value) if !value.is_empty() => Some(value),
        Ok(_) => None,
        Err(VarError::NotPresent) => None,
        Err(VarError::NotUnicode(os)) => Some(os.to_string_lossy().into_owned()),
    };

    match run_mode {
        "baseline" => Ok(OsString::from("")),
        "codex-sandbox" => Ok(OsString::from(
            env_value.unwrap_or_else(|| "workspace-write".to_string()),
        )),
        "codex-full" => Ok(OsString::from(
            env_value.unwrap_or_else(|| "danger-full-access".to_string()),
        )),
        other => bail!("Unknown mode: {other}"),
    }
}

fn build_command_spec(run_mode: &str, platform: &str, probe_path: &Path) -> Result<CommandSpec> {
    let probe_arg = probe_path.as_os_str().to_os_string();
    match run_mode {
        "baseline" => Ok(CommandSpec {
            program: probe_arg,
            args: Vec::new(),
        }),
        "codex-sandbox" => {
            ensure_codex_available()?;
            let target = platform_target(platform)?;
            Ok(CommandSpec {
                program: OsString::from("codex"),
                args: vec![
                    OsString::from("sandbox"),
                    OsString::from(target),
                    OsString::from("--full-auto"),
                    OsString::from("--"),
                    probe_arg,
                ],
            })
        }
        "codex-full" => {
            ensure_codex_available()?;
            let target = platform_target(platform)?;
            Ok(CommandSpec {
                program: OsString::from("codex"),
                args: vec![
                    OsString::from("--dangerously-bypass-approvals-and-sandbox"),
                    OsString::from("sandbox"),
                    OsString::from(target),
                    OsString::from("--"),
                    probe_arg,
                ],
            })
        }
        other => bail!("Unknown mode: {other}"),
    }
}

fn platform_target(platform: &str) -> Result<&'static str> {
    match platform {
        "Darwin" => Ok("macos"),
        "Linux" => Ok("linux"),
        other => bail!("Unsupported platform for codex-sandbox: {other}"),
    }
}

fn ensure_codex_available() -> Result<()> {
    if codex_present() {
        return Ok(());
    }
    bail!(
        "codex CLI not found; codex-* modes require codex. Install codex or run baseline instead."
    )
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
    run_mode: &str,
    sandbox_mode: &OsString,
    workspace_plan: &WorkspacePlan,
) -> Result<()> {
    let mut command = Command::new(&spec.program);
    for arg in &spec.args {
        command.arg(arg);
    }
    command.env("FENCE_RUN_MODE", run_mode);
    command.env("FENCE_SANDBOX_MODE", sandbox_mode);
    if let Some(value) = workspace_plan.export_value.as_ref() {
        command.env("FENCE_WORKSPACE_ROOT", value);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn resolve_probe_prefers_probes_dir() {
        let workspace = TempWorkspace::new();
        let probes = workspace.root.join("probes");
        fs::create_dir_all(&probes).unwrap();
        let script = probes.join("example.sh");
        fs::write(&script, "#!/usr/bin/env bash\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script, perms).unwrap();
        }
        let resolved = resolve_probe(&workspace.root, "example").unwrap();
        assert!(resolved.path.ends_with("example.sh"));
    }

    #[test]
    fn workspace_override_skip_export() {
        let plan = workspace_plan_from_override(WorkspaceOverride::SkipExport);
        assert!(plan.export_value.is_none());
    }

    #[test]
    fn workspace_override_canonicalizes_path() {
        let workspace = TempWorkspace::new();
        let plan = workspace_plan_from_override(WorkspaceOverride::UsePath(
            workspace.root.join("probes").into_os_string(),
        ));
        assert!(
            plan.export_value
                .unwrap()
                .to_string_lossy()
                .contains("probes")
        );
    }

    struct TempWorkspace {
        root: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let mut base = env::temp_dir();
            let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
            base.push(format!(
                "codex-fence-test-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(&base).unwrap();
            Self { root: base }
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
