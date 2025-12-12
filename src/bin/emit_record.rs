//! Translates probe CLI inputs into a boundary-event object.
//!
//! This binary is the authoritative serializer for probe output. It validates
//! capability IDs against the shipped catalog, shells out to `detect-stack` for
//! host context, resolves workspace roots following the documented fallback
//! order, and prints a single JSON record to stdout.

use anyhow::{Context, Result, anyhow, bail};
use fencerunner::emit_support::{
    JsonObjectBuilder, PayloadArgs, TextSource, normalize_secondary_ids, not_empty,
    validate_capability_id, validate_status,
};
use fencerunner::{
    BoundarySchema, CapabilityId, CapabilityIndex, CapabilitySnapshot, StackInfo, find_repo_root,
    resolve_boundary_schema_path, resolve_catalog_path, resolve_helper_binary, split_list,
};
use serde_json::{Value, json};
use std::env;
use std::ffi::OsString;
use std::fmt::Write;
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

    let detect_stack = resolve_helper_binary(&repo_root, "detect-stack")?;

    let capability_catalog_path =
        resolve_catalog_path(&repo_root, args.catalog_path.as_deref().map(Path::new));
    let capability_index = CapabilityIndex::load(&capability_catalog_path).with_context(|| {
        format!(
            "loading capability catalog from {}",
            capability_catalog_path.display()
        )
    })?;
    if capability_index.ids().next().is_none() {
        bail!(
            "No capability IDs found in {}",
            capability_catalog_path.display()
        );
    }

    validate_capability_id(
        &capability_index,
        &args.primary_capability_id,
        "primary capability id",
    )?;
    let secondary_capability_ids =
        normalize_secondary_ids(&capability_index, &args.secondary_capability_ids)?;

    let capabilities_schema_version = capability_index.key().clone();

    let payload = args.payload.build()?;
    let operation_args = args.operation_args.build("operation args")?;

    let stack_raw = run_command_json(&detect_stack, &[&args.run_mode])
        .with_context(|| format!("Failed to execute {}", detect_stack.display()))?;
    let stack: StackInfo = serde_json::from_value(stack_raw.clone()).context(
        "detect-stack emitted JSON that does not match the current stack schema",
    )?;
    let stack_json = serde_json::to_value(stack)?;

    let workspace_root = resolve_workspace_root()?;

    let result_json = json!({
        "observed_result": args.status,
        "raw_exit_code": args.raw_exit_code,
        "errno": args.errno,
        "message": args.message,
        "error_detail": args.error_detail,
    });

    let primary_capability = capability_index
        .capability(&args.primary_capability_id)
        .ok_or_else(|| {
            anyhow!(
                "Unable to resolve capability metadata for {}",
                args.primary_capability_id.0
            )
        })?;
    let secondary_capabilities =
        resolve_secondary_capabilities(&capability_index, &secondary_capability_ids)?;
    let primary_capability_snapshot = primary_capability.snapshot();
    let secondary_capability_snapshots: Vec<CapabilitySnapshot> = secondary_capabilities
        .iter()
        .map(|cap| cap.snapshot())
        .collect();

    let boundary_schema_path = resolve_boundary_schema_path(
        &repo_root,
        args.boundary_schema_path.as_deref().map(Path::new),
    )?;
    let boundary_schema = BoundarySchema::load(&boundary_schema_path).with_context(|| {
        format!(
            "loading boundary schema from {}",
            boundary_schema_path.display()
        )
    })?;
    let schema_key = boundary_schema.schema_key().ok_or_else(|| {
        anyhow!(
            "boundary schema at {} is missing a schema_key; provide a descriptor file",
            boundary_schema_path.display()
        )
    })?;

    let record = json!({
        "schema_version": boundary_schema.schema_version(),
        "schema_key": schema_key,
        "capabilities_schema_version": capabilities_schema_version,
        "stack": stack_json,
        "probe": {
            "id": args.probe_name,
            "version": args.probe_version,
            "primary_capability_id": args.primary_capability_id,
            "secondary_capability_ids": secondary_capability_ids,
        },
        "run": {
            "mode": args.run_mode,
            "workspace_root": workspace_root,
            "command": args.command,
        },
        "operation": {
            "category": args.category,
            "verb": args.verb,
            "target": args.target,
            "args": operation_args,
        },
        "result": result_json,
        "payload": payload,
        "capability_context": {
            "primary": primary_capability_snapshot,
            "secondary": secondary_capability_snapshots,
        }
    });

    boundary_schema.validate(&record)?;
    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}

/// Parsed command-line arguments for a single record emission.
///
/// Fields mirror the boundary-event envelope; most values are required because probes are
/// expected to normalize outcomes themselves before calling this binary.
struct CliArgs {
    catalog_path: Option<String>,
    boundary_schema_path: Option<String>,
    run_mode: String,
    probe_name: String,
    probe_version: String,
    category: String,
    verb: String,
    target: String,
    status: String,
    errno: Option<String>,
    message: Option<String>,
    raw_exit_code: Option<i64>,
    error_detail: Option<String>,
    payload: PayloadArgs,
    operation_args: JsonObjectBuilder,
    primary_capability_id: CapabilityId,
    secondary_capability_ids: Vec<CapabilityId>,
    command: String,
}

impl CliArgs {
    fn parse() -> Result<Self> {
        let mut args = env::args_os().skip(1);
        let mut config = PartialArgs::default();

        while let Some(arg_os) = args.next() {
            let arg = os_to_string(arg_os);
            match arg.as_str() {
                "--catalog" => {
                    let value = next_value(&mut args, "--catalog")?;
                    config.catalog_path = Some(value);
                }
                "--boundary" => {
                    let value = next_value(&mut args, "--boundary")?;
                    config.boundary_schema_path = Some(value);
                }
                "--run-mode" => config.run_mode = Some(next_value(&mut args, "--run-mode")?),
                "--probe-name" | "--probe-id" => {
                    config.probe_name = Some(next_value(&mut args, arg.as_str())?)
                }
                "--probe-version" => {
                    config.probe_version = Some(next_value(&mut args, "--probe-version")?)
                }
                "--category" => config.category = Some(next_value(&mut args, "--category")?),
                "--verb" => config.verb = Some(next_value(&mut args, "--verb")?),
                "--target" => config.target = Some(next_value(&mut args, "--target")?),
                "--status" => config.status = Some(next_value(&mut args, "--status")?),
                "--errno" => config.errno = Some(next_value(&mut args, "--errno")?),
                "--message" => config.message = Some(next_value(&mut args, "--message")?),
                "--raw-exit-code" => {
                    config.raw_exit_code = Some(parse_i64(
                        next_value(&mut args, "--raw-exit-code")?,
                        "raw-exit-code",
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
                "--primary-capability-id" => {
                    config.primary_capability_id =
                        Some(next_value(&mut args, "--primary-capability-id")?)
                }
                "--secondary-capability-id" => config
                    .secondary_capability_ids
                    .push(next_value(&mut args, "--secondary-capability-id")?),
                "--command" => config.command = Some(next_value(&mut args, "--command")?),
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(1);
                }
                other => {
                    eprintln!("Unknown flag: {other}");
                    print_usage();
                    std::process::exit(1);
                }
            }
        }

        let args = config.build()?;
        validate_status(&args.status)?;
        Ok(args)
    }
}

#[derive(Default)]
struct PartialArgs {
    catalog_path: Option<String>,
    boundary_schema_path: Option<String>,
    run_mode: Option<String>,
    probe_name: Option<String>,
    probe_version: Option<String>,
    category: Option<String>,
    verb: Option<String>,
    target: Option<String>,
    status: Option<String>,
    errno: Option<String>,
    message: Option<String>,
    raw_exit_code: Option<i64>,
    error_detail: Option<String>,
    payload: PayloadArgs,
    operation_args: JsonObjectBuilder,
    primary_capability_id: Option<String>,
    secondary_capability_ids: Vec<String>,
    command: Option<String>,
}

impl PartialArgs {
    fn build(self) -> Result<CliArgs> {
        let PartialArgs {
            catalog_path,
            boundary_schema_path,
            run_mode,
            probe_name,
            probe_version,
            category,
            verb,
            target,
            status,
            errno,
            message,
            raw_exit_code,
            error_detail,
            payload,
            operation_args,
            primary_capability_id,
            secondary_capability_ids,
            command,
        } = self;

        Ok(CliArgs {
            catalog_path,
            boundary_schema_path,
            run_mode: Self::require("--run-mode", run_mode)?,
            probe_name: Self::require("--probe-name", probe_name)?,
            probe_version: Self::require("--probe-version", probe_version)?,
            category: Self::require("--category", category)?,
            verb: Self::require("--verb", verb)?,
            target: Self::require("--target", target)?,
            status: Self::require("--status", status)?,
            errno: errno.filter(not_empty),
            message: message.filter(not_empty),
            raw_exit_code,
            error_detail: error_detail.filter(not_empty),
            payload,
            operation_args,
            primary_capability_id: CapabilityId(Self::require(
                "--primary-capability-id",
                primary_capability_id,
            )?),
            secondary_capability_ids: secondary_capability_ids
                .into_iter()
                .map(|id| CapabilityId(id))
                .collect(),
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

fn run_command_json(path: &Path, args: &[&str]) -> Result<Value> {
    let output = Command::new(path)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{} failed: {stderr}", path.display());
    }
    serde_json::from_slice(&output.stdout).context("Failed to parse command output as JSON")
}

fn resolve_secondary_capabilities<'a>(
    capabilities: &'a CapabilityIndex,
    ids: &[CapabilityId],
) -> Result<Vec<&'a fencerunner::Capability>> {
    let mut caps = Vec::new();
    for id in ids {
        let Some(cap) = capabilities.capability(id) else {
            bail!("Unable to resolve capability metadata for {}", id.0);
        };
        caps.push(cap);
    }
    Ok(caps)
}

fn resolve_workspace_root() -> Result<Option<String>> {
    if let Ok(env_root) = env::var("FENCE_WORKSPACE_ROOT") {
        if !env_root.is_empty() {
            return Ok(Some(canonicalize_string(env_root)));
        }
    }

    // Match the documented fallback order: prefer git top-level when available,
    // then PWD (if exported), finally the current working directory.
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    {
        if output.status.success() {
            let candidate = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !candidate.is_empty() {
                return Ok(Some(canonicalize_string(candidate)));
            }
        }
    }

    if let Ok(pwd) = env::var("PWD") {
        if !pwd.is_empty() {
            return Ok(Some(canonicalize_string(pwd)));
        }
    }

    let fallback = env::current_dir()?;
    let display = canonicalize_pathbuf(fallback);
    if display.is_empty() {
        return Ok(None);
    }
    Ok(Some(display))
}

fn canonicalize_string(path: String) -> String {
    let pathbuf = PathBuf::from(&path);
    canonicalize_pathbuf(pathbuf)
}

fn canonicalize_pathbuf(path: PathBuf) -> String {
    fs::canonicalize(&path)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn print_usage() {
    eprintln!("{}", usage());
}

fn usage() -> &'static str {
    "Usage: emit-record --run-mode MODE --probe-name NAME --probe-version VERSION \
  --primary-capability-id CAP_ID --command COMMAND \
  --category CATEGORY --verb VERB --target TARGET --status STATUS [options]\n\nOptions:\n  --errno ERRNO\n  --message MESSAGE\n  --raw-exit-code CODE\n  --error-detail TEXT\n  --secondary-capability-id CAP_ID   # repeat for multiple entries\n  --payload-file PATH (JSON object)\n  --payload-stdout TEXT | --payload-stdout-file PATH\n  --payload-stderr TEXT | --payload-stderr-file PATH\n  --payload-raw JSON_OBJECT | --payload-raw-file PATH\n  --payload-raw-field KEY VALUE\n  --payload-raw-field-json KEY JSON_VALUE\n  --payload-raw-null KEY\n  --payload-raw-list KEY \"a,b,c\"\n  --operation-args JSON_OBJECT | --operation-args-file PATH\n  --operation-arg KEY VALUE\n  --operation-arg-json KEY JSON_VALUE\n  --operation-arg-null KEY\n  --operation-arg-list KEY \"a,b,c\"\n"
}
