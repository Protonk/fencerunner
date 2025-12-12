# Probes

Probes are the smallest unit of observation in `probe`. Each one is a
single Bash script stored directly under `probes/<probe_id>.sh` (the filename
matches the probe id), performs a single well-defined action, and reports what
the harness observed in that sandbox. This document explains how probes are
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
  come from the capability catalog (`catalogs/macos_codex_v1.json` by default)
  and are validated at emit time through the Rust capability index (the legacy
  adapter in `tools/adapt_capabilities.sh` remains for automation).
- **Helpers:** Prefer the compiled helpers in `bin/` over ad-hoc logic.
  Path canonicalization routes through `bin/portable-path`; JSON extraction
  (when you must parse another program’s JSON) goes through `bin/json-extract`.
  Build payloads/operation args with `emit-record` flags
 (`--payload-stdout/-stderr`, `--payload-raw-field[-json|-list|-null]`,
  `--operation-arg[-json|-list|-null]`) rather than constructing JSON manually.

## How the harness runs a probe

`bin/probe-exec` executes probes under a specific mode:

1. **Environment setup** – determines the workspace root, sets the probe id,
   and exports `FENCE_RUN_MODE`, `FENCE_WORKSPACE_ROOT`, plus any caller-set
   `FENCE_SANDBOX_MODE` value. Override the exported workspace via
   `--workspace-root PATH` or by setting `FENCE_WORKSPACE_ROOT`; pass an empty
   value to defer to `bin/emit-record`’s `git rev-parse`/`pwd` fallback.
2. **Mode dispatch** – `baseline` runs the probe directly with no extra
   sandboxing. (This is the only supported mode after removing Codex-specific
   runners.)
3. **Result capture** – the probe prints one JSON boundary object to stdout.
  `probe --matrix` streams every record as NDJSON so you can capture and diff
  runs across modes, CLI versions, or host machines.

## What a probe emits

Every probe emits one [boundary object](boundary_object.md) that conforms to
the active boundary-object schema (defaults resolve from
`catalogs/defaults.json`, initially the `cfbo-v1` descriptor in `catalogs/`,
which points to the canonical `schema/boundary_object_schema.json` pattern and
carries `schema_key: "cfbo-v1"`). Required data includes:

- Probe identity (`probe.id`, `probe.version`,
  `probe.primary_capability_id`, `probe.secondary_capability_ids`).
- The exact command executed (`run.command`) and mode (`run.mode`).
- Operation metadata (`operation.category`, `verb`, `target`, `args`).
- Normalized outcome (`result.observed_result`, errno/message, exit codes).
- Evidence (`payload.stdout_snippet`, `payload.stderr_snippet`,
  `payload.raw` for structured notes).
- Capability context snapshots embedded by `bin/emit-record`.

`docs/boundary_object.md` describes every field along with examples. Treat
missing or malformed JSON as a probe failure—`bin/probe-exec` will not try to
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

## Probe contract gate

The **probe contract gate** is the harness-level guard that decides whether a
probe is acceptable. It is implemented in shell and Rust and currently has two
subsidiary checks—static and dynamic gating—but the top-level concept is “does
this probe obey the contract?” regardless of how the answer is computed.

### Entry points

- `tools/validate_contract_gate.sh` is the canonical gate implementation. It
  can:
  - scan all probes statically (no arguments; used by `probe-gate`), or
  - gate a single probe with `--probe <id|path>`, optionally restricted to
    specific run modes with `--modes`.
- `bin/probe-contract-gate` is a stable wrapper that execs
  `tools/validate_contract_gate.sh` from `bin/`, so external tooling can call
  the gate without knowing the layout under `tools/`.
- `bin/probe-gate` (`src/bin/probe_gate.rs`) is a Rust shim used by the
  test suite; `cargo test --test suite` shells out to it so the gate runs as
  part of the normal test loop.

When you are authoring or reviewing a probe, “running the contract gate” means
invoking one of these entry points, not re-implementing the checks by hand.

### Static gating

Static gating looks only at the probe script itself. It is implemented by
`gate_static_probe` inside `tools/validate_contract_gate.sh` and enforces:

- correct location and naming (`probes/<probe_id>.sh`, `probe_name` matches
  filename),
- basic script hygiene (`#!/usr/bin/env bash`, `set -euo pipefail`,
  `bash -n` passes),
- presence of `primary_capability_id`, and
- other structural expectations that must hold before the probe ever runs.

You normally run static gating via:

```sh
tools/validate_contract_gate.sh --probe <id> --static-only
```

or indirectly via `bin/probe-contract-gate <id>` when you only care whether the
probe passes the full gate.

### Dynamic gating

Dynamic gating runs the probe and validates what it does at runtime. It is
implemented by `gate_probe` and `run_dynamic_gate` inside
`tools/validate_contract_gate.sh`:

- The gate runs the probe through `bin/probe-exec` in one or more run modes
  (`baseline` by default).
- An embedded `emit-record` stub intercepts the probe’s call to `bin/emit-record`
  and checks:
  - all required flags are present and used at most once,
  - payload and operation-args are JSON objects within size limits,
  - `status` and `raw-exit-code` values are well-formed, and
  - `emit-record` is invoked exactly once with metadata that matches the probe
    script expectations (probe name, primary capability id, and run mode).
- The gate fails if the probe never calls `emit-record`, calls it multiple
  times, passes inconsistent metadata, or produces malformed JSON.

Use dynamic gating when you are confident the script is structurally sound and
want to validate its behavior across modes:

```sh
tools/validate_contract_gate.sh --probe <id> --modes "baseline"
bin/probe-contract-gate <id>             # uses default modes
```

### Gating in the test loop

The fast authoring loop favors single-probe runs, but the full suite enforces
gating on all checked-in probes:

- For individual probes during development:
  - `tools/validate_contract_gate.sh --probe <id>` to run static + dynamic
    checks for that probe.
  - `tools/validate_contract_gate.sh --probe <id> --static-only` when you only
    need quick structural feedback.
- For the entire repository:
  - `cargo test --test suite` exercises `bin/probe-gate`, which in turn
    runs `tools/validate_contract_gate.sh` with no arguments so the static
    contract gate scans every probe under `probes/`.
  - Additional tests in `tests/suite.rs` call the gate entry points directly to
    validate dynamic behavior and failure modes.
