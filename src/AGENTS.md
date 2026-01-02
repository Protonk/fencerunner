# Agent Guidance for `src/`

`src/` is the shared Rust crate used by every helper binary. The module layout
is intentional: callers should import from the module that owns the behavior,
not from `lib.rs`. This file is a router so you can land in the right place
fast without spelunking.

## Quick routing by task
- **Contract file names + small parsing helpers**
  - `src/repo_tools.rs`
- **Boundary object types and boundary contracts**
  - `src/boundary/types.rs` (record structs)
  - `src/boundary/contract.rs` (BoundaryContractIndex)
- **Probe commitment registry**
  - `src/commitments/model.rs` (registry structs)
  - `src/commitments/index.rs` (CommitmentIndex + schema enforcement)
- **Probe discovery**
  - `src/probes/discovery.rs` (resolve/list probes)
- **Probe gate enrollment (`gates.json`)**
  - `src/gates/contract.rs` (GatesContractIndex)
- **Harness/runtime helpers**
  - `src/harness/run_dir_plan.rs` (run-dir preflight + probe planning)
  - `src/harness/payload.rs` (emit-record payload builders + validation)
- **Shared JSON Schema loader**
  - `src/schema/loader.rs`
- **Module index only (no logic)**
  - `src/lib.rs`, `src/*/mod.rs`

## Patterns to preserve
- One source of truth per concern: do not duplicate path resolution, probe
  lookup, or schema validation in binaries.
- Keep errors actionable and consistent; binaries surface these directly.
- Portability is part of the contract: macOS 13-era hosts and CI containers
  must both run without extra runtime deps.
- When behavior is subtle, add a comment and a focused test.

## Working loop
- Prefer `cargo test` after changes.
- If you add, remove, or rename a module, update this file so the routing map
  stays accurate.
