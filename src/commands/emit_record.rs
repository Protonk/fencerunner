//! Internal subcommand that translates script CLI inputs into a boundary record.
//!
//! This is the authoritative serializer for script output. Scripts should call
//! the runner-provided `emit-record` shim so records stay schema-valid and
//! consistent with the run-dir `boundaries.json` contract.

use crate::boundary::{
    BoundaryContractIndex, BoundaryObject, CommitmentEnrollment, ContextInfo, OperationInfo,
    ResultDetails, ResultInfo, RunInfo, ScriptInfo,
};
use crate::commitments::model::CommitmentHelp;
use crate::harness::payload::{
    JsonObjectBuilder, PayloadArgs, TextSource, not_empty, validate_outcome,
};
use crate::scripts::discovery::canonical_run_dir;
use crate::repo_tools::{boundaries_contract_path, split_list};
use anyhow::{Context, Result, anyhow, bail};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

const ENV_ENROLLMENTS_PATH: &str = "FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH";

pub fn run(args: &[OsString]) -> Result<()> {
    let args = CliArgs::parse(args)?;

    // Resolve schema locations early so error messages mention concrete paths.
    let run_dir = resolve_run_dir()?;
    let boundaries_path = boundaries_contract_path(&run_dir);
    let boundaries_contract = BoundaryContractIndex::load(&boundaries_path)
        .with_context(|| format!("loading {}", boundaries_path.display()))?;

    let CliArgs {
        script_name,
        operation_kind,
        target,
        outcome,
        errno,
        message,
        exit_code,
        error_detail,
        payload,
        operation_args,
        command,
        ..
    } = args;

    // commit-help-me writes enrollments to a temp file; read and merge them here.
    let mut commitments = Vec::new();
    merge_commitment_enrollments_from_env(&mut commitments)?;

    let payload = payload.build()?;
    let operation_args = if operation_args.is_empty() {
        None
    } else {
        Some(operation_args.build("operation args")?)
    };

    let details =
        if exit_code.is_some() || errno.is_some() || message.is_some() || error_detail.is_some() {
            Some(ResultDetails {
                exit_code,
                errno,
                message,
                error_detail,
            })
        } else {
            None
        };

    let record = BoundaryObject {
        script: ScriptInfo { id: script_name },
        operation: OperationInfo {
            kind: operation_kind,
            target,
            args: operation_args,
        },
        result: ResultInfo { outcome, details },
        context: ContextInfo {
            commitments,
            run: Some(RunInfo {
                workspace_root: None,
                command,
            }),
            stack: None,
            extra: BTreeMap::new(),
        },
        payload,
        extensions: None,
    };

    // Validate against the run-dir-specific contract before emitting.
    let record_json = serde_json::to_value(&record)?;
    boundaries_contract
        .validate_record(&record_json)
        .with_context(|| format!("boundary record violates {}", boundaries_path.display()))?;

    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}

fn merge_commitment_enrollments_from_env(
    commitments: &mut Vec<CommitmentEnrollment>,
) -> Result<()> {
    let Some(raw_path) = env::var_os(ENV_ENROLLMENTS_PATH) else {
        return Ok(());
    };
    if raw_path.is_empty() {
        return Ok(());
    }

    let path = PathBuf::from(raw_path);
    if !path.exists() {
        return Ok(());
    }

    let contents =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    if contents.trim().is_empty() {
        return Ok(());
    }

    let mut pairs: Vec<(CommitmentHelp, String)> = Vec::new();
    for (idx, line) in contents.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (commitment_id, help) = trimmed.split_once('|').ok_or_else(|| {
            anyhow!(
                "Invalid enrollment line {} in {} (expected 'id|help'): {}",
                idx + 1,
                path.display(),
                trimmed
            )
        })?;
        let commitment_id = commitment_id.trim().to_string();
        validate_commitment_id(&commitment_id).with_context(|| {
            format!(
                "Invalid commitment id on line {} in {}",
                idx + 1,
                path.display()
            )
        })?;
        let help = parse_commitment_help(help.trim()).with_context(|| {
            format!(
                "Invalid commitment help on line {} in {}",
                idx + 1,
                path.display()
            )
        })?;
        pairs.push((help, commitment_id));
    }

    if pairs.is_empty() {
        return Ok(());
    }

    let extra = build_commitment_enrollments(&pairs)?;
    merge_commitment_enrollments(commitments, extra)?;
    Ok(())
}

fn merge_commitment_enrollments(
    base: &mut Vec<CommitmentEnrollment>,
    extra: Vec<CommitmentEnrollment>,
) -> Result<()> {
    let mut by_id: BTreeMap<String, BTreeSet<CommitmentHelp>> = BTreeMap::new();

    for enrollment in base.iter() {
        let helps = by_id.entry(enrollment.id.clone()).or_default();
        for help in enrollment.helps.iter().copied() {
            if !helps.insert(help) {
                bail!(
                    "Duplicate commitment enrollment: {} {}",
                    enrollment.id,
                    commitment_help_label(help)
                );
            }
        }
    }

    for enrollment in extra {
        let helps = by_id.entry(enrollment.id.clone()).or_default();
        for help in enrollment.helps.iter().copied() {
            if !helps.insert(help) {
                bail!(
                    "Duplicate commitment enrollment: {} {}",
                    enrollment.id,
                    commitment_help_label(help)
                );
            }
        }
    }

    *base = by_id
        .into_iter()
        .map(|(id, helps)| CommitmentEnrollment {
            id,
            helps: helps.into_iter().collect(),
        })
        .collect();
    Ok(())
}

fn resolve_run_dir() -> Result<PathBuf> {
    // fencerunner sets FENCERUNNER_RUN_DIR; emit-record is not meant to run standalone.
    let Some(value) = env::var_os("FENCERUNNER_RUN_DIR") else {
        bail!("FENCERUNNER_RUN_DIR is not set (run scripts via fencerunner)");
    };
    if value.is_empty() {
        bail!("FENCERUNNER_RUN_DIR is empty");
    }

    canonical_run_dir(Path::new(&value))
        .with_context(|| format!("invalid FENCERUNNER_RUN_DIR {}", value.to_string_lossy()))
}

struct CliArgs {
    script_name: String,
    operation_kind: String,
    target: String,
    outcome: String,
    errno: Option<String>,
    message: Option<String>,
    exit_code: Option<i64>,
    error_detail: Option<String>,
    payload: PayloadArgs,
    operation_args: JsonObjectBuilder,
    command: String,
}

impl CliArgs {
    fn parse(args: &[OsString]) -> Result<Self> {
        let mut args = args.iter().cloned();
        let mut config = PartialArgs::default();

        while let Some(arg_os) = args.next() {
            let arg = os_to_string(arg_os);
            match arg.as_str() {
                "--script-name" | "--script-id" => {
                    config.script_name = Some(next_value(&mut args, arg.as_str())?)
                }
                "--operation-kind" => {
                    config.operation_kind = Some(next_value(&mut args, "--operation-kind")?)
                }
                "--target" => config.target = Some(next_value(&mut args, "--target")?),
                "--outcome" => config.outcome = Some(next_value(&mut args, "--outcome")?),
                "--errno" => config.errno = Some(next_value(&mut args, "--errno")?),
                "--message" => config.message = Some(next_value(&mut args, "--message")?),
                "--exit-code" => {
                    config.exit_code = Some(parse_i64(
                        next_value(&mut args, "--exit-code")?,
                        "exit-code",
                    )?)
                }
                "--error-detail" => {
                    config.error_detail = Some(next_value(&mut args, "--error-detail")?)
                }
                "--payload-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-file")?);
                    config.payload.set_payload_file(value)?;
                }
                "--payload-stdout" => {
                    let value = next_value(&mut args, "--payload-stdout")?;
                    config.payload.set_stdout(TextSource::Inline(value))?;
                }
                "--payload-stdout-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-stdout-file")?);
                    config.payload.set_stdout(TextSource::File(value))?;
                }
                "--payload-stderr" => {
                    let value = next_value(&mut args, "--payload-stderr")?;
                    config.payload.set_stderr(TextSource::Inline(value))?;
                }
                "--payload-stderr-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-stderr-file")?);
                    config.payload.set_stderr(TextSource::File(value))?;
                }
                "--payload-raw" => {
                    let value = next_value(&mut args, "--payload-raw")?;
                    config
                        .payload
                        .raw_mut()
                        .merge_json_string(&value, "payload raw")?;
                }
                "--payload-raw-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-raw-file")?);
                    config
                        .payload
                        .raw_mut()
                        .merge_json_file(&value, "payload raw")?;
                }
                "--payload-raw-field" => {
                    let key = next_value(&mut args, "--payload-raw-field")?;
                    let value = next_value(&mut args, "--payload-raw-field")?;
                    config.payload.raw_mut().insert_string(key, value);
                }
                "--payload-raw-field-json" => {
                    let key = next_value(&mut args, "--payload-raw-field-json")?;
                    let value = next_value(&mut args, "--payload-raw-field-json")?;
                    config
                        .payload
                        .raw_mut()
                        .insert_json_value(key, value, "payload raw field")?;
                }
                "--payload-raw-null" => {
                    let key = next_value(&mut args, "--payload-raw-null")?;
                    config.payload.raw_mut().insert_null(key);
                }
                "--payload-raw-list" => {
                    let key = next_value(&mut args, "--payload-raw-list")?;
                    let value = next_value(&mut args, "--payload-raw-list")?;
                    let entries = split_list(&value);
                    config.payload.raw_mut().insert_list(key, entries);
                }
                "--operation-args" => {
                    let value = next_value(&mut args, "--operation-args")?;
                    config
                        .operation_args
                        .merge_json_string(&value, "operation args")?;
                }
                "--operation-args-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--operation-args-file")?);
                    config
                        .operation_args
                        .merge_json_file(&value, "operation args")?;
                }
                "--operation-arg" => {
                    let key = next_value(&mut args, "--operation-arg")?;
                    let value = next_value(&mut args, "--operation-arg")?;
                    config.operation_args.insert_string(key, value);
                }
                "--operation-arg-json" => {
                    let key = next_value(&mut args, "--operation-arg-json")?;
                    let value = next_value(&mut args, "--operation-arg-json")?;
                    config
                        .operation_args
                        .insert_json_value(key, value, "operation arg")?;
                }
                "--operation-arg-null" => {
                    let key = next_value(&mut args, "--operation-arg-null")?;
                    config.operation_args.insert_null(key);
                }
                "--operation-arg-list" => {
                    let key = next_value(&mut args, "--operation-arg-list")?;
                    let value = next_value(&mut args, "--operation-arg-list")?;
                    let entries = split_list(&value);
                    config.operation_args.insert_list(key, entries);
                }
                "--command" => config.command = Some(next_value(&mut args, "--command")?),
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                other if other.starts_with("--") => {
                    eprintln!("Unknown flag: {other}");
                    print_usage();
                    std::process::exit(1);
                }
                other => {
                    eprintln!("Unexpected positional argument: {other}");
                    print_usage();
                    std::process::exit(1);
                }
            }
        }

        let args = config.build()?;
        validate_outcome(&args.outcome)?;
        Ok(args)
    }
}

#[derive(Default)]
struct PartialArgs {
    script_name: Option<String>,
    operation_kind: Option<String>,
    target: Option<String>,
    outcome: Option<String>,
    errno: Option<String>,
    message: Option<String>,
    exit_code: Option<i64>,
    error_detail: Option<String>,
    payload: PayloadArgs,
    operation_args: JsonObjectBuilder,
    command: Option<String>,
}

impl PartialArgs {
    fn build(self) -> Result<CliArgs> {
        let PartialArgs {
            script_name,
            operation_kind,
            target,
            outcome,
            errno,
            message,
            exit_code,
            error_detail,
            payload,
            operation_args,
            command,
        } = self;

        Ok(CliArgs {
            script_name: Self::require("--script-name", script_name)?,
            operation_kind: Self::require("--operation-kind", operation_kind)?,
            target: Self::require("--target", target)?,
            outcome: Self::require("--outcome", outcome)?,
            errno: errno.filter(|value| not_empty(value)),
            message: message.filter(|value| not_empty(value)),
            exit_code,
            error_detail: error_detail.filter(|value| not_empty(value)),
            payload,
            operation_args,
            command: Self::require("--command", command)?,
        })
    }

    fn require(flag: &str, value: Option<String>) -> Result<String> {
        value.ok_or_else(|| anyhow!("Missing required flag: {flag}"))
    }
}

fn next_value(args: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<String> {
    args.next()
        .map(os_to_string)
        .ok_or_else(|| anyhow!("Missing value for {flag}"))
}

fn parse_i64(value: String, label: &str) -> Result<i64> {
    value
        .parse::<i64>()
        .with_context(|| format!("Failed to parse {label} as integer"))
}

fn parse_commitment_help(value: &str) -> Result<CommitmentHelp> {
    match value {
        "ensure" => Ok(CommitmentHelp::Ensure),
        "detect" => Ok(CommitmentHelp::Detect),
        "emit" => Ok(CommitmentHelp::Emit),
        other => bail!("Unknown commitment help: {other} (expected ensure|detect|emit)"),
    }
}

fn commitment_help_label(help: CommitmentHelp) -> &'static str {
    match help {
        CommitmentHelp::Ensure => "ensure",
        CommitmentHelp::Detect => "detect",
        CommitmentHelp::Emit => "emit",
    }
}

fn build_commitment_enrollments(
    pairs: &[(CommitmentHelp, String)],
) -> Result<Vec<CommitmentEnrollment>> {
    let mut by_id: BTreeMap<String, BTreeSet<CommitmentHelp>> = BTreeMap::new();

    for (help, id) in pairs {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            bail!("Commitment id must not be empty");
        }

        let helps = by_id.entry(trimmed.to_string()).or_default();
        if !helps.insert(*help) {
            bail!(
                "Duplicate commitment enrollment: {} {}",
                trimmed,
                commitment_help_label(*help)
            );
        }
    }

    Ok(by_id
        .into_iter()
        .map(|(id, helps)| CommitmentEnrollment {
            id,
            helps: helps.into_iter().collect(),
        })
        .collect())
}

fn validate_commitment_id(value: &str) -> Result<()> {
    // Keep in sync with `commit-help-me` and the JSON schemas.
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("Commitment id must not be empty");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!("Commitment id must match ^[A-Za-z0-9_.-]+$");
    }
    Ok(())
}

fn os_to_string(value: OsString) -> String {
    match value.into_string() {
        Ok(val) => val,
        Err(os) => escape_os_value(os),
    }
}

#[cfg(unix)]
fn escape_os_value(value: OsString) -> String {
    use std::os::unix::ffi::OsStrExt;
    escape_bytes(value.as_os_str().as_bytes())
}

#[cfg(not(unix))]
fn escape_os_value(value: OsString) -> String {
    value.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn escape_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii_graphic() || byte == b' ' {
            out.push(byte as char);
        } else {
            write!(&mut out, "\\x{byte:02X}").expect("write to string");
        }
    }
    out
}

fn print_usage() {
    eprintln!("{}", usage());
}

fn usage() -> &'static str {
    "Usage: emit-record --script-name NAME --command COMMAND \\\n  --operation-kind KIND --target TARGET --outcome OUTCOME [options]\n\nOptions:\n  --errno ERRNO\n  --message MESSAGE\n  --exit-code CODE\n  --error-detail TEXT\n  --payload-file PATH (JSON object with stdout_snippet/stderr_snippet)\n  --payload-stdout TEXT | --payload-stdout-file PATH (required unless --payload-file)\n  --payload-stderr TEXT | --payload-stderr-file PATH (required unless --payload-file)\n  --payload-raw JSON_OBJECT | --payload-raw-file PATH\n  --payload-raw-field KEY VALUE\n  --payload-raw-field-json KEY JSON_VALUE\n  --payload-raw-null KEY\n  --payload-raw-list KEY \"a,b,c\"\n  --operation-args JSON_OBJECT | --operation-args-file PATH\n  --operation-arg KEY VALUE\n  --operation-arg-json KEY JSON_VALUE\n  --operation-arg-null KEY\n  --operation-arg-list KEY \"a,b,c\"\n"
}
