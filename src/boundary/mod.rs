//! Serializable types for the `cfbo-v1` boundary object.
//!
//! Shared between the emit/listen binaries and the test suite. The structures
//! mirror `schema/boundary_object.json` so helpers can round-trip JSON without
//! re-parsing ad-hoc maps. When attaching capability context, callers are
//! expected to use snapshots from the capability catalog resolved at runtime.

use crate::catalog::{Capability, CapabilityId, CapabilitySnapshot, CatalogKey, CatalogRepository};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Full boundary object captured for a single probe execution.
///
/// This struct encodes the cfbo-v1 contract: stack metadata captured at runtime
/// plus the probe/run/operation/result blocks emitted by `bin/emit-record`.
/// `capabilities_schema_version` remains `None` until a catalog snapshot is
/// attached via [`BoundaryObject::with_capabilities`].
pub struct BoundaryObject {
    pub schema_version: String,
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
    pub codex_cli_version: Option<String>,
    #[serde(default)]
    pub codex_profile: Option<String>,
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
    // The cfbo schema requires `args`/`raw` to be JSON objects; default to an
    // empty map so callers never emit `null`.
    Value::Object(Default::default())
}
