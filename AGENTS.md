# AGENTS.md

If you are reading this, you have already seen `README.md` and `CONTRIBUTING.md` and you are ready to change something. This file is the shared contract for all agents—human or automated—working in this repository. It explains how to route yourself to the right place and what expectations apply everywhere, regardless of which subsystem you touch. Think of it as the index of agent responsibilities.

## Repository layout

For quick orientation, this is how the tree is organized.

| Path      | Purpose / Notes                                                                                                                                       |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bin/`    | Prebuilt Rust helper binaries (`probe`, `probe-exec`, `probe-matrix`, `emit-record`, `portable-path`, `detect-stack`, etc.) synced from `src/bin/`. |
| `catalogs/` | Capability catalogs and boundary schema descriptors (bundled examples: `macos_codex_v1.json`, `cfbo-v1.json`).                                      |
| `docs/`   | Human-readable explanations for schemas, probes, and boundary objects; kept aligned with machine contracts like `schema/*.json` and the tests.      |
| `probes/` | Flat directory of `<probe_id>.sh` scripts plus `probes/AGENTS.md`, the only code that directly exercises the sandboxed runtime.                     |
| `schema/` | Machine-readable schemas (`boundary_object_schema.json`, `capability_catalog.schema.json`) consumed by tooling.                                     |
| `src/`    | Rust sources for the CLI and helpers, including implementations for every binary under `bin/`.                                                      |
| `target/` | Cargo build artifacts created by `cargo build` or `cargo test`; safe to delete when you need a clean rebuild.                                       |
| `tests/`  | Rust guard rails (`tests/suite.rs`), shared helpers, and fixtures that enforce the contracts under `cargo test`.                                    |
| `tmp/`    | Scratch space for probe and test runs; populated with ephemeral `.tmp*` directories that are safe to purge.                                         |
| `tools/`  | Developer tooling (validation scripts, adapters, contract gates) used by the supported workflows described above.                                   |

When in doubt:
- Find the directory you are about to change.
- Read its `AGENTS.md` end-to-end before editing.
That is the main “rule of engagement” in this repository.

### Layered guidance

Once you know which part of the tree you are touching, defer to the `AGENTS.md` in that directory if it exists.
* `probes/AGENTS.md` — Probe Author contract: one observable action per script, boundary-object emission rules, capability selection, and how to use helpers like `emit-record`, `portable-path`, and `json-extract`.
* `tests/AGENTS.md` — Test harness contract: how `cargo test` is wired, where fixtures live, and how guard rails map to specific contracts (schema, catalog, CLI, workspace).
* `src/AGENTS.md` — Structure and expectations for Rust code under `src/`, excluding the helper CLIs.
* `src/bin/AGENTS.md` — Guarantees for the Rust helper binaries (`probe`, `probe-exec`, `emit-record`, `detect-stack`, etc.); how their CLIs and stack metadata must remain stable over time.
* `tools/AGENTS.md` — Contracts for helper scripts under `tools/` and how they fit into supported workflows.
* `docs/AGENTS.md` — How explanatory docs relate to machine contracts; when you add or change an explainer in `docs/`, this is the place that says how it should track the schemas and tests.

## Good habits for all agents

These habits let aggressive automation and human contributors coexist safely:

* Use the supported workflows. For probes, iterate with `tools/validate_contract_gate.sh --probe <id>` or `bin/probe-contract-gate <id>` to get a fast, local contract gate before running the full suite.
* Treat `bin/probe` as the top-level CLI for `--matrix`, `--target`, and `--listen`. It delegates to Rust helpers; keep its behavior aligned with the Makefile defaults and existing harness scripts rather than re-implementing probe logic in new places.
* Preserve portability: scripts must run identically under macOS `/bin/bash 3.2` and inside the CI container, using the Rust helpers shipped in `bin/`. Do not introduce new runtime dependencies beyond Bash and the existing Rust binaries. If you need new behavior, either express it in Bash or extend the Rust helpers and rebuild; do not add another interpreter or service to the runtime data path.
* Keep new policy in machine artifacts—schemas, probes, tests, tools. Documentation and AGENTS files explain those artifacts; they do not replace them. If you change a contract, there should be a schema and/or test that encodes it, and the relevant `*/AGENTS.md` should point to that enforcement.
* Use `cargo test` (or `cargo test --test suite`) as the single entry point for Rust guard rails, including schema validation and contract enforcement. All existing tests must pass to land a new contribution.
