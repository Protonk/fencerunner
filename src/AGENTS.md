# Agent Guidance for `src/`

`src/` is the shared Rust crate every helper links against. It encodes the
contracts promised in README/CONTRIBUTING/AGENTS and should make those layers
obvious: discover the repo, load the catalog, resolve probes inside the trusted
tree, emit/parse boundary-event records (default schema key `cfbo-v1`), and
share runtime helpers with the binaries under `src/bin/`.

## Map of responsibilities
- `lib.rs` — entry point and glue. Owns repo/root detection, helper resolution,
  and the small helper APIs the binaries depend on. Keep public surface small
  and documented here or in the target module.
- `boundary/` — boundary-event types and serde. Schema changes start in
  `schema/boundary_object_schema.json` (canonical pattern) and the bundled
  descriptor under `catalogs/` (default: `cfbo-v1.json`), then land here with
  tests.
- `catalog/` — capability catalog parsing and indexing. Pure Rust; no shelling
  out. Must stay aligned with `schema/capability_catalog.schema.json` and the
  bundled catalogs under `catalogs/`.
- `emit_support.rs`, `probe_metadata.rs`, `metadata_validation.rs`,
  `coverage.rs` — harness utilities (payload builders, static probe parsing,
  catalog/probe/record cross-checks). Add focused unit tests when touching them.
- `connectors.rs` — registry of run modes/connectors (baseline today). Owns
  defaults, sandbox env overrides, command planning, and the helper APIs
  binaries should call when adding new connector-aware behavior.
- `runtime.rs`, `fence_run_support.rs` — shared runtime mechanics (helper search
  order, workspace planning). CLIs should reuse these instead of re-implementing
  path/sandbox logic.

### Adding a new connector/run mode
- Extend `RunMode`/`MODE_SPECS` in `connectors.rs` with the new mode name,
  default gating, sandbox defaults, command spec, and any preflight hook.
- Subscribe binaries to the registry: use `plan_for_mode` in `probe-exec`,
  `default_mode_names`/`allowed_mode_names` for mode validation, and keep error
  messages aligned with existing ones.
- Add tests that exercise the new mode (connector presence, sandbox/env wiring,
  preflight flow) and adjust docs or examples that enumerate supported modes.

## Patterns to preserve
- One source of truth per concern: helper resolution lives in `runtime`, probe
  lookup in `resolve_probe`, catalog parsing in `catalog::*`. Subscribe to these
  instead of duplicating the logic.
- Errors should be actionable and consistent; binaries surface them directly.
- Portability is part of the contract: code must run on macOS 13-era hosts and
  in the CI container images without extra runtime deps.
- When behavior is subtle, add a comment and a test that explains why.

## Working loop
- Run `cargo test` after changes; it exercises unit tests and the integration
  suite in `tests/suite.rs`.
- After changing binary behavior, run `make build` to sync `bin/` artifacts
  with `src/bin/`.
- If you add a new module or responsibility, update this file so other agents
  can navigate quickly. If a sub-area needs deeper rules, add an `AGENTS.md`
  there and link back.
