# Fencerunner

> Run small, explicit probes against a sandbox or runtime and capture what actually happened as structured JSON.

Fencerunner is infrastructure. It does not impose a particular sandbox or policy;
instead, it gives you a way to **describe capabilities**, **exercise them with tiny
shell probes**, and **record the results as schema‑validated JSON “boundary objects”**
that can be analyzed later.

The top‑level CLI is called `fencerunner`. It discovers probes, runs them in
well‑defined flows, validates their outputs against schemas and capability
catalogs, and keeps the contract between “what probes promise” and “what
actually ran” tight.

For contributor‑focused details, see [`CONTRIBUTING.md`](CONTRIBUTING.md). For contract‑level
guidance, start with the AGENTS files.

## Mental model

At a high level, Fencerunner is built from three ideas:

- **Probes** — small Bash scripts under `probes/<probe_id>.sh`. Each performs
  exactly one observable action (for example, “write a file outside the
  workspace”) and calls a helper binary to emit a single JSON record describing
  what happened.
- **Capability catalogs** — a JSON catalog that names the behaviors you care
  about (`cap_fs_write_workspace_tree`, `cap_net_connect_loopback`, …) and
  maps each one to structured metadata (including relevant low‑level
  operations).
- **Boundary objects** — a schema‑validated JSON record emitted for each probe
  run. It captures the attempted operation and observed outcome with optional
  context and payload metadata.

Together, these map probe results to named capabilities and keep the
output analyzable over multiple runs. The contract harness is intentionally strict so
new probes add signal without breaking downstream consumers.

## Usage

### Requirements

Build:
- A Rust toolchain with `rustc`/`cargo` >= 1.85 (see `Cargo.toml` and `Cargo.lock` for the crate set: `Cargo.toml`, `Cargo.lock`).
- `make` and `/bin/bash` (used by the build scripts).
- `python3` available on PATH (used by `tools/sync_bin_helpers.sh` to read the helper manifest).

Run:
- macOS or Linux with `/bin/bash` 3.2+ and common Unix utilities (coreutils, `uname`, etc.).
- `python3` for the bundled loopback network probe.
- The compiled helper binaries under `bin/` (produced by `make build`); no other runtime dependencies or package installs are required.

### Build and run

Build the helpers into `bin/`:

```sh
make build
```

### Core CLI surface

The primary entry point is the `fencerunner` binary (synced into `bin/fencerunner`).

- `fencerunner --bang`  
  Run every probe once and stream each boundary object as NDJSON.

- **Run the full probe matrix with the bundled catalog and schema**

  ```sh
  fencerunner --bang
  ```

- `fencerunner --bundle <capability-id>`  
  Run all probes whose primary capability matches `<capability-id>`.

- `fencerunner --probe <probe-id>`  
  Run a single probe by id.

- **Run a single probe by id**

  ```sh
  fencerunner --probe fs_outside_workspace
  ```

- `fencerunner --listen`  
  Read boundary-object NDJSON (for example, from `fencerunner --bang`) on stdin
  and print a human‑readable summary. This is a text‑only viewer; it never
  changes the underlying JSON and accepts no additional flags.

- **Inspect results in a human‑readable form**

  ```sh
  fencerunner --bang | fencerunner --listen
  ```


- `schema-validate`  
  Validate JSON as a catalog (`--mode catalog`) or boundary (`--mode boundary`)
  against the bundled schemas or paths provided via `--catalog` / `--boundary`.

## Probes: how you measure a sandbox

Probes are intentionally boring:

- They are Bash scripts in `probes/<probe_id>.sh`.
- They use `#!/usr/bin/env bash` plus `set -euo pipefail`.
- They perform one focused operation.
- They call `bin/emit-record` exactly once to emit a JSON boundary object.
- They write nothing else to stdout (stderr is reserved for minimal diagnostics).

Each probe declares:

- a `probe.id` (the filename),
- a `primary_capability_id` and optional `secondary_capability_ids` in
  `context.probe`, and
- a normalized `result.outcome` (`success`, `denied`, `partial`, `error`) plus
  payload snippets that capture what actually happened.

The probe author contract, examples, and test‑backed rules live in
[`probes/AGENTS.md`](probes/AGENTS.md). Start there if you are writing or modifying probes.

## Catalogs and boundary schemas

Fencerunner’s contracts are expressed as JSON artifacts that can be swapped
independently: a capability catalog (what behaviors exist and what they mean)
and a boundary object schema (what a probe run must record).

### Capability catalogs

The bundled capability catalog (`catalogs/macos_codex_v1.json`) is a
`sandbox_catalog_v1` instance: it declares the catalog’s key and scope, a
category/layer taxonomy, a docs bibliography, and a set of capability entries
with stable ids, descriptions, and operation mappings (plus optional
notes/sources).

Conceptually, a capabilities catalog is a shared vocabulary of testable
propositions—stable names with structured meaning—so everyone can agree on what
a capability refers to without tying that meaning to any particular probe
implementation or runtime.

### Boundary object schema (and boundary objects)

The bundled boundary object schema (`boundary/boundary_object_schema.json`) defines
the minimal required record shape: probe identity (`probe.id`), attempted
operation (`operation.kind`, `operation.target`, optional `operation.args`), and
observed outcome (`result.outcome`, optional `result.details`). Optional
`context`, `payload`, and `extensions` blocks carry richer metadata without
changing the core contract.

Conceptually, the boundary object is the contract at the boundary between messy
execution and reliable interpretation: it forces each probe run to be expressed
as a small, schema‑checked statement of attempted operation and observed
outcome (with bounded context), so downstream consumers treat the JSON record—
not ad‑hoc logs, timing, or side effects—as the interface.

The harness always requires a catalog and a boundary object schema, but you can
swap them out without changing code:

- Use `--catalog <path>` or `CATALOG_PATH` to point helpers at a different
  catalog file. Defaults fall back to the bundled `catalogs/macos_codex_v1.json`
  when no overrides are provided.
- Use `--boundary <path>` or `BOUNDARY_PATH` to point helpers at an alternate
  boundary schema. Defaults resolve to `boundary/boundary_object_schema.json`;
  emitted records are validated against that schema at emit/listen time.

The Rust layer (`src/catalog`, `src/boundary`) validates catalogs and boundary
objects at load and emit time, and the integration tests under `tests/` assert
that the schemas, helpers, and sample data stay in sync.

For a narrative view of these contracts, see:

- [`catalogs/capabilities.md`](catalogs/capabilities.md)
- [`boundary/boundary_object.md`](boundary/boundary_object.md)
- [`probes/probes.md`](probes/probes.md)
- [`schema/README.md`](schema/README.md)

## Navigation

The top‑level `AGENTS.md` is the router for this project: it tells you which
directory‑specific `AGENTS.md` file to read before editing a given area.

Before you change behavior, skim:

- [`AGENTS.md`](AGENTS.md) at the repo root,
- the `AGENTS.md` for the directory you are touching, and
- the relevant guide for that area (`catalogs/capabilities.md`, `boundary/boundary_object.md`, `probes/probes.md`, or `schema/README.md`).

Those files explain the contracts that code and tests are expected to uphold. The tests in `tests/` are intentionally opinionated and high‑coverage: keeping them green is the easiest way to ensure usage remains compatible with the contracts described above.
