# Probe Contract and Boundary Object (cfbo-v1)

`codex-fence` records every probe run as a versioned JSON "boundary object". Version `cfbo-v1` is the canonical contract for new probes and ties directly into the capability map defined in `spec/capabilities.yaml`.

Each boundary object captures *one* focused probe operation executed under a single run mode. Probes are tiny scripts stored in `probes/` that:

1. Use `#!/usr/bin/env bash` with `set -euo pipefail`.
2. Perform exactly one observable action (write a file, open a socket, read sysctl, ...).
3. Capture the stdout/stderr snippets needed to describe that action.
4. Call `bin/emit-record` once with `--run-mode "$FENCE_RUN_MODE"` plus metadata described below.
5. Exit with status `0` after `emit-record` prints JSON. They must not write anything else to stdout; use stderr only for debugging.

See `AGENTS.md` for the detailed "Probe Author" workflow.

## Boundary object layout (cfbo-v1)

The machine-readable definition lives in `schema/boundary-object-v1.json`. The same schema is enforced by `bin/emit-record`.

| Field | Required | Description |
| --- | --- | --- |
| `schema_version` | yes | Always `"cfbo-v1"`. |
| `stack` | yes | Fingerprint of the Codex CLI + OS stack that hosted the probe. |
| `probe` | yes | Identity and capability linkage for the probe implementation. |
| `run` | yes | Execution metadata for this invocation (mode, workspace, command, timestamp). |
| `operation` | yes | Description of the sandbox-facing operation being attempted. |
| `result` | yes | Normalized observed outcome plus error metadata. |
| `payload` | yes | Small probe-specific diagnostics and structured raw data. |

### `stack`

Populated automatically by `bin/detect-stack`.

| Field | Required | Meaning |
| --- | --- | --- |
| `codex_cli_version` | yes (nullable) | Output of `codex --version` if available, else `null`. |
| `codex_profile` | yes (nullable) | Codex profile name if known (`FENCE_CODEX_PROFILE`). |
| `codex_model` | yes (nullable) | Model used for the run if known, else `null`. |
| `sandbox_mode` | yes (nullable) | `read-only`, `workspace-write`, `danger-full-access`, or `null` for baseline runs. |
| `os` | yes | Value from `uname -srm`. |
| `container_tag` | yes | Host/container label (e.g. `local-macos`, `local-linux`). |

### `probe`

Probe identity is now explicit and tied to the capability map in `spec/capabilities.yaml`.

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Stable slug (usually the probe filename) such as `fs_outside_workspace`. |
| `version` | yes | Probe-local semantic/string version; bump when behavior changes. |
| `primary_capability_id` | yes | Capability tested by this probe. **Must** match `capabilities[*].id`. |
| `secondary_capability_ids` | yes | Zero or more supporting capability ids (unique, may be empty). |

`bin/emit-record` validates all capability ids against `spec/capabilities.yaml`, so select them from that file before writing a probe.

### `run`

Execution context specific to the current invocation.

| Field | Required | Meaning |
| --- | --- | --- |
| `mode` | yes | `baseline`, `codex-sandbox`, or `codex-full`; matches `bin/fence-run`. |
| `workspace_root` | yes (nullable) | Root detected from `FENCE_WORKSPACE_ROOT` or `git rev-parse`. |
| `command` | yes | Human/machine-usable string describing the command that actually ran. |
| `observed_at` | yes | UTC timestamp when the record was emitted (ISO 8601). |

### `operation`

Same structure as before; used to describe the resource being touched.

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

Remains the catch-all for probe-specific breadcrumbs. Keep these small (<4 KB).

| Field | Required | Meaning |
| --- | --- | --- |
| `stdout_snippet` | yes (nullable) | Up to ~400 characters of stdout (truncated if needed). |
| `stderr_snippet` | yes (nullable) | Same for stderr. |
| `raw` | yes | Structured JSON object for any other data (timings, file stats, HTTP responses). |

## Example

A trimmed record from `probes/fs_outside_workspace.sh` (writes outside the workspace tree and expects a denial):

```json
{
  "schema_version": "cfbo-v1",
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
