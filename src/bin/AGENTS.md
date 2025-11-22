# Agent Guidance for Rust Binaries

All canonical helper implementations now live under `src/bin/`. `make build-bin`
copies the compiled binaries into `bin/` so probes/tests can keep invoking
`bin/<name>` directly; when editing code, assume Rust owns the real behavior and
update these helpers together.

## CLI entry points

### `codex-fence`
- **Purpose:** Front door for `--bang` and `--listen`. Delegates to the
  specialized helpers while guaranteeing `CODEX_FENCE_ROOT` points at the repo
  so binaries can find fixtures.
- **Expectations:**
  - Keep the CLI contract stable; add switches only when docs/tests are updated.
  - Prefer the compiled binaries (`bin/` first, then `target/{release,debug}`)
    when resolving helpers, falling back to `$PATH` only when necessary.
  - Propagate exit codes verbatim so harness automation can detect failures.

### `fence-run`
- **Purpose:** Resolve probe paths, enforce the requested sandbox mode, and
  export `FENCE_*` metadata for downstream scripts.
- **Expectations:**
  - Preserve strict probe resolution (only under `probes/`), and keep
    descriptive error messages when a probe cannot run.
  - Continue honoring `--workspace-root` / `FENCE_WORKSPACE_ROOT` overrides.
  - When touching sandbox modes, keep parity with codex CLI flags and ensure the
    resulting environment variables still match the run mode.

## Record helpers

### `emit-record`
- **Purpose:** Gather capability metadata, stack info, and operation payloads to
  emit cfbo-v1 JSON.
- **Expectations:**
  - Validate inputs aggressively and surface actionable errors.
  - Shell out only to `detect-stack`; all other work should remain pure Rust for
    portability and rely on the in-repo capability catalog instead of the
    legacy adapter.
  - Avoid printing to stdout except for the final JSON record.

### `detect-stack`
- **Purpose:** Capture codex CLI details, sandbox metadata, and OS information.
- **Expectations:**
  - Keep execution fast and dependency-free; it runs before every record.
  - Never remove existing JSON keys without versioning; new keys should default
    to `null`/sane fallbacks so older environments keep working.

## Harness helpers

### `fence-bang`
- **Purpose:** Iterate probes/modes and execute them via `fence-run`, printing
  NDJSON boundary objects for each run.
- **Expectations:**
  - Use `resolve_helper_binary` to run the Rust `fence-run` implementation and
    fail fast when binaries are missing, nudging users toward `make build-bin`.
  - Keep probe filtering (`PROBES`, `PROBES_RAW`) and mode selection logic in
    sync with docs; most automation depends on these env vars.

### `fence-listen`
- **Purpose:** Read cfbo-v1 JSON from stdin and display a human summary.
- **Expectations:**
  - Handle both NDJSON streams and JSON arrays; this binary is the main
    inspection tool when iterating locally.
  - Reject invalid input with clear error messages; don’t panic.

### `fence-test`
- **Purpose:** Execute `tools/validate_contract_gate.sh` for the
  full probe set while enforcing repo root detection and a predictable
  environment.
- **Expectations:**
  - Keep the CLI simple; any extra flags should mirror the static contract
    helper’s capabilities.
  - Surface script exit codes verbatim for CI consumption; callers now trigger
    the static contract by invoking this binary directly.

## Shared helpers

### `portable-path`
- **Purpose:** Provide `realpath`/`relpath` equivalents without depending on
  system Python/Perl.
- **Expectations:**
  - Keep the CLI (`portable-path <realpath|relpath> …`) stable.
  - Ensure outputs remain deterministic across macOS and Linux.

### `json-extract`
- **Purpose:** Minimal JSON pointer extractor for probes that need to read
  structured fields from helper output.
- **Expectations:**
  - Keep the CLI small and predictable (`--file/--stdin`, `--pointer`, `--type`,
    `--default`), returning compact JSON on stdout and actionable errors on
    stderr.
  - Favor deterministic failures over silent fallbacks; add tests when expanding
    semantics.

### `resolve_helper_binary`
- **Purpose:** Central helper (exposed from `lib.rs`) that prefers the synced
  binaries under `bin/` before falling back to Cargo build outputs.
- **Expectations:**
  - Use it whenever Rust code invokes another helper so we consistently exercise
    the Rust implementations.
  - Extend its tests whenever you add new search paths or semantics.

## General expectations
- Prefer explicit, defensive argument parsing; fail fast with actionable errors.
- Keep new policies reflected in docs/tests (README, `docs/*.md`, harness
  scripts) so shell callers stay in sync with the Rust behavior.
- Maintain portability: everything must run on macOS `/bin/bash 3.2` and inside
  the `codex-universal` container with only the shipped Rust binaries.
