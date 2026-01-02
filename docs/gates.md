# Gates

This document is for **run-dir authors**: people who assemble one or more run
directories and decide what those directories commit to running and emitting.

A *run directory* is a flat directory of scripts (`*.sh`) plus three
run-dir-local JSON files that define the contract:

- `gates.json` — run-dir-wide gate enrollment (validated by [`schema/gates.json`](../schema/gates.json)).
- `commitments.json` — run-dir-local dependency registry (validated by [`schema/commitments.json`](../schema/commitments.json)).
- `boundaries.json` — run-dir-local output contract (validated by [`schema/boundaries.json`](../schema/boundaries.json)).

These three files (the “triad”) are validated before any script runs. If they
don’t validate, the runner aborts early.

For a default example run dir, see `../scripts/`.

## Run-dir layout

A run dir is flat and contains:

- `gates.json`
- `commitments.json`
- `boundaries.json`
- `*.sh` scripts (regular files only; symlinked scripts are rejected)

The runner also provides a shared script library at `lib/library.sh`. Scripts
must start by sourcing it via:

- `source "${FENCERUNNER_ROOT}/lib/library.sh"`

## `gates.json`

`gates.json` is where a run dir opts into **additional script contract checks**
that are enforced by the runner when executing scripts.

### Validation flow

1. `gates.json` is validated against the meta-schema at [`schema/gates.json`](../schema/gates.json).
2. Runners preflight and require `gates.json` (missing/invalid is a hard error).
3. The runner reads `gates.json` to decide which optional checks to enforce.

### Shape (v1)

- `schema_version` — must be `"gates_v1"`.
- `gates.enforced_checks[]` — optional list of enforced check ids.
  - Values must be unique.
  - Unknown check ids are a schema validation error.

Currently supported check ids:

- `stderr.empty` — fail the run if a script writes anything to stderr.

`gates_v1` allows additional keys at the top level; run dirs may add their own
metadata fields, but core tooling only interprets `schema_version` and the
recognized gate options.

Independently of `gates.json`, the runner requires stdout to contain only the
boundary record (no extra output).

### Examples

Minimal:

```json
{
  "schema_version": "gates_v1"
}
```

Opt into stricter stderr behavior:

```json
{
  "schema_version": "gates_v1",
  "gates": {
    "enforced_checks": ["stderr.empty"]
  }
}
```

## Scripts (`*.sh`)

### Discovery and identity

- Every `*.sh` file directly under the run dir is a script.
- Scripts must be regular files; symlinked scripts are rejected.
- The script id is the filename stem: `<script_id>.sh`.
- Scripts execute in lexicographic id order within a run dir; across multiple run dirs, run dirs execute in CLI argument order.
- Boundary objects must record this identity under `script.id`.

When multiple run dirs are executed together, **script id collisions are
errors**.

For a minimal working example script, see [`scripts/minimal_example.sh`](../scripts/minimal_example.sh). It sources the runner library, enrolls in `emit.record`, and then emits one record.

### Execution environment

fencerunner executes scripts with the current working directory set to the run
dir. Use relative paths when you want writes to land in the run dir, or
reference `FENCERUNNER_RUN_DIR` when you need explicit absolute paths.

### Relationship to the triad

- If a script enrolls in a dependency via `commit_help_me <ensure|detect|emit> <id>`,
  run dirs are expected to declare that dependency in `commitments.json` (recommended; not enforced at runtime). See [`docs/commitments.md`](commitments.md).
- Enrollments are recorded in the emitted boundary object under
  `/context/commitments` as `(id, helps[])` pairs, and the record must validate
  against the run dir’s `boundaries.json` contract (see [`docs/boundaries.md`](boundaries.md)).

## Philosophy

These are lower-level “opinions” that guide script authorship and review:

- **Identity is file-based:** script ids come from `<script_id>.sh` file stems and must match `--script-name`.
- **Portable Bash first:** scripts are `#!/bin/bash`, use `set -euo pipefail`, then `source "${FENCERUNNER_ROOT}/lib/library.sh"` (Bash 3.2 compatible). We target macOS and prefer the fixed `/bin/bash` path for regularity in script output and execution.
- **Stdout is the record:** stdout is reserved for the boundary object only (no extra output).
- **Stderr is diagnostics:** stderr is allowed by default; run dirs may opt into `stderr.empty` via `<RUN_DIR>/gates.json`.
- **Dependencies are declared:** non-script help is declared in `<RUN_DIR>/commitments.json` and enrolled via `commit_help_me <ensure|detect|emit> <id>`.
- **Enrollments are recorded:** call `commit_help_me` to record enrollments, then call `emit-record ...` so those enrollments are serialized into `/context/commitments` as `(id, helps[])` pairs.
- **Output is validated:** emitted records must validate against `<RUN_DIR>/boundaries.json` and include `script.id`, `result.outcome`, `/context/commitments` (empty allowed), and `payload.stdout_snippet`/`payload.stderr_snippet` (empty allowed).
