# General Contributions

Thanks for improving codex-fence! This document covers repository-level work:
tests, helper libraries, tooling, docs, and automation. For probe-specific
expectations see the Probe Author contract in [probes/AGENTS.md](probes/AGENTS.md)--human and AI agents writing new probes should mainly concern themselves with that. 

## Scope

Use this guide when you plan to:
- Edit or add shell helpers under `tools/`, `lib/`, or `bin/`.
- Modify the Makefile, capability catalog, schema, or adapters.
- Extend `tests/` or its fixtures.
- Update documentation outside a single probe (README, `docs/*.md`, etc.).
Following this guide keeps the repo coherent for both human and AI probe
authors while preserving the portability guarantees that make `codex-fence`
valuable.

## Principles

- **Portability first.** Probes must not introduce spurious signals due to inconsistencies between platforms. Organize and write helper functions to support consistent harness behavior on e.g. macOS or the `codex-universal` container.
- **Single responsibility.** Helpers stay pure and composable; probes remain
  small; tooling avoids reaching into unrelated directories unless required.
- **Document contracts.** When adding configuration fields, schema changes, or
  helper functions, update the relevant documentation in the same change.

## Repository areas

### Helpers and tooling

- The fast probe contract entry point is
  `tools/contract_gate/static_gate.sh`; extend it for new
  syntax/structural checks instead of duplicating logic elsewhere.
- `bin/emit-record`, `bin/fence-run`, and any new helpers must avoid
  introducing runtime dependencies beyond Bash, `jq`, and the Rust standard
  library—keep probe plumbing lightweight and portable.
- After touching Rust helpers under `src/bin/`, run `make build-bin` (or
  `tools/sync_bin_helpers.sh`) so the synced binaries in `bin/` match
  your changes; probes, docs, and tests assume `bin/<helper>` resolves them
  directly.

### Tests

- The static probe contract lives at `tools/contract_gate/static_gate.sh`.
  Keep it lightweight so single-probe loops (`--probe <id>` or `make probe`)
  remain instant, and remember that `bin/fence-test` runs this helper across
  every probe.
- The Rust-based guard rails live in `tests/suite.rs` and run via
  `cargo test --test suite` (`boundary_object_schema`, `harness_smoke_probe_fixture`, `baseline_no_codex_smoke`, etc.). When expanding coverage, keep these tests
  hermetic and deterministic.
- The directory layout, fixtures, and suite expectations are captured in
  [`tests/AGENTS.md`](tests/AGENTS.md). Update that guide whenever you add a new
  suite or change workflows so agents know how to reproduce failures.
- Place reusable fixtures under `tests/shims/` and keep them synced
  with the capability catalog (the validation scripts scan these files too).
- Add new guard rails to `tests/suite.rs` when the checks are global or
  slow. Ensure they short-circuit quickly on missing prerequisites so macOS
  authors can still iterate with `cargo test`.
- Maintain the guard-rail block comment + inline notes that live at the top of
  each script. These comments are intentionally brief breadcrumbs so human and
  AI agents understand the purpose of a suite before editing it.

### Keeping schema documentation in sync

Updates to the capabilities catalog, located at `schema/capabilities.json`, or the boundary object schema (`schema/boundary_object.json`) require matching updates in their documentation:
- `schema/capabilities.json` is documented in `docs/capabilities.md`
- `schema/boundary_object.json` is documented in `docs/boundary_object.md`
Ensure these files stay in sync.
