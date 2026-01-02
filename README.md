# Fencerunner

Fencerunner is a flexible probe runner built to instrument potentially noisy probes. Probes can be “wild” Bash scripts that do anything the host and sandbox allow, but fencerunner turns the run into a clean, machine-readable stream of what happened.

The core promise is output rigor: one boundary record per probe, streamed as NDJSON to stdout. Each record summarizes the attempted operation and observed outcome in a shape that downstream tools can validate and consume deterministically.

Runs are defined by run dirs: flat directories of probe scripts plus three contracts. A run dir declares the commitments a probe intends to rely on (runner helpers and external runtimes), the gates it wants treated as hard failures, and the boundaries it commits to publishing as output. This design is about instrumentation, not containment.

The goal is build-time flexibility coupled with run-time rigidity. Strict mode treats contract breaks as failures; supervised runs prioritize a well-formed NDJSON stream over perfect probe behavior.

## Use

Run `fencerunner` with one or more run dirs. By default it runs in strict mode, treating contract breaks as failures; use `--supervised` when keeping a well-formed NDJSON boundary stream matters more than perfect probe behavior.

```sh
fencerunner probes
fencerunner ./probes /tmp/other-run-dir
```

## What makes a RUN_DIR

A run dir is a flat directory you pass to `fencerunner`. It bundles probe scripts with three run-dir-local contracts: **commitments**, **gates**, and **boundaries**.

A minimal run dir contains:

- `commitments.json` — a registry of declared commitments a probe may rely on (runner helpers and external runtimes) and the help verbs they support.
- `gates.json` — optional gate enrollments that tighten the probe contract for this run dir (for example enforcing `stderr.empty`).
- `boundaries.json` — the output contract for boundary records (stdout format and the schema each record must satisfy).
- one or more executable `*.sh` files — each `*.sh` at the top level is a probe; subdirectories are ignored.

Conventions and constraints:

- Probe ids are derived from filenames (`<probe_id>.sh`) and must be unique across all run dirs in a single run.
- Probes should source the runner library at startup, then emit exactly one boundary record to stdout (and nothing else on stdout).
- In strict runs, contract breaks are failures. In supervised runs, probe-level contract breaks are converted into synthetic error records so the NDJSON stream stays well-formed (preflight/runner failures still abort).

## Tests

Tests in this repo act more like a contract gate than a coverage exercise: they assert on the externally observable surfaces (schemas, helper CLI behavior, exit codes, and the NDJSON boundary stream) and fail hard when those surfaces drift or when a promised contract can no longer be validated.

The posture is integration-forward and deterministic. Tests build and run the real binaries, execute fixture probes (including the repo’s minimal example probe) inside temporary run dirs/workspaces, and validate that stdout stays a well-formed boundary-record stream while stderr remains a diagnostic channel.
