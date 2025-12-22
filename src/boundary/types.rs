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

use crate::catalog::{CapabilityId, CapabilitySnapshot, CatalogKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

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
