//! Contracted script runner.
//!
//! Public CLI:
//!   fencerunner [--strict|--supervised] <RUN_DIR>...
//!
//! In strict mode (default), contract breaks are failures (non-zero exit).
//! In supervised mode, contract breaks are converted into synthetic boundary
//! records so stdout remains well-formed NDJSON; supervised exits 0 unless
//! preflight or the runner itself fails.

use anyhow::{Context, Result, anyhow, bail};
use fencerunner::boundary::{
    BoundaryObject, CommitmentEnrollment, ContextInfo, OperationInfo, ResultDetails, ResultInfo,
    RunInfo, ScriptInfo,
};
use fencerunner::commands;
use fencerunner::commitments::model::CommitmentHelp;
use fencerunner::harness::run_dir_plan::{RunDirPlan, plan_scripts, preflight_run_dirs};
use fencerunner::harness::runner_root::RunnerRoot;
use fencerunner::scripts::discovery::Script;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::Builder as TempBuilder;

fn main() {
    if let Err(err) = dispatch() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn dispatch() -> Result<()> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    if args.is_empty() {
        usage(1);
    }

    if let Some(first) = args
        .first()
        .and_then(|arg| arg.to_str())
        .filter(|s| s.starts_with("__"))
    {
        match first {
            "__emit-record" => return commands::emit_record::run(&args[1..]),
            "__commit-help-me" => return commands::commit_help_me::run(&args[1..]),
            "__schema-validate" => return commands::schema_validate::run(&args[1..]),
            other => bail!("unknown internal subcommand: {other}"),
        }
    }

    let cli = Cli::parse(&args)?;
    run_fencerunner(&cli)
}

struct Cli {
    mode: RunMode,
    run_dirs: Vec<OsString>,
}

impl Cli {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut mode = RunMode::Strict;
        let mut explicit_mode: Option<RunMode> = None;
        let mut run_dirs: Vec<OsString> = Vec::new();

        // Parse a minimal set of flags and treat everything else as a run dir.
        for arg in args {
            let arg_str = arg
                .to_str()
                .ok_or_else(|| anyhow!("Invalid UTF-8 in argument"))?;
            match arg_str {
                "--help" | "-h" => usage(0),
                "--version" | "-V" => {
                    println!("{}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                "--strict" => {
                    if explicit_mode == Some(RunMode::Supervised) {
                        bail!("cannot combine --strict and --supervised");
                    }
                    explicit_mode = Some(RunMode::Strict);
                    mode = RunMode::Strict;
                }
                "--supervised" => {
                    if explicit_mode == Some(RunMode::Strict) {
                        bail!("cannot combine --strict and --supervised");
                    }
                    explicit_mode = Some(RunMode::Supervised);
                    mode = RunMode::Supervised;
                }
                other if other.starts_with('-') => bail!("unknown argument: {other}"),
                _ => run_dirs.push(arg.clone()),
            }
        }

        if run_dirs.is_empty() {
            usage(1);
        }

        Ok(Self { mode, run_dirs })
    }
}

fn usage(code: i32) -> ! {
    eprintln!(
        "Usage: fencerunner [--strict|--supervised] <RUN_DIR>...\n\nRuns every *.sh script inside each run directory (flat; non-recursive) and streams boundary objects as NDJSON.\n\nModes:\n  --strict        Treat contract breaks as failures (default).\n  --supervised    Emit synthetic boundary records on contract breaks; exit 0 unless preflight/runner fails.\n\nOptions:\n  -h, --help      Show this help text.\n  -V, --version   Show the version.\n\nExamples:\n  fencerunner scripts\n  fencerunner ./scripts /tmp/other-run-dir\n  fencerunner --supervised scripts"
    );
    std::process::exit(code);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunMode {
    Strict,
    Supervised,
}

const ENV_RUN_DIR: &str = "FENCERUNNER_RUN_DIR";
const ENV_ENROLLMENTS_PATH: &str = "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH";
const ENV_FENCERUNNER_BIN: &str = "FENCERUNNER_BIN";

fn run_fencerunner(cli: &Cli) -> Result<()> {
    let run_dir_paths: Vec<PathBuf> = cli.run_dirs.iter().map(PathBuf::from).collect();
    // Preflight validates run-dir contracts before any scripts execute.
    let run_dirs = preflight_run_dirs(&run_dir_paths)?;
    // Script planning enforces global id uniqueness and stable ordering.
    let execution_plan = plan_scripts(&run_dirs)?;
    let total = execution_plan.len();

    let runner_root = RunnerRoot::create()?;
    let fencerunner_bin = env::current_exe().context("resolving fencerunner binary path")?;
    let scratch_dir = tempfile::Builder::new()
        .prefix("fencerunner-run.")
        .tempdir()
        .context("allocating scratch dir")?;

    if cli.mode == RunMode::Supervised {
        eprintln!(
            "fencerunner: supervised: running {total} script(s) across {} run dir(s)",
            run_dirs.len()
        );
    }

    let mut errors: Vec<String> = Vec::new();
    for (idx, (run_dir_idx, script)) in execution_plan.iter().enumerate() {
        let run_dir = &run_dirs[*run_dir_idx];
        match cli.mode {
            RunMode::Strict => {
                if let Err(err) = run_script_strict(
                    &runner_root,
                    &fencerunner_bin,
                    scratch_dir.path(),
                    run_dir,
                    script,
                ) {
                    let message = format!("script {} failed: {err:#}", script.id);
                    eprintln!("fencerunner: {message}");
                    errors.push(message);
                }
            }
            RunMode::Supervised => {
                eprintln!("fencerunner: [{}/{}] {}", idx + 1, total, script.id);
                run_script_supervised(
                    &runner_root,
                    &fencerunner_bin,
                    scratch_dir.path(),
                    run_dir,
                    script,
                )?;
            }
        }
    }

    if cli.mode == RunMode::Strict && !errors.is_empty() {
        bail!(
            "{} script(s) failed; see stderr for details:\n{}",
            errors.len(),
            errors.join("\n")
        );
    }

    Ok(())
}

fn run_script_strict(
    runner_root: &RunnerRoot,
    fencerunner_bin: &Path,
    scratch_dir: &Path,
    run_dir: &RunDirPlan,
    script: &Script,
) -> Result<()> {
    ensure_script_executable(&script.path)?;
    // commit-help-me writes enrollments here; emit-record reads them back.
    let enrollments_file = TempBuilder::new()
        .prefix("fencerunner-commitments.")
        .tempfile_in(scratch_dir)
        .context("allocating commitment enrollment file")?;
    let output = run_script_command(
        runner_root,
        fencerunner_bin,
        scratch_dir,
        run_dir,
        script,
        enrollments_file.path(),
    )?;

    if !output.stderr.is_empty() {
        io::stderr()
            .lock()
            .write_all(&output.stderr)
            .context("forward script stderr")?;
    }

    if run_dir.enforce_stderr_empty && !output.stderr.is_empty() {
        bail!("script wrote to stderr but gates.json enforces stderr.empty");
    }

    if !output.status.success() {
        let code = output.status.code().unwrap_or(-1);
        bail!("script returned non-zero exit code {code}");
    }

    let value: Value = serde_json::from_slice(&output.stdout)
        .with_context(|| "failed to parse boundary object from script stdout")?;
    run_dir
        .boundary
        .validate_record(&value)
        .context("script emitted a record that violates boundaries.json")?;

    ensure_record_script_id_matches_file(script, &value)?;

    println!("{}", serde_json::to_string(&value)?);
    Ok(())
}

fn run_script_supervised(
    runner_root: &RunnerRoot,
    fencerunner_bin: &Path,
    scratch_dir: &Path,
    run_dir: &RunDirPlan,
    script: &Script,
) -> Result<()> {
    ensure_script_executable(&script.path)?;
    // commit-help-me writes enrollments here; synthetic records read it back.
    let enrollments_file = TempBuilder::new()
        .prefix("fencerunner-commitments.")
        .tempfile_in(scratch_dir)
        .context("allocating commitment enrollment file")?;
    let output = run_script_command(
        runner_root,
        fencerunner_bin,
        scratch_dir,
        run_dir,
        script,
        enrollments_file.path(),
    )?;
    let enrollments = load_commitment_enrollments(enrollments_file.path()).unwrap_or_else(|err| {
        eprintln!(
            "fencerunner: script {}: failed to parse commitment enrollments: {err:#}",
            script.id
        );
        Vec::new()
    });

    let status_code = output.status.code();

    if !output.stderr.is_empty() {
        // Preserve script stderr for diagnostics without treating it as output.
        eprintln!("fencerunner: script {} stderr:", script.id);
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        if !output.stderr.ends_with(b"\n") {
            eprintln!();
        }
    }

    if run_dir.enforce_stderr_empty && !output.stderr.is_empty() {
        emit_synthetic(
            script,
            run_dir,
            enrollments,
            &output.stdout,
            &output.stderr,
            status_code,
            "script wrote to stderr but gates.json enforces stderr.empty".to_string(),
        )?;
        return Ok(());
    }

    // Whitespace-only stdout counts as "no record emitted."
    let stdout_is_empty = output.stdout.iter().all(|byte| byte.is_ascii_whitespace());

    if output.status.success() && !stdout_is_empty {
        match serde_json::from_slice::<Value>(&output.stdout) {
            Ok(value) if value.is_object() => {
                if let Err(err) = run_dir.boundary.validate_record(&value) {
                    emit_synthetic(
                        script,
                        run_dir,
                        enrollments,
                        &output.stdout,
                        &output.stderr,
                        status_code,
                        format!("{err:#}"),
                    )?;
                    return Ok(());
                }

                if let Err(err) = ensure_record_script_id_matches_file(script, &value) {
                    emit_synthetic(
                        script,
                        run_dir,
                        enrollments,
                        &output.stdout,
                        &output.stderr,
                        status_code,
                        format!("{err:#}"),
                    )?;
                    return Ok(());
                }

                let outcome = extract_outcome(&value).unwrap_or_else(|| "unknown".to_string());
                println!("{}", serde_json::to_string(&value)?);
                eprintln!("fencerunner: script {} -> {}", script.id, outcome);
                return Ok(());
            }
            Ok(_) => {
                emit_synthetic(
                    script,
                    run_dir,
                    enrollments,
                    &output.stdout,
                    &output.stderr,
                    status_code,
                    "script emitted non-object JSON to stdout".to_string(),
                )?;
                return Ok(());
            }
            Err(err) => {
                emit_synthetic(
                    script,
                    run_dir,
                    enrollments,
                    &output.stdout,
                    &output.stderr,
                    status_code,
                    format!("script emitted invalid JSON: {err}"),
                )?;
                return Ok(());
            }
        }
    }

    let reason = if !output.status.success() {
        match status_code {
            Some(code) => format!("script exited non-zero ({code})"),
            None => "script terminated by signal".to_string(),
        }
    } else {
        "script emitted no boundary object on stdout".to_string()
    };

    emit_synthetic(
        script,
        run_dir,
        enrollments,
        &output.stdout,
        &output.stderr,
        status_code,
        reason,
    )?;
    Ok(())
}

fn run_script_command(
    runner_root: &RunnerRoot,
    fencerunner_bin: &Path,
    scratch_dir: &Path,
    run_dir: &RunDirPlan,
    script: &Script,
    enrollments_path: &Path,
) -> Result<std::process::Output> {
    let mut command = Command::new(&script.path);
    command
        // Run scripts from the run dir so relative paths stay local.
        .current_dir(&run_dir.path)
        .env(ENV_RUN_DIR, &run_dir.path)
        .env(ENV_ENROLLMENTS_PATH, enrollments_path.as_os_str())
        .env(ENV_FENCERUNNER_BIN, fencerunner_bin)
        .env("FENCERUNNER_ROOT", runner_root.path())
        .env("TMPDIR", scratch_dir);

    let mut path_entries = vec![runner_root.bin_dir().to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        path_entries.extend(env::split_paths(&existing));
    }
    command.env(
        // Runner shims come first so scripts always hit the right helpers.
        "PATH",
        env::join_paths(path_entries).context("joining PATH entries")?,
    );

    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to execute {}", script.path.display()))
}

fn extract_outcome(value: &Value) -> Option<String> {
    value
        .get("result")
        .and_then(|result| result.get("outcome"))
        .and_then(|outcome| outcome.as_str())
        .map(|s| s.to_string())
}

fn emit_synthetic(
    script: &Script,
    run_dir: &RunDirPlan,
    commitments: Vec<CommitmentEnrollment>,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
    status_code: Option<i32>,
    reason: String,
) -> Result<()> {
    let details = ResultDetails {
        exit_code: status_code.map(|code| code as i64),
        message: Some(reason.clone()),
        error_detail: Some(reason.clone()),
        ..ResultDetails::default()
    };

    let synthetic = synthetic_boundary_object(
        script,
        &run_dir.path,
        commitments,
        stdout_bytes,
        stderr_bytes,
        details,
    );
    let value = serde_json::to_value(&synthetic)?;
    if let Err(err) = run_dir.boundary.validate_record(&value) {
        eprintln!(
            "fencerunner: synthetic record for script {} violates boundaries.json: {err:#}",
            script.id
        );
    }

    println!("{}", serde_json::to_string(&synthetic)?);
    eprintln!("fencerunner: script {} -> synthetic(error)", script.id);
    Ok(())
}

fn synthetic_boundary_object(
    script: &Script,
    run_dir: &Path,
    commitments: Vec<CommitmentEnrollment>,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
    details: ResultDetails,
) -> BoundaryObject {
    let stdout_snippet = String::from_utf8_lossy(stdout_bytes).to_string();
    let stderr_snippet = String::from_utf8_lossy(stderr_bytes).to_string();

    BoundaryObject {
        script: ScriptInfo {
            id: script.id.clone(),
        },
        operation: OperationInfo {
            kind: "harness.supervised".to_string(),
            target: script.path.display().to_string(),
            args: None,
        },
        result: ResultInfo {
            outcome: "error".to_string(),
            details: Some(details),
        },
        context: ContextInfo {
            commitments,
            run: Some(RunInfo {
                workspace_root: None,
                command: script.path.display().to_string(),
            }),
            stack: None,
            extra: Default::default(),
        },
        payload: json!({
            "stdout_snippet": stdout_snippet,
            "stderr_snippet": stderr_snippet,
            // Raw details keep the supervised record traceable without logs.
            "raw": {
                "supervised": {
                    "run_dir": run_dir.display().to_string(),
                    "script_path": script.path.display().to_string(),
                }
            }
        }),
        extensions: Some(json!({
            "synthetic": {
                "emitted_by": "fencerunner",
            }
        })),
    }
}

fn ensure_script_executable(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("script not found or not executable: {}", path.display()))?;
    if !metadata.is_file() {
        bail!("script not found or not executable: {}", path.display());
    }
    if !has_execute_bit(&metadata) {
        bail!("script is not executable: {}", path.display());
    }
    Ok(())
}

fn has_execute_bit(metadata: &std::fs::Metadata) -> bool {
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

fn ensure_record_script_id_matches_file(script: &Script, record: &Value) -> Result<()> {
    let Some(record_id) = record
        .get("script")
        .and_then(|script| script.get("id"))
        .and_then(Value::as_str)
    else {
        bail!("boundary record missing script.id");
    };

    if record_id != script.id {
        bail!(
            "boundary record script.id '{}' does not match filename id '{}'",
            record_id,
            script.id
        );
    }
    Ok(())
}

fn load_commitment_enrollments(path: &Path) -> Result<Vec<CommitmentEnrollment>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    // The file is newline-separated "id|help" entries written by commit-help-me.
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("reading commitment enrollments {}", path.display()))?;
    if contents.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut by_id: BTreeMap<String, BTreeSet<CommitmentHelp>> = BTreeMap::new();
    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (id, help) = trimmed.split_once('|').ok_or_else(|| {
            anyhow!(
                "Invalid enrollment line {} in {} (expected 'id|help'): {}",
                idx + 1,
                path.display(),
                trimmed
            )
        })?;
        let id = id.trim();
        if id.is_empty() {
            bail!(
                "Invalid enrollment line {} in {} (empty id)",
                idx + 1,
                path.display()
            );
        }
        validate_commitment_id(id)?;
        let help = parse_commitment_help(help.trim())?;

        let helps = by_id.entry(id.to_string()).or_default();
        if !helps.insert(help) {
            bail!(
                "Duplicate commitment enrollment line {} in {}: {}",
                idx + 1,
                path.display(),
                trimmed
            );
        }
    }

    let mut enrollments: Vec<CommitmentEnrollment> = Vec::new();
    for (id, helps) in by_id {
        enrollments.push(CommitmentEnrollment {
            id,
            helps: helps.into_iter().collect(),
        });
    }
    Ok(enrollments)
}

fn parse_commitment_help(value: &str) -> Result<CommitmentHelp> {
    match value {
        "ensure" => Ok(CommitmentHelp::Ensure),
        "detect" => Ok(CommitmentHelp::Detect),
        "emit" => Ok(CommitmentHelp::Emit),
        other => bail!("unknown help verb '{other}' (expected ensure|detect|emit)"),
    }
}

fn validate_commitment_id(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("commitment id must not be empty");
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!("commitment id must match ^[A-Za-z0-9_.-]+$");
    }
    Ok(())
}
