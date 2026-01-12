# Problem Space: Why This Took So Long (and What Changed)

This document expands on the short answer given in-channel. It’s written as a
systems / workflow post‑mortem for the exercise, not as a blame assignment.

## What We Were Actually Doing

We were not “fixing a few scripts”. We were trying to take an inherited *bag* of
unknown Bash scripts and turn it into something that can be worked on
mechanically:

- Every script run yields **one schema-valid boundary record** (NDJSON) that
  downstream tools can consume deterministically.
- “Bad behavior” (extra stdout, missing record, hang, interactive prompt,
  privilege requests, etc.) becomes **data**, not a broken run.
- The bag becomes triageable because we can **count** failure classes and move
  those counts.
- We can run the corpus *safely enough* to learn from it without accidental
  self-harm.

That is a three-way constraint problem: **integrity** (records are real and
validated), **safety** (unknown code), and **throughput** (touch more of the bag
per unit time).

## Why It Took So Long (Expanded)

### 1) The Unit of Work Was Wrong at First

The default instinct—especially with cooperative tooling like fencerunner—is to
pick a script, wrap it, get it to emit, move on.

That produces *linear* progress:

- N scripts means N micro-decisions (how to wrap, what to emit, how to label).
- N scripts means N chances to overfit a wrapper to a one-off failure.
- Coordination is expensive because every script becomes a separate discussion.

Triage does not want N decisions. It wants **a small number of classes**:

> “There are 5 scripts that require Bash 4.”  
> “There are 2 scripts that hang.”  
> “There is 1 script that pushes to git.”

Once we shifted to “reduce the top failure classes across the corpus per turn”,
the work stopped being N manual patches and became a small number of bulk
sweeps.

### 2) We Grew the Coordination Surface Faster Than We Reduced the Bag

The early runbook work in `docs/EXAMPLES.md` was useful (it clarified the
contracts and the idea of a queue), but the exercise drifted into a failure mode:

- We built better and better “paperwork” for describing what we *meant* to do
  (headers, reports, inventories, deltas, promotions).
- Meanwhile, the corpus stayed largely unmoved.

This is a recognizable trap in agentic work: you can always justify adding “one
more guardrail” because guardrails feel like progress. But if the guardrails
aren’t directly coupled to moving a measured queue, they are ceremony.

The turning point was treating “new process work” as itself triageable: if the
process doesn’t help touch more scripts safely *today*, it is the thing that
needs triage.

### 3) We Didn’t Start With a Canonical, Empirical Queue

Early on, we had output streams and ad hoc observations, but not a single,
stable, batch-derived queue artifact.

Without a queue like `post/classes.tsv`, it’s impossible to enforce the central
rule:

> Every change must move a count.

In the absence of that, debate expands (taxonomy arguments, “is this triage?”,
etc.) because there is no shared scoreboard.

### 4) Safety Debt Had to Be Paid Before Speed Was Ethical

“Unknown scripts” is not just uncertainty; it’s *risk*:

- interactive prompts that hang automation
- implicit privilege escalation (`sudo`)
- destructive primitives (`rm -rf`, disk tools, `dd if=`)
- network primitives (`curl`, `git push`, `ssh`)

You can’t responsibly batch-run a corpus until you can prevent “one bad legacy”
from wedging or damaging the environment. That required:

- a **pre-exec hazard scan** (static grep over `*.legacy`)
- a **quarantine mechanism** that produces a record without executing
- **timeouts + kill escalation** so hangs can’t stall the batch runner
- gates that stop the line on backsliding (synthetics, quarantine violations,
  undeclared enrollments)

That safety infrastructure is real work. The mistake was not doing it; the
mistake was not building it as quickly and minimally as possible, tied to the
batch queue.

### 5) “Cooperating System” Work Is Front-Loaded

fencerunner’s model is contract-heavy by design: schemas, runtime validation,
and tests exist to force explicitness. That’s a feature, but it means you do
up-front work before you get exponential leverage:

- define the minimal record baseline
- generate wrappers consistently
- generate commitments registries consistently
- enforce that what you emit is what you declared

Once those invariants were enforced, later work became cheap: remove the hazard
or dependency at the source, rerun once, watch the class drop.

### 6) Bash 3.2 + Unknown Legacy Scripts Add Friction You Don’t See Up Front

This repo’s posture (macOS Bash 3.2, offline Rust builds, real scripts in tests)
is intentionally constraining. That’s good for determinism, but it means:

- “small” wrapper logic must be written in a conservative, portable style
- many inherited scripts silently assume Bash 4 features
- a naive timeout strategy can leave orphaned children or kill the wrapper
  itself if you accidentally signal your own process group

None of that is glamorous, but until it’s handled, you can’t batch-run safely.

## What Changed (Why It Suddenly Got Fast)

The moment we converged on these three primitives, throughput increased sharply:

1) **One entrypoint** (`scratch/inheritance/triage/triage.sh`) that always:
   - scans hazards
   - runs the whole corpus
   - produces canonical artifacts
   - enforces gates

2) **One queue artifact** (`post/classes.tsv`) that is:
   - stable
   - count-based
   - class-oriented
   - the only allowed worklist

3) **One bulk actuation surface** (`rules.json` + generated wrappers) so a “fix”
   is a rules change (or a legacy defang), not 40 manual wrapper edits.

After that, disagreements became cheap (they were just policy choices), and
progress became measurable (counts moved).

## What “Done” Meant (and Why It Matters)

We ended with a concrete definition of done that prevented endless debate:

- `pre/hazards.tsv` is empty (no detectable hazard patterns remain)
- `post/classes.tsv` has exactly one row: `legacy.exec success = N`
- frozen process: no changes unless a new non-success row appears

Without an explicit done condition, “triage” can become an infinite appetite for
more instrumentation. With it, the system becomes maintainable: new issues show
up as new class rows, and you respond surgically.

## If We Did This Again

If we repeated the exercise, the main change would be *when* we pivoted:

- Start by building the batch queue and hazard quarantine on day 1.
- Use docs/runbook work only insofar as it directly enables that batch queue.
- Treat per-script wrapper tweaks as a smell; demand class sweeps immediately.

The deeper lesson is that agentic coding work needs a progress primitive that is
hard to “storytell around”. Here, `post/classes.tsv` filled that role.

