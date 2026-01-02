//! Deserializable representation of a script commitments registry.
//!
//! The structs in this module mirror `schema/commitments.json`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommitmentRegistry {
    #[serde(rename = "schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub commitments: Vec<Commitment>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Commitment {
    pub id: String,
    pub provider: CommitmentProvider,
    pub helps: Vec<CommitmentHelp>,
    #[serde(rename = "is")]
    pub is_description: String,
    pub at: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum CommitmentProvider {
    Runner,
    System,
    User,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum CommitmentHelp {
    Ensure,
    Detect,
    Emit,
}
