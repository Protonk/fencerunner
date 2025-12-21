# Probe Contract and Boundary Objects (boundary_event_v1 pattern)

`probe` records every probe run as a versioned JSON boundary object. The
structure comes from the **boundary_event_v1** schema embedded in the default
descriptor `boundaries/cfbo-v1.json`. Descriptors follow the contract in
`schema/boundary_object_schema.json`: they declare the boundary schema key
(`schema_key`, e.g., `cfbo-v1`) and embed the boundary-event JSON Schema used to
validate emitted records.

Each boundary object captures *one* probe execution in one run mode. Probes are
tiny scripts stored under `probes/<probe_id>.sh` that:

1. Use `#!/usr/bin/env bash` with `set -euo pipefail`.
2. Perform exactly one observable action (write a file, open a socket, read
   `sysctl`, etc.).
3. Collect the stdout/stderr snippets needed to describe that action plus any
   structured payload.
4. Call `bin/emit-record` once with `--run-mode "$FENCE_RUN_MODE"` plus the
   metadata described below.
5. Exit with status `0` after emitting JSON. They must not print anything else
   to stdout; use stderr only for debugging.

See `probes/AGENTS.md` for the workflow details expected from probe authors.

## Formal commitments

The project commits to the boundary_event_v1 pattern as specified by:

- The descriptor contract at `schema/boundary_object_schema.json` (shape of
  boundary descriptors: `key`, metadata, embedded `boundary_schema`).
- Boundary schema descriptors under `boundaries/` (default `boundaries/cfbo-v1.json`)
  that embed the boundary-event JSON Schema (`schema_version: "boundary_event_v1"`,
  key `"cfbo-v1"`).
- This document’s field-by-field explanations.

Within boundary_event_v1, the required fields, field names, and semantics
described below are stable. Structural changes require a new pattern version
and matching documentation/tests. Adding a new schema key for the same pattern
only requires a new descriptor that embeds the schema for that key.

## Boundary object layout (boundary_event_v1 + schema_key)

The machine-readable definition lives under `boundaries/cfbo-v1.json` as
`boundary_schema` (validated by `schema/boundary_object_schema.json`) and is
enforced by `bin/emit-record`.

| Field | Required | Description |
| --- | --- | --- |
| `schema_version` | yes | Always `"boundary_event_v1"` (the pattern version). |
| `schema_key` | yes | Boundary schema key from the descriptor (e.g., `cfbo-v1`). |
| `capabilities_schema_version` | yes | The catalog key from the loaded capability catalog. It is a string with no whitespace (e.g., `example_catalog_key`). |
| `stack` | yes | Sandbox/OS fingerprint for the host that ran the probe. |
| `probe` | yes | Identity and capability linkage for the probe implementation. |
| `run` | yes | Execution metadata for this invocation (mode, workspace, command). This harness intentionally omits timestamps so records stay stateless. |
| `operation` | yes | Description of the sandbox-facing operation being attempted. |
| `result` | yes | Normalized observed outcome plus error metadata. |
| `payload` | yes | Small probe-specific diagnostics and structured raw data. |
| `capability_context` | yes | Snapshot of the primary/secondary capability entries as seen through the capability index. |

### `stack`

Populated automatically by `bin/detect-stack`.

| Field | Required | Meaning |
| --- | --- | --- |
| `sandbox_mode` | yes (nullable) | `read-only`, `workspace-write`, `danger-full-access`, or `null` when unspecified. |
| `os` | yes | Value from `uname -srm`. |

### `probe`

`bin/emit-record` validates capability IDs by loading the bundled capability
catalog directly (the legacy adapter remains for automation).

| Field | Required | Meaning |
| --- | --- | --- |
| `id` | yes | Stable slug (usually the probe filename) such as `fs_outside_workspace`. |
| `version` | yes | Probe-local semantic/string version; bump when behavior changes. |
| `primary_capability_id` | yes | Capability tested by this probe. Must match the capability catalog. |
| `secondary_capability_ids` | yes | Zero or more supporting capability ids (unique, may be empty). |

### `run`

boundary_event_v1 deliberately does **not** capture timestamps or run durations.
The harness stays stateless; downstream consumers that need clocks or diffing
data must add it outside the boundary object.

| Field | Required | Meaning |
| --- | --- | --- |
| `mode` | yes | `baseline`; matches `bin/probe-exec`. |
| `workspace_root` | yes (nullable) | Canonical workspace root exported by `bin/probe-exec` (`FENCE_WORKSPACE_ROOT`), falling back to `git rev-parse` / `pwd` if unset. |
| `command` | yes | Human/machine-usable string describing the actual command. |

### `operation`

Describes the resource being touched.

| Field | Required | Meaning |
| --- | --- | --- |
| `category` | yes | High-level domain: `fs`, `net`, `proc`, `sysctl`, `agent_policy`, etc. |
| `verb` | yes | `read`, `write`, `exec`, `connect`, ... depending on the probe. |
| `target` | yes | Path/host/syscall/descriptor being addressed. |
| `args` | yes | Free-form JSON object with structured flags (modes, sizes, offsets). Use `{}` if unused. |

### `result`

Normalized observation of what happened, regardless of how the probe implemented
it.

| Field | Required | Meaning |
| --- | --- | --- |
| `observed_result` | yes | One of `success`, `denied`, `partial`, `error`. |
| `raw_exit_code` | yes (nullable) | Exit code from the command that performed the operation. |
| `errno` | yes (nullable) | Errno mnemonic (`EACCES`, `EPERM`, ...) if inferred. |
| `message` | yes (nullable) | Short human summary of the outcome. |
| `error_detail` | yes (nullable) | Additional diagnostics for unexpected failures. |

Interpretation of `observed_result`:

- `success`: the sandbox allowed the operation outright.
- `denied`: explicitly blocked by sandbox/policy (permission denied, EPERM,
  etc.).
- `partial`: some sub-step succeeded while another failed; note details in
  `message` / `payload.raw`.
- `error`: probe failed for reasons unrelated to sandbox policy (implementation
  bug, transient infra error).

### `payload`

Catch-all for probe-specific breadcrumbs. Keep these small (<4 KB).

| Field | Required | Meaning |
| --- | --- | --- |
| `stdout_snippet` | yes (nullable) | Up to ~400 characters of stdout (truncated if needed). |
| `stderr_snippet` | yes (nullable) | Same for stderr. |
| `raw` | yes | Structured JSON object for any other data (timings, file stats, HTTP responses). |

### `capability_context`

Every record includes the capability snapshot(s) that were resolved when the
probe was emitted. This lets downstream tooling trace exactly which catalog key
and metadata were in effect.

| Field | Required | Meaning |
| --- | --- | --- |
| `primary` | yes | Object with `id`, `category`, `layer` from the capability index. |
| `secondary` | no | Array of the same structure (may be empty). |

## Example

A trimmed record from `probes/fs_outside_workspace.sh` (writes outside the
workspace and expects a denial):

```json
{
  "schema_version": "boundary_event_v1",
  "schema_key": "cfbo-v1",
  "capabilities_schema_version": "example_catalog_key",
  "probe": {
    "id": "fs_outside_workspace",
    "version": "1",
    "primary_capability_id": "cap_fs_write_workspace_tree",
    "secondary_capability_ids": []
  },
  "run": {
    "mode": "baseline",
    "workspace_root": "/Users/example/project",
    "command": "printf 'probe write ...' >> '/tmp/probe-outside-root-test'"
  },
  "operation": {
    "category": "fs",
    "verb": "write",
    "target": "/tmp/probe-outside-root-test",
    "args": {"write_mode": "append", "attempt_bytes": 38}
  },
  "result": {
    "observed_result": "denied",
    "raw_exit_code": 1,
    "errno": "EACCES",
    "message": "Permission denied",
    "error_detail": null
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

## Updating the commitments

When the boundary-object contract needs to change, follow this procedure:

1. For structural changes to the boundary-event schema, embed the new schema
   (with a new `schema_version`) in a new descriptor under `boundaries/` and
   update `schema/boundary_object_schema.json` only if the descriptor *shape*
   changes. Keep prior descriptors available for consumers that still need
   them.
2. For a new boundary schema key using the same pattern version, add a new
   descriptor under `boundaries/` with its own `key` and embedded schema for that
   key.
3. Update this document, `AGENTS.md`, `README.md`, probe docs, and any tooling
   (`bin/emit-record`, tests, listeners) that validates or emits boundary
   objects so the new pattern and descriptors remain in lockstep.
4. Use `--boundary` (or `BOUNDARY_PATH`) to validate or emit against a drop-in
   descriptor when experimenting with new schema keys or pattern versions. The
   `schema_version` in emitted records reflects the embedded pattern version;
   the `schema_key` reflects the descriptor key.
