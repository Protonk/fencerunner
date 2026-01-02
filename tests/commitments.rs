#![cfg(unix)]

// Probe commitments registry guard rails.
mod support;

use anyhow::Result;
use fencerunner::commitments::index::CommitmentIndex;
use fencerunner::commitments::model::CommitmentHelp;
use serde_json::json;
use support::repo_root;
use tempfile::NamedTempFile;

#[test]
fn load_default_commitments_registry_smoke() -> Result<()> {
    let repo_root = repo_root();
    let registry_path = repo_root.join("probes/commitments.json");
    let index = CommitmentIndex::load(&registry_path)?;
    assert!(index.supports_help("emit.record", CommitmentHelp::Emit));
    assert!(index.supports_help("python3", CommitmentHelp::Ensure));
    Ok(())
}

#[test]
fn commitments_registry_rejects_duplicate_id() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "commitments_v1",
            "commitments": [
                {"id":"dup","provider":"system","helps":["ensure"],"is":"a","at":"python3","version":"v1"},
                {"id":"dup","provider":"system","helps":["ensure"],"is":"b","at":"python3","version":"v1"}
            ]
        }),
    )?;
    assert!(CommitmentIndex::load(file.path()).is_err());
    Ok(())
}

#[test]
fn commitments_registry_rejects_runner_absolute_at() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "commitments_v1",
            "commitments": [
                {"id":"emit.record","provider":"runner","helps":["emit"],"is":"x","at":"/bin/emit-record","version":"v1"}
            ]
        }),
    )?;
    assert!(CommitmentIndex::load(file.path()).is_err());
    Ok(())
}

#[test]
fn commitments_registry_rejects_unknown_schema_version() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "unexpected",
            "commitments": []
        }),
    )?;
    assert!(CommitmentIndex::load(file.path()).is_err());
    Ok(())
}
