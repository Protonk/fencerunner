# Agent Guidance for `src/`

`src/` is the shared Rust crate every helper links against. It encodes the
contracts promised in README/CONTRIBUTING/AGENTS and should make those layers
obvious: discover the repo, load the catalog, resolve probes inside the trusted
tree, emit/parse cfbo-v1, and share runtime helpers with the binaries under
`src/bin/`.

## Map of responsibilities
- `lib.rs` — entry point and glue. Owns repo/root detection, helper resolution,
  and the small helper APIs the binaries depend on. Keep public surface small
  and documented here or in the target module.
- `boundary/` — cfbo-v1 types and serde. Schema changes start in
  `schema/boundary_object.json` and docs, then land here with tests.
- `catalog/` — capability catalog parsing and indexing. Pure Rust; no shelling
  out. Must stay aligned with `schema/capabilities.json`.
- `emit_support.rs`, `probe_metadata.rs`, `metadata_validation.rs`,
  `coverage.rs` — harness utilities (payload builders, static probe parsing,
  catalog/probe/record cross-checks). Add focused unit tests when touching them.
- `runtime.rs`, `fence_run_support.rs` — shared runtime mechanics (helper search
  order, workspace planning, preflight classification). CLIs should reuse these
  instead of re-implementing path/sandbox logic.

## Patterns to preserve
- One source of truth per concern: helper resolution lives in `runtime`, probe
  lookup in `resolve_probe`, catalog parsing in `catalog::*`. Subscribe to these
  instead of duplicating the logic.
- Errors should be actionable and consistent; binaries surface them directly.
- Portability is part of the contract: code must run on macOS 13-era hosts and
  in `codex-universal` without extra runtime deps.
- When behavior is subtle, add a comment and a test that explains why.

## Working loop
- Run `cargo test` after changes; it exercises unit tests and the integration
  suite in `tests/suite.rs`.
- After changing binary behavior, run `make build-bin` to sync `bin/` artifacts
  with `src/bin/`.
- If you add a new module or responsibility, update this file so other agents
  can navigate quickly. If a sub-area needs deeper rules, add an `AGENTS.md`
  there and link back.
