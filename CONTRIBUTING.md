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

- Probe helpers live under `lib/` (one function per script when you add new
  Bash utilities). Path canonicalization now lives in the Rust helper
  `bin/portable-path`, so prefer `portable-path realpath|relpath` instead of
  introducing interpreter fallbacks.
- Project-level scripts (lint, validation, adapters) live under `tools/`.
  The fast probe lint entry point, `tests/probe_contract/light_lint.sh`,
  lives next to the static probe contract suite—prefer extending it for new
  checks instead of duplicating logic elsewhere.
- `bin/emit-record`, `bin/fence-run`, and any new binaries must stay
  dependency-free beyond POSIX + `jq`.

### Tests

- `tests/run.sh` orchestrates two tiers: a fast lint/static-contract pass and
  the second-tier suites (`capability_map_sync`, `boundary_object_schema`,
  `harness_smoke`, `baseline_no_codex_smoke`). When expanding coverage, keep
  the fast tier lightweight so `tests/run.sh --probe <id>` remains instant.
- The directory layout, fixtures, and suite expectations are captured in
  [`tests/AGENTS.md`](tests/AGENTS.md). Update that guide whenever you add a new
  suite or change workflows so agents know how to reproduce failures.
- Place reusable fixtures under `tests/library/fixtures/` and keep them synced
  with the capability catalog (the validation scripts scan these files too).
- Add new suites under `tests/second_tier/` when the checks are global or slow.
  Ensure they short-circuit quickly on missing prerequisites so macOS authors
  can still run `make test`.
- Maintain the guard-rail block comment + inline notes that live at the top of
  each script. These comments are intentionally brief breadcrumbs so human and
  AI agents understand the purpose of a suite before editing it.

### Keeping schema documentation in sync

Updates to the capabilities catalog, located at `schema/capabilities.json`, or the boundary object schema (`schema/boundary_object.json`) require matching updates in their documentation:
- `schema/capabilities.json` is documented in `docs/capabilities.md`
- `schema/boundary_object.json` is documented in `docs/boundary_object.md`
Ensure these files stay in sync.
