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
