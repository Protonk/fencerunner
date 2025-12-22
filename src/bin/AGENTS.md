# Agent Guidance for Rust Binaries

`src/bin/` contains the canonical helper implementations. `make build`
copies them into `bin/` so probes/tests can keep invoking `bin/<name>`; when you
change behavior, update Rust here first and then sync the artifacts.

## CLI entry points (front doors)

### `fencerunner`
Front door for `--bang/--bundle/--probe/--listen`; its job is to locate helpers
and set `FENCE_ROOT` so downstream binaries find the repo. Keep the CLI
contract stable, prefer repo helpers before PATH, and propagate exit codes
verbatim.

### `probe-exec`
Executes a probe directly, exporting `FENCE_*` metadata and enforcing that probes live under `probes/`. Keep probe
resolution strict and honor `--workspace-root`/`FENCE_WORKSPACE_ROOT`.

## Record helpers (boundary emission/introspection)

### `emit-record`
Builds boundary-object JSON from probe CLI flags. Validate inputs aggressively,
rely on the in-repo catalog, and shell out only to `detect-stack`. stdout
should only carry the final JSON record.

### `detect-stack`
Captures sandbox metadata and OS info. Keep it dependency-free and fast; never
drop existing JSON keys without versioning, and default new keys sensibly.

### `probe-listen`
Reads boundary-object NDJSON/arrays and prints a human summary. Reject invalid
input with clear errors; don’t panic.

## Harness helpers (probe orchestration)

### `probe-matrix`
Iterates probes via `probe-exec`, emitting NDJSON. Reuse
`resolve_helper_binary`, enforce selection per docs (`--bang`, `--bundle`, or
`--probe`), and keep error messages actionable.

### `probe-gate`
Runs `tools/validate_contract_gate.sh` (the probe contract gate) with
predictable env/repo detection. Mirror the script’s flags and surface exit
codes verbatim.

## Utility helpers

### `portable-path`
Portable `realpath`/`relpath`. Keep the CLI stable and outputs deterministic
across macOS/Linux.

### `json-extract`
Minimal JSON pointer extractor for probes. Keep the small CLI surface, return
compact JSON, and prefer explicit failures over silent fallbacks.

## Expectations across binaries
- Subscribe to shared logic in `runtime.rs`/`fence_run_support.rs`/`lib.rs`
  instead of rolling your own path/sandbox/catalog handling.
- Keep argument parsing explicit and defensive; surface actionable errors.
- Reflect behavioral changes in the narrative guides and tests so shell callers stay in sync.
- Portability is non-negotiable: binaries must run on macOS `/bin/bash 3.2` and
  inside CI containers with only the shipped Rust helpers.
