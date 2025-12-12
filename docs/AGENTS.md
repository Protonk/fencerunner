# docs/AGENTS.md

This directory contains human-readable explanations of the *contracts* that `probe` enforces elsewhere in the repo. It is here to help both humans and model-based agents understand how to interpret:

- the capability catalog schema + bundled catalog (`schema/capability_catalog.schema.json`, `catalogs/*.json`)
- probe scripts (`probes/*.sh`)
- probe outputs (boundary objects streamed from `probe --matrix`)

The actual contracts are enforced by schemas, adapters, and tests. The documents in `docs/` must never become the primary source of truth about behavior or policy.

Use this file before you read or edit anything in `docs/`.

## What to know before reading these docs

1. Start with the project root `README.md` if you need the big picture: why `probe` exists, what a “probe” is, and how the harness runs.
2. Treat everything in `docs/` as *explanatory lenses* over:
   - `schema/capability_catalog.schema.json` + `catalogs/*.json` (capability map, versioned)
   - `schema/boundary_object_schema.json` (canonical boundary-event pattern) referenced by the bundled descriptor `catalogs/cfbo-v1.json`
   - the probe authoring contracts in `probes/AGENTS.md`
   - the test harness described in `tests/AGENTS.md`
3. Helper binaries accept drop-in artifacts: use `--catalog` / `CATALOG_PATH` plus `--boundary` / `BOUNDARY_PATH` to point at alternate boundary descriptors. Defaults resolve from `catalogs/defaults.json` (initially `catalogs/macos_codex_v1.json` and `catalogs/cfbo-v1.json`) and point to the canonical `schema/boundary_object_schema.json`.
3. When documentation and machine artifacts disagree, the machine artifacts win. Fix the docs to match the schema/tests, not the other way around.

If you are a model-based agent: prefer reading the JSON schemas and `AGENTS.md` contracts in other directories when you need normative rules. Use these docs to understand structure and intent.

---

## File guide

### `capabilities.md`

**Role**

Explains the structure and intent of the capability catalog:

- How the catalog is scoped (`scope`, `policy_layers`, `categories`).
- What each capability entry contains (`description`, `operations`, `meta_ops`, `agent_controls`, `notes`, `sources`, etc.).
- How capabilities link to probes and to external references.

**Read this if**

- You want to understand what a “capability” means in this project.
- You are adding or interpreting entries in the capability catalog.
- You are writing probes and need to see how they’re expected to align with capabilities.

### `probes.md`

**Role**

Describes what a probe is and how probes fit into the harness:

- Location and naming (`probes/<probe_id>.sh`).
- High-level probe contract (one observable action; emit exactly one boundary object).
- How probes are executed under the supported mode (`baseline`).
- What information a probe must emit into its boundary object.
- How to use the testing loop while authoring probes.

The authoritative probe-author contract lives in `probes/AGENTS.md` and the test scripts under `tests/`.

**Read this if**

- You are new to the project and want a narrative overview of probes.
- You are about to write your first probe and need a conceptual checklist.
- You are a model-based agent being asked to reason about how probes behave, but not to edit them directly.

### `boundary_object.md`

**Role**

Documents the boundary-event pattern (boundary_event_v1) and the default schema
descriptor (`cfbo-v1`):

- Explains how each probe run is captured as a JSON record.
- Walks each top-level field (`schema_version`, `capabilities_schema_version`, `stack`, `probe`, `run`, `operation`, `result`, `payload`, `capability_context`).
- Clarifies how `observed_result` should be interpreted and how payloads should remain small and structured.
- Describes the expected evolution path when the contract changes (new schema version, migration expectations, etc.).

The machine-readable contract is `schema/boundary_object_schema.json` (canonical) referenced by the bundled descriptor under `catalogs/` (default `catalogs/cfbo-v1.json`). Validation happens in `bin/emit-record` and the test suite; runtime rejects drift between the canonical schema and the bundled reference.

**Read this if**

- You are interpreting streamed boundary objects (e.g., from `probe --matrix`).
- You are modifying `bin/emit-record` or any adapters that build boundary objects.
- You are adding new consumers of boundary objects and need to know which fields are stable.

**Before you change it**

- Follow the “Updating the commitments” section at the end of this file:
  - Introduce a new schema file when breaking changes are needed.
  - Update `schema/boundary_object_schema.json` (and mirror it under `catalogs/` with a new descriptor if you add a new schema key).
  - Refresh references in `AGENTS.md`, `README.md`, probe docs, and tests.
- Never silently remove or repurpose fields without updating:
  - the schema,
  - the adapter scripts,
  - and the tests that validate stored boundary objects.
- Treat examples as illustrative only; the schema must stay ahead of the prose.

## Adding or changing docs in `docs/`

If you add a new document here:

1. Give it a clear, descriptive name (e.g., `linux_capabilities.md`, `design_goals.md`).
2. Decide which *machine* artifacts it explains (schema, script, catalog, test suite).
3. Add a short entry for it in this `AGENTS.md` file:
   - What it explains,
   - Who should read it,
   - What invariants must be preserved when it changes.
4. Avoid embedding new, untested “truths” about sandbox behavior here. If you discover new behavior:
   - add or update a capability in `catalogs/*.json`,
   - write or update probes under `probes/`,
   - and extend tests to cover it.

As a rule: documentation in `docs/` should help agents *interpret* the contracts that live elsewhere. It should not introduce contracts that only exist in prose.
