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
/// All fields are optional except `os`/`container_tag`, which always carry a
/// platform description so downstream consumers can correlate results with host
/// characteristics.
pub struct StackInfo {
    #[serde(default)]
    pub codex_cli_version: Option<String>,
    #[serde(default)]
    pub codex_profile: Option<String>,
    #[serde(default)]
    pub codex_model: Option<String>,
    #[serde(default)]
    pub sandbox_mode: Option<String>,
    pub os: String,
    pub container_tag: String,
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
    pub duration_ms: Option<i64>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::identity::{CapabilityCategory, CapabilityLayer, CatalogKey};
    use crate::catalog::load_catalog_from_path;
    use crate::catalog::repository::CatalogRepository;
    use std::path::PathBuf;

    fn catalog_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("schema")
            .join("capabilities.json")
    }

    fn sample_boundary_object() -> BoundaryObject {
        BoundaryObject {
            schema_version: "cfbo-v1".to_string(),
            capabilities_schema_version: None,
            stack: StackInfo {
                codex_cli_version: Some("1.0".to_string()),
                codex_profile: None,
                codex_model: Some("gpt".to_string()),
                sandbox_mode: Some("workspace-write".to_string()),
                os: "Darwin".to_string(),
                container_tag: "local-macos".to_string(),
            },
            probe: ProbeInfo {
                id: "probe".to_string(),
                version: "1".to_string(),
                primary_capability_id: CapabilityId("cap_id".to_string()),
                secondary_capability_ids: vec![],
            },
            run: RunInfo {
                mode: "baseline".to_string(),
                workspace_root: Some("/tmp".to_string()),
                command: "echo test".to_string(),
            },
            operation: OperationInfo {
                category: "fs".to_string(),
                verb: "read".to_string(),
                target: "/dev/null".to_string(),
                args: empty_object(),
            },
            result: ResultInfo {
                observed_result: "success".to_string(),
                raw_exit_code: Some(0),
                errno: None,
                message: None,
                duration_ms: Some(10),
                error_detail: None,
            },
            payload: Payload {
                stdout_snippet: None,
                stderr_snippet: None,
                raw: empty_object(),
            },
            capability_context: CapabilityContext {
                primary: CapabilitySnapshot {
                    id: CapabilityId("cap_id".to_string()),
                    category: CapabilityCategory::Other("cat".to_string()),
                    layer: CapabilityLayer::Other("layer".to_string()),
                },
                secondary: Vec::new(),
            },
        }
    }

    #[test]
    fn boundary_object_round_trips() {
        let bo = sample_boundary_object();
        let value = serde_json::to_value(&bo).expect("serialized");
        assert_eq!(
            value.get("schema_version").and_then(|v| v.as_str()),
            Some("cfbo-v1")
        );
        let back: BoundaryObject = serde_json::from_value(value).expect("deserialized");
        assert_eq!(back.schema_version, "cfbo-v1");
        assert_eq!(back.run.mode, "baseline");
        assert_eq!(back.capability_context.primary.id.0, "cap_id");
    }

    #[test]
    fn capabilities_schema_version_serializes() {
        let mut bo = sample_boundary_object();
        bo.capabilities_schema_version = Some(CatalogKey("macOS_codex_v1".to_string()));
        let value = serde_json::to_value(&bo).expect("serialized");
        assert_eq!(
            value
                .get("capabilities_schema_version")
                .and_then(|v| v.as_str()),
            Some("macOS_codex_v1")
        );
    }

    #[test]
    fn repository_lookup_context_matches_capabilities() {
        let catalog = load_catalog_from_path(&catalog_path()).expect("catalog loads");
        let key = catalog.key.clone();
        let (bo, primary_id, secondary_ids) = {
            let primary = catalog.capabilities.first().expect("cap present");
            let secondary = catalog
                .capabilities
                .get(1)
                .map(|cap| vec![cap])
                .unwrap_or_default();

            let bo = sample_boundary_object().with_capabilities(key.clone(), primary, &secondary);
            let primary_id = primary.id.clone();
            let secondary_ids: Vec<CapabilityId> =
                secondary.iter().map(|cap| cap.id.clone()).collect();

            (bo, primary_id, secondary_ids)
        };

        let mut repo = CatalogRepository::default();
        repo.register(catalog);

        let (resolved_primary, resolved_secondary) =
            repo.lookup_context(&bo).expect("context resolved");
        assert_eq!(resolved_primary.id, primary_id);
        if let Some(first_secondary) = secondary_ids.first() {
            assert_eq!(resolved_secondary.first().unwrap().id, *first_secondary);
        }
    }

    #[test]
    fn capability_snapshot_serializes_to_expected_shape() {
        let snapshot = CapabilitySnapshot {
            id: CapabilityId("cap_test".to_string()),
            category: CapabilityCategory::Filesystem,
            layer: CapabilityLayer::OsSandbox,
        };
        let ctx = CapabilityContext {
            primary: snapshot.clone(),
            secondary: vec![snapshot.clone()],
        };
        let value = serde_json::to_value(&ctx).unwrap();
        assert_eq!(
            value
                .get("primary")
                .and_then(|v| v.get("category"))
                .and_then(|v| v.as_str()),
            Some("filesystem")
        );
        assert_eq!(
            value
                .get("secondary")
                .and_then(|v| v.as_array())
                .map(|arr| arr.len()),
            Some(1)
        );
    }
}
