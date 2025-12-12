//! Serializable types for boundary-event records.
//!
//! Shared between the emit/listen binaries and the test suite. The structures
//! mirror the active boundary-object schema (default: `catalogs/cfbo-v1.json`
//! and `schema/boundary_object_schema.json`) so helpers can round-trip JSON without
//! re-parsing ad-hoc maps. When attaching capability context, callers are
//! expected to use snapshots from the capability catalog resolved at runtime.

use crate::catalog::{Capability, CapabilityId, CapabilitySnapshot, CatalogKey, CatalogRepository};
use crate::schema_loader::{SchemaLoadOptions, load_json_schema};
use anyhow::{Context, Result, bail};
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

const DEFAULT_BOUNDARY_PATTERN_VERSION: &str = "boundary_event_v1";
const BOUNDARY_DESCRIPTOR_VERSION: &str = "boundary_schema_v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Full boundary object captured for a single probe execution.
///
/// This struct encodes the boundary-event contract: stack metadata captured at
/// runtime plus the probe/run/operation/result blocks emitted by
/// `bin/emit-record`. `capabilities_schema_version` is expected to be set for
/// new records, but remains optional here so legacy boundary objects can still
/// be parsed.
pub struct BoundaryObject {
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities_schema_version: Option<CatalogKey>,
    pub stack: StackInfo,
    pub probe: ProbeInfo,
    pub run: RunInfo,
    pub operation: OperationInfo,
    pub result: ResultInfo,
    pub payload: Payload,
    pub capability_context: CapabilityContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Environment metadata emitted by `detect-stack`.
///
/// All fields are optional except `os`, which always carries a platform
/// description so downstream consumers can correlate results with host
/// characteristics.
pub struct StackInfo {
    #[serde(default)]
    pub sandbox_mode: Option<String>,
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Identifiers that tie the record back to a probe script and capability.
pub struct ProbeInfo {
    pub id: String,
    pub version: String,
    pub primary_capability_id: CapabilityId,
    #[serde(default)]
    pub secondary_capability_ids: Vec<CapabilityId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Execution context for a specific probe run.
///
/// `workspace_root` is optional because emit-record falls back to git/pwd
/// detection when no override is provided.
pub struct RunInfo {
    pub mode: String,
    #[serde(default)]
    pub workspace_root: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Operation the probe attempted to perform.
///
/// `args` defaults to an empty object to match the schema requirement that the
/// field always be a JSON object (never `null`).
pub struct OperationInfo {
    pub category: String,
    pub verb: String,
    pub target: String,
    #[serde(default = "empty_object")]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Normalized outcome reported by the probe.
pub struct ResultInfo {
    pub observed_result: String,
    #[serde(default)]
    pub raw_exit_code: Option<i64>,
    #[serde(default)]
    pub errno: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Probe-provided payload snippets and structured output.
///
/// `raw` is a free-form JSON object; it defaults to `{}` rather than `null` so
/// schema validation can rely on object semantics.
pub struct Payload {
    #[serde(default)]
    pub stdout_snippet: Option<String>,
    #[serde(default)]
    pub stderr_snippet: Option<String>,
    #[serde(default = "empty_object")]
    pub raw: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Capability snapshots captured alongside the record.
///
/// Snapshots denormalize catalog metadata so boundary objects remain
/// self-describing even if the catalog evolves after the run.
pub struct CapabilityContext {
    pub primary: CapabilitySnapshot,
    #[serde(default)]
    pub secondary: Vec<CapabilitySnapshot>,
}

/// Errors that can occur while reading NDJSON boundary object streams.
#[derive(Debug)]
pub enum BoundaryReadError {
    Io(std::io::Error),
    Parse {
        line: usize,
        error: serde_json::Error,
    },
}

impl BoundaryObject {
    /// Attach capability snapshots from the current catalog to the boundary
    /// object.
    ///
    /// Callers set the catalog version and snapshot fields before emitting the
    /// record so consumers can resolve metadata without reloading a catalog.
    pub fn with_capabilities(
        mut self,
        catalog_key: CatalogKey,
        primary: &Capability,
        secondary: &[&Capability],
    ) -> Self {
        self.capabilities_schema_version = Some(catalog_key);
        self.capability_context = CapabilityContext {
            primary: primary.snapshot(),
            secondary: secondary.iter().map(|c| c.snapshot()).collect(),
        };
        self
    }

    /// Convenience accessor for the primary capability id recorded in the
    /// context snapshot.
    pub fn primary_capability_id(&self) -> &CapabilityId {
        &self.capability_context.primary.id
    }
}

impl CatalogRepository {
    /// Resolve the capability metadata referenced by a boundary object against
    /// the registered catalogs.
    ///
    /// Returns `None` when the record references an unknown catalog key or
    /// capability id. This lookup intentionally trusts the
    /// `capabilities_schema_version` carried in the record so mismatches surface
    /// as empty lookups rather than cross-catalog ambiguities.
    pub fn lookup_context<'a>(
        &'a self,
        bo: &BoundaryObject,
    ) -> Option<(&'a Capability, Vec<&'a Capability>)> {
        let catalog_key = bo.capabilities_schema_version.as_ref()?;
        let catalog = self.get(catalog_key)?;
        let primary = catalog
            .capabilities
            .iter()
            .find(|c| c.id == bo.capability_context.primary.id)?;

        let secondary = bo
            .capability_context
            .secondary
            .iter()
            .filter_map(|snap| catalog.capabilities.iter().find(|c| c.id == snap.id))
            .collect();

        Some((primary, secondary))
    }
}

fn empty_object() -> Value {
    // The boundary schema requires `args`/`raw` to be JSON objects; default to
    // an empty map so callers never emit `null`.
    Value::Object(Default::default())
}

fn allowed_boundary_pattern_versions() -> BTreeSet<String> {
    BTreeSet::from_iter([default_boundary_pattern_version()])
}

fn default_boundary_pattern_version() -> String {
    canonical_boundary_pattern_version()
        .unwrap_or_else(|| DEFAULT_BOUNDARY_PATTERN_VERSION.to_string())
}

fn canonical_boundary_pattern_version() -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(crate::CANONICAL_BOUNDARY_SCHEMA_PATH);
    schema_version_from_path(&path)
}

fn schema_version_from_path(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let value: Value = serde_json::from_reader(file).ok()?;
    value
        .get("schema_version")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn validate_descriptor_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("boundary descriptor key must not be empty");
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        bail!("boundary descriptor key must match ^[A-Za-z0-9_.-]+$, got {key}");
    }
    Ok(())
}

#[derive(Debug)]
struct BoundaryDescriptor {
    key: String,
    pattern_version: String,
}

fn parse_boundary_descriptor(path: &Path) -> Result<Option<BoundaryDescriptor>> {
    let descriptor_file = File::open(path)
        .with_context(|| format!("opening boundary descriptor {}", path.display()))?;
    let descriptor_value: Value = serde_json::from_reader(descriptor_file)
        .with_context(|| format!("parsing boundary descriptor {}", path.display()))?;
    let has_schema_ref =
        descriptor_value.get("schema_path").is_some() || descriptor_value.get("schema").is_some();
    let key = descriptor_value.get("key").and_then(Value::as_str);

    if key.is_none() && !has_schema_ref {
        return Ok(None);
    }

    let version = descriptor_value
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "boundary descriptor {} missing schema_version",
                path.display()
            )
        })?;
    if version != BOUNDARY_DESCRIPTOR_VERSION {
        bail!(
            "boundary descriptor {} expected schema_version {} but found {}",
            path.display(),
            BOUNDARY_DESCRIPTOR_VERSION,
            version
        );
    }

    let key = key
        .ok_or_else(|| anyhow::anyhow!("boundary descriptor {} missing key", path.display()))?
        .to_string();
    validate_descriptor_key(&key)?;

    if !has_schema_ref {
        bail!(
            "boundary descriptor {} missing schema or schema_path field",
            path.display()
        );
    }

    let pattern_version = descriptor_value
        .get("pattern_version")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "boundary descriptor {} missing pattern_version",
                path.display()
            )
        })?
        .to_string();

    Ok(Some(BoundaryDescriptor {
        key,
        pattern_version,
    }))
}

#[derive(Debug)]
/// Loaded boundary-object schema with a cached JSONSchema validator.
pub struct BoundarySchema {
    schema_version: String,
    schema_key: Option<String>,
    compiled: JSONSchema,
    #[allow(dead_code)]
    raw: Arc<Value>,
}

impl BoundarySchema {
    /// Load a boundary-object schema from disk and compile it.
    pub fn load(path: &Path) -> Result<Self> {
        let descriptor = parse_boundary_descriptor(path)?;
        let canonical_schema =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(crate::CANONICAL_BOUNDARY_SCHEMA_PATH);
        let allowed_versions = allowed_boundary_pattern_versions();
        let options = SchemaLoadOptions {
            canonical_schema_path: Some(canonical_schema.as_path()),
            allowed_versions: Some(&allowed_versions),
            ..Default::default()
        };
        let loaded = load_json_schema(path, options)?;
        if let Some(descriptor) = descriptor.as_ref() {
            if descriptor.pattern_version != loaded.schema_version {
                bail!(
                    "boundary descriptor {} declares pattern_version {} but schema reports {}",
                    path.display(),
                    descriptor.pattern_version,
                    loaded.schema_version
                );
            }
        }
        Ok(Self {
            schema_version: loaded.schema_version,
            schema_key: descriptor.map(|d| d.key),
            compiled: loaded.compiled,
            raw: loaded.raw,
        })
    }

    /// Exposes the raw schema value backing the compiled validator.
    pub fn raw_schema(&self) -> &Value {
        &self.raw
    }

    /// Extracted schema version (from `properties.schema_version.const`).
    pub fn schema_version(&self) -> &str {
        &self.schema_version
    }

    /// Boundary schema key declared by the descriptor, if available.
    pub fn schema_key(&self) -> Option<&str> {
        self.schema_key.as_deref()
    }

    /// Validate a JSON value against the compiled schema.
    pub fn validate(&self, value: &Value) -> Result<()> {
        if let Some(expected_key) = &self.schema_key {
            match value.get("schema_key").and_then(Value::as_str) {
                Some(actual) if actual == expected_key => {}
                Some(actual) => bail!(
                    "boundary object schema_key '{}' does not match expected '{}'",
                    actual,
                    expected_key
                ),
                None => bail!(
                    "boundary object missing schema_key (expected {})",
                    expected_key
                ),
            }
        }
        if let Err(errors) = self.compiled.validate(value) {
            let mut details = Vec::new();
            for err in errors {
                details.push(err.to_string());
            }
            bail!(
                "boundary object failed schema validation:\n{}",
                details.join("\n")
            );
        }
        Ok(())
    }
}

impl fmt::Display for BoundaryReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BoundaryReadError::Io(err) => write!(f, "failed to read NDJSON stream: {err}"),
            BoundaryReadError::Parse { line, error } => {
                write!(f, "line {line}: unable to parse boundary object ({error})")
            }
        }
    }
}

impl std::error::Error for BoundaryReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BoundaryReadError::Io(err) => Some(err),
            BoundaryReadError::Parse { error, .. } => Some(error),
        }
    }
}

/// Read boundary objects from an NDJSON stream.
///
/// Lines containing only whitespace are skipped. Errors include the 1-based
/// line number where parsing failed to simplify diagnostics for callers.
pub fn read_boundary_objects<R: BufRead>(
    reader: R,
) -> Result<Vec<BoundaryObject>, BoundaryReadError> {
    let mut records = Vec::new();
    let mut line_buf = String::new();
    let mut reader = reader;
    let mut line_number = 0usize;

    loop {
        line_buf.clear();
        let bytes = reader
            .read_line(&mut line_buf)
            .map_err(BoundaryReadError::Io)?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        let trimmed = line_buf.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = serde_json::from_str::<BoundaryObject>(trimmed).map_err(|error| {
            BoundaryReadError::Parse {
                line: line_number,
                error,
            }
        })?;
        records.push(record);
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashSet;
    use std::io::{BufReader, Cursor};

    #[test]
    fn parses_golden_snippet_ndjson() {
        let records =
            read_boundary_objects(golden_snippet_reader()).expect("golden snippet parses");
        assert_eq!(records.len(), 3, "golden snippet should have 3 records");

        let has_success = records
            .iter()
            .any(|record| record.result.observed_result == "success");
        assert!(has_success, "expected at least one success record");

        let has_non_success = records
            .iter()
            .any(|record| record.result.observed_result != "success");
        assert!(
            has_non_success,
            "expected at least one non-success record for variety"
        );

        let unique_probes: HashSet<&str> = records
            .iter()
            .map(|record| record.probe.id.as_str())
            .collect();
        assert!(
            unique_probes.len() > 1,
            "expected multiple distinct probe ids"
        );
    }

    #[test]
    fn ignores_blank_lines() {
        let first = sample_record("probe_one", "success");
        let second = sample_record("probe_two", "partial");
        let ndjson = format!("{first}\n  \n{second}\n");
        let cursor = Cursor::new(ndjson.into_bytes());
        let records = read_boundary_objects(BufReader::new(cursor)).expect("parses with blanks");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].probe.id, "probe_one");
        assert_eq!(records[1].probe.id, "probe_two");
    }

    #[test]
    fn reports_line_numbers_on_parse_error() {
        let first = sample_record("probe_one", "success");
        let ndjson = format!("{first}\n{first}\n{{ invalid json }}\n");
        let cursor = Cursor::new(ndjson.into_bytes());
        let err = read_boundary_objects(BufReader::new(cursor)).expect_err("should fail");
        match err {
            BoundaryReadError::Parse { line, .. } => assert_eq!(line, 3),
            other => panic!("expected parse error, got {:?}", other),
        }
    }

    fn sample_record(probe_id: &str, observed_result: &str) -> String {
        let schema_version = current_schema_version();
        let schema_key = current_schema_key();
        let catalog_key = current_catalog_key();
        json!({
            "schema_version": schema_version,
            "schema_key": schema_key,
            "capabilities_schema_version": catalog_key,
            "stack": {
                "sandbox_mode": null,
                "os": "Darwin 23.4.0 arm64"
            },
            "probe": {
                "id": probe_id,
                "version": "1",
                "primary_capability_id": "cap_sample",
                "secondary_capability_ids": []
            },
            "run": {
                "mode": "baseline",
                "workspace_root": "/tmp/sample",
                "command": "/bin/true"
            },
            "operation": {
                "category": "fs",
                "verb": "read",
                "target": "sample",
                "args": {}
            },
            "result": {
                "observed_result": observed_result,
                "raw_exit_code": 0,
                "errno": null,
                "message": null,
                "error_detail": null
            },
            "payload": {
                "stdout_snippet": null,
                "stderr_snippet": null,
                "raw": {}
            },
            "capability_context": {
                "primary": {
                    "id": "cap_sample",
                    "category": "fs",
                    "layer": "os_sandbox"
                },
                "secondary": []
            }
        })
        .to_string()
    }

    fn golden_snippet_reader() -> BufReader<Cursor<Vec<u8>>> {
        let records = vec![
            sample_record("probe_success", "success"),
            sample_record("probe_denied", "denied"),
            sample_record("probe_partial", "partial"),
        ];
        let ndjson = records.join("\n");
        BufReader::new(Cursor::new(ndjson.into_bytes()))
    }

    fn current_schema_version() -> String {
        let repo = crate::find_repo_root().expect("repo root available");
        let path = crate::default_boundary_descriptor_path(&repo);
        BoundarySchema::load(&path)
            .expect("boundary schema loads")
            .schema_version()
            .to_string()
    }

    fn current_schema_key() -> String {
        let repo = crate::find_repo_root().expect("repo root available");
        let path = crate::default_boundary_descriptor_path(&repo);
        BoundarySchema::load(&path)
            .expect("boundary schema loads")
            .schema_key()
            .map(str::to_string)
            .expect("boundary schema key")
    }

    fn current_catalog_key() -> CatalogKey {
        let repo = crate::find_repo_root().expect("repo root available");
        let path = crate::default_catalog_path(&repo);
        crate::load_catalog_from_path(&path)
            .expect("catalog loads")
            .catalog
            .key
    }
}
