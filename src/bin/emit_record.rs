//! Translates probe CLI inputs into a cfbo-v1 boundary object.
//!
//! This binary is the authoritative serializer for probe output. It validates
//! capability IDs against the shipped catalog, shells out to `detect-stack` for
//! host context, resolves workspace roots following the documented fallback
//! order, and prints a single JSON record to stdout.

use anyhow::{Context, Result, anyhow, bail};
use codex_fence::{
    CapabilityId, CapabilityIndex, CapabilitySnapshot, find_repo_root, resolve_helper_binary,
    split_list,
};
use serde_json::{Map, Value, json};
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

    let payload = args.payload.build()?;
    let operation_args = args.operation_args.build("operation args")?;

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
                    config.payload.set_payload_file(value)?;
                }
                "--payload-stdout" => {
                    let value = next_value(&mut args, "--payload-stdout")?;
                    config
                        .payload
                        .set_stdout(TextSource::Inline(value))?;
                }
                "--payload-stdout-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-stdout-file")?);
                    config
                        .payload
                        .set_stdout(TextSource::File(value))?;
                }
                "--payload-stderr" => {
                    let value = next_value(&mut args, "--payload-stderr")?;
                    config
                        .payload
                        .set_stderr(TextSource::Inline(value))?;
                }
                "--payload-stderr-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-stderr-file")?);
                    config
                        .payload
                        .set_stderr(TextSource::File(value))?;
                }
                "--payload-raw" => {
                    let value = next_value(&mut args, "--payload-raw")?;
                    config
                        .payload
                        .raw
                        .merge_json_string(&value, "payload raw")?;
                }
                "--payload-raw-file" => {
                    let value = PathBuf::from(next_value(&mut args, "--payload-raw-file")?);
                    config
                        .payload
                        .raw
                        .merge_json_file(&value, "payload raw")?;
                }
                "--payload-raw-field" => {
                    let key = next_value(&mut args, "--payload-raw-field")?;
                    let value = next_value(&mut args, "--payload-raw-field")?;
                    config.payload.raw.insert_string(key, value);
                }
                "--payload-raw-field-json" => {
                    let key = next_value(&mut args, "--payload-raw-field-json")?;
                    let value = next_value(&mut args, "--payload-raw-field-json")?;
                    config
                        .payload
                        .raw
                        .insert_json_value(key, value, "payload raw field")?;
                }
                "--payload-raw-null" => {
                    let key = next_value(&mut args, "--payload-raw-null")?;
                    config.payload.raw.insert_null(key);
                }
                "--payload-raw-list" => {
                    let key = next_value(&mut args, "--payload-raw-list")?;
                    let value = next_value(&mut args, "--payload-raw-list")?;
                    let entries = split_list(&value);
                    config.payload.raw.insert_list(key, entries);
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
    payload: PayloadArgs,
    operation_args: JsonObjectBuilder,
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
            payload,
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

#[derive(Default, Clone)]
struct PayloadArgs {
    payload_file: Option<PathBuf>,
    stdout: Option<TextSource>,
    stderr: Option<TextSource>,
    raw: JsonObjectBuilder,
}

impl PayloadArgs {
    fn set_payload_file(&mut self, path: PathBuf) -> Result<()> {
        if self.payload_file.is_some() {
            bail!("--payload-file provided multiple times");
        }
        self.payload_file = Some(path);
        Ok(())
    }

    fn set_stdout(&mut self, source: TextSource) -> Result<()> {
        if self.stdout.is_some() {
            bail!("stdout snippet provided multiple times");
        }
        self.stdout = Some(source);
        Ok(())
    }

    fn set_stderr(&mut self, source: TextSource) -> Result<()> {
        if self.stderr.is_some() {
            bail!("stderr snippet provided multiple times");
        }
        self.stderr = Some(source);
        Ok(())
    }

    fn build(self) -> Result<Value> {
        if let Some(ref path) = self.payload_file {
            if self.has_inline_fields() {
                bail!("--payload-file cannot be combined with inline payload flags");
            }
            if !path.is_file() {
                bail!("Payload file not found: {}", path.display());
            }
            return read_json_file(&path);
        }

        let stdout_snippet = build_snippet_value(self.stdout)?;
        let stderr_snippet = build_snippet_value(self.stderr)?;
        let raw = self.raw.build("payload raw object")?;

        Ok(json!({
            "stdout_snippet": stdout_snippet,
            "stderr_snippet": stderr_snippet,
            "raw": raw,
        }))
    }

    fn has_inline_fields(&self) -> bool {
        self.stdout.is_some() || self.stderr.is_some() || !self.raw.is_empty()
    }
}

#[derive(Default, Clone)]
struct JsonObjectBuilder {
    sources: Vec<JsonValueSource>,
}

impl JsonObjectBuilder {
    fn merge_json_string(&mut self, raw: &str, label: &str) -> Result<()> {
        let value: Value =
            serde_json::from_str(raw).with_context(|| format!("Invalid JSON for {label}"))?;
        self.push_object(value, label)
    }

    fn merge_json_file(&mut self, path: &Path, label: &str) -> Result<()> {
        if !path.is_file() {
            bail!("{label} file not found: {}", path.display());
        }
        let value = read_json_file(path)?;
        self.push_object(value, label)
    }

    fn push_object(&mut self, value: Value, label: &str) -> Result<()> {
        match value {
            Value::Object(map) => {
                self.sources.push(JsonValueSource::MergeObject(map));
                Ok(())
            }
            _ => bail!("{label} must be a JSON object"),
        }
    }

    fn insert_string(&mut self, key: String, value: String) {
        self.sources.push(JsonValueSource::SetField {
            key,
            value: Value::String(value),
        });
    }

    fn insert_json_value(&mut self, key: String, raw: String, label: &str) -> Result<()> {
        let value: Value = serde_json::from_str(&raw)
            .with_context(|| format!("Invalid JSON for {label} value {key}"))?;
        self.sources.push(JsonValueSource::SetField { key, value });
        Ok(())
    }

    fn insert_null(&mut self, key: String) {
        self.sources
            .push(JsonValueSource::SetField { key, value: Value::Null });
    }

    fn insert_list(&mut self, key: String, values: Vec<String>) {
        let arr = values.into_iter().map(Value::String).collect();
        self.sources.push(JsonValueSource::SetField {
            key,
            value: Value::Array(arr),
        });
    }

    fn build(&self, label: &str) -> Result<Value> {
        let mut map: Map<String, Value> = Map::new();
        for source in &self.sources {
            match source {
                JsonValueSource::MergeObject(obj) => merge_object(&mut map, obj),
                JsonValueSource::SetField { key, value } => {
                    map.insert(key.clone(), value.clone());
                    Ok(())
                }
            }
            .with_context(|| format!("while building {label}"))?;
        }
        Ok(Value::Object(map))
    }

    fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

#[derive(Clone)]
enum JsonValueSource {
    MergeObject(Map<String, Value>),
    SetField { key: String, value: Value },
}

#[derive(Clone)]
enum TextSource {
    Inline(String),
    File(PathBuf),
}

fn merge_object(target: &mut Map<String, Value>, source: &Map<String, Value>) -> Result<()> {
    for (key, value) in source {
        target.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn build_snippet_value(source: Option<TextSource>) -> Result<Value> {
    let Some(src) = source else {
        return Ok(Value::Null);
    };
    let text = read_text_source(&src)?;
    Ok(Value::String(truncate_snippet(&text)))
}

fn read_text_source(source: &TextSource) -> Result<String> {
    let raw = match source {
        TextSource::Inline(value) => value.clone(),
        TextSource::File(path) => {
            if !path.is_file() {
                bail!("Snippet file not found: {}", path.display());
            }
            String::from_utf8_lossy(&fs::read(path)?).into_owned()
        }
    };
    Ok(clean_text(&raw))
}

fn clean_text(raw: &str) -> String {
    raw.replace('\0', "")
}

const SNIPPET_MAX_CHARS: usize = 400;
const SNIPPET_ELLIPSIS: &str = "\u{2026}";

fn truncate_snippet(text: &str) -> String {
    let mut acc = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= SNIPPET_MAX_CHARS {
            acc.push_str(SNIPPET_ELLIPSIS);
            return acc;
        }
        acc.push(ch);
    }
    acc
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
    serde_json::from_str(&data).context("File contained invalid JSON")
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
  --category CATEGORY --verb VERB --target TARGET --status STATUS [options]\n\nOptions:\n  --errno ERRNO\n  --message MESSAGE\n  --duration-ms MILLIS\n  --raw-exit-code CODE\n  --error-detail TEXT\n  --secondary-capability-id CAP_ID   # repeat for multiple entries\n  --payload-file PATH (JSON object)\n  --payload-stdout TEXT | --payload-stdout-file PATH\n  --payload-stderr TEXT | --payload-stderr-file PATH\n  --payload-raw JSON_OBJECT | --payload-raw-file PATH\n  --payload-raw-field KEY VALUE\n  --payload-raw-field-json KEY JSON_VALUE\n  --payload-raw-null KEY\n  --payload-raw-list KEY \"a,b,c\"\n  --operation-args JSON_OBJECT | --operation-args-file PATH\n  --operation-arg KEY VALUE\n  --operation-arg-json KEY JSON_VALUE\n  --operation-arg-null KEY\n  --operation-arg-list KEY \"a,b,c\"\n"
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

    #[test]
    fn json_object_builder_overrides_fields() {
        let mut builder = JsonObjectBuilder::default();
        builder
            .merge_json_string(r#"{"a":1,"b":2}"#, "object")
            .expect("merge");
        builder.insert_string("b".to_string(), "override".to_string());
        builder.insert_list(
            "c".to_string(),
            vec!["first".to_string(), "second".to_string()],
        );
        builder
            .insert_json_value("d".to_string(), "true".to_string(), "object")
            .expect("json value");
        let value = builder.build("test object").expect("build object");
        let obj = value.as_object().expect("object shape");
        assert_eq!(obj.get("a").and_then(Value::as_i64), Some(1));
        assert_eq!(obj.get("b").and_then(Value::as_str), Some("override"));
        assert_eq!(
            obj.get("c")
                .and_then(Value::as_array)
                .map(|arr| arr.len()),
            Some(2)
        );
        assert_eq!(obj.get("d").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn payload_builder_accepts_inline_snippets() {
        let mut payload = PayloadArgs::default();
        payload
            .set_stdout(TextSource::Inline("hello".to_string()))
            .unwrap();
        payload
            .set_stderr(TextSource::Inline("stderr".to_string()))
            .unwrap();
        payload.raw.insert_null("raw_key".to_string());
        let built = payload.build().expect("payload build");
        assert_eq!(
            built
                .pointer("/stdout_snippet")
                .and_then(Value::as_str),
            Some("hello")
        );
        assert_eq!(
            built
                .pointer("/stderr_snippet")
                .and_then(Value::as_str),
            Some("stderr")
        );
        assert!(built
            .pointer("/raw/raw_key")
            .map(|v| v.is_null())
            .unwrap_or(false));
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
