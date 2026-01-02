# fencerunner

`fencerunner` instruments Bash scripts on macOS like Peter Sellers in Being There, quietly opinionated about a small core of restrictions in the simplest manner possible whilst scripts go about their business. It is not oblivious; no matter what scripts do, fencerunner summarizes the attempt in a shape that downstream tools can validate and consume deterministically. Use it to help organize messy collections of scripts into a suite of instruments on your own terms and you may find life is a state of mind. 

Asking questions about macOS can be challenging, especially if you care about the answer. A useful way to share questions and check answers is to write in `Bash 3.2`, which will run the same<sup>*</sup> regardless of local color. Trouble is, collect enough questions and you'll discover bags of random Bash scripts are idiosyncratic and ornery about it. The old way around this would be to sigh and learn some useful logging or instrumentation framework, tolerating whatever opinions it had in order to avoid organizing a Borgean library. You can still do that. But it is 2026;<sup>**</sup> we have frontier models that can write code to any imaginable degree of articulation. fencerunner makes generating and coalescing wild scripts into a system of instruments with the magic of science feel like greased lightning.

It achieves this by orienting itself toward agentic coders. Contracts and validation pipelines are clearly laid out on disc; documentation is profuse, literal and tied to code; and a posture of build-time flexibility coupled with run-time rigidity affords useful feedback loops with clear test targets. Scripts are run in flat directories containing three contracts with commitments to declare, gates to be treated as hard failures, and boundaries of the output. The content of those contracts is almost entirely up to the user, with validation against a small json schema via a clear, mechanical pipeline. Your agent doesn't need to learn fencerunner. It already knows it. 

## Use

Run `fencerunner` with one or more run dirs. By default it runs in strict mode, treating contract breaks as failures; use `--supervised` when keeping a well-formed NDJSON boundary stream matters more than perfect script behavior.

```sh
fencerunner scripts
fencerunner ./scripts /tmp/other-run-dir
```

## What makes a RUN_DIR

A run dir is a flat (subdirectories are ignored) directory you pass to `fencerunner`. It bundles shell scripts with three run-dir-local contracts: **commitments**, **gates**, and **boundaries**. Fencerunner accepts mulitple run dirs, but script ids are derived from filenames and must be unique across all run dirs in a single run. A minimal run dir contains:

- `commitments.json` — a registry of declared commitments a script may rely on (runner helpers and external runtimes) and the help verbs they support.
- `gates.json` — optional gate enrollments that tighten the script contract for this run dir (for example enforcing `stderr.empty`).
- `boundaries.json` — the output contract for boundary records (stdout format and the schema each record must satisfy).
- one or more executable `*.sh` files — each `*.sh` at the top level is a script

## Tests

Tests here act more like a contract gate than a coverage exercise: they assert on the externally observable surfaces (schemas, helper CLI behavior, exit codes, and the NDJSON boundary stream) and fail hard when those surfaces drift or when a promised contract can no longer be validated.

The posture is integration-forward and deterministic. Tests build and run the real binaries, execute fixture scripts (including the repo’s minimal example script) inside temporary run dirs/workspaces, and validate that stdout stays a well-formed boundary-record stream while stderr remains a diagnostic channel.

---

<sup>*</sup>: No.

<sup>**</sup>: Yes.
