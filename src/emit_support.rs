use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{CapabilityId, CapabilityIndex};

#[derive(Default, Clone)]
/// Builder for probe payloads that enforces “single source of truth” rules.
///
/// The CLI is allowed to specify either a JSON file or inline snippets; mixing
/// both is a contract violation because it makes emitted records ambiguous.
pub struct PayloadArgs {
    payload_file: Option<PathBuf>,
    stdout: Option<TextSource>,
    stderr: Option<TextSource>,
    raw: JsonObjectBuilder,
}

impl PayloadArgs {
    pub fn set_payload_file(&mut self, path: PathBuf) -> Result<()> {
        if self.payload_file.is_some() {
            bail!("--payload-file provided multiple times");
        }
        self.payload_file = Some(path);
        Ok(())
    }

    pub fn set_stdout(&mut self, source: TextSource) -> Result<()> {
        if self.stdout.is_some() {
            bail!("stdout snippet provided multiple times");
        }
        self.stdout = Some(source);
        Ok(())
    }

    pub fn set_stderr(&mut self, source: TextSource) -> Result<()> {
        if self.stderr.is_some() {
            bail!("stderr snippet provided multiple times");
        }
        self.stderr = Some(source);
        Ok(())
    }

    pub fn build(self) -> Result<Value> {
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

    pub fn raw_mut(&mut self) -> &mut JsonObjectBuilder {
        &mut self.raw
    }
}

#[derive(Default, Clone)]
/// Merge-friendly JSON object builder used by payload/operation args.
pub struct JsonObjectBuilder {
    sources: Vec<JsonValueSource>,
}

impl JsonObjectBuilder {
    pub fn merge_json_string(&mut self, raw: &str, label: &str) -> Result<()> {
        let value: Value =
            serde_json::from_str(raw).with_context(|| format!("Invalid JSON for {label}"))?;
        self.push_object(value, label)
    }

    pub fn merge_json_file(&mut self, path: &Path, label: &str) -> Result<()> {
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

    pub fn insert_string(&mut self, key: String, value: String) {
        self.sources.push(JsonValueSource::SetField {
            key,
            value: Value::String(value),
        });
    }

    pub fn insert_json_value(&mut self, key: String, raw: String, label: &str) -> Result<()> {
        let value: Value = serde_json::from_str(&raw)
            .with_context(|| format!("Invalid JSON for {label} value {key}"))?;
        self.sources.push(JsonValueSource::SetField { key, value });
        Ok(())
    }

    pub fn insert_null(&mut self, key: String) {
        self.sources.push(JsonValueSource::SetField {
            key,
            value: Value::Null,
        });
    }

    pub fn insert_list(&mut self, key: String, values: Vec<String>) {
        let arr = values.into_iter().map(Value::String).collect();
        self.sources.push(JsonValueSource::SetField {
            key,
            value: Value::Array(arr),
        });
    }

    pub fn build(&self, label: &str) -> Result<Value> {
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

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }
}

#[derive(Clone)]
enum JsonValueSource {
    MergeObject(Map<String, Value>),
    SetField { key: String, value: Value },
}

#[derive(Clone)]
pub enum TextSource {
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

fn read_json_file(path: &Path) -> Result<Value> {
    let data = fs::read_to_string(path)?;
    serde_json::from_str(&data).context("File contained invalid JSON")
}

pub fn validate_status(status: &str) -> Result<()> {
    match status {
        "success" | "denied" | "partial" | "error" => Ok(()),
        other => bail!("Unknown status: {other} (expected success|denied|partial|error)"),
    }
}

pub fn validate_capability_id(
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

pub fn normalize_secondary_ids(
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

pub fn not_empty(value: &String) -> bool {
    !value.is_empty()
}
