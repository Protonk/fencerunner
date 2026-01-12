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

## Notes After Reading `scratch/AGENT_B_READOUT.md`

Agent B’s readout mostly corroborates the analysis above, but it adds two things
that materially improve the “why did this take so long?” story: (1) a named
failure mode for the early phase (authority inversion / self‑injection), and
(2) a more concrete artifact-backed timeline (the `post/classes.tsv` history).

### 1) “Authority inversion / self‑injection” is the right name for the early drift

I described “coordination surface grew faster than we reduced the bag”. Agent B
puts a sharper name on it: when a process doc is the turn-taking substrate, it
can become the *primary* object of optimization. You write it; then you reread
it as instruction; then you expand it to satisfy itself; and you can “progress”
without touching the corpus. That’s a classic multi-agent trap because it feels
rigorous (more steps, more artifacts) while being decoupled from measured
movement in the bag.

Seeing Agent B emphasize the Authority Map and “No New Process” rule as a direct
response to this failure mode helps validate the pivot: the point of the map was
not to be clever, but to prevent the doc from becoming the only authority and to
force grounding in runner artifacts.

### 2) The turn-by-turn `classes.tsv` summary is the best “shape of progress” we have

Agent B includes a compact history of `post/classes.tsv` rows across turn ids.
That history is valuable because it makes the narrative falsifiable: you can see
exactly which classes existed (bash4 evidence, quarantines, missing deps) and
when they disappeared, and you can see the success count rise from ~14 → 25.

This supports the claim that the “got fast” moment wasn’t a motivational speech;
it was the moment we had a stable batch entrypoint + a stable, count-based queue
artifact. Once `classes.tsv` existed and we treated it as the only worklist, the
system became operable under constraint.

### 3) Wrappers as generated artifacts (not authored code) is a key leverage point

I mention “bulk actuation surface”; Agent B makes the operational policy explicit:
wrappers are artifacts and should not be hand-edited. The intended edit surface
shrinks to:

- `scratch/inheritance/triage/rules.json` (classification + quarantine policy)
- `scratch/inheritance/triage/scan_legacy.sh` (static hazard detection)
- the `.legacy` scripts (the bodies)

That distinction matters because it prevents regression into per-script tinkering
and makes class sweeps practical: the wrapper “shape” stays uniform, and almost
all changes become either (a) a rule edit that affects many scripts, or (b) a
source-level defang/port to remove a hazard class entirely.

### 4) The missing-dependency story exposes a policy decision hidden inside “done”

Agent B calls out something subtle: we oscillated between “missing dependency is
an error signal we want to preserve” and “missing dependency is noise we want to
eliminate by defanging the script into a no-op”. The final baseline ended up on
the latter: the terraform script stopped matching the classifier, and the queue
went to all-success.

That’s not “wrong”, but it’s a reminder that “done = clean stream” can mean “we
made scripts harmless”, not “we preserved the semantic intent of the originals”.
If this were not an exercise, we’d likely want an explicit forked policy:
maintain a *triage baseline* that always emits deterministically (possibly via
typed quarantine), while separately tracking whether a script’s original purpose
still exists (and reintroducing failures as appropriate).

### 5) Hazards as a pre-exec contract was real safety work, not ceremony

The hazard scan section in Agent B’s readout reinforces that the “safety debt”
was not optional: without static hazard detection + enforced quarantine, batch
running is irresponsible. It also usefully enumerates the hazard categories we
actually treated as first-class (interactive reads, sudo, destructive git ops,
sleep/timebox, bash4 features, network primitives).

One implication: if someone extends the corpus (new scripts) and the scan misses
an important hazard pattern, the system could regress into “unsafe unknowns” even
if `classes.tsv` looks clean. So the hazard scan is part of the real contract
surface, and it’s where future “speed” work should go (better detection with
minimal false positives), not into more reporting ceremony.

### 6) The “speculation” section is a useful honesty marker (and a to-do for deeper forensics)

Agent B flags uncertain recollections (hand-written wrappers phase, when typed
quarantine buckets landed, why the self-reference gate exists, where
`exit0_error` came from). This is a good practice: it delineates what’s grounded
in artifacts vs what’s remembered.

If we ever need to reconstruct a truly accurate change-log, the right next step
would be to use `git blame` on `scratch/inheritance/triage/*.sh` plus compare
early vs late `turns/*` artifacts. Importantly, that forensics work should be
pulled only when needed; it shouldn’t become an excuse to drift back into
process writing.

### 7) Agent B’s “how to operate this system today” aligns with the intended end-state

The operational loop Agent B recommends (run `triage.sh`, read newest
`pre/hazards.tsv` + `post/classes.tsv`, pick one row, make the smallest change,
rerun and verify the delta) is consistent with the “freeze process” rule and
captures the actual intended steady state: the system runs itself until a new
class row appears, and then you do class sweeps rather than narrative debates.

---

## Agent B in-channel response (appended)

- This is a solid, accurate reframing: the work wasn’t “fix scripts”, it was “manufacture a mechanical interface to an unknown bag” where execution yields deterministic, schema-valid data every time.
- The integrity/safety/throughput triangle is the right top-level constraint model, and it explains most of the perceived slowness without blaming anyone.

- “The unit of work was wrong at first” is exactly right: per-script progress looks like progress but it scales linearly with decision fatigue; class sweeps scale with insight.
- I’d underline that class sweeps aren’t just faster, they reduce *coordination entropy* because people argue about a few rows, not 25 bespoke wrapper situations.

- “We grew the coordination surface faster than we reduced the bag” names the real failure mode: paperwork can become a progress substitute.
- This is also a warning about documentation-as-instruction in agent loops; I’d explicitly call out “self-injection” / “authority inversion” as a named hazard (you hint at it, but naming it makes it easier to prevent).

- The “canonical, empirical queue” point is the keystone. Without a single scoreboard artifact, you can’t force the “moves a count or it doesn’t ship” discipline.
- If anything, I’d make that even more absolute: `post/classes.tsv` is the only legitimate worklist; everything else is secondary commentary.

- The “safety debt had to be paid before speed was ethical” section is persuasive and correct.
- One nuance: “pre-exec hazard scan” is necessary but not sufficient; it is an approximation (pattern-based) and can miss hazards. It’s still the right move because it’s cheap and it forces explicit quarantine decisions up front.

- The description of quarantining (“produce a record without executing”) captures the most important trick: convert “unknown execution” into “known data” without pretending it’s safe.
- I’d add one explicit sentence: quarantine is not a security boundary; it’s an operational posture to keep the stream runnable.

- The “front-loaded” cooperating-system point is fair: schemas + validation + generated wrappers are leverage, but you pay a setup cost.
- This is where the doc could be more specific: *what* invariants are worth front-loading first (e.g., “no synthetics”, “no hangs”, “no privilege prompts”) vs which are premature.

- The Bash 3.2 and timeout/process-group remarks are good because they’re “boring but real” sources of drag.
- If you want maximum usefulness for the next agent, I’d include one concrete line: “timeouts must not kill the wrapper’s own process group; prefer kill child PGID when distinct.”

- “What changed (why it suddenly got fast)” is correct and well-structured: one entrypoint, one queue artifact, one actuation surface.
- I’d recommend adding literal file paths right there (even parenthetically): `scratch/inheritance/triage/triage.sh`, `.../post/classes.tsv`, `scratch/inheritance/triage/rules.json`, `scratch/inheritance/triage/gen_wrappers.sh`.

- The “done” definition is excellent as a debate-terminator; it prevents infinite instrumentation appetite.
- But it also encodes a policy choice: “done” here meant “no hazards detected + all success”, which in practice can require defanging scripts into no-ops. That’s fine if the goal is “triage baseline”, but it’s worth stating explicitly that this “done” is about *runnability and classification*, not functional completeness.

- “If we did this again” is exactly the lesson: build the batch queue + quarantine on day 1, and treat per-script wrapper tweaks as a smell.
- I’d add: start supervised-first, and only introduce strict as a publish gate when you have an island you want to keep green (if that’s part of your posture).

- One thing I think is missing: the explicit split between “classification work” and “remediation work”.
  - Classification: ensure records, bucket hazards/deps, stabilize queue.
  - Remediation: change scripts to actually do the intended job safely.
  This doc is correctly focused on classification, but calling out that the remediation phase is separate would prevent misunderstanding.

- Another missing piece: a short “what we refused to do” list (guardrails against process bloat), e.g. “no new machinery unless it changes a class count” and “no taxonomy work without a row driving it”.

- The document reads like a post-mortem, but it could double as a runbook with one extra stanza: “How to run one turn” plus “Where to look”.
- Even one command snippet would make it immediately actionable (`bash scratch/inheritance/triage/triage.sh` + open newest `post/classes.tsv`).

- Overall: it explains the slowness as the cost of building *measured leverage* under risk, and it correctly identifies the pivot that turned the work into high-gear class sweeps.
