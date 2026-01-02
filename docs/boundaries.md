# Boundaries

A run directory declares its output contract in `<RUN_DIR>/boundaries.json`.

Boundary output is **NDJSON on stdout**: one JSON object per line.

The contract is intentionally **flexible but enforceable**: a run dir can pick
its own `record_schema` (and evolve it over time), but emitted records must
still include a small required core so downstream tooling always has stable
identity/outcome/enrollment/payload channels.

## Required core (baseline policy)

Every run dir’s `record_schema` must require:

- `script.id` (string)
- `result.outcome` (string)
- `/context/commitments` (array of `{id, helps[]}` enrollment pairs; empty allowed)
- `/payload/stdout_snippet` (string; empty allowed)
- `/payload/stderr_snippet` (string; empty allowed)

The meta-schema at [`schema/boundaries.json`](../schema/boundaries.json) enforces that your `record_schema`
is written in a way that preserves these fields.

## Two-layer validation (schema → contract → records)

Validation is **two-layer**:

1. `boundaries.json` is validated against the meta-schema at [`schema/boundaries.json`](../schema/boundaries.json).
2. The runner compiles `boundaries.json.record_schema` (a JSON Schema) and uses it
   to validate each emitted boundary record at runtime.

This keeps the core contract stable while letting individual run dirs define
their own richer record shapes.

## `boundaries.json` shape (`boundaries_v1`)

Top level:

- `schema_version` — must be `"boundaries_v1"`.
- `stdout.format` — must be `"ndjson"`.
- `record_schema` — a JSON Schema (draft-07) for **one** boundary record.

`boundaries_v1` allows additional keys at the top level; run dirs may
add their own metadata fields, but core tooling only interprets the keys above.

## Examples

Minimal `boundaries.json` (requires only the core fields and otherwise stays permissive):

```json
{
  "schema_version": "boundaries_v1",
  "stdout": { "format": "ndjson" },
  "record_schema": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "type": "object",
    "required": ["script", "result", "context", "payload"],
    "properties": {
      "script": {
        "type": "object",
        "required": ["id"],
        "properties": {
          "id": { "type": "string" }
        }
      },
      "result": {
        "type": "object",
        "required": ["outcome"],
        "properties": {
          "outcome": { "type": "string" }
        }
      },
      "payload": {
        "type": "object",
        "required": ["stdout_snippet", "stderr_snippet"],
        "properties": {
          "stdout_snippet": { "type": "string" },
          "stderr_snippet": { "type": "string" }
        }
      },
      "context": {
        "type": "object",
        "required": ["commitments"],
        "properties": {
          "commitments": {
            "type": "array",
            "items": {
              "type": "object",
              "required": ["id", "helps"],
              "properties": {
                "id": {
                  "type": "string",
                  "pattern": "^[A-Za-z0-9_.-]+$"
                },
                "helps": {
                  "type": "array",
                  "minItems": 1,
                  "uniqueItems": true,
                  "items": {
                    "type": "string",
                    "enum": ["ensure", "detect", "emit"]
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

A stricter `boundaries.json` (requires an `operation` envelope and constrains `outcome`):

```json
{
  "schema_version": "boundaries_v1",
  "stdout": { "format": "ndjson" },
  "record_schema": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "type": "object",
    "required": ["script", "operation", "result", "context", "payload"],
    "properties": {
      "script": {
        "type": "object",
        "required": ["id"],
        "properties": {
          "id": {
            "type": "string",
            "pattern": "^[A-Za-z0-9_.-]+$"
          }
        }
      },
      "operation": {
        "type": "object",
        "required": ["kind", "target"],
        "properties": {
          "kind": { "type": "string" },
          "target": { "type": "string" }
        }
      },
      "result": {
        "type": "object",
        "required": ["outcome"],
        "properties": {
          "outcome": {
            "type": "string",
            "enum": ["success", "denied", "partial", "error"]
          }
        }
      },
      "payload": {
        "type": "object",
        "required": ["stdout_snippet", "stderr_snippet"],
        "properties": {
          "stdout_snippet": { "type": "string" },
          "stderr_snippet": { "type": "string" }
        }
      },
      "context": {
        "type": "object",
        "required": ["commitments"],
        "properties": {
          "commitments": {
            "type": "array",
            "items": {
              "type": "object",
              "required": ["id", "helps"],
              "properties": {
                "id": {
                  "type": "string",
                  "pattern": "^[A-Za-z0-9_.-]+$"
                },
                "helps": {
                  "type": "array",
                  "minItems": 1,
                  "uniqueItems": true,
                  "items": {
                    "type": "string",
                    "enum": ["ensure", "detect", "emit"]
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

## Extending within the schema

For run-dir authors, “extension” is primarily about evolving `record_schema`:

- Add new required/optional fields your domain needs (for example under
  `payload` or additional `context.*` keys).
- Decide whether to constrain values (`enum`, `pattern`, `minLength`, etc.).
- Decide how permissive the schema should be (`additionalProperties`).

If you use the repo’s `emit-record` helper, keep your `record_schema`
compatible with what it emits (it includes `script`, `operation`, `result`,
`context`, and `payload` with `stdout_snippet`/`stderr_snippet`).
