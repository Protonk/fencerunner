use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Versioned key for a capability catalog (e.g., `macOS_codex_v1`).
///
/// Stored alongside boundary objects so consumers can resolve capability IDs
/// against the correct catalog snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CatalogKey(pub String);

/// Stable identifier for an individual capability entry.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityId(pub String);

/// High-level capability grouping mirrored from the catalog schema.
///
/// Known variants keep serialization consistent; `Other` preserves forward
/// compatibility with catalogs that introduce new categories.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityCategory {
    Filesystem,
    Process,
    Network,
    Sysctl,
    Ipc,
    SandboxProfile,
    AgentSandboxPolicy,
    Other(String),
}

/// Policy layer exercised by the capability.
///
/// The values align with the catalog schema; `Other` allows new layers to be
/// represented without breaking older binaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CapabilityLayer {
    OsSandbox,
    AgentRuntime,
    Other(String),
}

/// Compact capability snapshot attached to boundary objects.
///
/// Snapshots denormalize catalog metadata into cfbo records so they remain
/// self-describing even when the catalog evolves on disk.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    pub id: CapabilityId,
    pub category: CapabilityCategory,
    pub layer: CapabilityLayer,
}

impl Serialize for CapabilityCategory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CapabilityCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_str(&value))
    }
}

impl CapabilityCategory {
    pub fn as_str(&self) -> &str {
        match self {
            CapabilityCategory::Filesystem => "filesystem",
            CapabilityCategory::Process => "process",
            CapabilityCategory::Network => "network",
            CapabilityCategory::Sysctl => "sysctl",
            CapabilityCategory::Ipc => "ipc",
            CapabilityCategory::SandboxProfile => "sandbox_profile",
            CapabilityCategory::AgentSandboxPolicy => "agent_sandbox_policy",
            CapabilityCategory::Other(value) => value.as_str(),
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "filesystem" => CapabilityCategory::Filesystem,
            "process" => CapabilityCategory::Process,
            "network" => CapabilityCategory::Network,
            "sysctl" => CapabilityCategory::Sysctl,
            "ipc" => CapabilityCategory::Ipc,
            "sandbox_profile" => CapabilityCategory::SandboxProfile,
            "agent_sandbox_policy" => CapabilityCategory::AgentSandboxPolicy,
            other => CapabilityCategory::Other(other.to_string()),
        }
    }
}

impl Serialize for CapabilityLayer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CapabilityLayer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_str(&value))
    }
}

impl CapabilityLayer {
    pub fn as_str(&self) -> &str {
        match self {
            CapabilityLayer::OsSandbox => "os_sandbox",
            CapabilityLayer::AgentRuntime => "agent_runtime",
            CapabilityLayer::Other(value) => value.as_str(),
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "os_sandbox" => CapabilityLayer::OsSandbox,
            "agent_runtime" => CapabilityLayer::AgentRuntime,
            other => CapabilityLayer::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_round_trips_known_and_unknown() {
        let known = CapabilityCategory::SandboxProfile;
        let json = serde_json::to_string(&known).unwrap();
        assert_eq!(json.trim_matches('"'), "sandbox_profile");
        let back: CapabilityCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(back, known);

        let custom_json = "\"custom_category\"";
        let parsed: CapabilityCategory = serde_json::from_str(custom_json).unwrap();
        assert_eq!(
            parsed,
            CapabilityCategory::Other("custom_category".to_string())
        );
        let serialized = serde_json::to_string(&parsed).unwrap();
        assert_eq!(serialized, custom_json);
    }

    #[test]
    fn layer_round_trips_known_and_unknown() {
        let known = CapabilityLayer::AgentRuntime;
        let json = serde_json::to_string(&known).unwrap();
        assert_eq!(json.trim_matches('"'), "agent_runtime");
        let back: CapabilityLayer = serde_json::from_str(&json).unwrap();
        assert_eq!(back, known);

        let other_json = "\"custom_layer\"";
        let parsed: CapabilityLayer = serde_json::from_str(other_json).unwrap();
        assert_eq!(parsed, CapabilityLayer::Other("custom_layer".to_string()));
        let serialized = serde_json::to_string(&parsed).unwrap();
        assert_eq!(serialized, other_json);
    }

    #[test]
    fn snapshot_serde_matches_schema() {
        let snapshot = CapabilitySnapshot {
            id: CapabilityId("cap_example".into()),
            category: CapabilityCategory::Filesystem,
            layer: CapabilityLayer::OsSandbox,
        };
        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("cap_example"));
        assert_eq!(
            json.get("category").and_then(|v| v.as_str()),
            Some("filesystem")
        );
        assert_eq!(
            json.get("layer").and_then(|v| v.as_str()),
            Some("os_sandbox")
        );

        let back: CapabilitySnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(back.id.0, "cap_example");
        assert!(matches!(back.category, CapabilityCategory::Filesystem));
        assert!(matches!(back.layer, CapabilityLayer::OsSandbox));
    }

    #[test]
    fn catalog_key_and_id_round_trip() {
        let key = CatalogKey("macOS_codex_v1".to_string());
        let serialized = serde_json::to_string(&key).unwrap();
        assert_eq!(serialized, "\"macOS_codex_v1\"");
        let parsed: CatalogKey = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, key);

        let id = CapabilityId("cap_fs_read_workspace_tree".to_string());
        let serialized_id = serde_json::to_string(&id).unwrap();
        assert_eq!(serialized_id, "\"cap_fs_read_workspace_tree\"");
        let parsed_id: CapabilityId = serde_json::from_str(&serialized_id).unwrap();
        assert_eq!(parsed_id, id);
    }
}
