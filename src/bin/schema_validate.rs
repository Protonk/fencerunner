//! Validate JSON documents against the catalog or boundary schemas.
//!
//! Usage:
//!   schema-validate --mode catalog --file catalogs/macos_codex_v1.json
//!   schema-validate --mode boundary --file boundaries/cfbo-v1.json
//!   schema-validate --mode boundary < payload.json

use anyhow::{Context, Result, bail};
use clap::Parser;
use fencerunner::{
    BoundarySchema, default_boundary_descriptor_path, default_catalog_path, find_repo_root,
};
use serde_json::Value;
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
    /// Optional catalog descriptor path (for boundary validation, used to resolve capability keys).
    #[arg(long)]
    catalog: Option<PathBuf>,
    /// Optional boundary descriptor path (for boundary validation).
    #[arg(long)]
    boundary: Option<PathBuf>,
}

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

fn validate_catalog(input: &Value, catalog_path: &PathBuf) -> Result<()> {
    let allowed = fencerunner::catalog::index::allowed_schema_versions();
    let schema_path = catalog_path
        .parent()
        .and_then(|p| p.parent())
        .map(|base| base.join("schema/capability_catalog.schema.json"))
        .unwrap_or_else(|| PathBuf::from("schema/capability_catalog.schema.json"));

    let raw_schema: Arc<Value> = Arc::new(
        serde_json::from_reader(
            File::open(&schema_path)
                .with_context(|| format!("opening catalog schema {}", schema_path.display()))?,
        )
        .with_context(|| format!("parsing catalog schema {}", schema_path.display()))?,
    );
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
        .with_context(|| format!("loading boundary descriptor {}", boundary_path.display()))?;
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
                .unwrap_or_else(|| default_boundary_descriptor_path(&repo_root));
            validate_boundary(&input, &boundary_path)?;
        }
        other => bail!("unknown mode '{}'", other),
    }

    Ok(())
}
