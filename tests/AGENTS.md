# tests/AGENTS.md

This guide orients agents who need to understand, extend, or debug the harness
tests. 

## Mental model

`tests/` enforces that probes and helpers stay portable and in sync with the
public boundary-object schema. The directory is split into three layers:

| Layer | Entry point | Purpose |
| --- | --- | --- |
| Library | `tests/library/` | Shared Bash helpers + fixtures depended on by every suite. |
| Fast tier | `tests/run.sh --probe <id>` | Syntax lint + static probe contract for one probe—the tight authoring loop. |
| Second tier | `tests/run.sh` | Global checks that validate documentation, schema, and harness plumbing. |

The default make target `make test` simply runs `tests/run.sh` with no
arguments, so anything added here must be portable (`/bin/bash 3.2` on macOS),
silent on success, and deterministic.

For full-repository or probe-focused audits, stop here and instead read
`tests/audits/AGENTS.md`, which contains prompts for holistic and probe-only
audits.

## Quick start for agents

1. **While editing a probe** use `tests/run.sh --probe <id>` (or `make probe
   PROBE=<id>`). This runs `probe_contract/light_lint.sh` followed by
   `probe_contract/static_probe_contract.sh` for the resolved probe path.
2. **Before sending a change** run `tests/run.sh`. It automatically lints every
   probe, then executes the second-tier suites. Failures are summarized with a
   `[FAIL]` line; re-run the failing script directly to iterate faster.
3. **Debugging**: All suites are normal Bash scripts. Run them directly (e.g.
   `tests/second_tier/harness_smoke.sh`) to reproduce failures. They only depend
   on in-repo helpers.

## Library components

- `tests/library/utils.sh` exposes `REPO_ROOT`, `extract_probe_var`, and
  `resolve_probe_script_path`. Source it from any new suite instead of duplicating
  path logic. It already sources `lib/portable_realpath.sh` so probe paths are
  canonicalized before prefix checks—reuse that helper whenever you need to
  reason about files under `probes/` or the workspace.
- `tests/library/json_schema_validator.sh` is a hermetic JSON Schema validator
  implemented entirely with `jq`. It covers the subset of Draft-07 the harness
  needs. Use it when validating emitted records against
  `schema/boundary_object.json` (see the boundary object suite for usage).
- `tests/library/fixtures/probe_fixture.sh` is a self-contained probe used by the
  smoke suites. It writes to a temporary workspace and pipes a deterministic
  record into `bin/emit-record`. Prefer copying this file when you need a dummy
  probe rather than inventing ad‑hoc scripts.

## Second-tier suite map

| Script | Purpose | Notes |
| --- | --- | --- |
| `second_tier/capability_map_sync.sh` | Confirms docs/data/probe_cap_coverage_map.json, tools/capabilities_adapter.sh, and probe metadata all reference the same capability ids. | Update docs + adapter when adding capabilities before rerunning this script. |
| `second_tier/boundary_object_schema.sh` | Runs `bin/emit-record` with a fixture payload and validates the resulting JSON with `jq`. | Extend the jq expression whenever schema/boundary_object.json grows. |
| `second_tier/harness_smoke.sh` | Runs the fixture probe via `bin/fence-run baseline` and checks the returned boundary object. | Keeps the baseline path honest; extend if fixtures gain new fields. |
| `second_tier/baseline_no_codex_smoke.sh` | Temporarily hides the Codex CLI from `PATH` and asserts baseline runs still succeed while codex modes fail. | Make sure new smoke fixtures do not depend on `codex`. |
| `second_tier/workspace_root_fallback.sh` | Executes the fixture probe with `FENCE_WORKSPACE_ROOT` cleared to confirm `bin/emit-record` falls back to `git rev-parse`/`pwd`. | Protects the documented workspace root fallback contract. |
| `second_tier/probe_resolution_guards.sh` | Attempts to run `bin/fence-run` against paths/symlinks outside `probes/` and expects hard failures. | Use as a template for future negative guard-rail tests. |

Add any heavier “whole repo” validation here. Follow the same structure: source
`tests/library/utils.sh`, short-circuit on missing prerequisites, and print
`name: PASS/FAIL` summaries.

## Adding or modifying tests

- **Guard-rail comments:** Every script now starts with a summary block—keep this
  habit so future agents know why a suite exists.
- **New probe-level checks:** Extend `tests/probe_contract/static_probe_contract.sh`
  (for structural, non-executing rules) or `tests/probe_contract/light_lint.sh`
  (for syntax-only issues). This keeps the single-probe workflow fast.
- **New fixtures:** Place them under `tests/library/fixtures/` so multiple suites
  can share them, and document any special behavior.
- **New suites:** Put them under `tests/second_tier/` and add the filename (minus
  `.sh`) to the `second_tier_suites` array in `tests/run.sh` so the orchestration
  picks them up.
- **Negative harness tests:** When adding guard rails (path canonicalization,
  workspace boundaries, etc.), follow the pattern used in
  `second_tier/probe_resolution_guards.sh` to assert that failure modes remain
  enforced.

## When things fail

- **Static probe contract errors** list per-file issues (missing shebang,
  mismatched `probe_name`, etc.). Open the failing script directly and fix the
  reported condition.
- **Capability coverage failures** mean either a document drift or a probe now
  references an unknown capability. Update `docs/data/probe_cap_coverage_map.json` and
  `tools/capabilities_adapter.sh` in the same commit.
- **Harness/baseline smoke failures** often indicate regressions in
  `bin/fence-run` or `bin/emit-record`. Run the failing script with `bash -x` to
  inspect the plumbing.
- **Schema failures** will dump the offending JSON record; compare it with
  `docs/boundary_object.md` and update the jq assertions in lock-step.

Keeping this structure light and documented lets agents diagnose a broken probe
run quickly, even when the error surfaced far away from their changes.
