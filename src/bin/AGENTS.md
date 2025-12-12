# Agent Guidance for Rust Binaries

`src/bin/` contains the canonical helper implementations. `make build`
copies them into `bin/` so probes/tests can keep invoking `bin/<name>`; when you
change behavior, update Rust here first and then sync the artifacts.

## CLI entry points (front doors)

### `probe`
Front door for `--matrix/--listen/--target`; its job is to locate helpers and set
`FENCE_ROOT` so downstream binaries find the repo. Keep the CLI contract stable, prefer repo helpers before
PATH, and propagate exit codes verbatim.

### `probe-exec`
Executes a probe in a requested mode, exporting `FENCE_*` metadata and enforcing that probes live under `probes/`. Keep probe
resolution strict, honor `--workspace-root`/`FENCE_WORKSPACE_ROOT`, and
ensure sandbox env matches the mode (baseline today).

## Record helpers (boundary emission/introspection)

### `emit-record`
Builds boundary-event JSON from probe CLI flags. Validate inputs aggressively,
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
Iterates probes/modes via `probe-exec`, emitting NDJSON. Reuse
`resolve_helper_binary`, enforce mode/probe selection per docs, and keep error
messages actionable.

### `probe-target`
Backs `probe --target` by selecting probes (by capability id or explicit
ids) and delegating execution to `probe-matrix`. Enforce the flag contract
(cap or probe required, `--mode` limited to baseline with the same defaults as
`probe-matrix`), use the bundled catalog for `--cap`, and keep list-only output
deterministic.

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
- Reflect behavioral changes in docs/tests so shell callers stay in sync.
- Portability is non-negotiable: binaries must run on macOS `/bin/bash 3.2` and
  inside CI containers with only the shipped Rust helpers.
