//! Serializable types for boundary-object records.
//!
//! Shared between fencerunner, the script-facing helpers, and the test suite.
//! The structures mirror the default run-dir boundaries contract shipped in `scripts/boundaries.json`
//! (record_schema) so helpers can round-trip JSON without ad-hoc maps. Optional
//! context blocks allow scripts to attach richer metadata without changing the
//! required shape.
//!
//! The boundary object is the "contract boundary" between a messy runtime and
//! clean analysis. Scripts can do anything internally, but they must summarize
//! the attempted operation and observed outcome using this schema so downstream
//! tools stay deterministic.

use crate::commitments::model::CommitmentHelp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Boundary object captured for a single script execution.
///
/// The boundary object always includes `context.commitments` and `payload`;
/// additional metadata may be carried under `extensions` or extra
/// `context` keys.
pub struct BoundaryObject {
    pub script: ScriptInfo,
    pub operation: OperationInfo,
    pub result: ResultInfo,
    #[serde(default)]
    pub context: ContextInfo,
    /// Required payload (includes stdout/stderr snippets).
    pub payload: Value,
    /// Extensions are allowed but not interpreted by core tooling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Identifiers that tie the record back to a script.
pub struct ScriptInfo {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Operation the script attempted to perform.
pub struct OperationInfo {
    pub kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Normalized outcome reported by the script.
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
    /// Commitments the script enrolled in via `commit_help_me`.
    #[serde(default)]
    pub commitments: Vec<CommitmentEnrollment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<StackInfo>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[serde(flatten)]
    // Unknown keys are preserved here so newer emitters do not break older
    // listeners. This is a deliberate forward-compatibility hook.
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Execution context for a specific script run.
///
/// `workspace_root` is optional and omitted by default; run dirs may choose to
/// populate it via non-core tooling if it helps downstream consumers.
pub struct RunInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Optional environment metadata emitted by the runner.
pub struct StackInfo {
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Commitment id plus the set of enrolled help verbs.
pub struct CommitmentEnrollment {
    pub id: String,
    pub helps: Vec<CommitmentHelp>,
}
