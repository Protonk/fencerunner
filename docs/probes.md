# Probes

Probes are the smallest unit of observation in `codex-fence`. Each one is a
single Bash script in `probes/` that performs a single, well-defined action
and reports what the Codex fence did with that action. This document explains
how probes are built, how the harness runs them, and how their results are
captured.

## What makes a probe

- **Location:** Scripts live under `probes/` and are named
  `probes/<probe_id>.sh`. The filename is the probe id.
- **Contract:** Start with `#!/usr/bin/env bash`, immediately enable
  `set -euo pipefail`, and exit `0` after emitting JSON via `bin/emit-record`.
- **Behavior:** Probe code performs one observable operation (write a file,
  read a sysctl, open a socket, spawn a process, etc.) and records what
  happened. Treat the sandbox as a black box; if the action partially succeeds,
  capture that nuance in the payload.
- **Capabilities:** Every probe declares exactly one
  `primary_capability_id` (with optional `secondary_capability_ids`). The ids
  come from `spec/capabilities.yaml` and are validated at emit time via the
  adapter in `tools/capabilities_adapter.sh`.
- **Helpers:** Shared utilities live in `tools/lib/helpers.sh`
  (portable path helpers, probe metadata extraction, etc.). Source this file
  when needed instead of re-implementing interpreter detection. Helpers are
  pure and portable so probes stay single-purpose.

## How the harness runs a probe

`bin/fence-run` executes probes under a specific fence mode:

1. **Environment setup** – determines the workspace root, sets the probe id,
   and exports `FENCE_RUN_MODE` plus mode-specific variables such as
   `FENCE_SANDBOX_MODE`.
2. **Mode dispatch**
   - `baseline` runs the probe directly with no sandboxing.
   - `codex-sandbox` shells out through `codex sandbox …` so the probe runs
     inside the seatbelt profile the CLI configures for the current platform.
     (Requires the Codex CLI in `PATH`.)
   - `codex-full` currently runs the probe directly but is reserved for future
     modes where Codex disables sandboxing entirely.
3. **Result capture** – the probe prints one JSON boundary object to stdout.
   `make matrix` stores each run as `out/<probe>.<mode>.json` so you can diff
   runs across modes, CLI versions, or host machines.

## What a probe emits

Every probe emits one [boundary object](boundary-object.md) that conforms to
`schema/boundary-object-cfbo-v2.json`. Required data includes:

- Probe identity (`probe.id`, `probe.version`,
  `probe.primary_capability_id`, `probe.secondary_capability_ids`).
- The exact command executed (`run.command`) and mode (`run.mode`).
- Operation metadata (`operation.category`, `verb`, `target`, `args`).
- Normalized outcome (`result.observed_result`, errno/message, exit codes).
- Evidence (`payload.stdout_snippet`, `payload.stderr_snippet`,
  `payload.raw` for structured notes).
- Capability context snapshots embedded by `bin/emit-record`.

`docs/boundary-object.md` describes every field along with examples. Treat
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

## Testing probes

The fast authoring loop favors single-probe runs:

- `tests/run.sh --probe <id>` (or `make probe PROBE=<id>`) lints the target
  script and enforces the static probe contract without touching the rest of
  the suite.
- `make test` runs the fast tier for every probe and then executes the second
  tier (capability map sync, schema validation, harness smoke tests). Use this
  before sending patches or running `make matrix`.
- `make validate-capabilities` checks that every probe, fixture, and stored
  boundary object references real capability ids.

See `AGENTS.md` for the full Probe Author workflow and contribution
expectations, and `README.md` for how probes fit into the broader harness.
