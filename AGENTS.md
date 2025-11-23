# AGENTS.md

## Start here
- `README.md` explains why `codex-fence` exists, how probes run across modes, and the harness vocabulary—skim it before touching anything else.
- `CONTRIBUTING.md` is the repo-level contract for changes outside a single probe; it encodes the portability + documentation rules you must honor.
- Keep schema, adapters, docs, and tests in lockstep. If you change capabilities or boundary objects, version the JSON under `schema/`, update tooling, and extend tests before landing.

## Root expectations
- Treat this file as a router: decide which subsystem you are editing, then obey the `*/AGENTS.md` in that directory so guidance stays layered instead of duplicated here.
- Use the supported workflows: `tools/validate_contract_gate.sh --probe <id>` (or `make probe PROBE=<id>`) for tight probe loops, `cargo test --test suite` for guard rails (which now shells out to `fence-test` to sweep every probe contract), and `codex-fence --bang` when you need to exercise probes across modes. Codex modes now preflight sandbox write access and emit a `preflight` record with `observed_result=denied` when the platform blocks mktemp/mkdir, so streamed runs keep going with an explicit signal instead of failing probes.
- `bin/codex-fence` is the top-level CLI for bang/listen/test and delegates to Rust helpers; keep it aligned with the Makefile defaults and existing harness scripts instead of reimplementing probe logic.
- Preserve the portability stance described in README/CONTRIBUTING—scripts must run on macOS `/bin/bash 3.2` and the `codex-universal` container with the Rust helpers that ship in `bin/` (sync them via `make build-bin`).
- **Do not introduce new runtime dependencies beyond Bash and the existing Rust binaries.** If you need new behavior, express it in Bash or extend the Rust helpers instead of pulling additional interpreters into the runtime path.
- Canonicalize paths before enforcing workspace/probe boundaries. Use `bin/portable-path realpath|relpath` instead of rolling ad‑hoc calls.
- Keep new policy in machine artifacts (schemas, scripts, tests). Documentation and AGENTS files explain those artifacts; they do not replace them.

## Layered contracts (read before editing those areas)
- `probes/AGENTS.md` — Probe author contract: one observable action per script, cfbo-v1 emission rules, capability metadata selection.
- `tests/AGENTS.md` — Structure of the test harness, fixture locations, and guidance for fast vs. second-tier suites.
- `src/bin/AGENTS.md` - Structure and expectations for Rust code, exclusive of the helpers in...
  - `src/bin/AGENTS.md` — Guarantees for the Rust helpers (`codex-fence`, `fence-run`, `emit-record`, `detect-stack`, etc.); keep their CLIs and stack metadata stable.
- `tools/AGENTS.md` — Explains the tool calls available to you.
- `docs/AGENTS.md` — How explanatory docs relate to machine contracts; update entries there whenever you add a new explainer.

## Directory map
| Path | Purpose / Notes |
| --- | --- |
| `bin/` | Prebuilt Rust helper binaries (`codex-fence`, `fence-run`, etc.) synced from the sources in `src/bin/`. |
| `docs/` | Human-readable explanations for schemas, probes, and boundary objects; keep these aligned with machine contracts. |
| `probes/` | Flat directory of `<probe_id>.sh` scripts and the probe author contract. |
| `schema/` | Machine-readable capability catalog plus boundary-object schema JSON consumed by tooling. |
| `src/` | Rust sources for the CLI and helpers. |
| `target/` | Cargo build artifacts; delete when you need a clean rebuild. |
| `tests/` | Library helpers, the static probe contract, and Rust guard rails (`tests/suite.rs`). |
| `tmp/` | Scratch space for probe/test runs; currently packed with ephemeral `.tmp*` directories that are safe to purge. |
| `tools/` | Developer tooling (validation scripts, helpers) used by supported workflows. |
