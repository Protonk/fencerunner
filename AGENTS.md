## Probe Author contract

As the Probe Author, you:
- Read `spec/capabilities.yaml` to understand the supported capability IDs, their categories, and descriptions.
- Read `schema/boundary-object-v1.json` and `docs/boundary_object.md` to understand the probe result contract.
- Inspect `probes/` to see existing probes and which capabilities they target.
- Keep a tight edit/test loop by running `make test` whenever you create or modify a probe. The suites in `tests/` (static probe contract, capability map sync, boundary-object schema, and harness smoke) are designed to fail fast with actionable messages so you can fix issues before attempting `make matrix` or a full fence run.

Prefer to add probes that:
- Target capability IDs with no existing probes, or
- Add edge-case variants for already-covered capabilities (e.g., symlink escapes, .git writes, network corner cases).

Keep each probe:
- Small and single-purpose.
- Clearly labeled with `primary_capability_id`. Open `spec/capabilities.yaml` and choose the `capabilities[*].id` that best represents what you are exercising. That id becomes `primary_capability_id`. Optionally list related capabilities in `secondary_capability_ids`. `bin/emit-record` validates all ids against `spec/capabilities.yaml`, so use the exact slugs.

Never:
- Print anything besides the JSON boundary object to stdout. Use stderr for debugging only when necessary.

## Probe description and agent guidance (cfbo-v1)

A probe: 
1. Is an executable script under `probes/`. Use `#!/usr/bin/env bash` and enable `set -euo pipefail`. Name the script `probes/<probe_id>.sh` so the filename matches the `probe.id`.
2. Performs exactly *one* focused operation inside the probe (a single file write, DNS lookup, process spawn, etc.). Gather whatever context you need to describe the attempt. Capture a string that describes the command you actually ran (for example via `printf -v command_executed "... %q"`). Pass this via `--command` so the boundary object contains reproducible execution context.
3. Collects stdout/stderr snippets (keep them short) and important structured data in the payload. Normalize probe outcomes into the allowed values: `success`, `denied`, `partial`, or `error`. Treat sandbox denials (`EACCES`, `EPERM`, network blocked, etc.) as `denied`.
4. Calls `bin/emit-record` once with the correct flags and payload file. Pass `--run-mode "$FENCE_RUN_MODE"` (exported by `bin/fence-run`) so the emitted record matches the mode selected by `bin/fence-run`.
5. Exits with status `0` after emitting JSON. `bin/fence-run` depends on this behavior to stream records to disk via `make matrix`.

### How a probe should emit JSON

Call `bin/emit-record` exactly once with:

- `--run-mode "$FENCE_RUN_MODE"` (already exported by `bin/fence-run`).
- `--probe-name "$probe_id"` and `--probe-version "<semver>"`.
- `--primary-capability-id`, zero or more `--secondary-capability-id`, and `--command`.
- `--category`, `--verb`, `--target`, and `--operation-args '{}'`.
- Outcome metadata (`--status` → `result.observed_result`, `--errno`, `--message`, `--raw-exit-code`, etc.) plus `--payload-file`.

See `docs/boundary-object.md` for a complete field description.

## Minimal example

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

```
{
  "schema_version": "cfbo-v1",
  "probe": {
    "id": "fs_outside_workspace",
    "version": "1",
    "primary_capability_id": "cap_fs_write_workspace_tree",
    "secondary_capability_ids": []
  },
  "run": {
    "mode": "baseline",
    "workspace_root": "/path/to/workspace",
    "command": "printf 'codex-fence write ...' >> '/tmp/codex-fence-outside-root-test'",
    "observed_at": "2024-03-04T17:18:19Z"
  },
  "result": {
    "observed_result": "denied",
    "raw_exit_code": 1,
    "errno": "EACCES",
    "message": "Permission denied",
    "duration_ms": null,
    "error_detail": null
  },
  "operation": {
    "category": "fs",
    "verb": "write",
    "target": "/tmp/codex-fence-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 43}
  },
  "payload": {
    "stdout_snippet": "",
    "stderr_snippet": "bash: /tmp/codex-fence-outside-root-test: Permission denied",
    "raw": {}
  },
  "stack": { "codex_cli_version": "codex 1.2.3", "codex_profile": "Auto", "codex_model": "gpt-4", "sandbox_mode": "workspace-write", "os": "Darwin 23.3.0 arm64", "container_tag": "local-macos" }
}
```

This JSON links the probe to capability `cap_fs_write_workspace_tree`, records the executed command, and classifies the outcome using the new `observed_result` vocabulary. Use this pattern whenever you add a new probe.
