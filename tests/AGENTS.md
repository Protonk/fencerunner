# tests/AGENTS.md

This document is the contract for anyone touching the test harness. Whether you
are a human developer or an automated agent, treat this as the playbook for
keeping the board green. Every change must leave `cargo test` passing, because a
single command executes the entire suite.

## Mission control

1. **Single entry point:** `make test` rebuilds helper binaries into `bin/` and
   then runs `cargo test`. There are no other Rust targets or doctests, so
   noisy output means something regressed.
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
| `tests/schema.rs` | Boundary + catalog schema guards. | Boundary schema validation and serde round-trips for boundary/capability types. |
| `tests/catalog.rs` | Catalog repository + lookup invariants. | Catalog loading, schema_version enforcement, and lookup context checks. |
| `tests/probe_execution.rs` | Probe execution + workspace planning. | `probe-exec` smokes, probe resolution fences, workspace overrides/tmpdir behavior. |
| `tests/contracts.rs` | Contract gates + emit-record rules. | Static/dynamic gates, emit-record flag enforcement, outcome/secondary normalization. |
| `tests/cli.rs` | CLI and harness behavior. | `fencerunner`/`probe-matrix` ergonomics, helper resolution, env propagation, sandbox detection. |
| `tests/helpers.rs` | Helper binaries + utility probes. | `json-extract`, `portable-path`, paging-stress, compiled probe smokes, builder helpers. |
| `tests/support/common.rs` | Cross-test fixtures. | Shared probe fixtures (`FixtureProbe`), repo/workspace guards, and sample boundary/capability builders used by multiple targets. |
| `tests/support/` | Shared helpers. | Builds helper binaries once per run, provides temp repositories, mutex guards, path utilities. Always prefer these over ad-hoc fixtures. |
| `tests/mocks/` | Shell fixtures used by the suite. | Minimal probes and data files that suite tests can execute. Keep side-effects inside the provided temp dirs. |

## Running and diagnosing tests

- **Full sweep:** `make test` (rebuilds helpers, then runs `cargo test`). Watch for the expected integration targets:
  `schema`, `catalog`, `probe_execution`, `contracts`, `cli`, `helpers`. Anything else means someone reintroduced stray targets.
- **Focused run:** `cargo test --test <target> <name>` (e.g.
  `cargo test --test cli fencerunner_bundle_runs_capability_subset`) to iterate on a failing case.
  Use `-- --nocapture` when you need stdout/stderr from helpers.
- **Probe contract loop:** `tools/validate_contract_gate.sh --probe <id|path>` (or
  `bin/probe-contract-gate --probe <id|path>`) is the fastest way to vet a single probe. The
  integration suite asserts those gates stay wired up.
- **Schema debugging:** the `boundary_object_schema` test writes the failing JSON
  payload to `tmp/` with the test name. Open that file before re-running to see
  what changed.

## Adding or modifying tests

1. **Decide the contract you are protecting.** Examples: boundary-object shape,
   helper CLI semantics, workspace isolation, catalog synchronization. Cite that
   contract in the test name or first comment.
2. **Use the shared helpers.**
   - `tests/support` provides `helper_binary`, `run_command`, and `repo_root` plus
     helper builders with cached compilation.
   - `tests/support/common` exposes `FixtureProbe`, `TempRepo`, `TempWorkspace`, repo locks,
     and sample boundary/capability builders. Never invent new path juggling logic
     when a helper already exists.
3. **Keep tests hermetic.** Write to the temp repo created by the helper, avoid
   touching the real workspace, and guard shared global state with the provided
   mutex.
4. **Structure:** prefer `Result<()>`-returning tests for easy `?` usage.
   Ensure failures `bail!` with actionable messages.
5. **Fixtures:** place new shell probes or data under `tests/mocks/`. Document
   expectations in comments and keep them deterministic so CI stays stable.
6. **Docs:** when a new test enforces a repo-wide promise, update this file and
   the relevant docs (usually `tests/AGENTS.md`, maybe `boundary/boundary_object.md`,
   `catalogs/capabilities.md`, or `probes/probes.md`) so future
   agents understand the coverage.

## Mapping tests to contracts

| Contract surface | Representative tests |
| --- | --- |
| Boundary object schema + payload semantics (schema.rs) | `boundary_object_schema`, `boundary_object_round_trips_structs`, `capabilities_schema_version_serializes_in_json` |
| Capability catalog + context wiring (catalog.rs) | `load_real_catalog_smoke`, `repository_lookup_context_matches_capabilities`, `capability_index_*` |
| Helper binaries & CLI ergonomics (cli.rs, helpers.rs) | `json_extract_*`, `portable_path_relpath_*`, `detect_stack_reports_os`, `paging_stress_*`, `payload_builder_rejects_large_payloads`, `contract_gate_*`, `probe_matrix_*`, `fencerunner_*` |
| Workspace + sandbox guarantees (probe_execution.rs) | `workspace_root_fallback`, `workspace_tmpdir_*`, `probe_resolution_guards`, `resolve_probe_metadata_prefers_script_values` |
| Probe contracts & fixtures (contracts.rs) | `harness_smoke_probe_fixture`, `dynamic_probe_contract_accepts_fixture`, `static_probe_contract_*`, `contract_gate_dynamic_flags_*`, `contract_gate_dynamic_rejects_payload_over_limit`, `paging_stress_probe_emits_expected_record` |

Use this table to decide where to plug a new test. If your change touches a
contract without an obvious row, add both the row and the tests.

## When failures occur

- **Schema or catalog diffs:** compare the emitted JSON against `boundary/boundary_object.md`
  or the capability catalog. Update schemas and regenerate helpers before
  re-running.
- **CLI guard rails:** reproduce locally with the same helper command printed by
  the test (they log the exact arguments). Most rely on binaries under `bin/`,
  so rebuild those if they drift.
- **Workspace/path issues:** rerun the failing test with `RUST_LOG=debug` to see
  the path planning traces emitted by `harness/workspace`.
- **Probe contract gates:** run `tools/validate_contract_gate.sh --probe <id|path>`,
  `bin/probe-gate --probe <id|path>`, or `bin/probe-contract-gate --probe <id|path>` to gate the
  offending script. Edit in a tight loop until the probe passes the contract
  gate--only then do you run the full suite.

Keeping this file accurate is part of the contract. If you add a new class of
checks, describe them here so the next agent knows exactly how the test suite
covers our promises.
