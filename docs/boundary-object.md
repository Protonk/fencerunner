# Probe Contract and Boundary Object (cfbo-v2)

`codex-fence` records every probe run as a versioned JSON “boundary object”. Version **cfbo-v2** is the current contract. It incorporates the v2 capability schema (via `tools/capabilities_adapter.sh`) so every record carries a snapshot of the capability metadata it referenced.

Each boundary object captures *one* probe execution in one run mode. Probes are tiny scripts stored in `probes/` that:

1. Use `#!/usr/bin/env bash` with `set -euo pipefail`.
2. Perform exactly one observable action (write a file, open a socket, read `sysctl`, etc.).
3. Collect the stdout/stderr snippets needed to describe that action plus any structured payload.
4. Call `bin/emit-record` once with `--run-mode "$FENCE_RUN_MODE"` plus the metadata described below.
5. Exit with status `0` after emitting JSON. They must not print anything else to stdout; use stderr only for debugging.

See `AGENTS.md` for the workflow details expected from probe authors.

## Boundary object layout (cfbo-v2)

The machine-readable definition lives in `schema/boundary-object-cfbo-v2.json` and is enforced by `bin/emit-record`.

| Field | Required | Description |
| --- | --- | --- |
| `schema_version` | yes | Always `"cfbo-v2"`. |
| `capabilities_schema_version` | yes (nullable) | The version from `spec/capabilities.yaml` that was loaded via the adapter (currently `2`). |
| `stack` | yes | Fingerprint of the Codex CLI + OS stack that hosted the probe. |
| `probe` | yes | Identity and capability linkage for the probe implementation. |
| `run` | yes | Execution metadata for this invocation (mode, workspace, command, timestamp). |
| `operation` | yes | Description of the sandbox-facing operation being attempted. |
| `result` | yes | Normalized observed outcome plus error metadata. |
| `payload` | yes | Small probe-specific diagnostics and structured raw data. |
| `capability_context` | yes | Snapshot of the primary/secondary capability entries as seen through the adapter. |

### `stack`

Populated automatically by `bin/detect-stack`.

| Field | Required | Meaning |
| --- | --- | --- |
| `codex_cli_version` | yes (nullable) | Output of `codex --version` if available, else `null`. |
| `codex_profile` | yes (nullable) | Codex profile name if known (`FENCE_CODEX_PROFILE`). |
| `codex_model` | yes (nullable) | Model used for the run if known, else `null`. |
| `sandbox_mode` | yes (nullable) | `read-only`, `workspace-write`, `danger-full-access`, or `null` for baseline runs. |
| `os` | yes | Value from `uname -srm`. |
| `container_tag` | yes | Host/container label (e.g., `local-macos`, `local-linux`). |

### `probe`

Probe identity stays explicit and tied to the capability catalog.

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Stable slug (usually the probe filename) such as `fs_outside_workspace`. |
| `version` | yes | Probe-local semantic/string version; bump when behavior changes. |
| `primary_capability_id` | yes | Capability tested by this probe. Must match the adapter output. |
| `secondary_capability_ids` | yes | Zero or more supporting capability ids (unique, may be empty). |

`bin/emit-record` validates capability IDs by piping `spec/capabilities.yaml` through `tools/capabilities_adapter.sh`. Add or update IDs there first.

### `run`

Execution context specific to the current invocation.

| Field | Required | Meaning |
| --- | --- | --- |
| `mode` | yes | `baseline`, `codex-sandbox`, or `codex-full`; matches `bin/fence-run`. |
| `workspace_root` | yes (nullable) | Root detected from `FENCE_WORKSPACE_ROOT` or `git rev-parse`. |
| `command` | yes | Human/machine-usable string describing the actual command. |
| `observed_at` | yes | UTC timestamp when the record was emitted (ISO 8601). |

### `operation`

Describes the resource being touched.

| Field | Required | Meaning |
| --- | --- | --- |
| `category` | yes | High-level domain: `fs`, `net`, `proc`, `sysctl`, `agent_policy`, etc. |
| `verb` | yes | `read`, `write`, `exec`, `connect`, ... depending on the probe. |
| `target` | yes | Path/host/syscall/descriptor being addressed. |
| `args` | yes | Free-form JSON object with structured flags (modes, sizes, offsets). Use `{}` if unused. |

### `result`

Normalized observation of what happened, regardless of how the probe implemented it.

| Field | Required | Meaning |
| --- | --- | --- |
| `observed_result` | yes | One of `success`, `denied`, `partial`, `error`. |
| `raw_exit_code` | yes (nullable) | Exit code from the command that performed the operation. |
| `errno` | yes (nullable) | Errno mnemonic (`EACCES`, `EPERM`, ...) if inferred. |
| `message` | yes (nullable) | Short human summary of the outcome. |
| `duration_ms` | yes (nullable) | Wall-clock time spent on the operation if measured. |
| `error_detail` | yes (nullable) | Additional diagnostics for unexpected failures. |

Interpretation of `observed_result`:

- `success`: the sandbox allowed the operation outright.
- `denied`: explicitly blocked by sandbox/policy (permission denied, EPERM, etc.).
- `partial`: some sub-step succeeded while another failed; note details in `message` / `payload.raw`.
- `error`: probe failed for reasons unrelated to sandbox policy (implementation bug, transient infra error).

### `payload`

Catch-all for probe-specific breadcrumbs. Keep these small (<4 KB).

| Field | Required | Meaning |
| --- | --- | --- |
| `stdout_snippet` | yes (nullable) | Up to ~400 characters of stdout (truncated if needed). |
| `stderr_snippet` | yes (nullable) | Same for stderr. |
| `raw` | yes | Structured JSON object for any other data (timings, file stats, HTTP responses). |

### `capability_context`

New in cfbo-v2. Every record includes the capability snapshot(s) that were resolved when the probe was emitted. This lets downstream tooling trace exactly which schema version and metadata were in effect.

| Field | Required | Meaning |
| --- | --- | --- |
| `primary` | yes | Object with `id`, `category`, `platform`, `layer`, `status` from the adapter. |
| `secondary` | no | Array of the same structure (may be empty). |

## Example

A trimmed record from `probes/fs_outside_workspace.sh` (writes outside the workspace and expects a denial):

```json
{
  "schema_version": "cfbo-v2",
  "capabilities_schema_version": 2,
  "probe": {
    "id": "fs_outside_workspace",
    "version": "1",
    "primary_capability_id": "cap_fs_write_workspace_tree",
    "secondary_capability_ids": []
  },
  "run": {
    "mode": "codex-sandbox",
    "workspace_root": "/Users/example/project",
    "command": "printf 'codex-fence write ...' >> '/tmp/codex-fence-outside-root-test'",
    "observed_at": "2024-03-04T17:18:19Z"
  },
  "operation": {
    "category": "fs",
    "verb": "write",
    "target": "/tmp/codex-fence-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 43}
  },
  "result": {
    "observed_result": "denied",
    "raw_exit_code": 1,
    "errno": "EACCES",
    "message": "Permission denied",
    "duration_ms": null,
    "error_detail": null
  },
  "payload": {
    "stdout_snippet": "",
    "stderr_snippet": "bash: /tmp/codex-fence-outside-root-test: Permission denied",
    "raw": {}
  },
  "capability_context": {
    "primary": {
      "id": "cap_fs_write_workspace_tree",
      "category": "filesystem",
      "platform": ["macos"],
      "layer": "os_sandbox",
      "status": "core"
    },
    "secondary": []
  },
  "stack": {
    "codex_cli_version": "codex 1.2.3",
    "codex_profile": "Auto",
    "codex_model": "gpt-4",
    "sandbox_mode": "workspace-write",
    "os": "Darwin 23.3.0 arm64",
    "container_tag": "local-macos"
  }
}
```
