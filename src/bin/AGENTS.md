# Agent Guidance for Rust Binaries

`src/bin/` contains the canonical helper implementations. `make build-bin`
copies them into `bin/` so probes/tests can keep invoking `bin/<name>`; when you
change behavior, update Rust here first and then sync the artifacts.

## CLI entry points (front doors)

### `codex-fence`
Front door for `--bang/--listen/--rattle`; its job is to locate helpers and set
`CODEX_FENCE_ROOT` so downstream binaries find the repo. Keep the CLI contract
stable, prefer repo helpers before PATH, and propagate exit codes verbatim.

### `fence-run`
Executes a probe in a requested mode, exporting `FENCE_*` metadata and enforcing
that probes live under `probes/`. Keep probe resolution strict, honor
`--workspace-root`/`FENCE_WORKSPACE_ROOT`, and ensure sandbox env matches the
mode (baseline vs codex modes).

## Record helpers (cfbo emission/introspection)

### `emit-record`
Builds cfbo-v1 JSON from probe CLI flags. Validate inputs aggressively, rely on
the in-repo catalog, and shell out only to `detect-stack`. stdout should only
carry the final JSON record.

### `detect-stack`
Captures codex CLI details, sandbox metadata, and OS info. Keep it dependency-
free and fast; never drop existing JSON keys without versioning, and default new
keys sensibly.

### `fence-listen`
Reads cfbo-v1 NDJSON/arrays and prints a human summary. Reject invalid input
with clear errors; don’t panic.

## Harness helpers (probe orchestration)

### `fence-bang`
Iterates probes/modes via `fence-run`, emitting NDJSON. Reuse
`resolve_helper_binary`, enforce mode/probe selection per docs, and keep error
messages actionable.

### `fence-rattle`
Backs `codex-fence --rattle` by selecting probes (by capability id or explicit
ids) and delegating execution to `fence-bang`. Enforce the flag contract, use
the bundled catalog for `--cap`, and keep list-only output deterministic.

### `fence-test`
Runs `tools/validate_contract_gate.sh` with predictable env/repo detection.
Mirror the static helper’s flags and surface exit codes verbatim.

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
  inside `codex-universal` with only the shipped Rust helpers.
