# fencerunner user guide (macOS / Apple Silicon)

This guide assumes:

- macOS (tested on Apple Silicon / `arm64`).
- Your scripts run under macOS `/bin/bash` (Bash 3.2).
- You install `fencerunner` by downloading a release binary and putting it on your `PATH`.

## The contract in one sentence

`fencerunner` runs every executable `*.sh` script in one or more **run directories**, and each script is expected to emit **exactly one JSON object on stdout**; `fencerunner` then streams those objects as **NDJSON** (one JSON object per line).
- **Stdout is the record.** Don’t print anything else to stdout.
- **One script → one record.** Exactly one boundary record per script.
- **Use `emit-record`.** It’s the easiest way to stay schema-valid.

---

## Install (v1.0.0)

1. Open the release page: `https://github.com/Protonk/fencerunner/releases/tag/v1.0.0`
2. Download the `aarch64-apple-darwin` artifact (`.tar.gz`) and its `.sha256` file.
3. Verify the SHA256 (from the directory you downloaded into):

   ```bash
   shasum -a 256 -c fencerunner-v1.0.0-aarch64-apple-darwin.tar.gz.sha256
   ```

4. Extract the tarball:

   ```bash
   tar -xzf fencerunner-v1.0.0-aarch64-apple-darwin.tar.gz
   ```

5. Put the extracted binary on your `PATH` (pick any directory that’s on `PATH` for your shell). For example, if you use `~/.local/bin`:

   ```bash
   mkdir -p ~/.local/bin
   install -m 0755 fencerunner-v1.0.0-aarch64-apple-darwin ~/.local/bin/fencerunner
   ```

6. If macOS blocks execution due to quarantine/Gatekeeper, remove the quarantine attribute from the installed binary:

   ```bash
   xattr -dr com.apple.quarantine ~/.local/bin/fencerunner
   ```

7. Confirm it runs:

   ```bash
   fencerunner --version
   fencerunner --help
   ```

Notes:

- `fencerunner` is tested on Apple Silicon. If you’re on Intel macOS, build from source or use a matching release artifact if one exists.

---

## What is a run directory?

A **run directory** (“run dir”) is a *flat* directory containing:

- `gates.json` — optional extra checks enforced by the runner.
- `commitments.json` — a dir-local dependency registry (metadata + a place to be explicit).
- `boundaries.json` — the output contract for boundary records.
- One or more executable `*.sh` scripts (top-level only; subdirectories are ignored).

Key rules:

- **Flat means flat:** only `*.sh` files directly under the run dir are treated as scripts.
- **Scripts must be executable:** `chmod +x your_script.sh`.
- **Script id is filename-based:** `my_probe.sh` has id `my_probe`, and the emitted record must report `script.id == "my_probe"`.
- **Script ids must be unique across all run dirs in one run.** If you run `fencerunner dirA dirB`, and both have `probe.sh`, that’s a hard error.

### What `fencerunner` provides to scripts at runtime

When `fencerunner` runs a script, it sets:

- The script’s **current working directory** (`CWD`) to the run dir.
- `FENCERUNNER_RUN_DIR` to the run dir’s absolute path.
- `FENCERUNNER_ROOT` to a runner-owned ephemeral directory that contains:
  - `lib/library.sh` (the script library)
  - `bin/emit-record` (a shim that emits schema-valid boundary records)
  - `bin/commit-help-me` (a shim used by `commit_help_me` for enrollment tracking)
- `PATH` so the runner shims (`emit-record`, `commit-help-me`) are found first.
- `TMPDIR` to a scratch directory for the run (use it for temp files).

One important implication: **run scripts via `fencerunner`**. If you execute scripts directly, the helper shims and environment won’t exist.

---

## Quickstart: create a run dir from scratch

This is the smallest “real” run dir: it has the triad and one script.

```bash
mkdir -p ./example-run-dir
cd ./example-run-dir
```

### 1) `gates.json`

Minimal gates contract (no extra checks):

```json
{
  "schema_version": "gates_v1"
}
```

### 2) `commitments.json`

Minimal commitments registry. This file is required even if you don’t use it yet.

This example declares:

- `emit.record` (the runner-provided boundary emitter)
- `python3` (a system dependency you *might* rely on)

```json
{
  "schema_version": "commitments_v1",
  "commitments": [
    {
      "id": "emit.record",
      "provider": "runner",
      "helps": ["emit"],
      "is": "Boundary record emitter",
      "at": "emit-record",
      "version": "v1"
    },
    {
      "id": "python3",
      "provider": "system",
      "helps": ["ensure"],
      "is": "Python 3 interpreter",
      "at": "python3",
      "version": "v1"
    }
  ]
}
```

### 3) `boundaries.json` (recommended baseline)

This contract is the “shape” each script must emit.

It’s intentionally:

- **Stable at the top level** (`additionalProperties: false`), so downstream tools can trust the envelope.
- **Flexible inside** `operation.args`, `context`, and `payload.raw`.

```json
{
  "schema_version": "boundaries_v1",
  "stdout": { "format": "ndjson" },
  "record_schema": {
    "$schema": "http://json-schema.org/draft-07/schema#",
    "type": "object",
    "additionalProperties": false,
    "required": ["script", "operation", "result", "context", "payload"],
    "properties": {
      "script": {
        "type": "object",
        "additionalProperties": false,
        "required": ["id"],
        "properties": {
          "id": { "type": "string", "pattern": "^[A-Za-z0-9_.-]+$" }
        }
      },
      "operation": {
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "target"],
        "properties": {
          "kind": { "type": "string" },
          "target": { "type": "string" },
          "args": { "type": "object", "additionalProperties": true }
        }
      },
      "result": {
        "type": "object",
        "additionalProperties": false,
        "required": ["outcome"],
        "properties": {
          "outcome": {
            "type": "string",
            "enum": ["success", "denied", "partial", "error"]
          },
          "details": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
              "exit_code": { "type": "integer" },
              "errno": { "type": "string" },
              "message": { "type": "string" },
              "error_detail": { "type": "string" }
            }
          }
        }
      },
      "context": {
        "type": "object",
        "additionalProperties": true,
        "required": ["commitments"],
        "properties": {
          "commitments": {
            "type": "array",
            "items": {
              "type": "object",
              "additionalProperties": false,
              "required": ["id", "helps"],
              "properties": {
                "id": { "type": "string", "pattern": "^[A-Za-z0-9_.-]+$" },
                "helps": {
                  "type": "array",
                  "minItems": 1,
                  "uniqueItems": true,
                  "items": { "type": "string", "enum": ["ensure", "detect", "emit"] }
                }
              }
            }
          }
        }
      },
      "payload": {
        "type": "object",
        "additionalProperties": true,
        "required": ["stdout_snippet", "stderr_snippet"],
        "properties": {
          "stdout_snippet": { "type": "string" },
          "stderr_snippet": { "type": "string" },
          "raw": { "type": "object", "additionalProperties": true }
        }
      },
      "extensions": {
        "type": "object",
        "additionalProperties": true
      }
    }
  }
}
```

### 4) Add a script (`probe_bash_exists.sh`)

Create `probe_bash_exists.sh`:

```bash
#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"

script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

target="/bin/bash"
if [[ -x "${target}" ]]; then
  outcome="success"
else
  outcome="error"
fi

# Record that this script relies on the runner's emitter.
commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "test -x ${target}" \
  --operation-kind "fs.exec_check" \
  --target "${target}" \
  --outcome "${outcome}" \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "path" "${target}"
```

Make it executable:

```bash
chmod +x ./probe_bash_exists.sh
```

### 5) Run it

From the directory *above* the run dir:

```bash
cd ..
fencerunner ./example-run-dir
```

You’ll get **one JSON object per line** on stdout. It will be one long line (NDJSON).

---

## The boundary record contract (deep)

### The rule: stdout is reserved

In strict mode (the default), `fencerunner` treats script stdout as **the boundary record**. If your script prints anything else (a log line, a progress bar, a stray `echo`), the record parse/validation will fail.

Practical pattern:

- Send *diagnostics* to stderr (`echo "...msg..." >&2`).
- Capture the outputs of commands you run into files/variables.
- Put the evidence you care about into `payload.stdout_snippet`, `payload.stderr_snippet`, and/or `payload.raw`.
- Emit one record via `emit-record`.

### The core fields you always get (and should treat as “API”)

The baseline contract requires:

- `script.id` — filename stem (`my_probe.sh` → `my_probe`).
- `operation.kind` — a stable *category* for what you attempted (ex: `proc.exec`, `fs.read`, `net.dns.lookup`).
- `operation.target` — the primary target (path/host/command/etc).
- `result.outcome` — recommended baseline enum: `success|denied|partial|error`.
- `context.commitments` — what the script enrolled into via `commit_help_me` (empty allowed).
- `payload.stdout_snippet`, `payload.stderr_snippet` — evidence channels (strings; empty allowed).
- `payload.raw` — your extension point for structured data (object; optional).

If you want two different run dirs to produce a unified stream, make `operation.kind` and `operation.target` mean the same thing everywhere.

### How to use `emit-record` (recommended)

`emit-record` is a runner-provided helper that:

- builds the JSON object,
- merges any enrolled commitments from `commit_help_me`,
- validates the record against the run dir’s `boundaries.json`,
- prints the record as a single line.

`emit-record` is available to scripts because `fencerunner` puts a shim on `PATH` when it runs your scripts. You do not install it separately.

### Capturing real command output without breaking stdout

If your script needs to run commands that write to stdout/stderr, capture them and pass them to `emit-record`:

```bash
#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"
script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

stdout_file="$(mktemp "${TMPDIR}/probe.stdout.XXXXXX")"
stderr_file="$(mktemp "${TMPDIR}/probe.stderr.XXXXXX")"

operation_kind="proc.exec"
target="uname -a"

set +e
uname -a >"${stdout_file}" 2>"${stderr_file}"
exit_code="$?"
set -e

if [[ "${exit_code}" -eq 0 ]]; then
  outcome="success"
else
  outcome="error"
fi

commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "${target}" \
  --operation-kind "${operation_kind}" \
  --target "${target}" \
  --outcome "${outcome}" \
  --exit-code "${exit_code}" \
  --payload-stdout-file "${stdout_file}" \
  --payload-stderr-file "${stderr_file}"
```

If you want to keep snippets small, trim them before emitting:

```bash
head -c 2000 "${stdout_file}" > "${stdout_file}.trimmed"
head -c 2000 "${stderr_file}" > "${stderr_file}.trimmed"
```

---

## Strict vs supervised

### Strict mode (default)

Strict mode is for “contract enforcement”:

- If a script emits invalid JSON, violates `boundaries.json`, has `script.id` mismatch, exits non-zero, or violates an enforced gate, the run fails.
- Strict mode returns a **non-zero exit code** if any script fails.
- In strict mode, a failing script does **not** produce a boundary record (the run aborts at the runner level for that script).

Use strict mode when you want a hard failure signal and you’re okay with missing records for failing scripts.

### Supervised mode (`--supervised`)

Supervised mode is for “always keep the NDJSON stream well-formed”:

- For each script, `fencerunner` will output **one NDJSON record**, even if the script misbehaves.
- When a script breaks the contract, `fencerunner` emits a **synthetic error record** that captures stdout/stderr snippets and explains the failure.
- Supervised mode exits `0` unless **preflight** or the **runner itself** fails (examples: missing contract files, invalid JSON contracts, script not executable, duplicate script ids across run dirs).

Use supervised mode when downstream tooling expects one record per script no matter what, and you want failures encoded in the stream instead of in exit codes.

---

## Making two different run dirs produce one clean stream

The easiest way to make output “undifferentiated as it streams” is:

1. Pick a **single shared `boundaries.json`** shape (like the baseline above).
2. Copy (or generate) that same `boundaries.json` into every run dir you plan to run together.
3. Make sure every script uses:
   - the same `result.outcome` vocabulary, and
   - stable `operation.kind` naming.

### Example: two different run dirs, one report

Create a suite folder:

```bash
mkdir -p ./suite/run_dirs/env_probes
mkdir -p ./suite/run_dirs/fs_probes
```

In both run dirs, create the triad:

- Copy the same `gates.json`, `commitments.json`, and `boundaries.json` (from the Quickstart) into:
  - `./suite/run_dirs/env_probes/`
  - `./suite/run_dirs/fs_probes/`

Now add one script per run dir (note: ids must be globally unique across the whole run):

`./suite/run_dirs/env_probes/env_python3_version.sh`

```bash
#!/bin/bash
set -euo pipefail
source "${FENCERUNNER_ROOT}/lib/library.sh"

script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

stdout_file="$(mktemp "${TMPDIR}/python3.stdout.XXXXXX")"
stderr_file="$(mktemp "${TMPDIR}/python3.stderr.XXXXXX")"

set +e
python3 --version >"${stdout_file}" 2>"${stderr_file}"
exit_code="$?"
set -e

if [[ "${exit_code}" -eq 0 ]]; then
  outcome="success"
else
  outcome="error"
fi

commit_help_me ensure python3
commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "python3 --version" \
  --operation-kind "proc.exec" \
  --target "python3 --version" \
  --outcome "${outcome}" \
  --exit-code "${exit_code}" \
  --payload-stdout-file "${stdout_file}" \
  --payload-stderr-file "${stderr_file}" \
  --payload-raw-field "tool" "python3"
```

`./suite/run_dirs/fs_probes/fs_ssh_dir_exists.sh`

```bash
#!/bin/bash
set -euo pipefail
source "${FENCERUNNER_ROOT}/lib/library.sh"

script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

target="${HOME}/.ssh"
if [[ -d "${target}" ]]; then
  outcome="success"
else
  outcome="error"
fi

commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "test -d ${target}" \
  --operation-kind "fs.stat" \
  --target "${target}" \
  --outcome "${outcome}" \
  --exit-code 0 \
  --payload-stdout "" \
  --payload-stderr "" \
  --payload-raw-field "path" "${target}" \
  --payload-raw-field "expected_kind" "directory"
```

Make both executable:

```bash
chmod +x ./suite/run_dirs/env_probes/env_python3_version.sh
chmod +x ./suite/run_dirs/fs_probes/fs_ssh_dir_exists.sh
```

Run both run dirs and stream a report:

```bash
fencerunner --supervised ./suite/run_dirs/env_probes ./suite/run_dirs/fs_probes \
  | jq -r '[.result.outcome, .operation.kind, .operation.target, .script.id] | @tsv'
```

That report doesn’t need to know which run dir a record came from; it treats everything as “an operation with an outcome”.

---

## Expanding `boundaries.json` in a non-trivial way

The baseline `boundaries.json` is flexible: it gives you a stable envelope (`script/operation/result/context/payload`) and leaves room for you to standardize on top.

Common evolution steps:

1. **Constrain `operation.kind`** to a known set (so typos become contract breaks).
2. **Standardize `operation.args`** keys (so consumers can rely on them).
3. **Require specific `payload.raw` fields** for specific operation kinds.
4. **Tighten `additionalProperties`** once your record shape is stable.

### Example: constrain `operation.kind`

In `boundaries.json`, change:

```json
{ "kind": { "type": "string" } }
```

to:

```json
{
  "kind": {
    "type": "string",
    "enum": ["proc.exec", "fs.stat"]
  }
}
```

Now “`proc.exe`” becomes a schema failure instead of a silent stream divergence.

### Example: require different fields for different operation kinds (advanced)

If you want a truly enforceable “mini DSL” inside the schema, you can use `oneOf` to express per-operation requirements.

Conceptually:

- `proc.exec` must include `operation.args.command` and `payload.raw.tool`.
- `fs.stat` must include `operation.args.path` and `payload.raw.expected_kind`.

You can encode that as a `oneOf` at the top level of `record_schema` (shown abbreviated here; keep your baseline envelope too):

```json
{
  "oneOf": [
    {
      "properties": {
        "operation": {
          "properties": {
            "kind": { "const": "proc.exec" },
            "args": {
              "type": "object",
              "required": ["command"],
              "properties": { "command": { "type": "string" } }
            }
          }
        },
        "payload": {
          "properties": {
            "raw": {
              "type": "object",
              "required": ["tool"],
              "properties": { "tool": { "type": "string" } }
            }
          }
        }
      }
    },
    {
      "properties": {
        "operation": {
          "properties": {
            "kind": { "const": "fs.stat" },
            "args": {
              "type": "object",
              "required": ["path"],
              "properties": { "path": { "type": "string" } }
            }
          }
        },
        "payload": {
          "properties": {
            "raw": {
              "type": "object",
              "required": ["expected_kind"],
              "properties": { "expected_kind": { "type": "string" } }
            }
          }
        }
      }
    }
  ]
}
```

Then scripts must fill those fields using `emit-record` flags like:

- `--operation-arg command "python3 --version"`
- `--operation-arg path "${HOME}/.ssh"`
- `--payload-raw-field tool "python3"`
- `--payload-raw-field expected_kind "directory"`

This is the “enforceable flexibility” sweet spot: your output is still domain-defined, but it’s now contract-checkable.

---

## Mini DSL tutorial: generate `boundaries.json` across many run dirs

If you maintain multiple run dirs, hand-editing JSON Schema in each one is error-prone. A practical pattern is:

1. Maintain a tiny DSL that lists your operation kinds and required fields.
2. Generate a `boundaries.json` into each run dir from that DSL.

Here’s a minimal line-based DSL (`operations.dsl`):

```text
# kind|required_operation_args|required_payload_raw
proc.exec|command|tool
fs.stat|path|expected_kind
```

Constraints for this toy DSL (so the generator stays simple):

- `kind`, arg keys, and raw keys must match `^[A-Za-z0-9_.-]+$`.
- Lists are comma-separated (no spaces).
- Empty lists are allowed (`proc.exec||tool`).

### `generate-boundaries.sh` (Bash 3.2, no dependencies)

This script reads `operations.dsl` and writes a `boundaries.json` whose `record_schema` includes:

- the baseline envelope, plus
- a `oneOf` clause per operation kind that requires the listed fields.

Save as `generate-boundaries.sh`:

```bash
#!/bin/bash
set -euo pipefail

dsl_path="${1:-operations.dsl}"
out_path="${2:-boundaries.json}"

token_re='^[A-Za-z0-9_.-]+$'

json_string_array() {
  # Input: comma-separated tokens (no spaces). Output: JSON array of strings.
  local csv="${1:-}"
  if [[ -z "${csv}" ]]; then
    printf '[]'
    return 0
  fi

  local out='['
  local first=1
  local token=""
  IFS=',' read -r -a parts <<< "${csv}"
  for token in "${parts[@]}"; do
    if [[ -z "${token}" ]]; then
      continue
    fi
    if ! [[ "${token}" =~ ${token_re} ]]; then
      echo "generate-boundaries: invalid token '${token}' (must match ${token_re})" >&2
      exit 1
    fi
    if [[ "${first}" -eq 0 ]]; then
      out="${out},"
    fi
    first=0
    out="${out}\"${token}\""
  done
  out="${out}]"
  printf '%s' "${out}"
}

json_object_with_required_keys() {
  # Input: comma-separated tokens. Output: JSON Schema fragment:
  #   { "type":"object", "required":[...], "properties": { key: {} ... }, "additionalProperties": true }
  local csv="${1:-}"
  local required
  required="$(json_string_array "${csv}")"

  local props='{'
  local first=1
  local token=""
  IFS=',' read -r -a parts <<< "${csv}"
  for token in "${parts[@]}"; do
    if [[ -z "${token}" ]]; then
      continue
    fi
    if [[ "${first}" -eq 0 ]]; then
      props="${props},"
    fi
    first=0
    props="${props}\"${token}\": {}"
  done
  props="${props}}"

  printf '{"type":"object","required":%s,"properties":%s,"additionalProperties":true}' "${required}" "${props}"
}

oneof_entries=""
while IFS= read -r line || [[ -n "${line}" ]]; do
  line="$(printf '%s' "${line}" | sed -e 's/#.*$//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  if [[ -z "${line}" ]]; then
    continue
  fi

  IFS='|' read -r kind op_args payload_raw <<< "${line}"
  if [[ -z "${kind}" ]]; then
    echo "generate-boundaries: missing kind in line: ${line}" >&2
    exit 1
  fi
  if ! [[ "${kind}" =~ ${token_re} ]]; then
    echo "generate-boundaries: invalid kind '${kind}' (must match ${token_re})" >&2
    exit 1
  fi

  op_args_schema="$(json_object_with_required_keys "${op_args:-}")"
  payload_raw_schema="$(json_object_with_required_keys "${payload_raw:-}")"

  op_required='["kind","target"]'
  if [[ -n "${op_args:-}" ]]; then
    op_required='["kind","target","args"]'
  fi

  payload_required='[]'
  if [[ -n "${payload_raw:-}" ]]; then
    payload_required='["raw"]'
  fi

  entry=$(
    cat <<EOF
{
  "properties": {
    "operation": {
      "type": "object",
      "required": ${op_required},
      "properties": {
        "kind": { "const": "${kind}" },
        "target": { "type": "string" },
        "args": ${op_args_schema}
      },
      "additionalProperties": true
    },
    "payload": {
      "type": "object",
      "required": ${payload_required},
      "properties": {
        "raw": ${payload_raw_schema}
      },
      "additionalProperties": true
    }
  }
}
EOF
  )

  if [[ -n "${oneof_entries}" ]]; then
    oneof_entries="${oneof_entries},"
  fi
  oneof_entries="${oneof_entries}${entry}"
done < "${dsl_path}"

if [[ -z "${oneof_entries}" ]]; then
  echo "generate-boundaries: no operations found in ${dsl_path}" >&2
  exit 1
fi

cat > "${out_path}" <<EOF
{
  "schema_version": "boundaries_v1",
  "stdout": { "format": "ndjson" },
  "record_schema": {
    "\$schema": "http://json-schema.org/draft-07/schema#",
    "type": "object",
    "additionalProperties": false,
    "required": ["script", "operation", "result", "context", "payload"],
    "properties": {
      "script": {
        "type": "object",
        "additionalProperties": false,
        "required": ["id"],
        "properties": {
          "id": { "type": "string", "pattern": "^[A-Za-z0-9_.-]+$" }
        }
      },
      "operation": {
        "type": "object",
        "additionalProperties": true,
        "required": ["kind", "target"],
        "properties": {
          "kind": { "type": "string" },
          "target": { "type": "string" },
          "args": { "type": "object", "additionalProperties": true }
        }
      },
      "result": {
        "type": "object",
        "additionalProperties": false,
        "required": ["outcome"],
        "properties": {
          "outcome": { "type": "string", "enum": ["success", "denied", "partial", "error"] },
          "details": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
              "exit_code": { "type": "integer" },
              "errno": { "type": "string" },
              "message": { "type": "string" },
              "error_detail": { "type": "string" }
            }
          }
        }
      },
      "context": {
        "type": "object",
        "additionalProperties": true,
        "required": ["commitments"],
        "properties": {
          "commitments": {
            "type": "array",
            "items": {
              "type": "object",
              "additionalProperties": false,
              "required": ["id", "helps"],
              "properties": {
                "id": { "type": "string", "pattern": "^[A-Za-z0-9_.-]+$" },
                "helps": {
                  "type": "array",
                  "minItems": 1,
                  "uniqueItems": true,
                  "items": { "type": "string", "enum": ["ensure", "detect", "emit"] }
                }
              }
            }
          }
        }
      },
      "payload": {
        "type": "object",
        "additionalProperties": true,
        "required": ["stdout_snippet", "stderr_snippet"],
        "properties": {
          "stdout_snippet": { "type": "string" },
          "stderr_snippet": { "type": "string" },
          "raw": { "type": "object", "additionalProperties": true }
        }
      },
      "extensions": { "type": "object", "additionalProperties": true }
    },
    "oneOf": [${oneof_entries}]
  }
}
EOF

echo "wrote ${out_path}"
```

Use it to generate a consistent contract into multiple run dirs:

```bash
chmod +x ./generate-boundaries.sh
./generate-boundaries.sh operations.dsl ./suite/run_dirs/env_probes/boundaries.json
./generate-boundaries.sh operations.dsl ./suite/run_dirs/fs_probes/boundaries.json
```

Now you can evolve your suite-wide contract by editing `operations.dsl` and re-running the generator.

---

## Troubleshooting (common failures)

### “missing/invalid contract” errors

Symptoms: `fencerunner` errors before running any scripts, mentioning `loading .../gates.json` (or `commitments.json` / `boundaries.json`).

Fixes:

- Ensure all three files exist in the run dir.
- Ensure `schema_version` is correct (`gates_v1`, `commitments_v1`, `boundaries_v1`).
- Ensure the JSON is valid (no trailing commas).

### “No scripts found under …”

- Ensure there is at least one top-level `*.sh`.
- Ensure it’s executable: `chmod +x your_script.sh`.

### “script is not executable”

- Run `chmod +x your_script.sh`.
- Ensure your filesystem didn’t strip executable bits when copying files.

### “failed to parse boundary object from script stdout”

- Your script wrote something that is not a single JSON object to stdout.
- Check for stray `echo`, progress output, or commands writing to stdout.
- Capture command output into files and pass via `--payload-stdout-file` / `--payload-stderr-file`.

Tip: run the same invocation with `--supervised` to get a synthetic record that captures stdout/stderr snippets.

### “record violates boundaries.json”

- Your script emitted JSON, but it didn’t satisfy the schema in `boundaries.json`.
- Compare the record against your `record_schema.required` and `properties` rules.

### “script.id … does not match filename id …”

- Your record says `script.id="x"`, but the file is named `y.sh`.
- Use this pattern to derive id:

  ```bash
  script_id="$(basename "${BASH_SOURCE[0]}" .sh)"
  ```

---

## Advanced footnote: supervised synthetic records vs strict schemas

In `--supervised` mode, `fencerunner` emits synthetic error records when a script breaks the contract. Those synthetic records include additional metadata (like `extensions.synthetic` and a `payload.raw.supervised` block).

If you tighten `boundaries.json` too aggressively (for example `additionalProperties: false` everywhere and an `operation.kind` enum that doesn’t include the synthetic kind), synthetic records may no longer validate against your contract.

What happens then:

- stdout still stays a well-formed NDJSON stream (you still get one line per script),
- but `fencerunner` may log on stderr that the synthetic record violated `boundaries.json`.

If you want supervised mode *and* strict schema validation of synthetic records, design your schema to allow:

- `extensions.synthetic`
- `payload.raw.supervised`
- `operation.kind = "harness.supervised"`

Keep this in the “advanced” bucket unless you have consumers that require “every line validates against `boundaries.json` even for synthetic errors”.
