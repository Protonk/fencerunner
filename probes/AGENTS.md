# probes/AGENTS.md

This directory contains probes: small programs built to test validated capabilities and emit contracted output, allowing us to probe a sandboxed runtime without assuming its exact policy. Read this file to understand the Probe and Probe Author contract.

## Probe Author contract

As the Probe Author, you:
- Use the active capability catalog (defaults to the bundled
  `catalogs/macos_codex_v1.json`, or whatever `--catalog` / `CATALOG_PATH`
  points to) to select accurate `primary_capability_id` values.
  `bin/emit-record` validates IDs, so use the exact slugs defined in that file.
- Read the active boundary object schema (defaults resolve to
  `boundary/boundary_object_schema.json`) alongside
  `boundary/boundary_object.md` to understand the required fields and the
  common context/payload conventions.
- Review existing probes under `probes/` to see which behaviors already have
  coverage and how outcomes are classified.
- Keep a tight edit/test loop. While iterating on a script, run the contract
  gate (`bin/probe-contract-gate --probe <id|path>`). This is a quick-fail static
  and dynamic probe tester designed for rapid use.

Keep each probe:
- Small and single-purpose. When you need reusable helpers (path
  canonicalization, metadata extraction, JSON parsing), shell out to the
  compiled utilities in `bin/` (for example `bin/portable-path` for realpath/
  relpath, `bin/json-extract` when you must parse JSON). Build payloads and
  operation args with `bin/emit-record` flags (`--payload-stdout/-stderr`,
  `--payload-raw-field[-json|-list|-null]`, `--operation-arg[...]`) instead of
  constructing JSON manually.
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
`bin/` (synced from `src/bin/` by `make build`). Keep the probe as the
orchestrator: pass explicit arguments, enforce a timeout, and emit the
single JSON record via `bin/emit-record`. Helpers must stay quiet on stdout,
run in the foreground, and use stable, documented exit codes (0 success, 1
invalid args, 2 internal error, 3 timeout). Keep helper CLIs small and
capability-aligned so their behavior is easy to reason about from the probe and
its README.

## Probe description and agent guidance (boundary object schema)

A probe:
1. Is an executable script under `probes/<probe_id>.sh`, where the filename
   matches the `probe.id`. Use `#!/usr/bin/env bash`, immediately enable
   `set -euo pipefail`, and keep the script focused on a single observation.
2. Performs exactly *one* focused operation (file IO, DNS, network socket,
   process spawn, etc.). Gather whatever context you need to describe the
   attempt. Capture the command you actually ran (e.g.,
   `printf -v command_executed "... %q" ...`) and pass it through `--command`
   so the boundary object contains reproducible execution context. The
   `context.run` object contains only workspace/command—no timestamps—so probes
   never need to track clocks.
3. Collects stdout/stderr snippets (keep them short) and structured data in the
   payload. Normalize probe outcomes into: `success`, `denied`, `partial`, or
   `error`. Treat sandbox denials (`EACCES`, `EPERM`, network blocked, etc.) as
   `denied`.
4. Calls `bin/emit-record` once with the correct flags (payload/operation args
   built inline).
5. Exits with status `0` after emitting JSON. `bin/probe-exec` relies on this
   behavior so `fencerunner --bang` can stream records as NDJSON.

### How a probe should emit JSON

Call `bin/emit-record` exactly once with:
- `--probe-name "$probe_id"`.
- `--primary-capability-id`, zero or more `--secondary-capability-id`, and
  `--command`.
- `--operation-kind`, `--target`, and optional `--operation-args '{}'`.
- Outcome metadata (`--outcome` → `result.outcome`, `--exit-code`, `--errno`,
  `--message`, `--error-detail` as needed) plus `--payload-file` or the
  `--payload-*` flags.

See `boundary/boundary_object.md` for a complete field description. The
boundary object schema allows optional `context`/`payload` blocks; `emit-record`
populates `context` with the catalog key (`capabilities_schema_version`),
capability snapshots, probe capability ids, and stack/run info. Probes declare
capability IDs only; the harness resolves those IDs to snapshots without
hard-coding a specific catalog.

### Minimal example

Excerpt from `probes/fs_outside_workspace.sh`:

```bash
primary_capability_id="cap_fs_write_workspace_tree"
# This probe targets cap_fs_write_workspace_tree by confirming writes are denied outside the allowed roots.
printf -v command_executed "printf %q >> %q" "${attempt_line}" "${target_path}"

"${emit_record_bin}" \
  --probe-name "${probe_name}" \
  --primary-capability-id "${primary_capability_id}" \
  --command "${command_executed}" \
  --operation-kind "fs.write" \
  --target "${target_path}" \
  --outcome "${outcome}" \
  --errno "${errno_value}" \
  --message "${message}" \
  --exit-code "${exit_code}" \
  --payload-file "${payload_tmp}" \
  --operation-args "${operation_args}"
```

Matching JSON output (trimmed for brevity):

```json
{
  "probe": {
    "id": "fs_outside_workspace"
  },
  "result": {
    "outcome": "denied",
    "details": {
      "exit_code": 1,
      "errno": "EACCES",
      "message": "Permission denied",
      "error_detail": null
    }
  },
  "operation": {
    "kind": "fs.write",
    "target": "/tmp/probe-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 38}
  },
  "payload": {
    "stdout_snippet": "",
    "stderr_snippet": "bash: /tmp/probe-outside-root-test: Permission denied",
    "raw": {}
  },
  "context": {
    "run": {
      "workspace_root": "/path/to/workspace",
      "command": "printf 'probe write ...' >> '/tmp/probe-outside-root-test'"
    },
    "stack": {
      "os": "Darwin 23.3.0 arm64"
    },
    "capabilities_schema_version": "example_catalog_key",
    "probe": {
      "primary_capability_id": "cap_fs_write_workspace_tree",
      "secondary_capability_ids": []
    },
    "capability_context": {
      "primary": {
        "id": "cap_fs_write_workspace_tree",
        "category": "filesystem",
        "layer": "os_sandbox"
      },
      "secondary": []
    }
  }
}
```

This JSON links the probe to capability `cap_fs_write_workspace_tree`, records
the executed command, and classifies the outcome using `result.outcome`. Use
this pattern whenever you add a new probe.
