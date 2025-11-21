//! Executes a probe in the requested run mode while enforcing workspace rules.
//!
//! Responsibilities:
//! - resolve probes strictly within `probes/`
//! - export `FENCE_*` environment expected by probe scripts and `emit-record`
//! - wrap Codex sandbox/full invocations when requested
//! - honor workspace overrides without silently falling back to host defaults

use anyhow::{Context, Result, bail};
use codex_fence::{codex_present, find_repo_root, resolve_probe};
use serde_json::json;
use std::env;
use std::env::VarError;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;

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
    let workspace_tmpdir = workspace_tmpdir(&workspace_root);

    let sandbox_mode = sandbox_mode_for_mode(&args.run_mode)?;
    let platform = detect_platform().unwrap_or_else(|| env::consts::OS.to_string());
    let command_spec = build_command_spec(&args.run_mode, &platform, &resolved_probe.path)?;

    if codex_mode(&args.run_mode) {
        if let Some(tmpdir) = workspace_tmpdir.as_ref() {
            if run_codex_preflight(
                &repo_root,
                &args.run_mode,
                &platform,
                tmpdir,
                &resolved_probe.path,
            )? {
                // Preflight emitted a denial record; skip running the probe.
                return Ok(());
            }
        }
    }

    run_command(
        command_spec,
        &args.run_mode,
        &sandbox_mode,
        &workspace_plan,
        workspace_tmpdir.as_deref(),
    )?;
    Ok(())
}

struct CliArgs {
    workspace_override: Option<WorkspaceOverride>,
    run_mode: String,
    probe_name: String,
}

#[derive(Clone)]
/// How the workspace root should be exported to the probe.
enum WorkspaceOverride {
    UsePath(OsString),
    SkipExport,
}

/// Finalized workspace export plan after considering CLI/env overrides.
struct WorkspacePlan {
    export_value: Option<OsString>,
}

/// Program and arguments used to execute the probe for a given mode.
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
    // CLI override wins; otherwise honor FENCE_WORKSPACE_ROOT if set, and only
    // then fall back to the repo root.
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

/// Prefer a workspace-scoped tmp dir so probes land temp files inside the
/// allowed tree even when system defaults are blocked.
fn workspace_tmpdir(workspace_root: &Path) -> Option<PathBuf> {
    let candidate = workspace_root.join("tmp");
    if fs::create_dir_all(&candidate).is_ok() {
        Some(canonicalize_path(&candidate))
    } else {
        None
    }
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

fn codex_mode(run_mode: &str) -> bool {
    matches!(run_mode, "codex-sandbox" | "codex-full")
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
    workspace_tmpdir: Option<&Path>,
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

fn classify_preflight_error(stderr: &str) -> (&'static str, Option<&'static str>, String) {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("operation not permitted") {
        ("denied", Some("EPERM"), "codex sandbox preflight denied (operation not permitted)".to_string())
    } else if lower.contains("permission denied") {
        ("denied", Some("EACCES"), "codex sandbox preflight denied (permission denied)".to_string())
    } else {
        ("error", None, "codex sandbox preflight failed".to_string())
    }
}

fn extract_probe_var(path: &Path, var: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(var) {
            continue;
        }
        if let Some(rest) = trimmed.splitn(2, '=').nth(1) {
            let val = rest
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(|c| c == '"' || c == '\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn write_temp_payload(value: &serde_json::Value) -> Result<PathBuf> {
    let mut file = NamedTempFile::new().context("create payload temp file")?;
    serde_json::to_writer(&mut file, value)?;
    let (_file, path) = file.keep().context("persist payload temp file")?;
    Ok(path)
}

fn emit_preflight_record(
    repo_root: &Path,
    probe_path: &Path,
    run_mode: &str,
    target_path: &Path,
    exit_code: i32,
    stderr: &str,
) -> Result<()> {
    let emit_record = codex_fence::resolve_helper_binary(repo_root, "emit-record")?;
    let probe_file = probe_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let probe_id = probe_file.trim_end_matches(".sh");
    let primary_capability = extract_probe_var(probe_path, "primary_capability_id")
        .unwrap_or_else(|| "cap_fs_read_workspace_tree".to_string());
    let probe_version = extract_probe_var(probe_path, "probe_version").unwrap_or_else(|| "1".to_string());
    let (status, errno, message) = classify_preflight_error(stderr);

    let command_str = format!(
        "codex {} mktemp -d {}",
        run_mode,
        target_path.to_string_lossy()
    );

    let payload = json!({
        "stdout_snippet": "",
        "stderr_snippet": stderr,
        "raw": {
            "preflight_target": target_path.to_string_lossy(),
            "preflight_kind": "codex_tmp",
            "stderr": stderr,
            "exit_code": exit_code
        }
    });

    let operation_args = json!({
        "preflight": true,
        "target_path": target_path.to_string_lossy(),
        "run_mode": run_mode
    });

    let payload_file = write_temp_payload(&payload)?;

    let mut cmd = Command::new(&emit_record);
    cmd.arg("--run-mode")
        .arg(run_mode)
        .arg("--probe-name")
        .arg(probe_id)
        .arg("--probe-version")
        .arg(probe_version)
        .arg("--primary-capability-id")
        .arg(primary_capability)
        .arg("--command")
        .arg(&command_str)
        .arg("--category")
        .arg("preflight")
        .arg("--verb")
        .arg("mktemp")
        .arg("--target")
        .arg(target_path.to_string_lossy().to_string())
        .arg("--status")
        .arg(status)
        .arg("--message")
        .arg(&message)
        .arg("--raw-exit-code")
        .arg(exit_code.to_string())
        .arg("--operation-args")
        .arg(operation_args.to_string())
        .arg("--payload-file")
        .arg(payload_file);

    if let Some(errno_val) = errno {
        cmd.arg("--errno").arg(errno_val);
    } else {
        cmd.arg("--errno").arg("");
    }

    let status_out = cmd.status().context("failed to emit preflight record")?;
    if !status_out.success() {
        bail!("emit-record failed for preflight (exit {:?})", status_out.code());
    }

    Ok(())
}

fn run_codex_preflight(
    repo_root: &Path,
    run_mode: &str,
    platform: &str,
    workspace_tmpdir: &Path,
    probe_path: &Path,
) -> Result<bool> {
    // Detect hosts that block codex sandbox writes before invoking the probe.
    // When blocked, emit a boundary object describing the denial so matrix runs
    // keep producing output for the affected mode.
    ensure_codex_available()?;
    let target = workspace_tmpdir.join("codex-preflight.XXXXXX");
    let platform_target = platform_target(platform)?;

    let mut args: Vec<OsString> = Vec::new();
    match run_mode {
        "codex-sandbox" => {
            args.push(OsString::from("sandbox"));
            args.push(OsString::from(platform_target));
            args.push(OsString::from("--full-auto"));
        }
        "codex-full" => {
            args.push(OsString::from("--dangerously-bypass-approvals-and-sandbox"));
            args.push(OsString::from("sandbox"));
            args.push(OsString::from(platform_target));
        }
        _ => return Ok(false),
    }
    args.push(OsString::from("--"));
    args.push(OsString::from("/usr/bin/mktemp"));
    args.push(OsString::from("-d"));
    args.push(OsString::from(
        target.as_os_str().to_string_lossy().to_string(),
    ));

    let mut cmd = Command::new("codex");
    cmd.args(&args);
    let output = cmd.output().context("codex preflight failed to spawn")?;

    if output.status.success() {
        return Ok(false);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    emit_preflight_record(repo_root, probe_path, run_mode, &target, code, &stderr)?;
    Ok(true)
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

    #[test]
    fn workspace_tmpdir_prefers_workspace_tree() {
        let workspace = TempWorkspace::new();
        let canonical_root = canonicalize_path(&workspace.root);
        let tmpdir = workspace_tmpdir(&canonical_root).expect("tmpdir");
        assert!(tmpdir.starts_with(&canonical_root));
        assert!(tmpdir.ends_with("tmp"));
        assert!(tmpdir.is_dir());
    }

    #[test]
    fn classify_preflight_recognizes_permission_denied() {
        let (status, errno, message) =
            classify_preflight_error("mktemp: Operation not permitted\n");
        assert_eq!(status, "denied");
        assert_eq!(errno, Some("EPERM"));
        assert!(message.contains("preflight"));
    }

    #[test]
    fn classify_preflight_defaults_to_error() {
        let (status, errno, _) = classify_preflight_error("unexpected failure");
        assert_eq!(status, "error");
        assert!(errno.is_none());
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
