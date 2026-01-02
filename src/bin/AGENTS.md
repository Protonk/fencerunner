# Agent Guidance for Rust Binaries

`src/bin/` contains the `fencerunner` CLI entry point. Build it with
`cargo build --bin fencerunner` (or via `cargo test`). When you change
behavior, update Rust here first and keep the tests/docs in sync.

## CLI entry points (front doors)

### `fencerunner`
Front door for:

- `fencerunner [--strict|--supervised] <RUN_DIR>...`

`fencerunner` performs run-dir preflight, executes each `*.sh` script, and
streams boundary records as NDJSON on stdout.

- **Strict mode (default):** contract breaks are failures (non-zero exit).
- **Supervised mode (`--supervised`):** contract breaks become synthetic error
  records; exit 0 unless preflight/runner fails.

## Runner-owned shims (script-facing)

Scripts call a small set of runner-provided helper commands, but they are not
installed as standalone binaries. Instead, fencerunner materializes an
ephemeral `FENCERUNNER_ROOT` at runtime (see `harness/runner_root`) containing:

- `${FENCERUNNER_ROOT}/lib/library.sh` — script library, sourced by every script.
- `${FENCERUNNER_ROOT}/bin/emit-record` — shim that execs `fencerunner __emit-record`.
- `${FENCERUNNER_ROOT}/bin/commit-help-me` — shim that execs `fencerunner __commit-help-me`.

These shims rely on `FENCERUNNER_BIN` being set by the runner so scripts always
invoke the correct fencerunner binary.

## Expectations across binaries
- Subscribe to shared logic in `repo_tools` and `lib.rs` instead of rolling your own
  path/sandbox handling.
- Keep argument parsing explicit and defensive; surface actionable errors.
- Reflect behavioral changes in the narrative guides and tests so shell callers stay in sync.
- Portability is non-negotiable: binaries must run on macOS `/bin/bash 3.2` and
  inside CI containers with only Bash plus the runner-provided shims described above.
