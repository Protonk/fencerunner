# tests/AGENTS.md

This document is the contract for anyone touching the test harness. Whether you
are a human developer or an automated agent, treat this as the playbook for
keeping the board green. Every change must leave `cargo test` passing, because a
single command now executes the entire suite.

## Mission control

1. **Single entry point:** `cargo test` (or `cargo test --test suite`) runs
   everything. There are no other Rust targets or doctests, so noisy output
   means something regressed.
2. **Board must stay green:** the suite encodes the portability + contract
   guarantees promised in `README.md`, `CONTRIBUTING.md`, and the schema docs.
   If the suite fails you either broke a contract or you discovered an existing
   gap—fix the code or extend the tests before landing.
3. **Document the why:** when you add a new guard rail, put the rationale in the
   test body and, if it enforces a repo-wide rule, mention it here too. Future
   agents should be able to map every expectation back to a contract statement.

## Directory map

| Path | Purpose | Notes |
| --- | --- | --- |
| `tests/suite.rs` | Single integration entry point. | Host for every guard rail: contract gates, schema checks, CLI smokes, workspace invariants. Target individual cases with `cargo test --test suite <name>`. |
| `tests/support/` | Shared helpers. | Builds helper binaries once per run, provides temp repositories, mutex guards, path utilities. Always prefer these over ad-hoc fixtures. |
| `tests/mocks/` | Shell fixtures used by the suite. | Minimal probes and data files that suite tests can execute. Keep side-effects inside the provided temp dirs. |

## Running and diagnosing tests

- **Full sweep:** `cargo test`. Watch for exactly two sections of output: the
  empty library unit bucket and `tests/suite.rs`. Anything else means someone
  reintroduced stray targets.
- **Focused run:** `cargo test --test suite <name>` to iterate on a failing case.
  Use `-- --nocapture` when you need stdout/stderr from helpers.
- **Probe contract loop:** `tools/validate_contract_gate.sh --probe <id>` (or
  `make probe PROBE=<id>`) is still the fastest way to vet a single probe. The
  integration suite asserts those gates stay wired up.
- **Schema debugging:** the `boundary_object_schema` test writes the failing JSON
  payload to `tmp/` with the test name. Open that file before re-running to see
  what changed.

## Adding or modifying tests

1. **Decide the contract you are protecting.** Examples: boundary-object shape,
   helper CLI semantics, workspace isolation, catalog synchronization. Cite that
   contract in the test name or first comment.
2. **Use `tests/support`.**
   - `support::helpers()` builds binaries once and caches their paths.
   - `TempRepo` hands you a throwaway workspace with automatic cleanup.
   - `ProbeFixture` gives you ready-made probe metadata. Never invent new path
     juggling logic when a helper already exists.
3. **Keep tests hermetic.** Write to the temp repo created by the helper, avoid
   touching the real workspace, and guard shared global state with the provided
   mutex.
4. **Structure:** prefer `Result<()>`-returning tests for easy `?` usage.
   Ensure failures `bail!` with actionable messages.
5. **Fixtures:** place new shell probes or data under `tests/mocks/`. Document
   expectations in comments and keep them deterministic so CI stays stable.
6. **Docs:** when a new test enforces a repo-wide promise, update this file and
   the relevant docs (usually `tests/AGENTS.md`, maybe `docs/*.md`) so future
   agents understand the coverage.

## Mapping tests to contracts

| Contract surface | Representative tests |
| --- | --- |
| Boundary object schema + payload semantics | `boundary_object_schema`, `boundary_object_round_trips_structs`, `capabilities_schema_version_serializes_in_json` |
| Capability catalog + context wiring | `load_real_catalog_smoke`, `repository_lookup_context_matches_capabilities`, `capability_snapshot_serializes_to_expected_shape` |
| Helper binaries & CLI ergonomics | `json_extract_*`, `portable_path_relpath_*`, `detect_stack_reports_expected_sandbox_modes`, `contract_gate_*`, `fence_bang_*` |
| Workspace + sandbox guarantees | `workspace_root_fallback`, `workspace_tmpdir_*`, `probe_resolution_guards`, `baseline_no_codex_smoke` |
| Probe contracts & fixtures | `harness_smoke_probe_fixture`, `dynamic_probe_contract_accepts_fixture`, `static_probe_contract_*` |

Use this table to decide where to plug a new test. If your change touches a
contract without an obvious row, add both the row and the tests.

## When failures occur

- **Schema or catalog diffs:** compare the emitted JSON against `docs/boundary_object.md`
  or `schema/capabilities.json`. Update schemas and regenerate helpers before
  re-running.
- **CLI guard rails:** reproduce locally with the same helper command printed by
  the test (they log the exact arguments). Most rely on binaries under `bin/`,
  so rebuild those if they drift.
- **Workspace/path issues:** rerun the failing test with `RUST_LOG=debug` to see
  the path planning traces emitted by `fence_run_support`.
- **Probe contract gates:** run `tools/validate_contract_gate.sh --probe <id>` or
  `fence-test --probe <id>` to gate the offending script. Edit in a tight loop until the probe passes the contract gate--only then do you run the full suite.

Keeping this file current is part of the contract. If you add a new class of
checks, describe them here so the next agent knows exactly how the test suite
covers our promises.
