#![cfg(unix)]

// Catalog repository and capability lookup guard rails.
mod support;
#[path = "support/common.rs"]
mod common;

use anyhow::Result;
use fencerunner::catalog::{
    CapabilityCategory, CapabilityIndex, CapabilityLayer, CatalogRepository, load_catalog_from_path,
};
use serde_json::json;
use std::fs;
use tempfile::{NamedTempFile, tempdir};

use common::{catalog_path, sample_boundary_object};

// Ensure catalog snapshots attached to a boundary object can be resolved back
// through the repository, preserving stable ids for downstream consumers.
#[test]
fn repository_lookup_context_matches_capabilities() -> Result<()> {
    let catalog = load_catalog_from_path(&catalog_path())?;
    let key = catalog.catalog.key.clone();
    let primary = catalog.capabilities.first().expect("cap present");
    let secondary = catalog
        .capabilities
        .get(1)
        .map(|cap| vec![cap])
        .unwrap_or_default();
    let primary_id = primary.id.clone();
    let secondary_ids: Vec<_> = secondary.iter().map(|cap| cap.id.clone()).collect();
    let bo = sample_boundary_object().with_capabilities(key.clone(), primary, &secondary);
    let mut repo = CatalogRepository::default();
    repo.register(catalog);

    let (resolved_primary, resolved_secondary) = repo.lookup_context(&bo).expect("context");
    assert_eq!(resolved_primary.id, primary_id);
    if let Some(expected_secondary) = secondary_ids.first() {
        assert_eq!(resolved_secondary.first().unwrap().id, *expected_secondary);
    }
    Ok(())
}

// Smoke-test that the bundled catalog loads and has non-empty metadata.
// Limitation: this doesn't validate every catalog field; schema tests do that.
#[test]
fn load_real_catalog_smoke() -> Result<()> {
    let catalog = load_catalog_from_path(&catalog_path())?;
    assert!(!catalog.catalog.key.0.is_empty());
    assert!(!catalog.capabilities.is_empty());
    for cap in catalog.capabilities {
        assert!(!cap.id.0.is_empty());
        assert!(
            !matches!(cap.category, CapabilityCategory::Other(ref v) if v.is_empty()),
            "category should not be empty"
        );
        assert!(
            !matches!(cap.layer, CapabilityLayer::Other(ref v) if v.is_empty()),
            "layer should not be empty"
        );
    }
    Ok(())
}

// Validate that CatalogRepository can resolve a known capability id once a
// catalog is registered.
#[test]
fn finds_capability_in_registered_catalog() -> Result<()> {
    let catalog = load_catalog_from_path(&catalog_path())?;
    let key = catalog.catalog.key.clone();
    let known_capability = catalog
        .capabilities
        .first()
        .expect("catalog should have capabilities")
        .id
        .clone();

    let mut repo = CatalogRepository::default();
    repo.register(catalog);

    let resolved = repo.find_capability(&key, &known_capability);
    assert!(resolved.is_some());
    Ok(())
}

// CapabilityIndex should reject unknown schema_version values early.
// This protects against silently loading mismatched catalogs.
#[test]
fn capability_index_enforces_schema_version() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "unexpected",
            "scope": {"description": "test", "policy_layers": {}, "categories": {}},
            "sources": {},
            "capabilities": []
        }),
    )?;
    assert!(CapabilityIndex::load(file.path()).is_err());
    Ok(())
}

// Custom catalog schema versions are not allowed; ensure they are rejected.
#[test]
fn capability_index_rejects_custom_schema_version() -> Result<()> {
    let mut temp = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut temp,
        &json!({
            "schema_version": "custom_catalog_v1",
            "catalog": {"key": "custom_catalog_v1", "title": "custom catalog"},
            "scope": {
                "description": "test",
                "policy_layers": {"os_sandbox": "fixture layer"},
                "categories": {"filesystem": "fixture"}
            },
            "sources": {},
            "capabilities": [{
                "id": "cap_fs_custom",
                "category": "filesystem",
                "layer": "os_sandbox",
                "description": "cap fs",
                "operations": {"allow": [], "deny": []}
            }]
        }),
    )?;

    assert!(
        CapabilityIndex::load(temp.path()).is_err(),
        "custom catalog schema_version should be rejected"
    );
    Ok(())
}

// When a catalog sits next to a schema file, CapabilityIndex should use it.
// This test feeds a schema with a wrong title to ensure the loader enforces it.
#[test]
fn capability_index_rejects_schema_with_wrong_title() -> Result<()> {
    let dir = tempdir()?;
    let schema_path = dir.path().join("capability_catalog.schema.json");
    let catalog_path = dir.path().join("catalog.json");

    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Wrong title",
        "type": "object",
        "properties": {
            "schema_version": {"const": "sandbox_catalog_v1"},
            "catalog": {},
            "scope": {},
            "sources": {},
            "capabilities": {},
            "extensions": {}
        }
    });
    fs::write(&schema_path, serde_json::to_vec(&schema)?)?;

    let catalog = json!({
        "schema_version": "sandbox_catalog_v1",
        "catalog": {"key": "fixture", "title": "fixture"},
        "scope": {
            "description": "fixture",
            "policy_layers": {"os_sandbox": "fixture layer"},
            "categories": {"filesystem": "fixture"}
        },
        "sources": {},
        "capabilities": [{
            "id": "cap_fixture",
            "category": "filesystem",
            "layer": "os_sandbox",
            "description": "fixture",
            "operations": {"allow": [], "deny": []}
        }]
    });
    fs::write(&catalog_path, serde_json::to_vec(&catalog)?)?;

    assert!(CapabilityIndex::load(&catalog_path).is_err());
    Ok(())
}

// Sources are optional but must be preserved when present; ensure they're parsed.
#[test]
fn capability_index_accepts_sources_map() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "schema_version": "sandbox_catalog_v1",
            "catalog": {"key": "sample_catalog_v1", "title": "sample catalog"},
            "scope": {
                "description": "test",
                "policy_layers": {"os_sandbox": "sandbox"},
                "categories": {"filesystem": "fs"}
            },
            "sources": {
                "apple_sandbox_guide": {"title": "Apple Sandbox Guide"}
            },
            "capabilities": [{
                "id": "cap_fs_read_workspace_tree",
                "category": "filesystem",
                "layer": "os_sandbox",
                "description": "fixture",
                "operations": {"allow": [], "deny": []},
                "sources": {
                    "apple_sandbox_guide": {"section": "2"}
                }
            }]
        }),
    )?;

    let index = CapabilityIndex::load(file.path())?;
    let capability = &index.catalog().capabilities[0];
    assert_eq!(capability.sources.len(), 1);
    assert_eq!(capability.sources[0].doc, "apple_sandbox_guide");
    assert_eq!(capability.sources[0].section.as_deref(), Some("2"));
    Ok(())
}
