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

>make the lens a file

Step 11 and Step 12 are intentionally tool-driven, but copy/paste is brittle.
If the dyad’s “triage view” is not reproducible, you will end up debating
results instead of sharing them.

Make the view a first-class artifact.

#### 1) Extract the `jq` program into a file (`tools/triage.jq`)

Create a directory the runner will ignore (subdirectories are ignored; only
top-level `*.sh` are run):

```bash
mkdir -p tools
```

Create `tools/triage.jq` with the exact logic from Step 11:

```bash
cat > tools/triage.jq <<'EOF'
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
EOF
```

Now generating a triage view is always the same command:

```bash
jq -r -f tools/triage.jq < inventories/0002/stream.ndjson > inventories/0002/triage.tsv
```

This is the point: you don’t “follow Step 11” anymore — you *run the same lens*
every turn.

#### 2) Optional: make snapshot generation a one-liner (`tools/inventory`)

If you notice humans skipping Step 12 because it feels fiddly, automate it.
Keep it tiny and explicit: write the invariant files, and if there is a
previous snapshot, write the join delta.

Create `tools/inventory`:

```bash
cat > tools/inventory <<'EOF'
#!/bin/bash
set -euo pipefail

run_dir="${1:?usage: tools/inventory <RUN_DIR>}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
inventories_dir="${root_dir}/inventories"

mkdir -p "${inventories_dir}"

last="$(ls -1 "${inventories_dir}" 2>/dev/null | awk '/^[0-9]{4}$/{print}' | sort | tail -n1 || true)"
if [[ -z "${last}" ]]; then
  n=1
else
  n=$((10#${last} + 1))
fi
id="$(printf '%04d' "${n}")"
dir="${inventories_dir}/${id}"
mkdir -p "${dir}"

fencerunner --supervised "${run_dir}" > "${dir}/stream.ndjson" 2> "${dir}/stream.stderr"
jq -r -f "${script_dir}/triage.jq" < "${dir}/stream.ndjson" > "${dir}/triage.tsv"
awk -F $'\t' 'BEGIN{OFS="\t"} {print $4,$1,$2,$3,$5,$6}' "${dir}/triage.tsv" \
  | sort -t $'\t' -k1,1 \
  > "${dir}/key.tsv"

if [[ -n "${last}" ]]; then
  prev="${inventories_dir}/${last}"
  join -t $'\t' -1 1 -2 1 "${prev}/key.tsv" "${dir}/key.tsv" > "${dir}/delta.join.tsv"
fi

printf '%s\n' "${dir}"
EOF
chmod +x tools/inventory
```

Now “end every turn with an inventory” becomes:

```bash
tools/inventory ./your-run-dir
```

The only remaining human work is what you paste into the shared channel: the
delta lists from Step 12.

### Step 14

>paste a report

The dyad’s coordination problem is now simple: the tools can generate the same
inventory artifacts every time, but humans still have to *talk* about what
changed.

Make that discussion concrete by standardizing what gets pasted into the shared
channel. The goal is not prose; it is a compact, reproducible report that lets
the other agent update their mental model without re-running your commands.

#### 1) Add one more dumb tool: `tools/report`

`tools/inventory` creates snapshots. `tools/report` turns a snapshot (and its
delta, if present) into pasteable text.

Create `tools/report`:

```bash
cat > tools/report <<'EOF'
#!/bin/bash
set -euo pipefail

inv_dir="${1:?usage: tools/report <INVENTORY_DIR>}"

key="${inv_dir}/key.tsv"
triage="${inv_dir}/triage.tsv"
delta="${inv_dir}/delta.join.tsv"

if [[ ! -f "${key}" ]]; then
  echo "missing ${key}" >&2
  exit 1
fi

inv_id="$(basename "${inv_dir}")"

total="$(wc -l < "${key}" | tr -d ' ')"
emitted="$(awk -F $'\t' '$6=="emitted"{c++} END{print c+0}' "${key}")"
synthetic="$(awk -F $'\t' '$6=="synthetic"{c++} END{print c+0}' "${key}")"
uncertain="$(awk -F $'\t' '$2=="1"{c++} END{print c+0}' "${key}")"
risk_high="$(awk -F $'\t' '$3=="2"{c++} END{print c+0}' "${key}")"

echo "inventory: ${inv_id}"
echo "scripts: ${total} (emitted=${emitted}, synthetic=${synthetic})"
echo "uncertain: ${uncertain}  risk_high: ${risk_high}"

if [[ -f "${triage}" ]]; then
  echo
  echo "top routes:"
  cut -f4 "${key}" | tr ',' '\n' | grep -v '^$' | sort | uniq -c | sort -nr | head -n 10
fi

if [[ -f "${delta}" ]]; then
  echo
  echo "delta:"
  echo "- synthetic -> emitted:"
  awk -F $'\t' '$6=="synthetic" && $11=="emitted"{print "  - " $1}' "${delta}" || true
  echo "- emitted -> synthetic:"
  awk -F $'\t' '$6=="emitted" && $11=="synthetic"{print "  - " $1}' "${delta}" || true
  echo "- route changes:"
  awk -F $'\t' '$4!=$9{print "  - " $1 ": " $4 " -> " $9}' "${delta}" || true
  echo "- outcome changes:"
  awk -F $'\t' '$5!=$10{print "  - " $1 ": " $5 " -> " $10}' "${delta}" || true
fi
EOF
chmod +x tools/report
```

Now the “end of turn” paste is always the same:

```bash
inv="$(tools/inventory ./your-run-dir)"
tools/report "${inv}"
```

Paste the output into the shared channel *with* your turn header (Step 10). The
header names intent; the report names outcomes.

### Step 15

>one turn, one delta

You now have enough tooling to make the *process* contract-like, not just the
records.

Treat each turn as a small experiment with an observable outcome. The experiment
is the work; the outcome is the delta.

#### 1) Start each turn by picking ids from the triage view (not from memory)

Look at the highest-priority lines (uncertain first, then risk if present, then
the route queue):

```bash
sort -t $'\t' -k1,1nr -k2,2nr -k3,3 -k4,4 triage.tsv | head -n 20
```

Pick a small set of script ids for this turn (1–3 is plenty at first):

```bash
sort -t $'\t' -k1,1nr -k2,2nr -k3,3 -k4,4 triage.tsv | head -n 3 | cut -f4
```

Post your Step 10 header with those ids before you touch anything. This keeps
the dyad from duplicating work and makes intent falsifiable.

#### 2) Choose one work type per turn

Keep the work atomic. Pick one move and apply it to the ids you chose:

- **wrap** (`synthetic` / `no_record` / `stdout_garbage`): do Step 5 (wrapper) so the script becomes `emitted`.
- **route** (`emitted` but no `recommend.*`): emit `recommend.needs_human_read` as the honest default, or add one flare that justifies a better route.
- **raise signal** (`recommend.needs_human_read` dominates): do Step 6 (signal flares) so uncertainty shrinks without pretending to fully understand.
- **ratchet baseline** (only when the dyad agrees): enforce the minimal wrapper baseline in `boundaries.json` (Step 10), but keep risk/confidence advisory.

The key is that you are not “improving the system”. You are making one specific
change whose effect you can see in the next snapshot.

#### 3) End the turn with an inventory + report, and paste it

Run the inventory tool and paste the report output into the channel:

```bash
inv="$(tools/inventory ./your-run-dir)"
tools/report "${inv}"
```

Do not summarize it. Paste it.

#### 4) Interpret progress only through deltas

A turn is “good” if it produces one of these deltas:

- at least one id moves `synthetic -> emitted`
- uncertainty decreases for an id you touched (`uncertain=1 -> 0`)
- a vague route becomes a concrete one (`recommend.needs_human_read -> recommend.*`)

A turn is “not done” if it produces a regression you caused (`emitted ->
synthetic`, route flips you didn’t intend, outcomes that flip unexpectedly).
Fix or revert before handing off, then regenerate the inventory and paste the
new report.

This is the whole move: the queue stays simple, the triad stays decoupled, and
the dyad coordinates through observable deltas rather than taste.

### Step 16

>strict islands

Supervised mode is for *inventory*: keep the NDJSON stream intact even when
scripts are messy. Strict mode is for *non-negotiables*: if the contract is
broken, stop.

You do not have to choose one posture for the whole bag. If you split the bag
into multiple run dirs and run `fencerunner` separately per subset, you can
apply strict where it helps and supervised where it’s still needed.

This is a practical way to create “islands of certainty” inside a chaotic bag.

#### 1) Split by maturity, not by meaning

Make two run dirs:

- `frontier/` (supervised): the messy majority; the goal is visibility.
- `trusted/` (strict): scripts you have already wrapped into cooperating emitters;
  the goal is stability.

Promotion is simple and mechanical: when a script is wrapped such that it always
emits exactly one schema-valid record and exits `0`, it becomes eligible for
`trusted/`.

This does *not* mean you understand what the script does. It means you can rely
on its behavior as a producer of data.

#### 2) Strict reduces blast radius (by failing fast)

In strict mode, the runner stops at the first script-level contract break. That
changes the posture: “something surprising happened” becomes “halt further
execution”.

This is not a security boundary, but it is still useful when hazard means “cost
of evaluation uncertainty”: strict makes it easy to stop running deeper into an
unknown bag when the first few scripts already show you that your assumptions
don’t hold.

#### 3) Strict forces the interface to be NDJSON, not process exit codes

In this runner, strict treats non-zero process exits as failures. The intended
pattern for cooperating scripts is:

- script process exits `0`
- script reports success/denied/partial/error in `result.outcome`
- evidence lives in payload snippets and `result.details.exit_code` / messages

That is a big triage lever: once a subset stays green under strict, downstream
tools can treat the NDJSON as the only interface and stop smuggling semantics
through shell control-flow.

Wrappers are the easiest way to make this true: capture the legacy exit code
and report it in the record, but keep the wrapper’s own exit code `0` unless
`emit-record` fails.

#### 4) Strict makes “policy as schema” bite (when you’re ready)

Supervised will happily emit synthetic records when your boundaries contract
tightens and wrappers drift. That is good for visibility, but it can also hide
the moment when your “cooperating baseline” stopped being true.

When you promote a subset into `trusted/` and run it under strict, tightening
`boundaries.json` becomes a real gate: drift is an immediate, undeniable
failure, not just another synthetic line among many.

This is the moment where a baseline like:

- “wrapper-emitted records must include `emit.record` and `policy.read_only`”
- “must emit exactly one `recommend.*` (or `recommend.needs_human_read`)”

stops being aspirational and becomes enforced.

#### 5) Two-pass pattern: strict guard rail + supervised frontier

A practical cadence that keeps artifacts honest:

1. Run strict on `trusted/`. If it fails, fix or revert before you accept any
   new inventory.
2. Run supervised on `frontier/` to keep the queue moving.

One caveat: strict can emit some records before failing fast. If you don’t want
automation to accidentally consume a partial strict stream, capture atomically:

```bash
tmp="$(mktemp)"
if fencerunner --strict ./trusted > "${tmp}" 2> strict.stderr; then
  mv "${tmp}" inventories/0002/trusted.ndjson
else
  rm -f "${tmp}"
  echo "strict failed; not publishing partial output" >&2
fi
```

The promotion path is now visible in your deltas: scripts move from “harness
rescued” → “wrapper-emitted” → “strict-green in trusted”. Over time, more of the
corpus lives behind strict, and supervised shrinks to the true frontier—exactly
where triage belongs.

### Step 17

>strict in the workflow

Strict becomes a phase change when it is not “something you sometimes run”, but
a **publish gate** in the same inventory/report workflow you already use.

The requirement is simple:

- supervised inventories are always publishable (they are meant for visibility)
- strict inventories are publishable **only when strict is green** (they are meant for integrity)

#### 1) Upgrade `tools/inventory` with a strict mode and a root

Replace `tools/inventory` with a version that supports:

- `--supervised` (default)
- `--strict` (publish only on success; no partial stream)
- `--root <DIR>` (so trusted and frontier can have separate time-series)

```bash
cat > tools/inventory <<'EOF'
#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: tools/inventory [--supervised|--strict] [--root DIR] <RUN_DIR>" >&2
}

mode="supervised"
root_override=""

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --supervised) mode="supervised"; shift ;;
    --strict) mode="strict"; shift ;;
    --root) root_override="${2:?missing value for --root}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) echo "unknown flag: ${1}" >&2; usage; exit 1 ;;
    *) break ;;
  esac
done

run_dir="${1:?missing RUN_DIR}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"
inventories_dir="${root_override:-${root_dir}/inventories}"

mkdir -p "${inventories_dir}"

last="$(ls -1 "${inventories_dir}" 2>/dev/null | awk '/^[0-9]{4}$/{print}' | sort | tail -n1 || true)"
if [[ -z "${last}" ]]; then
  n=1
else
  n=$((10#${last} + 1))
fi
id="$(printf '%04d' "${n}")"
dir="${inventories_dir}/${id}"
mkdir -p "${dir}"

if [[ "${mode}" == "supervised" ]]; then
  fencerunner --supervised "${run_dir}" > "${dir}/stream.ndjson" 2> "${dir}/stream.stderr"
else
  tmp_stream="$(mktemp)"
  tmp_stderr="$(mktemp)"
  if fencerunner --strict "${run_dir}" > "${tmp_stream}" 2> "${tmp_stderr}"; then
    mv "${tmp_stream}" "${dir}/stream.ndjson"
    mv "${tmp_stderr}" "${dir}/stream.stderr"
  else
    echo "strict failed; not publishing partial output" >&2
    cat "${tmp_stderr}" >&2 || true
    rm -f "${tmp_stream}" "${tmp_stderr}"
    rm -rf "${dir}"
    exit 1
  fi
fi

jq -r -f "${script_dir}/triage.jq" < "${dir}/stream.ndjson" > "${dir}/triage.tsv"
awk -F $'\t' 'BEGIN{OFS="\t"} {print $4,$1,$2,$3,$5,$6}' "${dir}/triage.tsv" \
  | sort -t $'\t' -k1,1 \
  > "${dir}/key.tsv"

if [[ -n "${last}" ]]; then
  prev="${inventories_dir}/${last}"
  join -t $'\t' -1 1 -2 1 "${prev}/key.tsv" "${dir}/key.tsv" > "${dir}/delta.join.tsv"
fi

printf '%s\n' "${dir}"
EOF
chmod +x tools/inventory
```

This is the strict “keep it honest” move:

- if strict fails, you get an error and no snapshot is published
- if strict succeeds, you get the same invariant files as supervised

#### 2) Use two inventory roots: `trusted` and `frontier`

Run strict inventories for your island of certainty:

```bash
trusted_inv="$(tools/inventory --root inventories/trusted --strict ./trusted)"
tools/report "${trusted_inv}"
```

Run supervised inventories for the chaotic frontier:

```bash
frontier_inv="$(tools/inventory --root inventories/frontier --supervised ./frontier)"
tools/report "${frontier_inv}"
```

Always run `trusted` first. If it fails, stop: your integrity gate is telling
you “don’t publish a new turn; fix drift first”.

#### 3) Paste two reports, not one story

End-of-turn channel update (header from Step 10 + reports from Step 14):

- `trusted` report: proves the island stayed green under strict
- `frontier` report: shows the queue moved under supervised

This is the phase change in practice: strict is no longer a flag. It is the
thing that decides whether you are allowed to publish a new inventory.

Schema note: keep enforcement minimal for now (`emit.record` + `policy.read_only`
+ some `recommend.*` / `recommend.needs_human_read` for wrapper records). Strict
is what makes that baseline bite without dragging risk/confidence into “required”
too early.

### Step 18

>promote to trusted

Separating `frontier/` and `trusted/` is only useful if scripts can move between
them without ceremony. Treat this as a mechanical promotion path, not an honor
system.

#### 1) Split on disk (once)

If you have been working in a single run dir so far, make it your frontier and
create an empty trusted run dir:

```bash
mv ./your-run-dir ./frontier
mkdir -p ./trusted
cp ./frontier/{gates.json,commitments.json,boundaries.json} ./trusted/
```

Start with `trusted/` empty (no `*.sh`), and treat promotion as the only way
scripts enter it.

#### 2) Promote only what you can keep green under strict

Pick candidates from the *frontier* inventory, not by gut. A simple rule that
usually works:

- `emitted` (not synthetic)
- `uncertain=0`
- route is present and not just `recommend.needs_human_read`

From the latest `inventories/frontier/NNNN/key.tsv`:

```bash
awk -F $'\t' '$2=="0" && $6=="emitted" && $4!="" && $4!="recommend.needs_human_read"{print $1}' \
  inventories/frontier/0002/key.tsv
```

Promotion move (keep the wrapper and its legacy body together):

```bash
id="foo"
mv "./frontier/${id}.sh" "./trusted/${id}.sh"
mv "./frontier/${id}.legacy" "./trusted/${id}.legacy"
```

Then immediately run the strict publish gate:

```bash
trusted_inv="$(tools/inventory --root inventories/trusted --strict ./trusted)"
tools/report "${trusted_inv}"
```

If strict fails, undo the move (or fix drift) before you continue. The whole
point of trusted is “this subset stays green”.

#### 3) Keep enforcement minimal (for now)

In `trusted/`, strict makes a *minimal* wrapper baseline bite without forcing
you to prematurely standardize everything:

- require `emit.record` + `policy.read_only`
- require *some* `recommend.*` (or `recommend.needs_human_read`)
- keep risk/confidence signals optional until the dyad agrees otherwise

The strict island is about integrity, not about completing the taxonomy.

#### 4) Talk in promotions + deltas

In the shared channel, record promotions explicitly:

- ids promoted (frontier → trusted)
- whether trusted strict stayed green
- what changed in the frontier delta

This keeps the dyad aligned: supervised inventories are where uncertainty lives;
strict inventories are where drift is not allowed.

### Step 19

>make promotion show up

Promotion is now a file move plus an immediate strict gate (Step 18). That makes
`trusted/` operational.

One problem remains: the delta join in Step 12 only compares ids that exist in
both snapshots. Promotions and demotions are *adds/removes*, so they can vanish
from `delta.join.tsv` unless you call them out manually.

Fix that by making “added ids” and “removed ids” first-class delta artifacts.

#### 1) Teach `tools/inventory` to write added/removed deltas

Update the delta section of `tools/inventory` so that when a previous snapshot
exists it writes three delta files:

- `delta.join.tsv` (changed fields for ids present in both)
- `delta.added.tsv` (ids present now, absent before)
- `delta.removed.tsv` (ids absent now, present before)

In the script, extend the existing `if [[ -n "${last}" ]]; then ... fi`:

```bash
if [[ -n "${last}" ]]; then
  prev="${inventories_dir}/${last}"
  join -t $'\t' -1 1 -2 1 "${prev}/key.tsv" "${dir}/key.tsv" > "${dir}/delta.join.tsv"

  comm -13 <(cut -f1 "${prev}/key.tsv" | sort) <(cut -f1 "${dir}/key.tsv" | sort) \
    > "${dir}/delta.added.tsv"

  comm -23 <(cut -f1 "${prev}/key.tsv" | sort) <(cut -f1 "${dir}/key.tsv" | sort) \
    > "${dir}/delta.removed.tsv"
fi
```

Now promotions are visible as deltas:

- **frontier:** promoted scripts show up in `delta.removed.tsv`
- **trusted:** promoted scripts show up in `delta.added.tsv`

That turns “promotion” from a narrative you remember to a file your tools can
print.

#### 2) Teach `tools/report` to print added/removed ids

Update `tools/report` so that when `delta.added.tsv` or `delta.removed.tsv`
exist it prints them.

For example, after the existing “delta:” block header:

```bash
added="${inv_dir}/delta.added.tsv"
removed="${inv_dir}/delta.removed.tsv"

if [[ -f "${added}" ]]; then
  echo "- added ids:"
  awk '{print "  - " $1}' "${added}" || true
fi

if [[ -f "${removed}" ]]; then
  echo "- removed ids:"
  awk '{print "  - " $1}' "${removed}" || true
fi
```

With that change, your end-of-turn paste can carry promotions/demotions without
extra explanation.

#### 3) Make “promotion” a delta rule, not a debate

Once added/removed ids are printed by the reports, you can make a simple rule:

- a promotion only “counts” if the *trusted strict* inventory exists (Step 17)
- and the trusted report shows the id under “added ids”

That keeps “trusted growth” measurable.

### Step 20

>one command, one paste

At this point your process has two run dirs, two postures, two inventory roots,
and (thanks to Step 19) promotions show up mechanically as added/removed ids.

That’s great — but it is still easy for a dyad to “communicate” by accident:
one agent pastes only frontier, or labels things inconsistently, or forgets to
stop the line when trusted strict fails.

Make the end-of-turn paste a single command.

#### 1) Add a tiny orchestrator: `tools/turn`

Create `tools/turn` that runs the strict publish gate first, then the frontier
inventory, and prints both reports with explicit labels:

```bash
cat > tools/turn <<'EOF'
#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

trusted_inv="$("${script_dir}/inventory" --root inventories/trusted --strict ./trusted)"
echo "== trusted =="
echo "inventory_dir: ${trusted_inv}"
"${script_dir}/report" "${trusted_inv}"
echo

frontier_inv="$("${script_dir}/inventory" --root inventories/frontier --supervised ./frontier)"
echo "== frontier =="
echo "inventory_dir: ${frontier_inv}"
"${script_dir}/report" "${frontier_inv}"
EOF
chmod +x tools/turn
```

Now your end-of-turn artifacts are generated the same way every time:

```bash
tools/turn
```

If trusted strict fails, `tools/turn` fails (and `tools/inventory` will refuse
to publish a partial snapshot), so you cannot accidentally “keep going” into a
new frontier inventory with a broken island.

#### 2) Paste the whole output (don’t curate it)

In the shared channel, paste your Step 10 header *and then paste the full
output of `tools/turn`*. Let the tools tell the story; keep prose out of it.

That is the operational definition of the compromise: tools-on-the-page for
what happened, discussion-in-channel for what it means.

### Step 21

>intent is an artifact

If it matters, it becomes an artifact.

Step 20 made the end-of-turn paste hard to forget, but your intent (Step 10
header) is still soft: it lives only in chat.

Make intent a file, and make `tools/turn` refuse to run without it.

#### 1) Require the Step 10 header as flags

Replace `tools/turn` with a version that requires four fields and prints them
before it runs anything. Flags are canonical; env vars are accepted as a
fallback, but the tool will warn when it uses them.

- `--route-focus`
- `--triage-focus`
- `--script-ids`
- `--contract-changes`

```bash
cat > tools/turn <<'EOF'
#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: tools/turn \\" >&2
  echo "  --route-focus <TEXT> \\" >&2
  echo "  --triage-focus <TEXT> \\" >&2
  echo "  --script-ids <TEXT> \\" >&2
  echo "  --contract-changes <TEXT>" >&2
  echo "" >&2
  echo "fallback (discouraged): set env vars ROUTE_FOCUS, TRIAGE_FOCUS, SCRIPT_IDS, CONTRACT_CHANGES" >&2
}

route_focus=""
triage_focus=""
script_ids=""
contract_changes=""

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --route-focus) route_focus="${2:?missing value for --route-focus}"; shift 2 ;;
    --triage-focus) triage_focus="${2:?missing value for --triage-focus}"; shift 2 ;;
    --script-ids) script_ids="${2:?missing value for --script-ids}"; shift 2 ;;
    --contract-changes) contract_changes="${2:?missing value for --contract-changes}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) echo "unknown flag: ${1}" >&2; usage; exit 1 ;;
    *) echo "unexpected arg: ${1}" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${route_focus}" && -n "${ROUTE_FOCUS:-}" ]]; then
  route_focus="${ROUTE_FOCUS}"
  echo "warning: using ROUTE_FOCUS env var; prefer --route-focus" >&2
fi

if [[ -z "${triage_focus}" && -n "${TRIAGE_FOCUS:-}" ]]; then
  triage_focus="${TRIAGE_FOCUS}"
  echo "warning: using TRIAGE_FOCUS env var; prefer --triage-focus" >&2
fi

if [[ -z "${script_ids}" && -n "${SCRIPT_IDS:-}" ]]; then
  script_ids="${SCRIPT_IDS}"
  echo "warning: using SCRIPT_IDS env var; prefer --script-ids" >&2
fi

if [[ -z "${contract_changes}" && -n "${CONTRACT_CHANGES:-}" ]]; then
  contract_changes="${CONTRACT_CHANGES}"
  echo "warning: using CONTRACT_CHANGES env var; prefer --contract-changes" >&2
fi

if [[ -z "${route_focus}" || -z "${triage_focus}" || -z "${script_ids}" || -z "${contract_changes}" ]]; then
  echo "missing required header fields" >&2
  usage
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

header="$(cat <<HDR
route_focus: ${route_focus}
triage_focus: ${triage_focus}
script_ids: ${script_ids}
contract_changes: ${contract_changes}
HDR
)"

echo "== intent =="
echo "${header}"
echo

trusted_inv="$("${script_dir}/inventory" --root inventories/trusted --strict ./trusted)"
printf '%s\n' "${header}" > "${trusted_inv}/turn.header.txt"
echo "== trusted =="
echo "inventory_dir: ${trusted_inv}"
"${script_dir}/report" "${trusted_inv}"
echo

frontier_inv="$("${script_dir}/inventory" --root inventories/frontier --supervised ./frontier)"
printf '%s\n' "${header}" > "${frontier_inv}/turn.header.txt"
echo "== frontier =="
echo "inventory_dir: ${frontier_inv}"
"${script_dir}/report" "${frontier_inv}"
EOF
chmod +x tools/turn
```

Now intent is coupled to outcomes:

- it is printed in the same paste as the reports
- it is written into both inventory dirs as `turn.header.txt`

If someone later asks “what did you mean to do?”, the answer is not “scroll up
in chat”. It is “open the inventory directory”.

#### 2) Run one command, paste one thing

Your end-of-turn command becomes:

```bash
tools/turn \
  --route-focus "recommend.install_dependency" \
  --triage-focus "uncertain first" \
  --script-ids "foo bar baz" \
  --contract-changes "commitments.json (added finding.tool_missing)"
```

Paste the entire output. No separate Step 10 header is required anymore,
because the tool refuses to run without it.

### Step 22

>one command, one delta

Promotion and demotion are now part of your *operational* contract: they change
what is behind strict, and they change what “trusted” means.

So treat them like the rest of this runbook: make them one-command moves that
immediately run the trusted strict publish gate.

#### 1) Add `tools/promote` (frontier → trusted + strict gate)

Create `tools/promote`:

```bash
cat > tools/promote <<'EOF'
#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: tools/promote --id <SCRIPT_ID>" >&2
}

id=""

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --id) id="${2:?missing value for --id}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) echo "unknown flag: ${1}" >&2; usage; exit 1 ;;
    *) echo "unexpected arg: ${1}" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${id}" ]]; then
  usage
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"

frontier_dir="${root_dir}/frontier"
trusted_dir="${root_dir}/trusted"

src_sh="${frontier_dir}/${id}.sh"
src_legacy="${frontier_dir}/${id}.legacy"
dst_sh="${trusted_dir}/${id}.sh"
dst_legacy="${trusted_dir}/${id}.legacy"

if [[ ! -f "${src_sh}" ]]; then
  echo "missing ${src_sh}" >&2
  exit 1
fi

if [[ ! -f "${src_legacy}" ]]; then
  echo "missing ${src_legacy}" >&2
  exit 1
fi

if [[ -e "${dst_sh}" || -e "${dst_legacy}" ]]; then
  echo "destination already exists for id=${id}" >&2
  exit 1
fi

mv "${src_sh}" "${dst_sh}"
mv "${src_legacy}" "${dst_legacy}"

if ! trusted_inv="$("${script_dir}/inventory" --root inventories/trusted --strict "${trusted_dir}")"; then
  echo "strict failed after promotion; rolling back id=${id}" >&2
  mv "${dst_sh}" "${src_sh}"
  mv "${dst_legacy}" "${src_legacy}"
  exit 1
fi

echo "== trusted (after promote id=${id}) =="
echo "inventory_dir: ${trusted_inv}"
"${script_dir}/report" "${trusted_inv}"
EOF
chmod +x tools/promote
```

Now a promotion is one command, and it either succeeds under strict or rolls
back:

```bash
tools/promote --id foo
```

#### 2) Add `tools/demote` (trusted → frontier + strict gate)

Create `tools/demote`:

```bash
cat > tools/demote <<'EOF'
#!/bin/bash
set -euo pipefail

usage() {
  echo "usage: tools/demote --id <SCRIPT_ID>" >&2
}

id=""

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --id) id="${2:?missing value for --id}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) echo "unknown flag: ${1}" >&2; usage; exit 1 ;;
    *) echo "unexpected arg: ${1}" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${id}" ]]; then
  usage
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "${script_dir}/.." && pwd)"

frontier_dir="${root_dir}/frontier"
trusted_dir="${root_dir}/trusted"

src_sh="${trusted_dir}/${id}.sh"
src_legacy="${trusted_dir}/${id}.legacy"
dst_sh="${frontier_dir}/${id}.sh"
dst_legacy="${frontier_dir}/${id}.legacy"

if [[ ! -f "${src_sh}" ]]; then
  echo "missing ${src_sh}" >&2
  exit 1
fi

if [[ ! -f "${src_legacy}" ]]; then
  echo "missing ${src_legacy}" >&2
  exit 1
fi

if [[ -e "${dst_sh}" || -e "${dst_legacy}" ]]; then
  echo "destination already exists for id=${id}" >&2
  exit 1
fi

mv "${src_sh}" "${dst_sh}"
mv "${src_legacy}" "${dst_legacy}"
trusted_inv="$("${script_dir}/inventory" --root inventories/trusted --strict "${trusted_dir}")"

echo "== trusted (after demote id=${id}) =="
echo "inventory_dir: ${trusted_inv}"
"${script_dir}/report" "${trusted_inv}"
EOF
chmod +x tools/demote
```

Now demotion is also one command (and it immediately publishes a strict snapshot
if trusted is green):

```bash
tools/demote --id foo
```

### Step 23

>promote without meetings

You now have the mechanics to *maintain* `trusted/` (strict publish gate) and to
*move* scripts (`tools/promote` / `tools/demote`). What you still need is the
default motion that grows the strict island without a committee meeting.

The posture is: **trusted growth is the default outcome of the process**, not a
social decision.

So: promote because the frontier inventory makes it obvious, not because you
“feel done” with a script.

#### 1) Define “boring enough to promote” as a filter over `key.tsv`

Use the same criteria as Step 18 (frontier says it is already behaving like
data, and it is operationally legible):

- `emitted` (not synthetic)
- `uncertain=0`
- has a real route (non-empty, not only `recommend.needs_human_read`)

That’s a single line in the frontier snapshot key:

```bash
awk -F $'\t' '
  $2=="0" && $6=="emitted" && $4!="" && $4!="recommend.needs_human_read" {
    print $1 "\t" $4 "\t" $5 "\t" "risk=" $3
  }
' inventories/frontier/0002/key.tsv
```

This is a promotion candidate list: `id`, `routes`, `outcome`, and `risk_score`
(still advisory).

Default rule: if the frontier snapshot yields candidates, promoting one is the
next “boring” move. If you choose not to promote, make that choice explicit in
your intent header (why the strict island shouldn’t grow this turn).

#### 2) Make candidates show up in the frontier report (optional, but reduces friction)

If promotions are “supposed to happen”, make them visible where you already
look: the output of `tools/turn`.

Update `tools/report` to print a small promotion candidate section when the
inventory dir is under `inventories/frontier/`:

```bash
# At the end of tools/report (after the delta block), add:
if [[ "${inv_dir}" == *"/inventories/frontier/"* ]]; then
  echo
  echo "promotion candidates (frontier -> trusted):"
  awk -F $'\t' '$2=="0" && $6=="emitted" && $4!="" && $4!="recommend.needs_human_read"{print "  - " $1 "\t" $4 "\t" $5 "\t" "risk=" $3}' "${key}" \
    | head -n 10 || true
fi
```

Now every end-of-turn paste includes (a) what changed and (b) what is ready to
be promoted next, without anyone remembering a separate command.

#### 3) Promotion is a work type (and should show up in your intent header)

When you promote, treat it as the turn’s work. Don’t sneak it in.

Run:

```bash
tools/promote --id foo
```

Then run your normal end-of-turn command with intent that names the promotion
and publishes *both* inventories:

```bash
tools/turn \
  --route-focus "promote" \
  --triage-focus "grow strict island" \
  --script-ids "foo" \
  --contract-changes "promoted foo (frontier -> trusted)"
```

The counting rule is simple: **if the promotion doesn’t show up as deltas in the
two reports, it doesn’t count**.

Thanks to Step 19, the two reports will show this mechanically:

- trusted: `added ids: foo`
- frontier: `removed ids: foo`

That is “operational trusted”: growth is visible as deltas, not as vibes.

#### 4) Demotion is an escape hatch, not a habit

If trusted strict fails because a script can’t stay green under your minimal
baseline, you have two honest options:

- fix drift (preferred)
- demote the script back to frontier so strict stays meaningful

Use:

```bash
tools/demote --id foo
```

Then run `tools/turn` with a header that names the demotion. The island stays
green; the frontier regains the uncertainty.

### Step 24

>touch more scripts

If this is starting to feel like ceremony, treat the *process* as the thing
that needs triage.

The scaffolding above exists for one reason: to let you touch more unknown
scripts safely **today**, not to produce prettier receipts tomorrow.

So do the ruthless loop.

#### 1) Pick a small batch from the frontier triage view

Run `tools/turn` (with whatever intent header is true) and copy the printed
frontier `inventory_dir` into a variable:

```bash
frontier_inv="inventories/frontier/0007" # from tools/turn output
```

Now pick the next few unknowns from the top of the triage order:

```bash
sort -t $'\t' -k1,1nr -k2,2nr -k3,3 -k4,4 "${frontier_inv}/triage.tsv" \
  | head -n 5 \
  | column -t -s $'\t'
```

Choose 2–3 `script.id` values from that list (don’t overthink it). The goal is
to move uncertainty, not to perfectly prioritize.

#### 2) Wrap first, flare second (two flares max)

For each chosen id:

1. If it is still `synthetic`, wrap it (Step 5): make it emit exactly one
   record with captured stdout/stderr.
2. Add **at most two** flares (Step 6), only when evidence is strong. Keep the
   first flares painfully boring:
   - permission barrier → `outcome=denied`, `recommend.rerun_sudo`
   - missing tool/runtime → `recommend.install_dependency`
   - interactive prompt markers → `recommend.run_interactively`
   - otherwise → `recommend.needs_human_read` (honest default)

If you are tempted to add new commitment ids to explain nuance, don’t. Route it
to `recommend.needs_human_read` and move on.

#### 3) Re-run and move on (don’t polish)

End the turn by running `tools/turn` again and pasting the full output. Your
only question is: did any of the ids you touched show up as deltas (Step 12/19)
in the two reports?

A good turn is one where at least one of these becomes true:

- `synthetic -> emitted`
- `uncertain=1 -> 0`
- `recommend.needs_human_read -> recommend.*`

If none of those happen, triage the process: you are spending attention
somewhere that is not reducing evaluation uncertainty. Cut the overhead and
touch the next scripts anyway.

### Step 25

>TODO:PITH

TODO: CONTENT
