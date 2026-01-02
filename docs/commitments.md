# Commitments

Commitments are **run-dir-local declared dependencies**: intentional
commitments that scripts rely on when they need help that cannot be expressed
purely inside the script.

- Declared in `<RUN_DIR>/commitments.json`.
- Validated by the meta-schema at [`schema/commitments.json`](../schema/commitments.json).
- Enrolled at runtime via `commit_help_me <ensure|detect|emit> <commitment.id>` (provided by [`lib/library.sh`](../lib/library.sh)).
- Recorded in boundary objects under `/context/commitments` as `(id, helps[])` pairs.

This document explains what `commitments.json` commits to, how it is
validated, and what “extension” means for run-dir authors.

## Validation flow

1. The run dir’s `commitments.json` is validated against [`schema/commitments.json`](../schema/commitments.json).
2. Runners preflight and require `commitments.json` (missing/invalid is a hard error).
3. During script execution:
   - `commit_help_me` records `(commitment.id, help)` enrollments for the current script run (returns `0` on success, `1` on error).
   - `emit-record` reads those enrollments via `FENCERUNNER_COMMITMENT_ENROLLMENTS_PATH` and serializes them into `/context/commitments`.

`commit_help_me` does **not** validate enrollments against `commitments.json` at runtime; `commitments.json` is a structured dependency registry for run-dir authors and downstream consumers.

## File shape (`commitments_v1`)

Top level:

- `schema_version` — must be `"commitments_v1"`.
- `commitments[]` — list of commitment definitions (may be empty).

Each commitment definition:

- `id` — identifier (run-dir-local namespace; referenced by scripts).
- `provider` — one of `runner`, `system`, `user`.
- `helps[]` — one or more verbs from `{ensure, detect, emit}` (unique).
- `is` — short human description (max 100 chars).
- `at` — string location/handle (semantics vary by `provider`).
- `version` — string version label.

## Field semantics and recommended patterns

### `id`

- Keep it stable: it becomes the durable key recorded in `/context/commitments`.
- Must match `^[A-Za-z0-9_.-]+$` (letters/digits plus `_`, `.`, `-`).
- Uniqueness is required **within a single run dir**.

Why the tight alphabet: commitment ids are designed to be portable, queryable
tokens that work well in Bash, JSON, and downstream tools. Keeping ids free of
whitespace, slashes, and shell-significant punctuation avoids quoting
surprises, keeps enrollment recording simple (`id|help` lines), and reduces
“looks the same but isn’t” issues when agents are generating or editing run
dirs.

Runtime note: the runner and helpers enforce the same id contract at runtime.
If a script calls `commit_help_me` with an invalid id, enrollment fails and the
script should treat that as a hard error.

### `provider`

`provider` communicates *where the run dir expects the dependency to come from*:

- `runner` — provided by the fencerunner runner (typically a helper binary built from this repo, e.g. `emit-record`).
- `system` — provided by the host system/runtime (for example `python3` on PATH).
- `user` — provided by the user/operator (for example a tool installed outside the repo).

The runner treats this as descriptive metadata today (it does not attempt to
prove a `system` or `user` dependency exists).

### `helps[]`

`helps[]` is the explicit list of supported verbs for the dependency description.
Scripts enroll by choosing a verb and id at the callsite (`commit_help_me <verb> <id>`).

The runner does not currently enforce that the requested verb appears in the
declared `helps[]`; `commit_help_me` only validates that `<verb>` is one of
`ensure|detect|emit`.

### `is`

Keep `is` short and concrete. It should answer “what is this dependency?” in one
sentence fragment.

### `at`

`at` is a string that points at *what to use*:

- For `provider=runner`, it is intended to be a **repo-relative path** (schema
  rejects absolute paths); in practice this is often a helper name like `emit-record`.
- For `provider=system|user`, `at` is currently treated as an **opaque string**
  (often a command name like `python3` or a path).

### `version`

`version` is a string you control. Use it to signal compatibility expectations
to humans and downstream tooling.

## Enrollment and recording

When a script enrolls in a commitment via `commit_help_me`, the enrollment is meant
to show up in the boundary object as a `(id, helps[])` pair under
`/context/commitments`.

The runner does not persist separate enrollment logs: enrollment data lives in
the boundary object stream.

## Examples

Minimal (no declared commitments):

```json
{
  "schema_version": "commitments_v1",
  "commitments": []
}
```

A run dir that declares the record emitter plus one system runtime:

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

## Extending within the schema

For run-dir authors today, “extension” means:

- adding new commitment entries under `commitments[]`,
- adding additional verbs to `helps[]` (when your scripts actually use them),
- versioning/changing `is`/`at`/`version` as your run dir evolves.

`commitments_v1` is intentionally strict: commitment definitions cannot carry
extra keys. If you need additional fields, that requires a schema + validator
change.

## Philosophy

- High level: **Declared dependencies, not inferred reality** — commitments are an authoring-time declaration (“this script expects help from X”) and a trustworthy signal from a willing author, not a security boundary or a detector of what the host *actually* provides.

Lower-level opinions:

- **Dir-local registry:** commitment ids must be unique within one run dir; the runner does not coalesce registries across run dirs.
- **Minimal preflight:** runners validate that `commitments.json` is present and schema-valid; `commit_help_me` validates only its own inputs and duplicate pairs (no registry enforcement).
- **Simple verbs:** `helps[]` is constrained to `{ensure, detect, emit}` by schema; `commit_help_me` accepts only those verbs.
- **Stable shape:** each commitment has `{id, provider, helps, is, at, version}`; extra keys are rejected by the schema.
- **Runner `at` is relative:** `provider=runner` expects a repo-relative `at` (schema rejects absolute paths); other providers treat `at` as an opaque string for now.
- **Recording is in the record:** enrollment data lives only in the boundary object stream (no separate logs).
