//! Translates probe CLI inputs into a cfbo-v1 boundary object.
//!
//! This binary is the authoritative serializer for probe output. It validates
//! capability IDs against the shipped catalog, shells out to `detect-stack` for
//! host context, resolves workspace roots following the documented fallback
//! order, and prints a single JSON record to stdout.

use anyhow::{Context, Result, anyhow, bail};
use codex_fence::{
    CapabilityId, CapabilityIndex, CapabilitySnapshot, find_repo_root, resolve_helper_binary,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
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

    let capability_catalog_path = repo_root.join("schema/capabilities.json");
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

    let payload = match &args.payload_file {
        Some(path) => {
            if !path.is_file() {
                bail!("Payload file not found: {}", path.display());
            }
            read_json_file(path)?
        }
        None => default_payload(),
    };

    let operation_args = parse_json_string(&args.operation_args, "operation args")?;

    let stack_json = run_command_json(&detect_stack, &[&args.run_mode])
        .with_context(|| format!("Failed to execute {}", detect_stack.display()))?;

    let workspace_root = resolve_workspace_root()?;

    let result_json = json!({
        "observed_result": args.status,
        "raw_exit_code": args.raw_exit_code,
        "errno": args.errno,
        "message": args.message,
        "duration_ms": args.duration_ms,
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

    let record = json!({
        "schema_version": "cfbo-v1",
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

    println!("{}", serde_json::to_string(&record)?);
    Ok(())
}

/// Parsed command-line arguments for a single record emission.
///
/// Fields mirror the cfbo envelope; most values are required because probes are
/// expected to normalize outcomes themselves before calling this binary.
struct CliArgs {
    run_mode: String,
    probe_name: String,
    probe_version: String,
    category: String,
    verb: String,
    target: String,
    status: String,
    errno: Option<String>,
    message: Option<String>,
    duration_ms: Option<i64>,
    raw_exit_code: Option<i64>,
    error_detail: Option<String>,
    payload_file: Option<PathBuf>,
    operation_args: String,
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
                "--duration-ms" => {
                    config.duration_ms = Some(parse_i64(
                        next_value(&mut args, "--duration-ms")?,
                        "duration-ms",
                    )?)
                }
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
                    config.payload_file = Some(value);
                }
                "--operation-args" => {
                    config.operation_args = Some(next_value(&mut args, "--operation-args")?)
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
    run_mode: Option<String>,
    probe_name: Option<String>,
    probe_version: Option<String>,
    category: Option<String>,
    verb: Option<String>,
    target: Option<String>,
    status: Option<String>,
    errno: Option<String>,
    message: Option<String>,
    duration_ms: Option<i64>,
    raw_exit_code: Option<i64>,
    error_detail: Option<String>,
    payload_file: Option<PathBuf>,
    operation_args: Option<String>,
    primary_capability_id: Option<String>,
    secondary_capability_ids: Vec<String>,
    command: Option<String>,
}

impl PartialArgs {
    fn build(self) -> Result<CliArgs> {
        let PartialArgs {
            run_mode,
            probe_name,
            probe_version,
            category,
            verb,
            target,
            status,
            errno,
            message,
            duration_ms,
            raw_exit_code,
            error_detail,
            payload_file,
            operation_args,
            primary_capability_id,
            secondary_capability_ids,
            command,
        } = self;

        Ok(CliArgs {
            run_mode: Self::require("--run-mode", run_mode)?,
            probe_name: Self::require("--probe-name", probe_name)?,
            probe_version: Self::require("--probe-version", probe_version)?,
            category: Self::require("--category", category)?,
            verb: Self::require("--verb", verb)?,
            target: Self::require("--target", target)?,
            status: Self::require("--status", status)?,
            errno: errno.filter(not_empty),
            message: message.filter(not_empty),
            duration_ms,
            raw_exit_code,
            error_detail: error_detail.filter(not_empty),
            payload_file,
            operation_args: operation_args.unwrap_or_else(|| "{}".to_string()),
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

fn validate_status(status: &str) -> Result<()> {
    match status {
        "success" | "denied" | "partial" | "error" => Ok(()),
        other => bail!("Unknown status: {other} (expected success|denied|partial|error)"),
    }
}

fn normalize_secondary_ids(
    capabilities: &CapabilityIndex,
    raw: &[CapabilityId],
) -> Result<Vec<CapabilityId>> {
    let mut acc: BTreeSet<CapabilityId> = BTreeSet::new();
    for value in raw {
        let trimmed = value.0.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = CapabilityId(trimmed.to_string());
        validate_capability_id(capabilities, &normalized, "secondary capability id")?;
        acc.insert(normalized);
    }
    Ok(acc.into_iter().collect())
}

fn validate_capability_id(
    capabilities: &CapabilityIndex,
    value: &CapabilityId,
    label: &str,
) -> Result<()> {
    if capabilities.capability(value).is_some() {
        return Ok(());
    }
    bail!(
        "Unknown {label}: {}. Expected one of the IDs in schema/capabilities.json.",
        value.0
    );
}

fn read_json_file(path: &Path) -> Result<Value> {
    let data = fs::read_to_string(path)?;
    serde_json::from_str(&data).context("Payload file contained invalid JSON")
}

fn parse_json_string(raw: &str, label: &str) -> Result<Value> {
    serde_json::from_str(raw).with_context(|| format!("Invalid JSON for {label}"))
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

fn default_payload() -> Value {
    json!({
        "stdout_snippet": Value::Null,
        "stderr_snippet": Value::Null,
        "raw": {},
    })
}

fn resolve_secondary_capabilities<'a>(
    capabilities: &'a CapabilityIndex,
    ids: &[CapabilityId],
) -> Result<Vec<&'a codex_fence::Capability>> {
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
            return Ok(Some(env_root));
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
                return Ok(Some(candidate));
            }
        }
    }

    if let Ok(pwd) = env::var("PWD") {
        if !pwd.is_empty() {
            return Ok(Some(pwd));
        }
    }

    let fallback = env::current_dir()?;
    let display = fallback.display().to_string();
    if display.is_empty() {
        return Ok(None);
    }
    Ok(Some(display))
}

fn print_usage() {
    eprintln!("{}", usage());
}

fn usage() -> &'static str {
    "Usage: emit-record --run-mode MODE --probe-name NAME --probe-version VERSION \
  --primary-capability-id CAP_ID --command COMMAND \
  --category CATEGORY --verb VERB --target TARGET --status STATUS [options]\n\nOptions:\n  --errno ERRNO\n  --message MESSAGE\n  --duration-ms MILLIS\n  --raw-exit-code CODE\n  --error-detail TEXT\n  --secondary-capability-id CAP_ID   # repeat for multiple entries\n  --payload-file PATH\n  --operation-args JSON_OBJECT\n"
}

fn not_empty(value: &String) -> bool {
    !value.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn validate_status_allows_known_values() {
        for value in ["success", "denied", "partial", "error"] {
            validate_status(value).expect("status should pass");
        }
        assert!(validate_status("bogus").is_err());
    }

    #[test]
    fn normalize_secondary_deduplicates_and_trims() {
        let caps = sample_index(&[
            ("cap_a", "filesystem", "os_sandbox"),
            ("cap_b", "process", "agent_runtime"),
        ]);
        let input = vec![
            CapabilityId(" cap_a ".to_string()),
            CapabilityId("cap_b".to_string()),
            CapabilityId("".to_string()),
            CapabilityId("cap_a".to_string()),
        ];
        let normalized = normalize_secondary_ids(&caps, &input).expect("normalized");
        assert_eq!(
            normalized,
            vec![
                CapabilityId("cap_a".to_string()),
                CapabilityId("cap_b".to_string())
            ]
        );
    }

    #[test]
    fn normalize_secondary_rejects_unknown() {
        let caps = sample_index(&[("cap_a", "filesystem", "os_sandbox")]);
        let input = vec![
            CapabilityId("cap_a".to_string()),
            CapabilityId("cap_missing".to_string()),
        ];
        assert!(normalize_secondary_ids(&caps, &input).is_err());
    }

    #[test]
    fn capability_index_enforces_schema_version() {
        let mut file = NamedTempFile::new().unwrap();
        serde_json::to_writer(
            &mut file,
            &json!({
                "schema_version": "unexpected",
                "scope": {
                    "description": "test",
                    "policy_layers": [],
                    "categories": {}
                },
                "docs": {},
                "capabilities": []
            }),
        )
        .unwrap();
        assert!(CapabilityIndex::load(file.path()).is_err());
    }

    fn sample_index(entries: &[(&str, &str, &str)]) -> CapabilityIndex {
        let mut file = NamedTempFile::new().unwrap();
        let capabilities: Vec<Value> = entries
            .iter()
            .map(|(id, category, layer)| {
                json!({
                    "id": id,
                    "category": category,
                    "layer": layer,
                    "description": format!("cap {id}"),
                    "operations": {"allow": [], "deny": []}
                })
            })
            .collect();
        serde_json::to_writer(
            &mut file,
            &json!({
                "schema_version": "macOS_codex_v1",
                "scope": {
                    "description": "test",
                    "policy_layers": [],
                    "categories": {}
                },
                "docs": {},
                "capabilities": capabilities
            }),
        )
        .unwrap();
        CapabilityIndex::load(file.path()).expect("index loads")
    }
}
