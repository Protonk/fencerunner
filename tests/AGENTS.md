# tests/AGENTS.md

This guide orients agents who need to understand, extend, or debug the harness
tests. 

## Mental model

`tests/` enforces that probes and helpers stay portable and in sync with the
public boundary-object schema. The directory is split into four layers:

| Layer | Entry point | Purpose |
| --- | --- | --- |
| Audits | `tests/audits/` | Agent instructions for conducting holistics and probe audits. |
| Shims | `tests/shims/` | Shared Bash helpers + fixtures depended on by every suite. |
| Tests | `tests/suite.rs` | Global checks that validate documentation, schema, and harness plumbing (run via `cargo test --test suite`). |

## Quick start for agents

1. **While editing a probe** run
  `tools/validate_contract_gate.sh --probe <id>` (or
  `make probe PROBE=<id>`). This invokes the interpreted contract tester for the
  resolved probe path and surfaces syntax/structural issues immediately.
2. **Before sending a change** run `bin/fence-test` to sweep the static
  contract across every probe. 
3. **Debugging**: The second-tier guard rails are standard Rust integration
  tests. Use `cargo test --test suite <name>` (for example
  `cargo test --test suite workspace_root_fallback`) to focus on one
  failing case. They only depend on in-repo helpers.

## Library components

- Schema validation now lives entirely in the Rust guard rails—run
  `cargo test --test suite boundary_object_schema` to lint the emitted
  boundary object against `schema/boundary_object.json` with the `jsonschema`
  crate.
- `tests/shims/minimal_probe.sh` is a self-contained probe used by the
  smoke suites. It writes to a temporary workspace and pipes a deterministic
  record into `bin/emit-record`. Prefer copying this file when you need a dummy
  probe rather than inventing ad‑hoc scripts.

## suite map

All guard rails now live in `tests/suite.rs` and run as Rust integration
tests. Target a specific scenario with `cargo test --test suite <name>`.

| Test | Purpose | Notes |
| --- | --- | --- |
| `boundary_object_schema` | Runs `bin/emit-record` with a fixture payload and validates the resulting JSON plus the boundary object schema. | Extend the assertions and schema when the boundary_object contract grows. |
| `harness_smoke_probe_fixture` | Runs the fixture probe via `bin/fence-run baseline` and checks the returned boundary object. | Keeps the baseline path honest; extend if fixtures gain new fields. |
| `baseline_no_codex_smoke` | Temporarily hides the Codex CLI from `PATH` and asserts baseline runs still succeed while codex modes fail. | Make sure new smoke fixtures do not depend on `codex`. |
| `workspace_root_fallback` | Executes the fixture probe with `FENCE_WORKSPACE_ROOT` cleared to confirm `bin/emit-record` falls back to `git rev-parse`/`pwd`. | Protects the documented workspace root fallback contract. |
| `probe_resolution_guards` | Attempts to run `bin/fence-run` against paths/symlinks outside `probes/` and expects hard failures. | Use as a template for future negative guard-rail tests. |
| `dynamic_probe_contract_accepts_fixture` | Runs `tools/validate_contract_gate.sh --probe …` (dynamic gate) against the fixture probe to keep the stub parser aligned with emit-record flags. | Keep in sync with contract gate behavior and emit patterns. |
| `json_extract_enforces_pointer_and_type` | Validates `bin/json-extract` pointer/type/default semantics. | Extend when `json-extract` grows new flags. |

Add any heavier “whole repo” validation here. Follow the same structure: short-
circuit on missing prerequisites, and print `name: PASS/FAIL` summaries.

## Adding or modifying tests

- **Guard-rail comments:** Every script now starts with a summary block—keep this
  habit so future agents know why a suite exists.
- **New probe-level checks:** Extend `tools/validate_contract_gate.sh`
  when adding additional structural or syntax rules so the single-probe
  workflow stays fast.
- **New fixtures:** Place them under `tests/shims/` so multiple suites
  can share them, and document any special behavior.
- **New suites:** Add more Rust tests to `tests/suite.rs`. Keep them
  hermetic, reuse the fixture helpers, and gate probe directory mutations with
  the shared mutex already defined in that file.
- **Negative harness tests:** When adding guard rails (path canonicalization,
  workspace boundaries, etc.), follow the pattern used in the
  `probe_resolution_guards` test inside `tests/suite.rs` to ensure failure
  modes remain enforced.

## When things fail

- **Static probe contract errors** list per-file issues (missing shebang,
  mismatched `probe_name`, etc.). Open the failing script directly and fix the
  reported condition.
- **Harness/baseline smoke failures** often indicate regressions in
  `bin/fence-run` or `bin/emit-record`. Run the failing script with `bash -x` to
  inspect the plumbing.
- **Schema failures** will dump the offending JSON record; compare it with
  `docs/boundary_object.md` and update the schema/tests in lock-step. The Rust
  guard rails enforce the JSON schema; the dynamic gate only verifies shape and
  required flags.

Keeping this structure light and documented lets agents diagnose a broken probe
run quickly, even when the error surfaced far away from their changes.
