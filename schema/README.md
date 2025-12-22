# Schema Contracts

This directory holds the canonical JSON Schema contracts used by the harness.
Each schema has a fixed pattern (schema_version const) and a small, explicit
load/validate pipeline. This document summarizes the pattern -> loader ->
validator -> instance path for each contract.

## Terms

- Pattern: the versioned schema contract that defines what "valid" means.
- Loader: the Rust entry point that reads the schema and compiles a validator.
- Validator: the compiled JSONSchema plus any extra semantic checks.
- Instance: the JSON file or emitted record validated at runtime.

## Capability catalog (sandbox_catalog_v1)

Pattern:
- `schema/capability_catalog.schema.json` (schema_version: `sandbox_catalog_v1`)

Loader:
- `src/catalog/index.rs` -> `CapabilityIndex::load`
- Uses `src/schema_loader.rs` to read and compile the JSON schema.

Validator:
- JSONSchema validation against `schema/capability_catalog.schema.json`
- Additional checks in `CapabilityIndex` for schema_version allowlist, duplicate
  ids, category/layer references, and doc references.

Instance:
- Default catalog: `catalogs/macos_codex_v1.json`
- Overrides: `--catalog` or `CATALOG_PATH`

## Boundary descriptors and boundary objects (boundary_event_v1)

Pattern:
- Descriptor contract: `schema/boundary_object_schema.json`
- Embedded boundary schema pattern: `boundary_event_v1` (schema_version const
  inside the descriptor's boundary_schema)

Loader:
- `src/boundary/mod.rs` -> `BoundarySchema::load`
- Path resolution via `resolve_boundary_schema_path` in `src/lib.rs`

Validator:
- Descriptor JSON must pass `schema/boundary_object_schema.json`
- Embedded boundary_schema is compiled; `schema_key` const must match the
  descriptor key
- Boundary objects are validated by `BoundarySchema::validate` (used by
  `bin/emit-record`, `bin/probe-listen`, and tests)

Instance:
- Default descriptor: `boundaries/cfbo-v1.json`
- Emitted boundary objects: NDJSON from probes (`bin/emit-record`)
- Overrides: `--boundary` or `BOUNDARY_PATH`

## Manual validation

- `schema-validate --mode catalog --file <path>`
- `schema-validate --mode boundary --file <path>`

## Guard rails

- `tests/schema.rs` asserts descriptor and catalog schemas stay aligned with
  their instances.
