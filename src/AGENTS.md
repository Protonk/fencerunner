# Agent Guidance for `src/`

`src/` holds the shared Rust crate that all helpers link against plus the
compiled CLI entry points under `src/bin/`. Follow the repo-level contracts in
`README.md`, `CONTRIBUTING.md`, and the root `AGENTS.md` before touching code
here.

## Start here
- Need CLI guidance? jump to `src/bin/AGENTS.md`; this file covers the shared
  crate only.
- Skim `docs/boundary_object.md` and `docs/capabilities.md` so schema updates
  land alongside code.
- Use `make build-bin` or `cargo test --test suite` whenever you change shared
  logic; the guard rails exercise this crate.

## Layout cheat sheet
- `lib.rs` wires the crate modules together and exports helper utility
  functions (repo discovery, helper resolution, stream parsing). Keep new public
  APIs minimal and documented here or in the relevant module.
- `boundary/` defines the cfbo-v1 structs and serde glue. Any schema changes
  must be versioned under `schema/` first.
- `catalog/` loads and indexes the capability catalog JSON. Keep parsing pure
  Rust—no shell outs.
- `coverage.rs`, `probe_metadata.rs`, `metadata_validation.rs`, and
  `emit_support.rs` are the harness utilities consumed by binaries; add focused
  unit tests when mutating them.
- `fence_run_support.rs` contains the shared sandbox/workspace mechanics used
  by the run/bang helpers. Preserve workspace boundary enforcement and reuse
  `portable-path` instead of rolling new path logic.

## Rust-specific expectations
- Stay on stable Rust with the editions/features already declared in
  `Cargo.toml`; ask before adding new crates or enabling nightly gates.
- Keep the crate dependency-light. Prefer std + existing dependencies; if a new
  crate is indispensable, document why in `CONTRIBUTING.md` and add tests.
- Maintain portability: code must run on macOS 13-era toolchains and the
  `codex-universal` container. Avoid platform-specific syscalls unless guarded
  and tested.
- Favor explicit error paths using `anyhow` contexts so CLI binaries surface
  actionable messages.
- When exposing helpers for probes/tests, keep the API surface backward
  compatible; if you must break it, update docs, schema, and the Rust callers in
  the same change.

## Testing & workflows
- `cargo test` (and `cargo test --test suite`) is the fast feedback loop; keep
  new helpers covered with unit tests near their modules.
- Use `tools/validate_contract_gate.sh --probe <id>` or `make probe` after
  modifying probe-facing logic to ensure harness parity.
- Regenerate binaries via `make build-bin` before committing so `bin/`
  artifacts stay in sync with `src/bin/`.

Keep this file aligned with future module additions—new subdirectories need a
brief blurb here so other agents know where to work. If deeper rules are
required for a specific sub-area, add an `AGENTS.md` in that directory and link
back to this overview.
