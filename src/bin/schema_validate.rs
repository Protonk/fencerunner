//! Validate JSON documents against the catalog or boundary schemas.
//!
//! Usage:
//!   schema-validate --mode catalog --file catalogs/macos_codex_v1.json
//!   schema-validate --mode boundary --file tmp/boundary_record.json
//!   schema-validate --mode boundary < record.json

use anyhow::{Context, Result, anyhow, bail};
use clap::Parser;
use fencerunner::{
    BoundarySchema, default_boundary_schema_path, default_catalog_path, find_repo_root,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, stdin};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "schema-validate")]
#[command(about = "Validate JSON against catalog or boundary schemas")]
struct Cli {
    /// Validation mode: catalog or boundary.
    #[arg(long, value_parser = ["catalog", "boundary"])]
    mode: String,
    /// Optional input file; reads stdin when omitted.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Optional catalog path (catalog validation only).
    #[arg(long)]
    catalog: Option<PathBuf>,
    /// Optional boundary schema path (boundary validation only).
    #[arg(long)]
    boundary: Option<PathBuf>,
}

const CATALOG_SCHEMA_TITLE: &str = "Sandbox capability catalog (v1)";
const CATALOG_SCHEMA_REQUIRED_POINTERS: [&str; 6] = [
    "/properties/schema_version/const",
    "/properties/catalog",
    "/properties/scope",
    "/properties/docs",
    "/properties/capabilities",
    "/properties/extensions",
];

fn read_input(file: Option<PathBuf>) -> Result<Value> {
    let mut buf = String::new();
    if let Some(path) = file {
        File::open(&path)
            .with_context(|| format!("opening input file {}", path.display()))?
            .read_to_string(&mut buf)
            .with_context(|| format!("reading input file {}", path.display()))?;
    } else {
        stdin()
            .read_to_string(&mut buf)
            .context("reading stdin for input JSON")?;
    }
    let value: Value = serde_json::from_str(&buf).context("parsing input JSON")?;
    Ok(value)
}

fn extract_schema_version(schema: &Value, pointer: &str) -> Option<String> {
    let version = schema.pointer(pointer).and_then(Value::as_str)?;
    if version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    {
        Some(version.to_string())
    } else {
        None
    }
}

fn validate_catalog_schema_shape(
    schema: &Value,
    schema_path: &PathBuf,
    allowed: &BTreeSet<String>,
) -> Result<()> {
    let title = schema.get("title").and_then(Value::as_str).unwrap_or_default();
    if title != CATALOG_SCHEMA_TITLE {
        bail!(
            "catalog schema title '{}' does not match expected '{}' ({})",
            title,
            CATALOG_SCHEMA_TITLE,
            schema_path.display()
        );
    }

    let schema_type = schema.get("type").and_then(Value::as_str).unwrap_or_default();
    if schema_type != "object" {
        bail!(
            "catalog schema type '{}' does not match expected 'object' ({})",
            schema_type,
            schema_path.display()
        );
    }

    for pointer in CATALOG_SCHEMA_REQUIRED_POINTERS {
        if schema.pointer(pointer).is_none() {
            bail!(
                "catalog schema missing required pointer '{}' ({})",
                pointer,
                schema_path.display()
            );
        }
    }

    let schema_version =
        extract_schema_version(schema, "/properties/schema_version/const").ok_or_else(|| {
            anyhow!("catalog schema missing schema_version const ({})", schema_path.display())
        })?;
    if !allowed.contains(&schema_version) {
        bail!(
            "catalog schema_version '{}' not in allowed set {:?} ({})",
            schema_version,
            allowed,
            schema_path.display()
        );
    }

    Ok(())
}

fn resolve_catalog_schema_path(catalog_path: &PathBuf) -> PathBuf {
    if let Some(parent) = catalog_path.parent() {
        let candidate = parent.join("capability_catalog.schema.json");
        if candidate.exists() {
            return candidate;
        }
    }

    if let Some(base) = catalog_path.parent().and_then(|p| p.parent()) {
        let candidate = base.join("catalogs/capability_catalog.schema.json");
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from("catalogs/capability_catalog.schema.json")
}

fn validate_catalog(input: &Value, catalog_path: &PathBuf) -> Result<()> {
    let allowed = fencerunner::catalog::index::allowed_schema_versions();
    let schema_path = resolve_catalog_schema_path(catalog_path);

    let raw_schema: Arc<Value> = Arc::new(
        serde_json::from_reader(
            File::open(&schema_path)
                .with_context(|| format!("opening catalog schema {}", schema_path.display()))?,
        )
        .with_context(|| format!("parsing catalog schema {}", schema_path.display()))?,
    );
    validate_catalog_schema_shape(&raw_schema, &schema_path, &allowed)?;
    let raw_static: &'static Value = unsafe { &*(Arc::as_ptr(&raw_schema)) };
    let compiled = jsonschema::JSONSchema::compile(raw_static)
        .with_context(|| format!("compiling catalog schema {}", schema_path.display()))?;
    if let Err(errors) = compiled.validate(input) {
        let details = errors.map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
        bail!("catalog failed schema validation:\n{}", details);
    }

    let version = input
        .get("schema_version")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !allowed.contains(&version) {
        bail!(
            "catalog schema_version '{}' not in allowed set {:?}",
            version,
            allowed
        );
    }
    Ok(())
}

fn validate_boundary(input: &Value, boundary_path: &PathBuf) -> Result<()> {
    let schema = BoundarySchema::load(boundary_path)
        .with_context(|| format!("loading boundary schema {}", boundary_path.display()))?;
    schema.validate(input)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = find_repo_root().context("locating repo root")?;
    let input = read_input(cli.file)?;

    match cli.mode.as_str() {
        "catalog" => {
            let catalog_path = cli
                .catalog
                .unwrap_or_else(|| default_catalog_path(&repo_root));
            validate_catalog(&input, &catalog_path)?;
        }
        "boundary" => {
            let boundary_path = cli
                .boundary
                .unwrap_or_else(|| default_boundary_schema_path(&repo_root));
            validate_boundary(&input, &boundary_path)?;
        }
        other => bail!("unknown mode '{}'", other),
    }

    Ok(())
}
