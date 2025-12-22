# Boundary Objects (minimal schema)

`fencerunner` records every probe run as a JSON boundary object. The minimal
schema lives at `schema/boundary_object_schema.json` and is enforced by
`bin/emit-record` plus the test suite. Probes emit a single boundary object to
stdout; any additional metadata belongs in `context`, `payload`, or
`extensions`.

See `probes/AGENTS.md` for the Probe Author contract and usage details.

## Required shape

A boundary object must include three top-level fields:

| Field | Required | Description |
| --- | --- | --- |
| `probe` | yes | Identifies the probe that ran. |
| `operation` | yes | Describes the attempted operation. |
| `result` | yes | Captures the observed outcome. |

### `probe`

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Stable probe id (usually the probe filename). |

### `operation`

| Field | Required | Meaning |
| --- | --- | --- |
| `kind` | yes | Operation kind such as `fs.read` or `net.connect`. |
| `target` | yes | Primary target (path, host, command, etc.). |
| `args` | no | JSON object with structured arguments. |

### `result`

| Field | Required | Meaning |
| --- | --- | --- |
| `outcome` | yes | One of `success`, `denied`, `partial`, `error`. |
| `details` | no | Optional details object (see below). |

`result.details` fields (all optional):

| Field | Meaning |
| --- | --- |
| `exit_code` | Exit code from the command that performed the operation. |
| `errno` | Errno mnemonic (`EACCES`, `EPERM`, etc.) if inferred. |
| `message` | Short human summary. |
| `error_detail` | Additional diagnostics for unexpected failures. |

Interpretation of `result.outcome`:

- `success`: the sandbox allowed the operation outright.
- `denied`: explicitly blocked by sandbox/policy (permission denied, EPERM,
  etc.).
- `partial`: some sub-step succeeded while another failed; note details in
  `details` or `payload`.
- `error`: probe failed for reasons unrelated to sandbox policy (implementation
  bug, transient infra error).

## Optional extensions

The schema allows optional top-level blocks:

| Field | Purpose |
| --- | --- |
| `context` | Run, stack, capability context, or other metadata. |
| `payload` | Probe-specific evidence (stdout/stderr snippets, raw JSON, etc.). |
| `extensions` | Reserved for non-standard fields. |

`bin/emit-record` populates `context` and `payload` in a consistent way so
records remain self-describing even as probes vary.

### `context` (emit-record)

`context` is intentionally free-form. `bin/emit-record` emits the following
keys when it is used:

| Field | Meaning |
| --- | --- |
| `run` | Workspace root + command string. |
| `stack` | Host metadata from `bin/detect-stack` (currently `os`). |
| `probe` | Capability ids (`primary_capability_id`, optional `secondary_capability_ids`). |
| `capabilities_schema_version` | Catalog key used for capability resolution. |
| `capability_context` | Snapshot of the primary/secondary capability entries. |

### `payload` (emit-record)

`payload` is also free-form. `bin/emit-record` emits:

| Field | Meaning |
| --- | --- |
| `stdout_snippet` | Up to ~400 characters of stdout (truncated if needed). |
| `stderr_snippet` | Same for stderr. |
| `raw` | Structured JSON object for any other data. |

Keep payloads small (<= 4096 bytes); `emit-record` and the contract gate enforce
this limit on serialized payload JSON.

## Example

A trimmed record from `probes/fs_outside_workspace.sh`:

```json
{
  "probe": {
    "id": "fs_outside_workspace"
  },
  "operation": {
    "kind": "fs.write",
    "target": "/tmp/probe-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 38}
  },
  "result": {
    "outcome": "denied",
    "details": {
      "exit_code": 1,
      "errno": "EACCES",
      "message": "Permission denied"
    }
  },
  "context": {
    "capabilities_schema_version": "example_catalog_key",
    "probe": {
      "primary_capability_id": "cap_fs_write_workspace_tree",
      "secondary_capability_ids": []
    },
    "run": {
      "workspace_root": "/Users/example/project",
      "command": "printf 'probe write ...' >> '/tmp/probe-outside-root-test'"
    },
    "stack": {
      "os": "Darwin 23.3.0 arm64"
    },
    "capability_context": {
      "primary": {
        "id": "cap_fs_write_workspace_tree",
        "category": "filesystem",
        "layer": "os_sandbox"
      },
      "secondary": []
    }
  },
  "payload": {
    "stdout_snippet": "",
    "stderr_snippet": "bash: /tmp/probe-outside-root-test: Permission denied",
    "raw": {}
  }
}
```

## Updating the contract

When the boundary-object contract changes:

1. Update `schema/boundary_object_schema.json` to reflect the new minimal
   requirements.
2. Update this document, `README.md`, probe docs, and any tooling (`emit-record`,
   tests, listeners) that validates or emits boundary objects so everything
   stays in lockstep.
3. Prefer adding new semantics under `context`, `payload`, or `extensions`
   whenever the core `probe`/`operation`/`result` shape can remain stable.
