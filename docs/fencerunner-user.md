# fencerunner user guide

>Run Bash scripts as a deterministic NDJSON stream.

Use this guide when you want a directory of scripts to behave like a single instrumented suite. You point `fencerunner` at one or more run dirs; it executes each script and produces an NDJSON stream where every line is a boundary record describing what a script tried to do and how it went.

Most of the guide is about making that stream pleasant to consume and easy to evolve: keep stdout clean, emit records with `emit-record`, and shape `boundaries.json` so wildly different scripts still read cleanly as they stream. Later sections show how to add lightweight instrumentation with commitments and how to keep contracts consistent across many run dirs.

## Contents

- [The contract](#the-contract)
- [Runtime](#runtime)
- [Quickstart](#quickstart)
- [Boundary records](#boundary-records)
- [Run modes](#run-modes)
- [Commitments](#commitments)
- [Run directories](#run-directories)
- [Unified streams](#unified-streams)
- [Evolving boundaries](#evolving-boundaries)
- [Generating boundaries](#generating-boundaries)
- [Troubleshooting](#troubleshooting)
- [Advanced](#advanced)
- [Signal audit grab bag](#signal-audit-grab-bag)

## The contract

>stdout is reserved.

`fencerunner` treats a script’s stdout as its interface: the boundary record. Anything else on stdout (logs, progress, stray `echo`) is a contract break. Send diagnostics to stderr, capture any command output you care about, and emit exactly one boundary record when the script is done.

Each top-level `*.sh` is one script, and each script emits one record. The record must declare `script.id`, and that id must match the filename stem (`my_probe.sh` → `my_probe`), so identity is stable and deterministic. In a single run, script ids must be unique so the stream can be consumed without ambiguity. `fencerunner` also enforces a fixed result.outcome vocabulary (success|denied|partial|error) for all scripts so downstream tools can treat `result.outcome` as a stable enum.

That’s most of what `fencerunner` insists on. Everything else—what you encode in the record, how strict the schema is, which operation kinds exist—is defined by the contracts you choose to write in the run dir.

---

## Runtime

When `fencerunner` executes a script it sets `CWD` to the run dir, exports `FENCERUNNER_RUN_DIR`, and provides a temporary `FENCERUNNER_ROOT` containing:

- `lib/library.sh`: Runner-owned Bash library you `source` to get stable helpers (including `commit_help_me`). Mandated so scripts have one shared interface for runner integration instead of re-implementing it ad hoc.
- `bin/emit-record`: Runner-provided record emitter you invoke as `emit-record`; it builds a boundary record and prints it as one NDJSON line. Mandated because it keeps stdout clean and records schema-valid.
- `bin/commit-help-me`: Runner-provided enrollment recorder used by `commit_help_me <verb> <commitment.id>`. Mandated so commitment enrollments are captured consistently and serialized into `context.commitments`.

These are runner-provided helpers. In practice, you source the library, enroll any commitments you care about, and emit the one boundary record with `emit-record`.

---

## Quickstart

Follow this once to get a runnable run dir and see a boundary record stream on stdout.

### Install

Install from a prebuilt release on macOS by placing a downloaded binary on your `PATH`. Releases are currently tested on Apple Silicon (`arm64`), so the steps below use the `aarch64-apple-darwin` artifact.

1. Open the v1.0.0 release page: `https://github.com/Protonk/fencerunner/releases/tag/v1.0.0`
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

`fencerunner` is tested on Apple Silicon. If you’re on Intel macOS, build from source or use a matching release artifact if one exists.

### Create a run dir

```bash
mkdir -p ./example-run-dir
cd ./example-run-dir
```

### Add the contracts

A run dir becomes runnable once it has the triad (`gates.json`, `commitments.json`, `boundaries.json`) plus at least one executable `*.sh` script file (not a symlink). `fencerunner` validates the contracts before it runs any scripts, then validates each script’s record against `boundaries.json` at runtime.

The examples below are a minimal, schema-valid baseline. `gates.json` stays empty unless you opt into extra checks, `commitments.json` declares a vocabulary for lightweight instrumentation, and `boundaries.json` defines the shape of the boundary records you’ll stream on stdout.

#### `gates.json`

```json
{
  "schema_version": "gates_v1"
}
```

#### `commitments.json`

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

#### `boundaries.json`

This is a stable envelope with a flexible interior: downstream tools can trust the top-level shape, while you can standardize `operation.args` and `payload.raw` over time.

You don’t need to understand every field yet. For now, treat this as a copy-paste baseline; the sections below explain what the record means and how to evolve it deliberately.

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

### Add a script

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

### Run it

From the directory *above* the run dir:

```bash
cd ..
fencerunner ./example-run-dir
```

You’ll get one JSON object per line on stdout (NDJSON).

If you’re feeding the stream into a pipeline, try `--supervised` to keep stdout well-formed even when scripts break (see Run modes).

Next, the guide explains what that boundary record means and how to evolve the schema without losing stream determinism.

---

## Boundary records

>Your boundary record is your interface.

### Stdout is reserved

In strict mode (the default), `fencerunner` treats script stdout as **the boundary record**. If your script prints anything else (a log line, a progress bar, a stray `echo`), the record parse/validation will fail.

The practical pattern is simple: send diagnostics to stderr (`... >&2`), capture command stdout/stderr into files or variables, place whatever evidence you care about into `payload.stdout_snippet`, `payload.stderr_snippet`, and/or `payload.raw`, then emit exactly one record via `emit-record`.

### Core shape

The recommended baseline treats every boundary record as the same five-part envelope: `script` (identity), `operation` (what you attempted), `result` (how it went), `context` (instrumentation), and `payload` (evidence).

In that envelope, downstream tools almost always care about the same few fields: `script.id` (filename stem), `operation.kind` and `operation.target` (what this record *is about*), `result.outcome` (fixed enum: `success|denied|partial|error`), and whatever you choose to put in `payload.raw` as your suite-specific structured payload.

`context.commitments` is where scripts record lightweight “I relied on / observed / emitted X” signals via `commit_help_me` (empty allowed). `payload.stdout_snippet` and `payload.stderr_snippet` are string evidence channels (empty allowed). If you want multiple run dirs to produce a unified stream, make `operation.kind` and `operation.target` mean the same thing everywhere.

### Using `emit-record`

`emit-record` is a runner-provided helper that builds the JSON object, merges any enrolled commitments from `commit_help_me`, validates the record against the run dir’s `boundaries.json`, and prints the record as a single NDJSON line.

`emit-record` is available to scripts because `fencerunner` puts a shim on `PATH` when it runs your scripts. You do not install it separately.

### Capturing output

If your script needs to run commands that write to stdout/stderr, capture them and pass them to `emit-record`:

```bash
#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"
script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

stdout_file="$(mktemp -t probe.stdout)"
stderr_file="$(mktemp -t probe.stderr)"

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
  --payload-stdout-file "${stdout_file}.trimmed" \
  --payload-stderr-file "${stderr_file}.trimmed"
```

If you want to keep snippets small, trim them before emitting:

```bash
head -c 2000 "${stdout_file}" > "${stdout_file}.trimmed"
head -c 2000 "${stderr_file}" > "${stderr_file}.trimmed"
```

Then pass the trimmed files to `emit-record` so stdout stays clean and payloads stay bounded.

---

## Run modes

>Strict fails fast. Supervised keeps the stream intact.

Most users end up using both modes: strict when authoring and evolving a suite, supervised when consuming the stream in a pipeline where “always one record per script” matters.

### Strict mode

Strict mode is the default. Use it when you want contract breaks to fail the run with a non-zero exit code. In strict mode a script must emit exactly one schema-valid boundary record on stdout; if it emits invalid JSON, violates `boundaries.json`, mismatches `script.id`, exits non-zero, or violates an enforced gate, the run fails and no record is emitted for that script.

Strict mode fails fast: the runner stops at the first script-level contract break, and any remaining scripts are not executed.

### Supervised mode

Supervised mode (`--supervised`) is for pipelines where a well-formed NDJSON stream matters more than perfect script behavior. `fencerunner` will output one record per script; when a script breaks the contract it emits a synthetic error record that captures stdout/stderr snippets and explains what happened. Supervised exits `0` unless preflight or the runner itself fails (missing contracts, invalid contracts, script not executable, duplicate script ids, and similar harness-level failures).

---

## Commitments

>Lightweight instrumentation that travels with the stream.

Commitments are a **deliberate, lightweight instrumentation channel**: scripts enroll in named commitments as they run, and those enrollments are recorded under `/context/commitments` in the boundary record stream.

They are not a security boundary. They’re a way for cooperating authors (human or agentic) to leave behind structured “I meant to do X” signals that downstream tooling can query.

Some examples below use `jq` for reporting; `fencerunner` does not ship it.

Commitment ids are **simple tokens**: `^[A-Za-z0-9_.-]+$` (letters/digits plus `_`, `.`, `-`). If you need spaces, slashes, or other punctuation, put that detail into `payload.raw` or `operation.args` and keep the commitment id as the stable label. If you call `commit_help_me` with an invalid id, it fails and your script should treat that as a hard error.

### Branching canaries

Sometimes you want the thinnest possible instrumentation: “did this code path run?”

In `<RUN_DIR>/commitments.json`, declare three commitments: `main.canary` and `branch.canary` (both `emit`) and `policy.read_only` (`ensure`).

Then in your script, enroll `main.canary`, take a trivial branch, and enroll `branch.canary` inside it:

```bash
commit_help_me emit main.canary
commit_help_me ensure policy.read_only

if true; then
  commit_help_me emit branch.canary
fi
```

Here, `emit` is used as a paper-thin “record that we hit this marker” verb, and `ensure` is used as a paper-thin “this script claims a policy/assumption” verb. Both are still valuable: they’re queryable and they travel with the record stream.

Two constraints matter. First, commitments can be bespoke and paper-thin: a canary id is still useful because it’s queryable. Second, enrollment order is **not preserved**. Downstream should treat `/context/commitments` as a set: presence/absence is meaningful; ordering is not.

To check whether the branch marker is present:

```bash
fencerunner --supervised ./your-run-dir | jq -e '.context.commitments | any(.id=="branch.canary")'
```

### Two patterns

The canaries above are intentionally trivial: they work because commitment ids are durable labels you can query for, not because they carry rich data. Once you start using commitments across a suite, they become a small vocabulary that keeps your NDJSON stream interpretable even when scripts differ. Keep ids high-level and stable; put per-run specifics (paths, ticket ids, hashes, counts) into `payload.raw` or `operation.args`.

There are two common ways to spend this budget. One treats commitments as a lightweight map of *dependencies and capabilities* (“what did this script rely on or exercise?”). The other treats commitments as lightweight *context tags* (“under what policies/assumptions should a consumer interpret this record?”). Both are instrumentation, not enforcement, and you can mix them freely.

#### Dependencies and capabilities

Use commitments to say what your scripts relied on or exercised: tools, runtimes, privileges, or environmental capabilities. This is closest to “dependency declaration”, but still used as instrumentation (not enforcement).

`commitments.json` (excerpt):

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
      "helps": ["ensure", "detect"],
      "is": "Python 3 interpreter",
      "at": "python3",
      "version": "v1"
    },
    {
      "id": "net.dns",
      "provider": "system",
      "helps": ["detect"],
      "is": "DNS resolver available",
      "at": "scutil --dns",
      "version": "v1"
    }
  ]
}
```

Enroll close to the point of use: call `commit_help_me ensure python3` before invoking `python3`, call `commit_help_me detect net.dns` when your probe actually inspects resolver state, and call `commit_help_me emit emit.record` before emitting the record.

Why this is useful: you can build reports like “show me all records that depended on python3” or “which probes performed DNS detection”.

#### Policies and provenance

Use commitments to tag records with the *interpretive context* your downstream tooling needs: “read-only run”, “operator-authorized”, “inventory source”, “baseline version”.

These are not “software dependencies”; they’re promises and provenance statements. The detailed per-run value (ticket id, inventory file path, baseline hash) belongs in `payload.raw` or `operation.args`. The commitment id is the stable category.

`commitments.json` (excerpt):

```json
{
  "schema_version": "commitments_v1",
  "commitments": [
    {
      "id": "policy.read_only",
      "provider": "user",
      "helps": ["ensure"],
      "is": "Run is intended to be non-destructive",
      "at": "runbook:read-only-probes",
      "version": "v1"
    },
    {
      "id": "provenance.asset_inventory",
      "provider": "user",
      "helps": ["ensure"],
      "is": "Asset inventory is supplied out-of-band",
      "at": "docs:inventory-format",
      "version": "v1"
    },
    {
      "id": "baseline.outcomes_v1",
      "provider": "user",
      "helps": ["ensure"],
      "is": "Outcome vocabulary is success|denied|partial|error",
      "at": "docs:boundaries",
      "version": "v1"
    }
  ]
}
```

Treat these as suite-wide context tags: call `commit_help_me ensure policy.read_only` at the top of every script, and when you read an inventory file (or similar out-of-band input), enroll `commit_help_me ensure provenance.asset_inventory` while putting the concrete detail into `payload.raw.inventory_path` (not into the commitment id).

Why this is useful: you can run wildly different run dirs (different probes, different domains) and still produce a single stream that downstream tooling interprets consistently because the records carry the same “context tags”.

### Practical workflow

Start by deciding what you want to instrument: dependencies, capabilities, policies, assumptions, provenance. Encode those as stable commitment ids (think “tags with meaning”) such as `python3`, `net.dns`, `policy.read_only`, or `provenance.asset_inventory`.

Declare them in `<RUN_DIR>/commitments.json` using short, human-auditable metadata. `provider` answers “where does this come from?” (`runner|system|user`), `is` answers “what is it?”, `at` answers “where do I look for instructions?”, and `version` answers “which flavor of this promise do you mean?”.

In scripts, call `commit_help_me <ensure|detect|emit> <id>` at meaningful points. Downstream, treat `/context/commitments` as a queryable channel for reports and coverage checks.

Recommended semantics for the verbs (you can change these, but pick one meaning and stick to it): `ensure` is “this run assumes/depends on X”, `detect` is “this script measured/confirmed X”, and `emit` is “this script used X as part of its emission/instrumentation”.

Important: `commitments.json` is validated at preflight, but enrollment is intentionally **record-only** at runtime. A script can enroll in ids that are not declared; prefer declaring them anyway so your ids stay reviewable and consistent across run dirs.

### Querying commitments

Print the commitment ids each script enrolled into:

```bash
fencerunner --supervised ./your-run-dir \
  | jq -r '.script.id as $s | .context.commitments[]? | "\($s)\t\(.id)\t\(.helps|join(","))"'
```

Compute the set of all commitment ids seen in a run:

```bash
fencerunner --supervised ./dirA ./dirB \
  | jq -r -s 'map(.context.commitments[].id) | unique | .[]'
```

---

## Run directories

A **run directory** (“run dir”) is the unit you point `fencerunner` at: a flat folder of executable `*.sh` files plus the three JSON contracts (`gates.json`, `commitments.json`, `boundaries.json`). Quickstart built one; in practice you’ll end up with a small collection. Treat each run dir as a portable suite: something you can copy, review, and run in isolation.

The runner only looks at the top level: every executable `*.sh` regular file is treated as one script and subdirectories are ignored. Symlinked scripts are rejected. Script ids come from filenames, and they must be unique across all run dirs in a single run. Within a run dir, scripts execute in lexicographic `script.id` order; across run dirs, run dirs execute in CLI argument order. Scripts must be executable (`chmod +x your_script.sh`). On macOS, target `/bin/bash` (Bash 3.2) for consistent behavior across machines.

The triad is where you decide how strict or flexible the suite should be. Start with the recommended baseline from Quickstart, then evolve `boundaries.json` and your commitments vocabulary as your consumers become more demanding.

---

## Unified streams

To make output “undifferentiated as it streams”, keep your run dirs aligned on the same `boundaries.json` shape (often literally the same file), and treat `operation.kind` and `operation.target` as suite-wide identifiers rather than one-off script details. If every record speaks the same top-level language, downstream tooling can summarize the stream without knowing which run dir a record came from.

### Two run dirs

Create a suite folder:

```bash
mkdir -p ./suite/run_dirs/env_probes
mkdir -p ./suite/run_dirs/fs_probes
```

In both run dirs, create the triad by copying the same `gates.json`, `commitments.json`, and `boundaries.json` (from the Quickstart) into `./suite/run_dirs/env_probes/` and `./suite/run_dirs/fs_probes/`.

Now add one script per run dir (note: ids must be globally unique across the whole run):

`./suite/run_dirs/env_probes/env_python3_version.sh`

```bash
#!/bin/bash
set -euo pipefail
source "${FENCERUNNER_ROOT}/lib/library.sh"

script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

stdout_file="$(mktemp -t python3.stdout)"
stderr_file="$(mktemp -t python3.stderr)"

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

## Evolving boundaries

>Tighten the schema without losing flexibility.

The baseline `boundaries.json` is flexible: it gives you a stable envelope (`script/operation/result/context/payload`) and leaves room for you to standardize on top.

In practice, “evolving the contract” usually means tightening it in stages: start permissive, then constrain `operation.kind` so typos become contract breaks, standardize `operation.args` so consumers can rely on keys, require specific `payload.raw` fields for specific operation kinds, and finally tighten `additionalProperties` once you trust the shape.

### Constrain `operation.kind`

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

### Per-operation schema

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

## Generating boundaries

If you maintain multiple run dirs, hand-editing JSON Schema in each one is error-prone. A practical pattern is to keep a tiny DSL that lists your operation kinds and required fields, then generate `boundaries.json` into each run dir from that DSL.

Here’s a minimal line-based DSL (`operations.dsl`):

```text
# kind|required_operation_args|required_payload_raw
proc.exec|command|tool
fs.stat|path|expected_kind
```

Constraints for this toy DSL (so the generator stays simple):

`kind`, arg keys, and raw keys must match `^[A-Za-z0-9_.-]+$`. Lists are comma-separated (no spaces). Empty lists are allowed (`proc.exec||tool`).

### `generate-boundaries.sh`

This script (plain Bash 3.2; no dependencies) reads `operations.dsl` and writes a `boundaries.json` whose `record_schema` includes the baseline envelope plus a `oneOf` clause per operation kind that requires the listed fields.

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

## Troubleshooting

>Most failures are contract mismatches.

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

### “Symlinked scripts are not allowed”

- The runner rejects symlinked `*.sh` files (even if the symlink points back into the run dir).
- Fix: copy the script into the run dir as a real file, or generate a real `*.sh` in place (don’t symlink).

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

## Advanced

>Synthetic records and strict schemas.

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

---

## Signal audit grab bag

>Loose ends worth validating.

### Payload size limits

`emit-record` (and supervised synthetic records) keep records compact and predictable by enforcing a size limit and truncating snippets:

 - `payload` (as serialized JSON) is capped at 16 KiB (16384 bytes).
- `payload.stdout_snippet` and `payload.stderr_snippet` are NUL-stripped and truncated to 2000 characters (with an ellipsis).
- Common failure: `Payload exceeds 16384 bytes (got N)`; keep `payload.raw` summary-sized and write large artifacts to files (then reference paths/hashes in `payload.raw`).

### Duplicate commitment enrollments

`commit_help_me` treats duplicates as a contract break. If a script calls the same `<verb> <commitment.id>` pair twice, `commit-help-me` exits non-zero and the script should fail fast.

### Duplicate script ids across run dirs

Script ids must be globally unique in a single run. If you run `fencerunner ./dirA ./dirB` and both contain `probe.sh` (id `probe`), preflight fails with a “Duplicate script id …” error. Fix: rename one of the scripts (or split runs).

### Outcome vocabulary

`fencerunner` enforces a fixed outcome vocabulary: `success|denied|partial|error` (and `emit-record` enforces it too). If a script emits any other `result.outcome`, strict mode fails and supervised mode emits a synthetic error record. If you need richer result semantics, keep `result.outcome` in this vocabulary and encode your extra meaning under `operation.*` or `payload.raw`.
