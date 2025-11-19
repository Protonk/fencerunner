# AGENTS.md

## Start here
- `README.md` explains why `codex-fence` exists, how probes run across modes, and the harness vocabulary—skim it before touching anything else.
- `CONTRIBUTING.md` is the repo-level contract for changes outside a single probe; it encodes the portability + documentation rules you must honor.
- Keep schema, adapters, docs, and tests in lockstep. If you change capabilities or boundary objects, version the JSON under `schema/`, update tooling, and extend tests before landing.

## Root expectations
- Treat this file as a router: decide which subsystem you are editing, then obey the `*/AGENTS.md` in that directory so guidance stays layered instead of duplicated here.
- Use the supported workflows: `tests/run.sh --probe <id>` (or `make probe PROBE=<id>`) for tight probe loops, `tests/run.sh`/`make test` for repo-wide checks, and `make matrix` when you need to exercise probes across modes.
- `bin/codex-fence` is the top-level CLI for bang/listen/test and delegates to Rust helpers; keep it aligned with the Makefile defaults and existing harness scripts instead of reimplementing probe logic.
- Preserve the portability stance described in README/CONTRIBUTING—scripts must run on macOS `/bin/bash 3.2` and the `codex-universal` container with only `jq` plus the stock Python → Perl fallback used by `lib/portable_{real,rel}path.sh`.
- **Do not introduce new runtime dependencies beyond Bash + jq + the existing Python→Perl fallback used by the helpers.** If you need new behavior, express it in Bash/jq or justify the change in capability/docs updates rather than pulling in extra interpreters.
- Canonicalize paths before enforcing workspace/probe boundaries. Source `lib/portable_realpath.sh` / `lib/portable_relpath.sh` instead of rolling ad‑hoc `readlink`/`python` calls—mixed strategies are how regressions like probe path escapes reappear.
- Keep new policy in machine artifacts (schemas, scripts, tests). Documentation and AGENTS files explain those artifacts; they do not replace them.

## Layered contracts (read before editing those areas)
- `probes/AGENTS.md` — Probe author contract: one observable action per script, cfbo-v1 emission rules, capability metadata selection.
- `tests/AGENTS.md` — Structure of the test harness, fixture locations, and guidance for fast vs. second-tier suites.
  - `tests/audits/AGENTS.md` - Instructions for agents engaged in audits of the whole project or a subset of probes. 
- `bin/AGENTS.md` — Guarantees for `fence-run`, `emit-record`, and `detect-stack`; keep their CLIs and stack metadata stable.
- `lib/AGENTS.md` — Helper purity + one-function-per-file rule so probes/tests can source helpers safely.
- `tools/AGENTS.md` — Capabilities adapter/validator contracts; reuse them instead of parsing `schema/capabilities.json` manually.
- `docs/AGENTS.md` — How explanatory docs relate to machine contracts; update entries there whenever you add a new explainer.

## Directory map
| Path | Purpose / Notes |
| --- | --- |
| `bin/` | Core entry points (`fence-run`, `emit-record`, `detect-stack`) plus the `codex-fence` CLI shims for bang/listen/test; orchestrate probe execution + record emission. |
| `lib/` | Single-function Bash helpers shared by probes/tests; pure, portable scripts. |
| `tools/` | Capability adapters and validators invoked by bin/tests; keep metadata normalized. |
| `probes/` | Flat directory of `<probe_id>.sh` scripts plus the probe author contract. |
| `tests/` | Library helpers, fast-tier lint/static contract, and second-tier suites driven by `tests/run.sh`. |
| `docs/` | Human-readable explanations for schemas, probes, and boundary objects; cross-check with machine artifacts. |
| `schema/` | Machine-readable capability catalog and boundary-object schema consumed by tooling. |
| `out/` | Probe outputs (`<probe>.<mode>.json`) produced by `bin/fence-run`; inspect diffs here. |
| `tmp/` | Scratch space for probe/test runs; safe to clean. |
| `Makefile` | Convenience targets (`matrix`, `test`, `probe`) that tie bin/tools/tests together. |
| `README.md` / `CONTRIBUTING.md` | Motivation plus repo-level contribution principles referenced above. |
