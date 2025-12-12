# probes/AGENTS.md

This directory contains probes: small programs built to test validated capabilities and emit contracted output, allowing us to probe a sandboxed runtime without assuming its exact policy. Read this file to understand the Probe and Probe Author contract.

## Probe Author contract

As the Probe Author, you:
- Use the capability catalog in `catalogs/macos_codex_v1.json` to select accurate
  `primary_capability_id` values. `bin/emit-record` validates IDs, so use the
  exact slugs defined in that file.
- Read the active boundary schema descriptor (defaults resolve from
  `catalogs/defaults.json`, initially `catalogs/cfbo-v1.json`,
  which points at `schema/boundary_object_schema.json`) alongside
  `docs/boundary_object.md` to understand every field the probe must provide.
- Review existing probes under `probes/` to see which behaviors already have
  coverage and how outcomes are classified.
- Keep a tight edit/test loop. While iterating on a script, run the contract
  gate (`bin/probe-contract-gate probes/<id>.sh`). This is a quick-fail static
  and dynamic probe tester designed for rapid use.

Keep each probe:
- Small and single-purpose. When you need reusable helpers (path
  canonicalization, metadata extraction, JSON parsing), shell out to the
  compiled utilities in `bin/` (for example `bin/portable-path` for realpath/
  relpath, `bin/json-extract` when you must parse JSON). Build payloads and
  operation args with `bin/emit-record` flags (`--payload-stdout/-stderr`,
  `--payload-raw-field[-json|-list|-null]`, `--operation-arg[...]`) instead of
  constructing JSON manually. Include `--run-mode "$FENCE_RUN_MODE"` so
  emitted records capture the active mode (`probe-exec` exports both
  `FENCE_*` and legacy `FENCE_*` names).
- Clearly labeled with `primary_capability_id`. Choose the best match from the
  catalog and optionally list related capabilities in
  `secondary_capability_ids`. `bin/emit-record` enforces these IDs.

Never:
- Print anything besides the JSON boundary object to stdout. Use stderr for
  debugging only when necessary.
- Branch behavior based on interpreter availability. If a
  required tool is missing, fail explicitly.

## Probe layout

All probes live directly under the `probes/` directory with filenames that match
their `probe.id` (for example, `probes/fs_outside_workspace.sh`). This flat
layout explicitly decides against role- and category-specific subdirectories—every script is
just a probe. Keep capability metadata accurate so downstream tooling can reason
about coverage without depending on directory names.

## Using compiled helpers from probes

Probes may delegate narrowly scoped work to compiled helpers under
`probe-runtime/` (synced into `bin/` by `make build`). Keep the probe as the
orchestrator: pass explicit arguments, enforce a timeout, and still emit the
single JSON record via `bin/emit-record`. Helpers must stay quiet on stdout,
run in the foreground, and use stable, documented exit codes (0 success, 1
invalid args, 2 internal error, 3 timeout). Keep helper CLIs small and
capability-aligned so their behavior is easy to reason about from the probe and
its README.

## Probe description and agent guidance (boundary_event_v1 + cfbo-v1 schema key)

A probe:
1. Is an executable script under `probes/<probe_id>.sh`, where the filename
   matches the `probe.id`. Use `#!/usr/bin/env bash`, immediately enable
   `set -euo pipefail`, and keep the script focused on a single observation.
2. Performs exactly *one* focused operation (file IO, DNS, network socket,
   process spawn, etc.). Gather whatever context you need to describe the
   attempt. Capture the command you actually ran (e.g.,
   `printf -v command_executed "... %q" ...`) and pass it through `--command`
   so the boundary object contains reproducible execution context. The `run`
   object contains only mode/workspace/command—no timestamps—so probes never
   need to track clocks.
3. Collects stdout/stderr snippets (keep them short) and structured data in the
   payload. Normalize probe outcomes into: `success`, `denied`, `partial`, or
   `error`. Treat sandbox denials (`EACCES`, `EPERM`, network blocked, etc.) as
   `denied`.
4. Calls `bin/emit-record` once with the correct flags (payload/operation args
   built inline). Pass `--run-mode "$FENCE_RUN_MODE"` (exported by
   `bin/probe-exec`) so the emitted record matches the current mode.
5. Exits with status `0` after emitting JSON. `bin/probe-exec` relies on this
  behavior so `probe --matrix` can stream records as NDJSON.

### How a probe should emit JSON

Call `bin/emit-record` exactly once with:

- `--run-mode "$FENCE_RUN_MODE"` (already exported by `bin/probe-exec`).
- `--probe-name "$probe_id"` and `--probe-version "<semver>"`.
- `--primary-capability-id`, zero or more `--secondary-capability-id`, and
  `--command`.
- `--category`, `--verb`, `--target`, and `--operation-args '{}'`.
- Outcome metadata (`--status` → `result.observed_result`, `--errno`,
  `--message`, `--raw-exit-code`, etc.) plus `--payload-file`.

See `docs/boundary_object.md` for a complete field description (the
boundary_event_v1 pattern includes `capabilities_schema_version` and
`capability_context` snapshots to
provide full context for every record).
`capabilities_schema_version` is the CatalogKey chosen by the harness when it
loads capability catalogs via the Rust `CatalogRepository` (`src/catalog/`).
Probes should continue to declare capability IDs only; the harness resolves
those IDs to snapshots without hard-coding a specific catalog.

### Minimal example

Excerpt from `probes/fs_outside_workspace.sh`:

```bash
primary_capability_id="cap_fs_write_workspace_tree"
# This probe targets cap_fs_write_workspace_tree by confirming writes are denied outside the allowed roots.
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_path}"

"${emit_record_bin}" \
  --run-mode "${FENCE_RUN_MODE}" \
  --probe-name "${probe_name}" \
  --probe-version "1" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --category "fs" \
  --verb "write" \
  --target "${target_path}" \
  --status "${status}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --raw-exit-code "${raw_exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
```

Matching JSON output (trimmed for brevity):

```json
{
  "schema_version": "boundary_event_v1",
  "schema_key": "cfbo-v1",
  "capabilities_schema_version": "macOS_codex_v1",
  "probe": {
    "id": "fs_outside_workspace",
    "version": "1",
    "primary_capability_id": "cap_fs_write_workspace_tree",
    "secondary_capability_ids": []
  },
  "run": {
    "mode": "baseline",
    "workspace_root": "/path/to/workspace",
    "command": "printf 'probe write ...' >> '/tmp/probe-outside-root-test'"
  },
  "result": {
    "observed_result": "denied",
    "raw_exit_code": 1,
    "errno": "EACCES",
    "message": "Permission denied",
    "error_detail": null
  },
  "operation": {
    "category": "fs",
    "verb": "write",
    "target": "/tmp/probe-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 38}
  },
  "payload": {
    "stdout_snippet": "",
    "stderr_snippet": "bash: /tmp/probe-outside-root-test: Permission denied",
    "raw": {}
  },
  "capability_context": {
    "primary": {
      "id": "cap_fs_write_workspace_tree",
      "category": "filesystem",
      "layer": "os_sandbox"
    },
    "secondary": []
  },
  "stack": {
    "sandbox_mode": null,
    "os": "Darwin 23.3.0 arm64"
  }
}
```

This JSON links the probe to capability `cap_fs_write_workspace_tree`, records
the executed command, and classifies the outcome using the `observed_result`
vocabulary. Use this pattern whenever you add a new probe.
