//! Serializable types for boundary-object records.
//!
//! Shared between the emit/listen binaries and the test suite. The structures
//! mirror the minimal boundary-object schema in
//! `boundary/boundary_object_schema.json` so helpers can round-trip JSON without
//! ad-hoc maps. Optional context/payload blocks allow probes to attach richer
//! metadata without changing the required shape.
//!
//! The boundary object is the "contract boundary" between a messy runtime and
//! clean analysis. Probes can do anything internally, but they must summarize
//! the attempted operation and observed outcome using this schema so downstream
//! tools stay deterministic.

use crate::catalog::{Capability, CapabilityId, CapabilitySnapshot, CatalogKey, CatalogRepository};
use anyhow::{Context, Result, bail};
use jsonschema::JSONSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Boundary object captured for a single probe execution.
///
/// Only `probe`, `operation`, and `result` are required; additional metadata
/// may be carried under `context`, `payload`, or `extensions`.
pub struct BoundaryObject {
    pub probe: ProbeInfo,
    pub operation: OperationInfo,
    pub result: ResultInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// Extensions are allowed but not interpreted by core tooling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Identifiers that tie the record back to a probe script.
pub struct ProbeInfo {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Operation the probe attempted to perform.
pub struct OperationInfo {
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Normalized outcome reported by the probe.
pub struct ResultInfo {
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<ResultDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
/// Optional details about the result (exit codes, error messages, etc.).
pub struct ResultDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errno: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
/// Optional context attached to a boundary object.
///
/// Known fields are modeled explicitly; unknown entries are preserved in
/// `extra` so the record can be round-tripped without loss.
pub struct ContextInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<StackInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities_schema_version: Option<CatalogKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_context: Option<CapabilityContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<ProbeContext>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(flatten)]
    // Unknown keys are preserved here so newer emitters do not break older
    // listeners. This is a deliberate forward-compatibility hook.
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Capability identifiers associated with a probe.
pub struct ProbeContext {
    pub primary_capability_id: CapabilityId,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secondary_capability_ids: Vec<CapabilityId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Execution context for a specific probe run.
///
/// `workspace_root` is optional because emit-record falls back to git/pwd
/// detection when no override is provided.
pub struct RunInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Environment metadata emitted by `detect-stack`.
pub struct StackInfo {
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Capability snapshots captured alongside the record.
///
/// Snapshots denormalize catalog metadata so boundary objects remain
/// self-describing even if the catalog evolves after the run.
pub struct CapabilityContext {
    pub primary: CapabilitySnapshot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
        // Mutate in place because the emitter typically constructs the record
        // first and then attaches catalog snapshots before serialization.
        let context = self.ensure_context();
        context.capabilities_schema_version = Some(catalog_key);
        context.capability_context = Some(CapabilityContext {
            primary: primary.snapshot(),
            secondary: secondary.iter().map(|c| c.snapshot()).collect(),
        });
        self
    }

    fn ensure_context(&mut self) -> &mut ContextInfo {
        if self.context.is_none() {
            self.context = Some(ContextInfo::default());
        }
        self.context.as_mut().expect("context exists")
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
        // Use the catalog key embedded in the record. This keeps lookups
        // explicit even if multiple catalogs are loaded in memory.
        let context = bo.context.as_ref()?;
        let catalog_key = context.capabilities_schema_version.as_ref()?;
        let snapshot = context.capability_context.as_ref()?;
        let catalog = self.get(catalog_key)?;
        let primary = catalog
            .capabilities
            .iter()
            .find(|c| c.id == snapshot.primary.id)?;

        let secondary = snapshot
            .secondary
            .iter()
            .filter_map(|snap| catalog.capabilities.iter().find(|c| c.id == snap.id))
            .collect();

        Some((primary, secondary))
    }
}

#[derive(Debug)]
/// Loaded boundary-object schema with a cached JSONSchema validator.
pub struct BoundarySchema {
    compiled: JSONSchema,
    #[allow(dead_code)]
    raw: Arc<Value>,
}

impl BoundarySchema {
    /// Load a boundary-object schema from disk and compile it.
    pub fn load(path: &Path) -> Result<Self> {
        let value: Value = serde_json::from_reader(
            File::open(path).with_context(|| format!("opening boundary schema {}", path.display()))?,
        )
        .with_context(|| format!("parsing boundary schema {}", path.display()))?;

        // Keep a strong reference to the raw schema alongside the compiled
        // validator so callers can inspect it for diagnostics.
        let raw = Arc::new(value);
        let raw_static: &'static Value = unsafe { &*(Arc::as_ptr(&raw)) };
        let compiled = JSONSchema::compile(raw_static)
            .with_context(|| format!("compiling boundary schema {}", path.display()))?;
        Ok(Self { compiled, raw })
    }

    /// Exposes the raw schema value backing the compiled validator.
    pub fn raw_schema(&self) -> &Value {
        &self.raw
    }

    /// Validate a JSON value against the compiled schema.
    pub fn validate(&self, value: &Value) -> Result<()> {
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
    // Streaming read so large NDJSON inputs do not need to fit in memory.
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
            .any(|record| record.result.outcome == "success");
        assert!(has_success, "expected at least one success record");

        let has_non_success = records
            .iter()
            .any(|record| record.result.outcome != "success");
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

    fn sample_record(probe_id: &str, outcome: &str) -> String {
        json!({
            "probe": {
                "id": probe_id
            },
            "operation": {
                "kind": "fs.read",
                "target": "sample",
                "args": {}
            },
            "result": {
                "outcome": outcome,
                "details": {
                    "exit_code": 0
                }
            },
            "context": {
                "run": {
                    "workspace_root": "/tmp/sample",
                    "command": "/bin/true"
                },
                "stack": {
                    "os": "Darwin 23.4.0 arm64"
                }
            },
            "payload": {
                "stdout_snippet": null,
                "stderr_snippet": null,
                "raw": {}
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
}
