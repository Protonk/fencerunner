# AGENTS.md

If you are about to change something, read `README.md` and `CONTRIBUTING.md`, then use this file to learn where to look, what must remain true, and how to extend the system without entangling it.

This repo uses **nested AGENTS** as scoped contracts. Before editing a file, read the nearest `AGENTS.md` in that file’s directory tree:

- `src/AGENTS.md` — module routing + Rust expectations.
- `src/bin/AGENTS.md` — CLI front door + runner-owned shims.
- `tests/AGENTS.md` — test harness playbook + fixture rules.

## What's special about this place

This repository is optimized for **bold refactors with low friction**. It stays that way by treating “how the system behaves” as a set of overlapping, explicit contracts: JSON schemas, runtime validators, a single small CLI surface, and integration-heavy tests that fail hard when any of those drift. 
- **Contracts are APIs.** `commitments.json`, `gates.json`, and `boundaries.json` are not suggestions; they are validated and enforced by code and tests.
- **Overlapping guard rails.** The same rule should often appear in more than one place: schema shape, runtime checks, and tests. Redundancy is a feature here.
- **High visibility, low cleverness.** Prefer explicit parsing, explicit errors, small modules, and stable names over “smart” abstractions.
- **No hidden coupling.** Avoid hard-coded interdependencies between subsystems; it makes future refactors expensive and unpredictable.
The system’s posture assumes **authors are cooperating** (instrumentation, not containment). “Enrollment” is a signal, not a security boundary.

## What's here

>fencerunner runs run dirs: flat directories containing `*.sh` scripts and three local contracts.

| Path | Purpose |
| --- | --- |
| `schema/` | Meta-schemas for run-dir contracts: `schema/commitments.json`, `schema/gates.json`, `schema/boundaries.json`. |
| `docs/` | Narrative docs for run-dir authors: `docs/commitments.md`, `docs/gates.md`, `docs/boundaries.md`. |
| `scripts/` | Default/example run dir shipped with the repo (and used by tests): `scripts/{commitments,gates,boundaries}.json` + `scripts/*.sh`. |
| `lib/` | Runner-owned script library: `lib/library.sh` (Bash 3.2 compatible). |
| `src/` | Rust crate. `src/bin/fencerunner.rs` is the only installed binary. |
| `tests/` | Contract gate: integration tests that build/run the real binary and execute real scripts. |
| `vendor/` | Vendored crates for offline builds; keep in sync with `Cargo.lock`. |

This repo vendors its Rust dependencies under `vendor/` and forces offline builds via `.cargo/config.toml`. If you update dependencies, keep `Cargo.lock` and `vendor/` in sync by running `cargo vendor vendor --locked`.

## Build + release artifacts

- **Local build:** `cargo build --bin fencerunner` (debug) or `cargo build --release --bin fencerunner`.
- **Release artifacts:** `cargo dist` builds `--release` and writes `dist/fencerunner-v<VERSION>-<TARGET>` plus `.tar.gz` + `.sha256` files.
  - `<VERSION>` comes from `[package].version` in `Cargo.toml`.
  - `<TARGET>` is the host triple by default (`rustc -vV`), or `cargo dist --target <triple>`.

### Run-dir shape
- Run dirs are **flat**: every top-level `*.sh` is a script; subdirectories are ignored.
- Script ids come from filenames (`<script_id>.sh`) and must be **globally unique across all run dirs** in one run.
- Scripts must be **executable**; otherwise the run is a preflight/runner failure (even in `--supervised`).
- Scripts must be **real files** (no symlinks); symlinked scripts are rejected at discovery.
- Scripts execute with **CWD set to the run dir**; relative paths resolve there.

## How the pieces fit

>schema → validator → run dir → tests

- `schema/*.json` are the **meta-schemas**. They define what a run-dir contract file is allowed to look like.
- The runner **embeds** these schemas and validates `<RUN_DIR>/{commitments,gates,boundaries}.json` at preflight.
- Scripts source the runner library (`${FENCERUNNER_ROOT}/lib/library.sh`) and use runner shims:
  - `commit_help_me <ensure|detect|emit> <commitment.id>` records enrollments for the current script run.
  - `emit-record ...` emits a boundary object and serializes enrollments into `/context/commitments`.
- `cargo test` exercises the whole stack end-to-end (including the repo’s minimal example script). If a contract changes, tests should force you to update code and docs deliberately.

### Output contract
- Scripts emit **one boundary record per script** as NDJSON to stdout (strict enforces “parse + validate”; supervised synthesizes records for script-level breaks).
- Boundary records must include a `context.commitments` array (empty allowed). Commitments are recorded as `(id, helps[])` pairs.
- Stderr is diagnostic; it should never be treated as a structured output channel.

## Low friction working loop

1. **Decide which contract you’re changing.** Name it (schema / runtime check / test) before coding.
2. **Keep changes local.** Don’t make one subsystem depend on incidental details of another.
3. **Update the overlapping layers.** If you changed behavior, update docs and tests in the same patch.
4. **Validate with `cargo test`.** Treat failures as contract disagreements, not annoyances.
