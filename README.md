# fencerunner

`fencerunner` instruments Bash scripts on macOS like Peter Sellers in Being There, quietly opinionated about a small core of restrictions in the simplest manner possible whilst scripts go about their business. It is not oblivious; no matter what scripts do, fencerunner summarizes the attempt in a shape that downstream tools can validate and consume deterministically. Use it to help organize messy collections of scripts into a suite of instruments on your own terms and you may find life is a state of mind. 

Asking questions about macOS can be challenging, especially if you care about the answer. A useful way to share questions and check answers is to write in `Bash 3.2`, which will run the same<sup>*</sup> regardless of local color. Trouble is, collect enough questions and you'll discover bags of random Bash scripts are idiosyncratic and ornery about it. The old way around was sigh and learn some logging or instrumentation framework, tolerating whatever opinions it had in order to avoid organizing a Borgean library. You can still do that. But it is 2026;<sup>**</sup> frontier models can write code to any imaginable degree of articulation. fencerunner makes generating and coalescing wild scripts into a system of instruments with the magic of science feel like greased lightning.

It achieves this by orienting itself toward agentic coders and treating them as willing participants. A posture of build-time flexibility coupled with run-time rigidity affords useful feedback loops with clear test targets. Contracts clearly laid out on disc and easy to opt into--scripts are run in flat directories containing three contracts with commitments to declare, gates to be treated as hard failures, and boundaries of the output. The content of those contracts is almost entirely up to the user, with validation against a small json schema via a clear, mechanical pipeline. Your agent doesn't need to learn fencerunner. It already knows it. 

Follow the [user guide](docs/fencerunner-user.md) for detailed instruction and examples. 

## Three contracts

A *run directory* is a flat directory of scripts (`*.sh`) plus three run-dir-local JSON contracts: `commitments.json`, `gates.json`, and `boundaries.json`. Collectively, the triad is the user-facing API surface of fencerunner. The runner validates all three before any script runs. If any contract is missing or invalid, the run aborts early. Each contract is opted in to at the directory level. 

### `commitments.json` — declared dependencies

`commitments.json` declares dependencies scripts may rely on: runner helpers, host runtimes, or operator-installed tools. It is a registry for humans and downstream consumers: scripts enroll in commitments at runtime and those enrollments are serialized into each boundary record under `/context/commitments`.

- Schema: `schema/commitments.json`
- Narrative guide: [docs/commitments.md](docs/commitments.md)

### `gates.json` — optional enforced checks

`gates.json` is where a run dir opts into additional checks enforced by the runner. Gates are intentionally small and explicit: they tighten the script contract without adding hidden coupling between scripts and the runner.

- Schema: `schema/gates.json`
- Narrative guide: [docs/gates.md](docs/gates.md)

### `boundaries.json` — output contract

`boundaries.json` defines the shape of the boundary records emitted on stdout. It is flexible-but-enforceable: each run dir provides its own `record_schema` (a JSON Schema), but the runner’s meta-schema requires a small stable core so downstream tools always have consistent identity/outcome/enrollment/payload channels.

This is enforced in two layers. First, `boundaries.json` itself is validated against `schema/boundaries.json`. Then, each record emitted by a script is validated against the run dir’s `record_schema` at runtime. The user guide shows how to author and evolve these contracts deliberately, and how to keep multiple run dirs producing a clean, unified NDJSON stream.

- Schema: `schema/boundaries.json`
- Narrative guide: [docs/boundaries.md](docs/boundaries.md)

## Tests

Tests here assert on the externally observable surfaces (schemas, helper CLI behavior, exit codes, and the NDJSON boundary stream) and fail hard when those surfaces drift or when a promised contract can no longer be validated. The posture is integration-forward and deterministic. Tests build and run the real binaries, execute fixture scripts (including the repo’s minimal example script) inside temporary run dirs/workspaces, and validate that stdout stays a well-formed boundary-record stream while stderr remains a diagnostic channel.

---

<sup>*</sup>: No.

<sup>**</sup>: Yes.
