# tests/AGENTS.md

This document is the contract for anyone touching the test harness. Whether you
are a human developer or an automated agent, treat this as the playbook for
keeping the board green. Every change must leave `cargo test` passing, because a
single command executes the entire suite.

## Mission control

1. **Single entry point:** `cargo test`. The suite builds required helper
   binaries under `target/` and runs the integration targets; noisy output
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
| `tests/schema.rs` | Boundaries contract guards. | Boundary contract validation (via `scripts/boundaries.json`) and serde round-trips for boundary/commitment enrollment types. |
| `tests/commitments.rs` | Commitments registry guards. | Registry schema validation, schema_version enforcement, and duplicate id rejection for `commitments.json`. |
| `tests/script_execution.rs` | Run-dir execution guard rails. | Symlink escape checks during script discovery. |
| `tests/contracts.rs` | emit-record rules. | emit-record flag enforcement (including required payload snippets), outcome normalization, commitment enrollment helpers. |
| `tests/cli.rs` | CLI and harness behavior. | `fencerunner` strict/supervised ergonomics, exit codes, NDJSON stream behavior. |
| `tests/helpers.rs` | Script + payload helper guards. | Script discovery semantics (`list_scripts`/`resolve_script`) and payload builder invariants. |
| `tests/support/common.rs` | Cross-test fixtures. | Shared script fixtures (`FixtureScript`), repo/workspace guards, and sample boundary/commitment builders used by multiple targets. |
| `tests/support/` | Shared helpers. | Builds helper binaries once per run, provides temp repositories, mutex guards, path utilities. Always prefer these over ad-hoc fixtures. |

## Running and diagnosing tests

- **Full sweep:** `cargo test`. Watch for the expected integration targets:
  `schema`, `commitments`, `script_execution`, `contracts`, `cli`, `helpers`. Anything else means someone reintroduced stray targets.
- **Focused run:** `cargo test --test <target> <name>` (e.g.
  `cargo test --test cli fencerunner_runs_all_scripts_in_run_dir`) to iterate on a failing case.
  Use `-- --nocapture` when you need stdout/stderr from helpers.
- **Schema debugging:** the `boundary_object_schema` test writes the failing JSON
  payload to `tmp/` with the test name. Open that file before re-running to see
  what changed (compare against `scripts/boundaries.json` and the run dir’s `boundaries.json`).

## Adding or modifying tests

1. **Decide the contract you are protecting.** Examples: boundary-object shape,
   helper CLI semantics, workspace isolation, commitments registry synchronization. Cite that
   contract in the test name or first comment.
2. **Use the shared helpers.**
   - `tests/support` provides `fencerunner_binary`, `run_command`, and `repo_root` plus
     helper builders with cached compilation.
   - `tests/support/common` exposes `FixtureScript`, `TempRepo`, repo locks, and
     sample boundary/commitment builders. Never invent new path juggling logic
     when a helper already exists.
3. **Keep tests hermetic.** Write to the temp repo created by the helper, avoid
   touching the real workspace, and guard shared global state with the provided
   mutex.
4. **Structure:** prefer `Result<()>`-returning tests for easy `?` usage.
   Ensure failures `bail!` with actionable messages.
5. **Fixtures:** prefer `scripts/minimal_example.sh` as the canonical runnable script; for custom script behaviors use `FixtureScript::install_from_contents_in_run_dir` with inline script contents.
6. **Docs:** when a new test enforces a repo-wide promise, update this file and
   the relevant docs (usually `tests/AGENTS.md`, maybe `docs/boundaries.md`,
   or `docs/gates.md`) so future
   agents understand the coverage.

## Mapping tests to contracts

| Contract surface | Representative tests |
| --- | --- |
| Boundaries contract + payload semantics (schema.rs) | `boundary_object_schema`, `boundary_object_round_trips_structs`, `schema_validate_*`, `commitment_enrollment_serializes_to_expected_shape` |
| Commitments registry schema + preflight (commitments.rs) | `load_default_commitments_registry_smoke`, `commitments_registry_rejects_duplicate_id`, `commitments_registry_rejects_unknown_schema_version` |
| CLI ergonomics & exit codes (cli.rs) | `fencerunner_*` |
| Script execution environment (cli.rs) | `fencerunner_runs_script_with_run_dir_cwd` |
| Workspace + sandbox guarantees (script_execution.rs) | `script_resolution_guards` |
| Script contracts & fixtures (contracts.rs) | `emit_record_*`, `commit_help_me_*`, `validate_outcome_*` |

Use this table to decide where to plug a new test. If your change touches a
contract without an obvious row, add both the row and the tests.

## When failures occur

- **Schema or commitments diffs:** compare the emitted JSON against `docs/boundaries.md`
  or the run-dir `commitments.json` schema. Update schemas and regenerate helpers before
  re-running.
- **CLI guard rails:** reproduce locally with the same helper command printed by
  the test (they log the exact arguments). Most rely on binaries under `target/`,
  so rebuild with `cargo build --bin fencerunner` if they drift.
- **Workspace/path issues:** rerun the failing test with `RUST_LOG=debug` to see
  the script discovery traces emitted by the harness.

Keeping this file accurate is part of the contract. If you add a new class of
checks, describe them here so the next agent knows exactly how the test suite
covers our promises.
