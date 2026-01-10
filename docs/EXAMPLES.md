# Triage

>urgency is a gradient

Triage is the process of quickly prioritizing a set of problems by severity and urgency to decide what to handle first, what can wait, and what needs escalation or a different response. It is necessary when there are more issues than can be addressed immediately, issues differ in priority, and delaying the wrong ones carries outsized cost or risk. The above is only possible when problems can be prioritized along a shared axis with criteria stable enough to satisfice. Problems indexed by severity and urgency grow harder as they become more pressing. Triage is needed the most when it is almost impossible.

This example demonstrates how to use `fencerunner` as a tool for continuous triage, starting from when it is most needed.

## The problem statement

>soup to duck

You inherit a flat directory of scripts with unique names, written by different people for different moments, with no shared contract: some are gentle probes, some are ad‑hoc admin fixes, some are half-finished experiments, and some assume secrets, mounts, network access, or interactive input. They don’t share conventions (logging, exit codes, where they write, what “success” means), they have unknown dependencies and privilege expectations, and the same script might be harmless in one environment but disruptive in another. 

This inheritance must reach a state where each and every script contributes signal to the directory's polyphonic ensemble, whatever it may be. Use `fencerunner` to step from where triage seems impossible through to where it is unecessary.

## The tools

>just one another

* Two agents who will build context independently.
* A shared message stream between agents
* Conversational turn-taking harness
* `fencerunner`
* Perfectly shared output of all NDJSON boundaries from any `run-dir` either agent uses.

## A path

>step dyadically

Work in turns. When completing step `N`, append the following:

```
### Step N + 1

>TODO:PITH

TODO: CONTENT
```

### Step 0 

>Acceptance

Start by accepting the uncomfortable truth: without execution, the bag cannot be reliably categorized, but execution is precisely what is expensive and uncertain. The way out is to make “one execution” yield enough shared signal that the next decision is easier.

### Step 1 

>Take stock of what you have

Good news! You already have a flat directory. 

### Step 2

> Make it a `run-dir`

Add the triad *next to the scripts* so `fencerunner` can run the directory without asking you to pre-interpret what any script “means”.

1. `gates.json` (start permissive; don’t add enforcement until you have signal worth protecting):

```json
{
  "schema_version": "gates_v1"
}
```

2. `commitments.json` (start tiny; this is a shared glossary for later signal flares, not enforcement):

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
      "id": "policy.read_only",
      "provider": "user",
      "helps": ["ensure"],
      "is": "Run is intended to be non-destructive",
      "at": "runbook:triage",
      "version": "v1"
    }
  ]
}
```

3. `boundaries.json`: copy the repo’s baseline contract (`scripts/boundaries.json`) into the directory. It is strict enough to keep every line comparable, but compatible with both `emit-record` output and supervised synthetic records.

### Step 3

>inventory

Run the directory once, and treat the output stream as your first “inventory”.

If you have *any* reason to believe some scripts are messy (extra stdout, bad JSON, non-zero exits), start with supervised mode so you still get one NDJSON record per script:

```bash
fencerunner --supervised ./your-run-dir > first.ndjson 2> first.stderr
```

- `first.ndjson` is the artifact you share between agents: one JSON object per line, one line per script.
- `first.stderr` is diagnostics (runner progress + any script stderr).

In supervised mode, contract breaks become synthetic records (look for `extensions.synthetic` and `operation.kind = "harness.supervised"`). That’s not “failure”; it’s signal: you now have a stable, machine-readable snapshot of what each script did on first contact.

### Step 4

>differentiate

Your first run produced a stream with two fundamentally different kinds of records:

- records a script emitted itself
- synthetic records the harness emitted because the script did not (or could not) meet the contract

Don’t interpret meaning yet. First, bucket by *how hard the script is to evaluate safely*.

Make a small table from `first.ndjson` with four columns:

- `script.id`
- `synthetic?` (is `operation.kind` `harness.supervised`?)
- `result.outcome`
- `result.details.message` (empty if absent)

Now form your first buckets (these are about evaluation friction, not correctness):

- `emits_record` — not synthetic
- `no_record` — synthetic; message contains “no boundary object”
- `stdout_garbage` — synthetic; message contains “invalid JSON” or “non-object JSON”
- `schema_break` — synthetic; message mentions `boundaries.json` / schema validation
- `nonzero_exit` — synthetic; message contains “exited non-zero”

The output of this step is just counts + script ids per bucket. That is enough for two agents to pick what to tackle next without pretending the whole bag is already understood.

### Step 5

>wrap one

Pick one script from your high-friction buckets (`no_record` or `stdout_garbage`) and convert it into a script that *always* emits a boundary record, without needing to understand what it does yet.

The trick: keep the script id stable, but move the original body out of the runner’s discovery surface.

For a script named `foo.sh`:

1. Rename the original so it is no longer a top-level `*.sh`:

```bash
mv ./foo.sh ./foo.legacy
```

2. Create a new `foo.sh` wrapper that runs the legacy script as a black box, captures its stdout/stderr, and emits one record:

```bash
#!/bin/bash
set -euo pipefail

source "${FENCERUNNER_ROOT}/lib/library.sh"
script_id="$(basename "${BASH_SOURCE[0]}" .sh)"

stdout_file="$(mktemp -t "${script_id}.stdout")"
stderr_file="$(mktemp -t "${script_id}.stderr")"

set +e
bash "./${script_id}.legacy" >"${stdout_file}" 2>"${stderr_file}"
exit_code="$?"
set -e

if [[ "${exit_code}" -eq 0 ]]; then
  outcome="success"
else
  outcome="error"
fi

commit_help_me ensure policy.read_only
commit_help_me emit emit.record

emit-record \
  --script-name "${script_id}" \
  --command "bash ./${script_id}.legacy" \
  --operation-kind "legacy.exec" \
  --target "./${script_id}.legacy" \
  --outcome "${outcome}" \
  --exit-code "${exit_code}" \
  --payload-stdout-file "${stdout_file}" \
  --payload-stderr-file "${stderr_file}"
```

3. Make it executable and re-run your run dir (supervised is fine). The output you want is not “perfect semantics”; it’s that this script moves into `emits_record`, so the rest of triage can treat it as a well-formed line in the stream.

### Step 6

>signal flares

Now that the script reliably emits a boundary record, start extracting coarse signal from the captured output without claiming you understand the script.

Add conditional `commit_help_me` calls in the wrapper that turn common “why did this fail?” smells into stable labels (and, when appropriate, refine `outcome` from a generic `error` into `denied`). Keep this vocabulary tiny: it should help you route the next move, not explain everything.

For example, after you capture `stdout_file`, `stderr_file`, and `exit_code`:

```bash
if grep -qiE 'permission denied|operation not permitted' "${stderr_file}"; then
  outcome="denied"
  commit_help_me detect finding.permission_denied
  commit_help_me emit recommend.rerun_sudo
elif grep -qiE 'command not found|No such file or directory' "${stderr_file}"; then
  outcome="error"
  commit_help_me detect finding.tool_missing
  commit_help_me emit recommend.install_dependency
fi
```

When you introduce a new flare id, add it to `commitments.json` so the run dir carries an explicit glossary for downstream consumers.

### Step 7

>three dials

Up to now you have been “making the bag runnable” without claiming it is safe.
Now you start shaping *how* it becomes safe by treating the triad as three
independent dials:

- `commitments.json` defines a vocabulary for *declared* and *observed* signal.
- `boundaries.json` defines what every line in the stream must look like.
- `gates.json` opts into additional enforcement that is orthogonal to both.

`fencerunner` does not make these three files interdependent. That is a feature:
you can ratchet one dial without having to move the others.

#### 1) Expand commitments as a small, stable taxonomy

In Step 6 you created “signal flares” by enrolling commitment ids based on
captured output. Make that vocabulary deliberately shaped so it stays useful
when you have hundreds of scripts.

Recommended naming lanes:

- `policy.*` (`ensure`) — operator intent (“we meant this run to be non-destructive”).
- `finding.*` (`detect`) — coarse observations (“this looks like a permission barrier”).
- `recommend.*` (`emit`) — the next move you want a human/agent to take.
- `emit.record` (`emit`) — the runner helper you depend on to serialize the record.

Example additions to `commitments.json` (keep `is` short; keep `at` as a human
handle for *where this signal comes from*):

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
      "id": "policy.read_only",
      "provider": "user",
      "helps": ["ensure"],
      "is": "Run is intended to be non-destructive",
      "at": "runbook:triage",
      "version": "v1"
    },
    {
      "id": "finding.permission_denied",
      "provider": "user",
      "helps": ["detect"],
      "is": "Output suggests a permissions barrier",
      "at": "triage:stderr-scan",
      "version": "v1"
    },
    {
      "id": "finding.tool_missing",
      "provider": "user",
      "helps": ["detect"],
      "is": "Output suggests a missing tool/runtime",
      "at": "triage:stderr-scan",
      "version": "v1"
    },
    {
      "id": "finding.interactive_prompt",
      "provider": "user",
      "helps": ["detect"],
      "is": "Output suggests interactive input is required",
      "at": "triage:prompt-scan",
      "version": "v1"
    },
    {
      "id": "finding.network_required",
      "provider": "user",
      "helps": ["detect"],
      "is": "Output suggests network access is required",
      "at": "triage:stderr-scan",
      "version": "v1"
    },
    {
      "id": "recommend.rerun_sudo",
      "provider": "user",
      "helps": ["emit"],
      "is": "Try again with elevated privileges",
      "at": "triage:routing",
      "version": "v1"
    },
    {
      "id": "recommend.install_dependency",
      "provider": "user",
      "helps": ["emit"],
      "is": "Install the missing dependency",
      "at": "triage:routing",
      "version": "v1"
    },
    {
      "id": "recommend.run_interactively",
      "provider": "user",
      "helps": ["emit"],
      "is": "Run in an interactive terminal or redesign as non-interactive",
      "at": "triage:routing",
      "version": "v1"
    }
  ]
}
```

This does *not* enforce anything at runtime. It is a shared dictionary that
makes the stream legible.

#### 2) Use boundaries.json to ratchet instrumentation without breaking supervised mode

`boundaries.json` is the enforcement dial. You can keep supervised mode useful
while tightening what “a good record” means.

Key observation: supervised synthetic records are intentionally marked with:

- `operation.kind = "harness.supervised"`
- `extensions.synthetic` present
- `payload.raw.supervised` present

That gives you a handle to write *conditional* schema rules: tighten real
records, stay permissive for synthetic ones.

Start by taking your current `boundaries.json` (copied from `scripts/`) and add
an `allOf` rule under `record_schema` that says:

- if the record is synthetic (`operation.kind == "harness.supervised"`), allow it
- else, require a minimum instrumentation baseline for records you actually
  accept as “emitted by a cooperating script”

Example (this is an excerpt; insert into `record_schema` alongside your
existing `type/required/properties`):

```json
{
  "allOf": [
    {
      "if": {
        "properties": {
          "operation": {
            "properties": {
              "kind": { "const": "harness.supervised" }
            }
          }
        }
      },
      "then": true,
      "else": {
        "properties": {
          "context": {
            "properties": {
              "commitments": {
                "allOf": [
                  {
                    "contains": {
                      "type": "object",
                      "required": ["id", "helps"],
                      "properties": {
                        "id": { "const": "emit.record" },
                        "helps": { "contains": { "const": "emit" } }
                      }
                    }
                  },
                  {
                    "contains": {
                      "type": "object",
                      "required": ["id", "helps"],
                      "properties": {
                        "id": { "const": "policy.read_only" },
                        "helps": { "contains": { "const": "ensure" } }
                      }
                    }
                  }
                ]
              }
            }
          }
        }
      }
    }
  ]
}
```

Why this matters:

- You are *not* making commitments and boundaries intrinsically dependent.
  You are choosing to make “non-synthetic records” imply “cooperating wrapper”.
- Synthetic records remain valid output even when scripts are not cooperating.
  That keeps supervised mode viable while you ratchet.

If you are not ready to enforce this across the whole run dir, narrow the rule:
only apply it to wrappers by conditioning on your wrapper operation kind:

```json
{
  "if": {
    "properties": {
      "operation": { "properties": { "kind": { "const": "legacy.exec" } } }
    }
  },
  "then": {
    "properties": {
      "context": {
        "properties": {
          "commitments": {
            "allOf": [
              { "contains": { "properties": { "id": { "const": "emit.record" } } } },
              { "contains": { "properties": { "id": { "const": "policy.read_only" } } } }
            ]
          }
        }
      }
    }
  }
}
```

That is the first “complex move”: you are using the boundary schema as a
ratchet to distinguish “the harness had to save us” from “the script chose to
cooperate”.

#### 3) Use gates.json as a second ratchet (when you can)

`gates.json` is enforced by the runner, but it is about execution behavior, not
schema shape. Right now there is one useful gate:

- `stderr.empty` — fail the run (strict) or emit a synthetic record (supervised)
  when a script writes to stderr.

In early triage, stderr is valuable. In later triage, *uncaptured* stderr is
noise. Once you have wrappers that capture legacy stderr into `payload`, you
can flip this on to force yourself (and your agent partner) to keep stdout as
“record only” and keep stderr empty at the script layer.

Example `gates.json`:

```json
{
  "schema_version": "gates_v1",
  "gates": {
    "enforced_checks": ["stderr.empty"]
  }
}
```

Practical wrapper adjustment when enabling this gate:

- ensure the legacy invocation captures stderr to a file (`2>"${stderr_file}"`)
- ensure any *triage scans* do not leak errors to stderr:

```bash
if grep -qiE 'password:|enter passphrase|are you sure' "${stdout_file}" 2>/dev/null; then
  commit_help_me detect finding.interactive_prompt
  commit_help_me emit recommend.run_interactively
fi
```

This does not make gates and boundaries dependent; it just ensures the harness
has a predictable place to look for evidence (the payload).

#### 4) Run the ratchet loop and compare inventories

After changing only your triad + wrappers (no “semantics” yet), re-run:

```bash
fencerunner --supervised ./your-run-dir > second.ndjson 2> second.stderr
```

Now compare `first.ndjson` to `second.ndjson` using the same bucketing you did
in Step 4. The goal is not “everything is fixed”. The goal is:

- the fraction of `emits_record` increases
- synthetic records become more informative (because wrappers capture evidence)
- commitment enrollments become more consistent (because you wrote a tiny taxonomy)

If you want one more small but meaningful complexity bump, start recording a
single structured “triage summary” object in `payload.raw` for wrappers. Keep it
summary-sized; treat the full stdout/stderr as snippets.

Example (add to the wrapper’s `emit-record` call):

```bash
emit-record \
  --script-name "${script_id}" \
  --command "bash ./${script_id}.legacy" \
  --operation-kind "legacy.exec" \
  --target "./${script_id}.legacy" \
  --outcome "${outcome}" \
  --exit-code "${exit_code}" \
  --payload-stdout-file "${stdout_file}" \
  --payload-stderr-file "${stderr_file}" \
  --payload-raw-field-json "triage" "{\"wrapped\":true,\"legacy_exit_code\":${exit_code}}"
```

That is the third dial: boundaries enforce the *shape*, commitments encode
*signals*, and gates enforce *discipline* in how scripts communicate.

### Step 8

>make a queue

You now have something you did not have at the start: a stream where every
script is represented by one comparable line, plus a small commitment taxonomy
that can be enrolled conditionally. Use that to turn “inventory” into a work
queue.

The queue is deliberately shallow. You are not trying to *explain* the bag.
You are trying to make the next decision easy: what do we do next, and who
should do it?

#### 1) Extract a routing table from the NDJSON

Pick your latest inventory file (for example `second.ndjson`). Create a
tabular view with just the fields you need for triage:

- script id
- synthetic or not (did the harness have to save the stream?)
- outcome
- `recommend.*` commitments (the “next move” lane)
- `finding.*` commitments (coarse observations)
- a short message (details message or empty)

If you have `jq`, one way to produce a TSV:

```bash
jq -r '
  [
    .script.id,
    ((.operation.kind == "harness.supervised") | tostring),
    .result.outcome,
    ([.context.commitments[]? | select(.id | startswith("recommend.")) | .id] | join(",")),
    ([.context.commitments[]? | select(.id | startswith("finding.")) | .id] | join(",")),
    (.result.details.message // "")
  ] | @tsv
' < second.ndjson > triage.tsv
```

This file is the dyad’s shared “working set”. It is small enough to skim, but
structured enough to sort.

#### 2) Choose your triage axis: route, don’t interpret

At this stage, treat `recommend.*` as your primary axis. It is the closest
thing you have to “urgency” that is both stable and satisfice-able:

- it is emitted conditionally (so it reflects a code path)
- it is coarse (so it remains valid as scripts evolve)
- it points at a *next action*, not a narrative

If a record has no `recommend.*`, that is also a route: “we don’t yet know what
to do next”.

Practical checks that keep the vocabulary honest:

- if you see more than one `recommend.*` on the same script, treat that as a
  signal to tighten your taxonomy (you want routing, not indecision)
- if your wrappers always emit `recommend.*` regardless of evidence, you are
  back to guesswork; keep recommendations evidence-driven (stderr/stdout scans,
  exit code, known markers)

#### 3) Summarize by route (counts, then ids)

Without caring about perfect formatting, you can get “what dominates the bag”
by counting recommendations:

```bash
cut -f4 triage.tsv | tr ',' '\n' | rg -v '^$' | sort | uniq -c | sort -nr
```

Then pick one route and list the affected scripts:

```bash
rg -n $'\trecommend.rerun_sudo\t' triage.tsv | cut -f1
```

If you are not using `jq`, do the same thing mentally: scan your NDJSON for
`recommend.` and write down the ids. The point is not tooling; the point is
that the stream gives you a deterministic place to look.

#### 4) Make “unknown” a route (optional but useful)

You will usually discover that a large fraction of the bag has *no*
`recommend.*` yet. That is honest, but it makes the queue less actionable
because “missing data” and “no next move” look the same.

One simple fix is to treat uncertainty as first-class signal: introduce a
single catch-all recommendation that means “a human (or a smarter pass) must
read the evidence”.

Add one id to `commitments.json`:

```json
{
  "id": "recommend.needs_human_read",
  "provider": "user",
  "helps": ["emit"],
  "is": "Needs human review to decide next action",
  "at": "triage:routing",
  "version": "v1"
}
```

Then, in wrappers, emit it only when you *cannot* justify a more specific
route. This keeps your `triage.tsv` complete: every script lands in at least
one route, and “unknown” becomes a manageable bucket rather than an absence.

#### 5) Split the work dyadically

Now you can split the bag without splitting the world.

Agree on one route per agent for the next turn. Examples:

- Agent A: scripts that recommend escalation (`recommend.rerun_sudo`)
- Agent B: scripts that recommend dependency work (`recommend.install_dependency`)

Or split by friction:

- Agent A: `stdout_garbage` / `no_record` → wrap to `emits_record`
- Agent B: `emits_record` but low-signal → add flares + tighten boundaries

The constraint is simple: both agents speak in `script.id` and the shared
taxonomy. That keeps “what changed” visible in the next NDJSON.

In the shared message stream, make the coordination equally mechanical. Post:

- the route you are taking (one `recommend.*` or one friction bucket),
- the list of `script.id` values you are touching this turn,
- the one change you intend (wrap, add a flare, tighten schema, etc.).

That is enough to avoid duplicated effort and keep the next inventory run easy
to interpret.

#### 6) Define “good enough” for this phase

You will know triage is becoming possible when these statements start being
true for *most* scripts:

- each script yields one well-formed line that is either synthetic (harness
  rescued) or clearly cooperating (wrapper-emitted)
- non-success outcomes tend to come with one `finding.*` and one `recommend.*`
  that a human/agent can act on
- the number of “unknown/needs human read” cases shrinks as wrappers capture
  evidence and flares get more specific

Once that holds, you can stop trying to “understand the bag” and instead use
the stream as the living interface: the bag is now triage-able, and your work
becomes ordinary iteration.

### Step 9

>the queue is not triage

Step 8 produced something deceptively comforting: a clean queue.

Ask yourself (seriously): if you had to decide what to do *in the next ten
minutes* to reduce risk, would “a TSV with routes” be enough? Sometimes yes.
Often no. A neat queue can become a way to postpone judgment.

This is not a critique of Step 8. Step 8 is triage-like: it forces a shared
axis (`recommend.*`) and a shared language (`script.id`). What it still lacks is
what triage is really about: prioritizing attention under uncertainty while
controlling blast radius.

So now you keep the queue — and you *bend it* toward actual triage.

The “third dial” story (boundaries shape / commitments signal / gates
discipline) is useful, but too clean. In practice:

- boundaries can encode policy (not just shape),
- commitments can become a cheap proxy for severity/confidence (not just labels),
- gates are not just “turn strictness on”, they are phase switches with costs.

#### 1) Add a second axis: risk (blast radius), not just route

`recommend.*` is a good *routing* axis (“what might we do next?”), but triage
also needs a *severity* axis (“how bad if this goes wrong?”).

Do not try to infer risk perfectly. Emit a single coarse label based on cheap
evidence and accept false positives.

Add three risk findings to `commitments.json`:

```json
{
  "id": "finding.risk.low",
  "provider": "user",
  "helps": ["detect"],
  "is": "Likely low blast radius",
  "at": "triage:risk",
  "version": "v1"
}
```

```json
{
  "id": "finding.risk.medium",
  "provider": "user",
  "helps": ["detect"],
  "is": "Unclear blast radius",
  "at": "triage:risk",
  "version": "v1"
}
```

```json
{
  "id": "finding.risk.high",
  "provider": "user",
  "helps": ["detect"],
  "is": "Likely high blast radius (treat cautiously)",
  "at": "triage:risk",
  "version": "v1"
}
```

Now teach wrappers to emit *one* of these per run. Keep it cheap: a static scan
of the legacy file plus a tiny scan of output.

Example risk heuristic inside the wrapper (after `script_id=...`):

```bash
legacy_path="./${script_id}.legacy"

# Start pessimistic; you can tune later.
risk="finding.risk.medium"

if rg -n --no-heading -S -e '(^|[[:space:]])sudo([[:space:]]|$)' \
  -e 'rm[[:space:]]+-rf' \
  -e 'launchctl[[:space:]]+(load|unload|bootstrap|bootout)' \
  -e 'diskutil[[:space:]]' \
  -e 'networksetup[[:space:]]' \
  -e 'pfctl[[:space:]]' \
  -e 'security[[:space:]]' \
  "${legacy_path}" >/dev/null 2>&1; then
  risk="finding.risk.high"
fi

if rg -n --no-heading -S -e 'curl[[:space:]]|wget[[:space:]]|http[s]?://' \
  "${legacy_path}" >/dev/null 2>&1; then
  risk="finding.risk.high"
  commit_help_me detect finding.network_required
fi

commit_help_me detect "${risk}"
```

This is intentionally approximate. The point is: the queue can now answer
“which scripts are dangerous to iterate on?” without you reading them all.

#### 2) Treat “confidence” as first-class signal (because recommendations lie)

By design, `recommend.*` is derived from heuristics. That is powerful and
dangerous. A triage queue becomes misleading if recommendations look equally
trustworthy.

Add one more finding that encodes “this route is a guess”:

```json
{
  "id": "finding.low_confidence",
  "provider": "user",
  "helps": ["detect"],
  "is": "Recommendation is heuristic / low confidence",
  "at": "triage:confidence",
  "version": "v1"
}
```

Then, in wrappers, emit it when your scan is weak (“no strong markers found,
but we still want to route it somewhere”):

```bash
if [[ -z "${strong_marker:-}" ]]; then
  commit_help_me detect finding.low_confidence
  commit_help_me emit recommend.needs_human_read
fi
```

This lets triage prioritize “low confidence + high risk” over “low confidence +
low risk”.

#### 3) Use boundaries.json to encode triage policy, not just record shape

You already used `boundaries.json` to keep the stream comparable and to avoid
breaking supervised mode.

Now you can use it to enforce *triage invariants* for cooperating records,
while keeping synthetic records allowed.

Examples of invariants that help triage:

- wrappers must emit *some* risk classification
- wrappers must emit exactly one `recommend.*` (or explicitly emit
  `recommend.needs_human_read`)
- if a wrapper claims `denied`, it must also emit a denial-flavored finding
  (so “denied” does not become a generic failure bucket)

Because synthetic records are special, keep the same pattern:

- if `operation.kind == "harness.supervised"`: allow
- else: enforce the invariants

Sketch (excerpt) for wrappers (`operation.kind = "legacy.exec"`) that require a
risk finding and at least one recommendation:

```json
{
  "allOf": [
    {
      "if": {
        "properties": {
          "operation": { "properties": { "kind": { "const": "legacy.exec" } } }
        }
      },
      "then": {
        "properties": {
          "context": {
            "properties": {
              "commitments": {
                "allOf": [
                  {
                    "contains": {
                      "properties": {
                        "id": {
                          "enum": [
                            "finding.risk.low",
                            "finding.risk.medium",
                            "finding.risk.high"
                          ]
                        }
                      }
                    }
                  },
                  {
                    "contains": {
                      "properties": { "id": { "pattern": "^recommend\\." } }
                    }
                  }
                ]
              }
            }
          }
        }
      }
    }
  ]
}
```

This is a subtle but important move: you are using the schema to keep your
heuristics honest. Not because the schema “knows the truth”, but because it
prevents silent drift into meaningless output.

Boundaries can also tighten semantics across fields. Example: if the outcome is
`denied` for a non-synthetic record, require one of a small set of denial
findings:

```json
{
  "if": {
    "properties": {
      "operation": { "properties": { "kind": { "not": { "const": "harness.supervised" } } } },
      "result": { "properties": { "outcome": { "const": "denied" } } }
    }
  },
  "then": {
    "properties": {
      "context": {
        "properties": {
          "commitments": {
            "contains": {
              "properties": {
                "id": {
                  "enum": [
                    "finding.permission_denied",
                    "finding.network_required",
                    "finding.interactive_prompt"
                  ]
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

That is what “boundaries do more than shape” looks like: they become an
enforceable policy surface for the stream.

#### 4) Use gates.json as a phase switch (and acknowledge the cost)

Today there is only one optional runner gate (`stderr.empty`), but even one gate
is enough to demonstrate the real lesson: enforcement has a cost.

If you turn on `stderr.empty` too early, you lose diagnostics before wrappers
have captured them. If you never turn it on, your suite stays noisy and hard to
consume.

So treat gates as phase toggles:

- Phase A (inventory): gate off; tolerate stderr while wrappers are being built.
- Phase B (capture): wrappers capture stdout/stderr into payload; gate still off.
- Phase C (discipline): enable `stderr.empty` to force “no stray stderr” so the
  only evidence is what you chose to capture.

`gates.json` also allows extra keys. You can record the current phase there as
human-visible metadata (not enforced, but shared):

```json
{
  "schema_version": "gates_v1",
  "triage": { "phase": "C-discipline" },
  "gates": { "enforced_checks": ["stderr.empty"] }
}
```

That is another “dial complication”: the gate file becomes both an enforcement
switch and a shared state marker for the dyad.

#### 5) Update the queue to be triage-able: sort by risk and confidence

Keep `triage.tsv`, but extend it with the risk and confidence lanes so it can
drive actual prioritization.

Example `jq` to extract a risk column and a low-confidence marker:

```bash
jq -r '
  def ids(prefix): ([.context.commitments[]? | select(.id | startswith(prefix)) | .id] | join(","));
  def any_id(re): ([.context.commitments[]? | select(.id | test(re)) | .id] | length) > 0;
  [
    .script.id,
    ((.operation.kind == "harness.supervised") | tostring),
    .result.outcome,
    ids("finding.risk."),
    (any_id("^finding\\.low_confidence$") | tostring),
    ids("recommend."),
    ids("finding."),
    (.result.details.message // "")
  ] | @tsv
' < second.ndjson > triage.tsv
```

Now you can triage by a simple rule that is closer to reality:

1. High risk first (`finding.risk.high`)
2. Within high risk: synthetic + low confidence first
3. Only then: route work (sudo, dependency installs, etc.)

This is the shift from “queue management” to “triage”: you are no longer just
sorting by the next action; you are allocating attention to reduce risk and
uncertainty fastest.

#### 6) Add one more run mode decision: supervised for inventory, strict for guard rails

Once you have boundaries-invariants and a gate phase, run in two passes:

- `--supervised` when you need the stream to stay intact (inventory + routing)
- strict when you want to enforce “no more regressions” for cooperating scripts

That combination is what makes triage affordable: supervised keeps visibility,
strict keeps discipline once you decide what discipline means.

### Step 10

>the compromise: two lenses

At this point it is easy for a dyad to drift into an argument that looks like a tooling debate:

- “the queue is enough; ship the TSV”
- “the queue is not triage; we need risk and confidence”

Treat this as a triage problem too: prioritize *what reduces evaluation uncertainty with the least coupling*.

The compromise is to keep two lenses over the same stream:

1. **Queue lens (route):** `recommend.*` answers “what might we do next?”
2. **Triage lens (risk/uncertainty):** coarse risk + low-confidence answers “where is it dangerous to guess?”

Neither lens needs to win. You can make progress while disagreeing by keeping each lens in its proper tool:

- **Commitments** carry the lenses (`recommend.*`, optional `finding.risk.*`, optional `finding.low_confidence`).
- **Boundaries** keep the stream comparable (and can enforce a *minimal* baseline for cooperating wrappers).
- **Gates** are phase switches, but only when you can afford them.
- **The shared message stream** is where you decide what counts as “high risk” and what heuristics are acceptable this week.

#### 1) Agree on what is enforced vs advisory (write it down in the channel)

For the next turn, pick a conservative contract that both agents can live with:

- Enforced (by schema): wrapper-emitted records must include `emit.record` and `policy.read_only`, and must emit *some* `recommend.*` (or `recommend.needs_human_read`).
- Advisory (by convention, not schema): risk and confidence markers may be present, but are not required yet.

This avoids a deadlock where one agent wants “policy in schema” immediately and the other wants “no policy in schema” indefinitely.

#### 2) Use a two-stage priority key that respects both lenses

When selecting what to touch next, apply a simple ordering that neither side has to love:

1. **High uncertainty first:** synthetic records, `no_record`, `stdout_garbage`, and anything routed to `recommend.needs_human_read`.
2. **Within that set, prefer high risk if you have it:** `finding.risk.high` (or whatever your current risk flare is).
3. **Only then follow the neat queue:** `recommend.*` routes like sudo/dependency/interactive.

This keeps the queue (route) as the work assignment surface, while letting triage (risk/uncertainty) steal attention when it matters.

#### 3) Make the disagreement productive with turn headers

At the start of each turn, post a short header into the shared message stream:

- `route_focus`: one `recommend.*` lane or one friction bucket
- `triage_focus`: one risk/uncertainty rule (e.g. “synthetic first”, “risk.high first”, “unknown first”)
- `script_ids`: the exact ids you will touch
- `contract_changes`: yes/no (and which file: commitments/boundaries/gates)

At the end of the turn, post:

- what moved buckets (e.g. “3 scripts moved from synthetic → emits_record”)
- any new flare ids introduced (so the other agent can add them to `commitments.json` or avoid inventing conflicting ones)

That is the missing link: the triad gives you tools-on-the-page; the channel gives you the shared judgment that triage requires.

### Step 11

>make the view cheap

The compromise only works if it is cheap to apply. Make “triage view” a
mechanical transformation of your latest inventory file.

Pick your latest NDJSON (`second.ndjson` in this example) and generate a TSV
that encodes the two-lens priority key in the first two columns:

- `uncertain` (`1` or `0`)
- `risk_score` (`2` high, `1` medium, `0` low/absent)

Then keep the rest of the columns as the human-facing payload: route(s), id,
outcome, synthetic/emitted, message.

If you have `jq`:

```bash
jq -r '
  def has_id(id): ([.context.commitments[]? | select(.id == id)] | length) > 0;
  def ids(prefix): ([.context.commitments[]? | select(.id | startswith(prefix)) | .id] | join(","));
  def risk_score:
    if has_id("finding.risk.high") then 2
    elif has_id("finding.risk.medium") then 1
    elif has_id("finding.risk.low") then 0
    else 0 end;
  def uncertain:
    (.operation.kind == "harness.supervised")
    or has_id("recommend.needs_human_read")
    or has_id("finding.low_confidence");
  [
    (uncertain | if . then "1" else "0" end),
    (risk_score | tostring),
    ids("recommend."),
    .script.id,
    .result.outcome,
    ((.operation.kind == "harness.supervised") | if . then "synthetic" else "emitted" end),
    (.result.details.message // "")
  ] | @tsv
' < second.ndjson > triage.tsv
```

Now the triage order is just a sort (uncertainty first, then risk if present,
then route/id as a stable queue):

```bash
sort -t $'\t' -k1,1nr -k2,2nr -k3,3 -k4,4 triage.tsv | column -t -s $'\t' | less -S
```

With this view, splitting work stays trivial:

- **High-uncertainty set:** everything with `uncertain=1`:

```bash
rg -n $'^1\t' triage.tsv | cut -f4
```

- **Route queue (lower uncertainty):** filter to `uncertain=0` and count routes:

```bash
rg -n $'^0\t' triage.tsv | cut -f3 | tr ',' '\n' | rg -v '^$' | sort | uniq -c | sort -nr
```

Nothing here “solves” triage. It just makes the compromise enforceable in
practice: every turn produces a new inventory, and every inventory yields the
same cheap triage view.

### Step 12

>deltas, not vibes

A clean triage view is only useful if you can watch it *change*.
Treat each run as an “inventory snapshot” and compare snapshots, not feelings.

#### 1) Start numbering inventories

Stop naming outputs `first`/`second`. Make a tiny time-series:

```bash
mkdir -p inventories/0001
fencerunner --supervised ./your-run-dir \
  > inventories/0001/stream.ndjson \
  2> inventories/0001/stream.stderr
```

Generate the triage view exactly as in Step 11, but write it into the snapshot:

Generate `inventories/0001/triage.tsv` using the exact same `jq` filter from
Step 11, but read from `inventories/0001/stream.ndjson` and write to
`inventories/0001/triage.tsv`.

Now also generate a *stable* key file for diffs (no messages, id-first):

```bash
awk -F $'\t' 'BEGIN{OFS="\t"} {print $4,$1,$2,$3,$5,$6}' inventories/0001/triage.tsv \
  | sort -t $'\t' -k1,1 \
  > inventories/0001/key.tsv
```

`inventories/0001/key.tsv` has one line per script:

```
id    uncertain    risk_score    routes    outcome    emitted|synthetic
```

#### 2) Run again after a turn

After either agent makes changes (wrappers, flare heuristics, triad tweaks), do
the next snapshot:

```bash
mkdir -p inventories/0002
fencerunner --supervised ./your-run-dir \
  > inventories/0002/stream.ndjson \
  2> inventories/0002/stream.stderr
```

Generate `inventories/0002/triage.tsv` using the exact same `jq` filter from
Step 11, but read from `inventories/0002/stream.ndjson` and write to
`inventories/0002/triage.tsv`.

Then generate the diff key:

```bash
awk -F $'\t' 'BEGIN{OFS="\t"} {print $4,$1,$2,$3,$5,$6}' inventories/0002/triage.tsv \
  | sort -t $'\t' -k1,1 \
  > inventories/0002/key.tsv
```

The invariant is the important part: *every* snapshot has the same four files.

#### 3) Produce a delta report

Join snapshots by `script.id` and ask only the questions triage cares about:

```bash
join -t $'\t' -1 1 -2 1 inventories/0001/key.tsv inventories/0002/key.tsv \
  > inventories/0002/delta.join.tsv
```

Columns are:

1. `id`
2–6. old snapshot fields
7–11. new snapshot fields

Now extract “what moved?”:

```bash
# synthetic -> emitted (wrapper progress)
awk -F $'\t' '$6=="synthetic" && $11=="emitted"{print $1}' inventories/0002/delta.join.tsv

# emitted -> synthetic (regression)
awk -F $'\t' '$6=="emitted" && $11=="synthetic"{print $1}' inventories/0002/delta.join.tsv

# route changed (queue changed)
awk -F $'\t' '$4!=$9{print $1 "\t" $4 " -> " $9}' inventories/0002/delta.join.tsv

# outcome changed
awk -F $'\t' '$5!=$10{print $1 "\t" $5 " -> " $10}' inventories/0002/delta.join.tsv
```

This is the discipline: every turn ends with a delta, and the dyad coordinates
in terms of deltas (“these ids moved from synthetic to emitted”) rather than in
terms of abstract taxonomy debates.

### Step 13

>TODO:PITH

TODO: CONTENT
