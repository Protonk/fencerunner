# Boundary Object Schema

This document is a companion to the boundary object JSON Schema at
`boundary/boundary_object_schema.json`. It explains how the schema is used and
how to interpret the fields that appear in a boundary object emitted by a probe
run. The schema itself is the contract; this document provides guidance and
examples.

## What a boundary object is

A boundary object is one JSON record emitted for each probe run. The record is
validated against the schema and then streamed as NDJSON by the harness. The
schema is intentionally minimal so probes can vary widely while producing
structured output.

## Minimal required shape

The schema requires exactly three top-level fields:

| JSON pointer | Required | Meaning |
| --- | --- | --- |
| `/probe/id` | yes | Stable probe identifier (usually the probe filename). |
| `/operation/kind` | yes | Operation kind, e.g. `fs.read`, `net.connect`. |
| `/operation/target` | yes | Primary target (path, host, command, etc.). |
| `/result/outcome` | yes | One of `success`, `denied`, `partial`, `error`. |

Everything else is optional and lives under `operation.args`, `result.details`,
`context`, `payload`, or `extensions`.

### `operation`

- `operation.kind`: a compact, namespaced verb for the attempted action. Use
  a consistent scheme per domain, for example `fs.read`, `fs.write`,
  `proc.exec`, `net.connect`, `sysctl.read`.
- `operation.target`: the primary resource acted on (file path, host, command
  path, sysctl key, etc.).
- `operation.args` (optional): a JSON object with structured arguments that
  further describe the operation.

### `result`

- `result.outcome`: normalized outcome vocabulary.
  - `success`: operation allowed and completed as expected.
  - `denied`: operation blocked by policy (permission denied, EPERM, etc.).
  - `partial`: some sub-step succeeded while another failed.
  - `error`: probe failed for reasons unrelated to policy (bug, missing helper,
    transient infra failure).
- `result.details` (optional): additional structured metadata about the
  outcome. Fields are optional and include:
  - `exit_code`: integer exit status of the command that performed the
    operation.
  - `errno`: errno mnemonic when available (`EACCES`, `EPERM`, etc.).
  - `message`: short human summary.
  - `error_detail`: extra diagnostics for unexpected failures.

## Optional sections

### `context`

`context` is optional and schema-free. When you emit via `bin/emit-record`, it
populates a stable context shape so records are self-describing:

- `context.run`: `{workspace_root, command}`
- `context.stack`: output from `bin/detect-stack` (`os`)
- `context.probe`: `{primary_capability_id, secondary_capability_ids}`
- `context.capabilities_schema_version`: catalog key used for lookups
- `context.capability_context`: snapshot of primary/secondary capability
  entries (`id`, `category`, `layer`, plus other catalog fields)

The schema does not require these fields, but the harness relies on them for
analysis and test coverage. If you add new context keys, keep them under
`context` so the core contract is stable.

### `payload`

`payload` is optional and schema-free. `bin/emit-record` uses a common shape:

- `payload.stdout_snippet` (string)
- `payload.stderr_snippet` (string)
- `payload.raw` (object)

Payloads are size-limited to keep records lightweight. `emit-record` and the
contract gate enforce a 4096-byte limit on the serialized payload JSON.

### `extensions`

`extensions` is a reserved object for experimental fields that do not belong in
`context` or `payload`. Prefer adding new data under `context`/`payload` unless
there is a strong reason to keep it separate.

## Example

A typical record emitted by `bin/emit-record`:

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

## Validation flow

- `bin/emit-record` validates output against
  `boundary/boundary_object_schema.json` before emitting JSON.
- `bin/probe-listen` validates inbound NDJSON before summarizing.
- `schema-validate --mode boundary --file <path>` validates a JSON document
  against the boundary schema.
- `tests/schema.rs` guards the schema and typed round-trips.

## Updating the contract

When the boundary object contract changes:

1. Update `boundary/boundary_object_schema.json` with the new required fields.
2. Update this document and any probe guidance docs to reflect the change.
3. Update emit/listen/test logic so records remain in sync with the schema.
4. Prefer extending `context`, `payload`, or `extensions` before adding new
   top-level required fields.
