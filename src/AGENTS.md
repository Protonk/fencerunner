# Agent Guidance for `src/`

`src/` is the shared Rust crate used by every helper binary. The module layout
is intentional: callers should import from the module that owns the behavior,
not from `lib.rs`. This file is a router so you can land in the right place
fast without spelunking.

## Quick routing by task
- **Repo discovery, default paths, helper binary lookup, split_list**
  - `src/repo_tools.rs`
- **Boundary object types, schema, parsing, NDJSON read, catalog lookups**
  - `src/boundary/types.rs` (record structs)
  - `src/boundary/schema.rs` (BoundarySchema)
  - `src/boundary/stream.rs` (parse JSON/NDJSON strings)
  - `src/boundary/read.rs` (NDJSON reader + BoundaryReadError)
  - `src/boundary/lookup.rs` (attach/resolve catalog snapshots)
- **Capability catalogs and indexing**
  - `src/catalog/model.rs` (catalog structs + load_catalog_from_path)
  - `src/catalog/index.rs` (CapabilityIndex + schema enforcement)
  - `src/catalog/identity.rs` (ids + enums)
  - `src/catalog/repository.rs` (CatalogRepository)
  - `src/catalog/defaults.rs` (DEFAULT_CATALOG_PATH)
- **Probe discovery + metadata + coverage**
  - `src/probes/discovery.rs` (resolve/list probes)
  - `src/probes/metadata.rs` (static metadata parsing)
  - `src/probes/coverage.rs` (coverage accounting)
- **Harness/runtime helpers**
  - `src/harness/binaries.rs` (helper resolution, PATH lookup)
  - `src/harness/workspace.rs` (workspace planning, tmpdir)
  - `src/harness/payload.rs` (emit-record payload builders + validation)
  - `src/harness/contract.rs` (probe/record cross-checks)
- **Shared JSON Schema loader**
  - `src/schema/loader.rs`
- **Module index only (no logic)**
  - `src/lib.rs`, `src/*/mod.rs`

## Patterns to preserve
- One source of truth per concern: do not duplicate path resolution, probe
  lookup, or catalog validation in binaries.
- Keep errors actionable and consistent; binaries surface these directly.
- Portability is part of the contract: macOS 13-era hosts and CI containers
  must both run without extra runtime deps.
- When behavior is subtle, add a comment and a focused test.

## Working loop
- Prefer `make test` after changes; it rebuilds helpers into `bin/` and runs
  `cargo test`.
- If you add, remove, or rename a module, update this file so the routing map
  stays accurate.
