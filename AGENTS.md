# AGENTS.md

## Start here
- `README.md` explains why `codex-fence` exists, how probes run across modes, and the harness vocabulary—skim it before touching anything else.
- `CONTRIBUTING.md` is the repo-level contract for changes outside a single probe; it encodes the portability + documentation rules you must honor.
- Keep schema, adapters, docs, and tests in lockstep. If you change capabilities or boundary objects, version the JSON under `schema/`, update tooling, and extend tests before landing.

## Root expectations
- Treat this file as a router: decide which subsystem you are editing, then obey the `*/AGENTS.md` in that directory so guidance stays layered instead of duplicated here.
- Use the supported workflows: `tools/contract_gate/static_gate.sh --probe <id>` (or `make probe PROBE=<id>`) for tight probe loops, `bin/fence-test` to sweep every probe contract, `cargo test --test second_tier` for guard rails, and `make matrix` when you need to exercise probes across modes.
- `bin/codex-fence` is the top-level CLI for bang/listen/test and delegates to Rust helpers; keep it aligned with the Makefile defaults and existing harness scripts instead of reimplementing probe logic.
- Preserve the portability stance described in README/CONTRIBUTING—scripts must run on macOS `/bin/bash 3.2` and the `codex-universal` container with the Rust helpers that ship in `bin/` (sync them via `make build-bin`).
- **Do not introduce new runtime dependencies beyond Bash and the existing Rust binaries.** If you need new behavior, express it in Bash or extend the Rust helpers instead of pulling additional interpreters into the runtime path.
- Canonicalize paths before enforcing workspace/probe boundaries. Use `bin/portable-path realpath|relpath` instead of rolling ad‑hoc calls.
- Keep new policy in machine artifacts (schemas, scripts, tests). Documentation and AGENTS files explain those artifacts; they do not replace them.

## Layered contracts (read before editing those areas)
- `probes/AGENTS.md` — Probe author contract: one observable action per script, cfbo-v1 emission rules, capability metadata selection.
- `tests/AGENTS.md` — Structure of the test harness, fixture locations, and guidance for fast vs. second-tier suites.
- `src/bin/AGENTS.md` — Guarantees for the Rust helpers (`codex-fence`, `fence-run`, `emit-record`, `detect-stack`, etc.); keep their CLIs and stack metadata stable.
- `tools/AGENTS.md` — Explains the tool calls available to you.
- `docs/AGENTS.md` — How explanatory docs relate to machine contracts; update entries there whenever you add a new explainer.

## Directory map
| Path | Purpose / Notes |
| --- | --- |
| `bin/` | Synced Rust binaries produced by `make build-bin`; keep them aligned with the sources under `src/bin/`. |
| `src/bin/` | Rust implementations of `codex-fence`, `fence-run`, `emit-record`, `detect-stack`, `fence-bang/listen/test`, and `portable-path`. |
| `tools/` | Tools to support agent tool calls during development.  |
| `probes/` | Flat directory of `<probe_id>.sh` scripts plus the probe author contract. |
| `tests/` | Library helpers, the static probe contract, and Rust guard rails (`tests/second_tier.rs`). |
| `docs/` | Human-readable explanations for schemas, probes, and boundary objects; cross-check with machine artifacts. |
| `schema/` | Machine-readable capability catalog and boundary-object schema consumed by tooling. |
| `out/` | Probe outputs (`<probe>.<mode>.json`) produced by `bin/fence-run`; inspect diffs here. |
| `tmp/` | Scratch space for probe/test runs; safe to clean. |
| `Makefile` | Convenience targets (`matrix`, `probe`, `install`, `build-bin`) that tie bin/tools/tests together. |
| `README.md` / `CONTRIBUTING.md` | Motivation plus repo-level contribution principles referenced above. |
