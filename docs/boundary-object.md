# Boundary Object (bo-v0)

`codex-fence` represents every probe result as a "boundary object": a stable JSON record that carries
just enough structure for automated processing while staying easy for humans to read. Each record tracks
*one* attempted operation executed against *one* runtime stack and is designed to survive copy/paste and
log aggregation.

## Top-level fields

| Field | Required | Description |
| --- | --- | --- |
| `schema_version` | yes | Always `"bo-v0"`. Keeps the document extensible without breaking older readers. |
| `stack` | yes | Fingerprint of the Codex + OS stack that hosted the probe (see below). |
| `env` | yes | Logical execution environment information (which probe, which run mode, workspace root). |
| `operation` | yes | What the probe attempted to do (category/verb/target/args). |
| `outcome` | yes | Result classification (`allowed`, `denied`, `error`, `inconclusive`). |
| `payload` | yes | Opaque probe-specific data captured alongside the outcome. |

### `stack`

| Field | Required | Meaning |
| --- | --- | --- |
| `codex_cli_version` | yes (nullable) | Output of `codex --version` if available, otherwise `null`. |
| `codex_profile` | yes (nullable) | Codex profile name if known (e.g. from `FENCE_CODEX_PROFILE`). |
| `codex_model` | yes (nullable) | Model used for the run if known, else `null`. |
| `sandbox_mode` | yes (nullable) | One of `read-only`, `workspace-write`, `danger-full-access`, or `null` for baseline runs. |
| `os` | yes | Value from `uname -srm`. |
| `container_tag` | yes | Short label for the host/container (e.g. `local-macos`, `openai-universal`). |

### `env`

| Field | Required | Meaning |
| --- | --- | --- |
| `run_mode` | yes | `baseline`, `codex-sandbox`, or `codex-full`. Matches the mode passed to `bin/fence-run`. |
| `probe_name` | yes | Short slug (filename without extension) that identifies the probe. |
| `probe_version` | yes | Probe-local semantic version string. Increment whenever behavior changes. |
| `workspace_root` | yes (nullable) | Absolute path to the workspace root if known, else `null`. |

### `operation`

| Field | Required | Meaning |
| --- | --- | --- |
| `category` | yes | High-level area such as `fs`, `net`, `proc`, `sysinfo`, `env`, `time`, etc. |
| `verb` | yes | `read`, `write`, `stat`, `exec`, `connect`, ... depending on the probe. |
| `target` | yes | Path/host/syscall being touched. |
| `args` | yes | Free-form JSON object with structured parameters (modes, flags, payload sizes). Empty object if unused. |

### `outcome`

| Field | Required | Meaning |
| --- | --- | --- |
| `status` | yes | `allowed`, `denied`, `error`, or `inconclusive`. |
| `errno` | yes (nullable) | Platform errno mnemonic (e.g. `EACCES`), or `null` if not inferred. |
| `message` | yes (nullable) | Short human-friendly summary. |
| `duration_ms` | yes (nullable) | Wall-clock milliseconds spent performing the operation, or `null` if not measured. |

### `payload`

The payload keeps probe-specific scraps that would be lossy if converted to fixed fields.

| Field | Required | Meaning |
| --- | --- | --- |
| `stdout_snippet` | yes (nullable) | Up to ~400 characters of captured stdout (truncated with an ellipsis). |
| `stderr_snippet` | yes (nullable) | Same for stderr. |
| `raw` | yes | Arbitrary JSON object with structured data for the probe (counts, timings, metadata). |

Probes should only place *small* data in the payload—ideally <4 KB per record. Large logs should live
elsewhere and be referenced via the `raw` object. Maintaining this discipline keeps the boundary objects
portable and easy to diff.
