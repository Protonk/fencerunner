# Probes

Probes are the smallest unit of observation in `codex-fence`. Each one is a
single Bash script stored directly under `probes/<probe_id>.sh` (the filename
matches the probe id), performs a single well-defined action, and reports what
the Codex fence did with that action. This document explains how probes are
built, how the harness runs them, and how their results are captured.

This file serves as documentation. For authoritative, test-enforced Probe and Probe Author contracts, follow [probes/AGENTS.md](probes/AGENTS.md).

## What makes a probe

- **Location:** Scripts live under `probes/<probe_id>.sh`, and the filename is
  the probe id recorded in boundary objects. The tree is flat—categorization is
  handled by the capability metadata rather than subdirectories.
- **Contract:** Start with `#!/usr/bin/env bash`, immediately enable
  `set -euo pipefail`, perform a single observable action, and exit `0` after
  emitting exactly one JSON record via `bin/emit-record`. stdout must contain
  only the boundary object.
- **Behavior:** Perform one observable operation (write a file, read a sysctl,
  open a socket, spawn a process, etc.) and record what happened. Treat the
  sandbox as a black box; if the action partially succeeds, capture that nuance
  in the payload and status.
- **Capabilities:** Every probe declares exactly one
  `primary_capability_id` (with optional `secondary_capability_ids`). The ids
  come from `schema/capabilities.json` and are validated at emit time through
  the Rust capability index (the legacy adapter in `tools/adapt_capabilities.sh`
  remains for automation).
- **Helpers:** Prefer the compiled helpers in `bin/` over ad-hoc logic.
  Path canonicalization routes through `bin/portable-path`; JSON extraction
  (when you must parse another program’s JSON) goes through `bin/json-extract`.
  Build payloads/operation args with `emit-record` flags
  (`--payload-stdout/-stderr`, `--payload-raw-field[-json|-list|-null]`,
  `--operation-arg[-json|-list|-null]`) rather than constructing JSON manually.

## How the harness runs a probe

`bin/fence-run` executes probes under a specific fence mode:

1. **Environment setup** – determines the workspace root, sets the probe id,
   and exports `FENCE_RUN_MODE`, `FENCE_WORKSPACE_ROOT`, plus mode-specific
   variables such as `FENCE_SANDBOX_MODE`. Override the exported workspace via
   `--workspace-root PATH` or by setting `FENCE_WORKSPACE_ROOT`; pass an empty
   value to defer to `bin/emit-record`’s `git rev-parse`/`pwd` fallback.
   Codex modes preflight sandbox write access via `mktemp`; if the host blocks
   the sandbox from creating temp directories, `fence-run` emits a `preflight`
   record (`observed_result=denied`) instead of invoking the probe.
2. **Mode dispatch**
   - `baseline` runs the probe directly with no sandboxing.
   - `codex-sandbox` shells out through `codex sandbox macos --full-auto -- …`
     so the probe runs inside the same workspace-write seatbelt profile Codex
     applies to a trusted repo. Skip `--full-auto` only when you explicitly want
     the read-only profile; it will deny writes to both the workspace and `/tmp`.
     (Requires the Codex CLI in `PATH`.)
   - `codex-full` uses the Codex CLI with
     `--dangerously-bypass-approvals-and-sandbox` *without* the `sandbox`
     subcommand (`codex --dangerously-bypass-approvals-and-sandbox -- …`) so the
     probe runs with no Codex-applied Seatbelt profile. (Requires the Codex CLI
     in `PATH`.)
3. **Result capture** – the probe prints one JSON boundary object to stdout.
   `make matrix` stores each run as `out/<probe>.<mode>.json` so you can diff
   runs across modes, CLI versions, or host machines.

## What a probe emits

Every probe emits one [boundary object](boundary_object.md) that conforms to
`schema/boundary_object.json`. Required data includes:

- Probe identity (`probe.id`, `probe.version`,
  `probe.primary_capability_id`, `probe.secondary_capability_ids`).
- The exact command executed (`run.command`) and mode (`run.mode`).
- Operation metadata (`operation.category`, `verb`, `target`, `args`).
- Normalized outcome (`result.observed_result`, errno/message, exit codes).
- Evidence (`payload.stdout_snippet`, `payload.stderr_snippet`,
  `payload.raw` for structured notes).
- Capability context snapshots embedded by `bin/emit-record`.

`docs/boundary_object.md` describes every field along with examples. Treat
missing or malformed JSON as a probe failure—`bin/fence-run` will not try to
coerce bad output into a result.

## Implementation constraints

- **Single responsibility:** Each script exercises one policy surface. If you
  want variants (symlinks, alternate paths, different targets) create new
  probes that share helpers.
- **Portability:** Bash must run under macOS `/bin/bash 3.2` and Linux
  `/usr/bin/env bash`. Avoid features that are unavailable in 3.2 (associative
  arrays, namerefs, etc.) unless you gate them behind helper functions.
- **No noise:** stdout is reserved for the boundary object. Use stderr for
  debugging only when unavoidable and keep it short.
- **Non-interactive:** Never read from stdin or assume a TTY.
- **Workspace awareness:** Stay inside the workspace unless the probe’s sole
  purpose is to cross that boundary, and record the target you touched.
- **Deterministic JSON:** Let `emit-record` and the Rust helpers handle JSON
  serialization. Do not branch on interpreter availability—fail loudly if a
  required tool is missing.

## Testing probes

The fast authoring loop favors single-probe runs:

- `tools/validate_contract_gate.sh --probe probes/<id>.sh` runs
  the interpreted, quick-fail contract (syntax + structural checks) while you
  iterate on a single script.
- `bin/fence-test` runs the static contract against every probe in the
  repository.
- `cargo test --test suite` covers the Rust guard rails (schema validation
  and harness smoke tests).
