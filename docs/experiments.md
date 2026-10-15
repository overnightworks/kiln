# kiln: experiment gate rules (E1, E2, E3)

`docs/concept.md` states where a builder starts. This document holds, for
each of the three experiments that test kiln's thesis, everything that
decides whether it passes or fails: the hypothesis, the gating predicate and
numerator, the gating sizes, the aggregation rule, the conjunctive success
condition, and the audits that keep the measurement honest. It does not
supply the concrete inputs a builder runs against. Each experiment's own
item — #2 for E1, #3 for E2, a new item opened when this document lands for
E3 — produces those inputs, expressed in the IR's reference meaning
(`docs/concept.md` §2.1), as that item's first deliverable.

The thresholds below are carried over unmodified from the operator's ruling
and the ideation brief (`docs/research/ideation/codex-astra-ideation.md`,
`docs/research/ideation/fable-ideation.md`): E1 at 70%/50%, E2 at 3x on 16
cores. Nothing in this document changes them; it states, precisely, what
must be measured to check them honestly.

## Freeze protocol (E1, E2 and E3 alike)

Before any implementation code is written and before any result-bearing run,
an experiment's concrete inputs — named under that experiment's "Deferred
to ..." heading below — are written down in that item, in the IR's
reference meaning, and independently reviewed. A number produced before that
freeze, from a prototype, a spike, or a run against unreviewed inputs, is
void: it is not reported as evidence toward or against the thresholds below,
and the experiment reruns against the frozen inputs once they exist. For E3,
the six families are frozen and independently reviewed before the grammar is
designed, by a different author than the grammar's — the family set cannot
be shaped by what the grammar already happens to handle.

## Shared dependency: the reference interpreter

E1's lowering audit, E2's correctness comparison, and E3's checker each need
one executable interpreter of the IR (`docs/concept.md` §2.1): something
that runs a transaction's IR against a concrete state and produces its
response, its effects, and its failures, so each experiment's oracle has
something to compare against other than another copy of the system under
test. This is one shared dependency, not three private ones. Its own item
opens when #1 lands; E1, E2 and E3 each depend on it rather than building a
private interpreter.

## E1 — classification coverage (issue #2)

**Hypothesis.** At least 70% of frequency-weighted transaction instances,
measured on TPC-C and on a corpus of real open-source applications, classify
into a footprint category the compiler can analyze rather than unknown.
Below 50% falsifies the thesis.

**Scope.** A footprint extractor plus classifier only — no runtime, no
storage engine, no kernel from `docs/concept.md` §4.

### Footprint categories

A category describes what can be statically proven about a transaction
*function's* footprint; this is independent of the pairwise compatibility
classes of `docs/concept.md` §3, which describe a relationship between a
*pair* of calls. The two overlap in practice, but they answer different
questions and are reported as two separate numbers, never conflated:

- **Static.** The complete set of rows, ranges, or keys the function can
  touch is a pure function of its arguments and the schema, with no read of
  current data. Example: `read(accounts, key = from_id)`.
- **One-hop.** The footprint requires exactly one data-dependent read whose
  result determines a further access, and that one hop resolves through an
  index or foreign key the compiler can identify ahead of time — not an
  arbitrary secondary-attribute scan. Example: look up an order by
  `order_id`, then touch the customer through `order.customer_id`, a foreign
  key into `customers`.
- **Domain-bounded.** Not static or one-hop, but provably confined to one
  named partition of the schema (a tenant id, a warehouse id) that is itself
  a static argument. This is a fact about a function's footprint, measured
  regardless of whether the engine exploits it — the adopted kernel
  (`docs/concept.md` §4) has no domain structure, unlike the rejected
  domain/ticket proposal an earlier draft's "single-domain" qualifier named.
- **Unknown.** None of the above. Includes navigational code, a
  secondary-attribute lookup with no usable index hint, and any function the
  source-to-IR boundary (`docs/concept.md` §2.1) refuses to translate in
  full — an unsupported function is never dropped from the denominator; it
  is folded into this category's numerator, so a classifier cannot raise its
  coverage by silently failing to extract what it would have trouble with.

### Gating numerator and verdict

The gating numerator is the percentage of frequency-weighted transaction
instances whose category above is static or one-hop. Domain-bounded
instances are not in the numerator; their share is reported separately and
never gates the verdict.

**Application-side gating metric.** #2's freeze
(`experiments/e1-classification-coverage/inputs.md` §4) establishes, before
any classification result exists, that no trace source exists for any
application in the frozen corpus. The application half of the gate
therefore does not wait on traces the corpus cannot produce: it gates on
the median, over the frozen application corpus, of each application's own
unweighted coverage share — one unit per enumerated transaction function
(`inputs.md` §3), with no frequency weight, that classifies static or
one-hop — reported and labelled **"unweighted — no trace source exists."**
This replaces the "at least five traced applications" condition this
section previously carried: that condition assumed a trace source would
exist for some applications by the time a result was produced, and the
freeze establishes the opposite before any result exists, so the condition
is retired now rather than left to fail by construction on every run.
TPC-C's own transaction mix is unaffected by this and stays weighted
(`inputs.md` §1).

- **Pass** when TPC-C's weighted coverage is at least 70% *and* the median
  unweighted coverage over the frozen application corpus is at least 70%.
- **Fail** when TPC-C's coverage is below 50% *or* that median is below 50%.
- **Inconclusive** otherwise. A unit landing strictly between 50% and 70% is
  reported as inconclusive for that unit specifically, never rounded into a
  pass or a fail.

Separately from the gating numerator, the builder reports the percentage of
instances whose pairwise execution class (`docs/concept.md` §3) could be
resolved to class 1, 2, or 3 against their observed neighbors in the traced
workload — a different measurement from footprint coverage, never presented
as the same result.

### Audits

- **Evidence requirement.** The classifier's output must carry checkable
  evidence — which IR nodes justified each category assignment — not only
  the final label. Before any coverage number is trusted, the builder runs
  the classifier against a deliberately degenerate stub (one that labels
  every transaction "static" with no analysis) and confirms the evaluation
  harness rejects that stub's output for lacking evidence.
- **Lowering audit.** A classifier can raise its coverage number by
  weakening what "static" or "one-hop" require. Before the first result
  counts, and again on every change that reclassifies transactions, a
  sample of the classified transactions is re-checked: that the IR the
  front end produced for each one is equivalent to the function's actual
  source behavior, judged against the IR's reference meaning
  (`docs/concept.md` §2.1) through the reference interpreter — not merely
  that the classifier's own logic about its own IR is internally
  consistent.
- **Containment audit.** Independently of the lowering audit, and on the
  same before-first-result-and-every-reclassification schedule, confirm
  that the resources a classified transaction actually touched while
  running stay within the footprint the classifier inferred for it.
- A result not preceded by both audits, or a reclassification not
  re-checked by both, does not count as evidence toward or against the
  70%/50% thresholds.

### What this adds beyond the 111-application study

The 111-application Django/TypeORM study already reports that for 90% of
the applications it studied, at least 58% of transactions have fully
inferable read/write sets, and that about 27% query by a secondary attribute
(`docs/research/notes/open_problems_and_challenged_assumptions.md:72-73`).
E1 is not a restatement of that count: it adds the one-hop category (the
study's binary inferable/not-inferable split does not distinguish static
from one-hop), frequency weighting (the study's figure is a count over
transactions, not over frequency-weighted instances), and kiln's IR
unsupported-source boundary (`docs/concept.md` §2.1) as the exact rule for
what falls into "unknown." If the study's own per-application data can be
obtained, it is a cheaper first pass at an order-of-magnitude estimate — not
a substitute for E1's own measurement against kiln's categories and IR.

### Deferred to #2

The corpus (TPC-C's pinned spec version; the application set, or a
documented substitute and its selection rule), the transaction-enumeration
rule that decides which functions are extracted, and the trace source and
weights behind "frequency-weighted" are #2's first deliverable, frozen and
independently reviewed under the protocol above before extraction runs and
before any result-bearing classification.

## E2 — parallel evaluation of ordered transactions (issue #3)

**Hypothesis.** At least 3x throughput on 16 cores, against an optimized
serial-batching baseline with identical per-call outcomes, for class-3
transaction families (`docs/concept.md` §3.3), at batch sizes 1-256.

**Scope.** Execution-only: no persistence, no durability, no crash recovery.
This measures the summary algebra and its parallel evaluation in memory.

### Required families

At least the three families already named by the operator's wish — quota
allocation, conditional inventory, and mixed account operations — are
tested, and each must pass independently; one family failing fails E2
overall.

### Gating sizes and aggregation

Throughput and correctness are measured and reported across the full 1-256
progression. The pass/fail verdict is evaluated only at batch sizes 64, 128,
and 256: a family passes only if the 3x ratio holds at *each* of those three
sizes. Batch sizes below 64 are reported as context, where parallel overhead
can dominate and a 3x ratio is not expected; they never enter the pass/fail
aggregate. Fixing this split before the run, not after seeing results, is
what keeps the aggregation honest.

### Baseline

The "optimized serial-batching baseline" is one specific, named
implementation: a straightforward serial fold over the batch — apply each
transaction's effect one at a time, in order, with no summary mechanism —
written with the same data structures and the same execution boundary
(in-process, in-memory, no persistence) as the class-3 path it is compared
against. "Optimized" means ordinary optimization of that serial fold, not a
second parallel mechanism. The 3x figure is the summary-and-scan path's
throughput divided by this exact serial fold's throughput, at the same batch
size, on the same hardware, in the same process.

### Correctness comparison

For every batch tested, every call's response — including semantic
failures — and the final state after the batch are compared element by
element between the class-3 path and the serial-fold baseline, using the
reference interpreter as the independent oracle for the baseline's own
expected outcomes. Any divergence fails that batch's correctness check
regardless of throughput; "identical per-call outcomes" is checked on every
batch, never assumed from the algebra's design.

### Admissible bound class

For each family, state and then measure — never infer from associativity
alone:

- **Summary size bound.** The compact summary stays within a fixed, stated
  bound (for example O(1) relative to batch size), measured in actual bytes
  per summary. A family whose summary grows with batch size fails this bound
  and is reported as failing, never silently excluded.
- **Combine/apply cost bound.** Combining two summaries, and applying a
  summary to a concrete state, are each a fixed cost independent of batch
  size (for example O(1)), measured in actual operation counts or
  wall-clock micro-benchmarks; a family whose combine or apply cost grows
  with batch size fails this bound, reported as failing rather than
  excluded. Associativity alone does not imply O(n) total work and O(log n)
  depth — a tree of closures that just replays the original sequential
  computation is associative but not compact — so measuring bytes and work,
  not inferring them from the algebra's shape, is what catches that case.

Both measurements are reported alongside throughput for every family and
batch size. Together they are the **admissible bound class** E3 also gates
against (below).

### Deferred to #3

Each family's exact IR shape, every response and failure it can produce,
its overflow behavior at any declared numeric bound, its guard shape, and
the exact associative summary representation with its combine/apply
operators — one frozen semantics per family, not a name — are #3's first
deliverable, frozen and independently reviewed under the protocol above
before implementation and before any result-bearing benchmark.

## E3 — automatic synthesis and checking of transaction summaries (new)

This item opens its own board entry when the concept document lands.

**Hypothesis.** A fixed rule library automatically admits at least 4 of 6
preregistered transaction families within ten minutes each, without
hand-written per-family batch procedures, and correctly rejects semantic
mutants of every family it admits.

**Why this experiment exists.** E2 can pass using hand-written summaries for
hand-picked families; it does not test whether the compiler can *discover*
a compact summary on its own. The source ideation named a certification
experiment for exactly this
(`docs/research/ideation/codex-astra-ideation.md`), and it must run before
investing in framework front ends or multicore execution: a synthesis-method
failure here makes E2's hand-written results moot as evidence for automatic
discovery.

### Freeze order and authorship

The freeze protocol above applies to the six families in full: frozen and
independently reviewed before the grammar or rule library is designed, by a
different author than the grammar's. The freeze reviewer additionally
certifies, for the six as a set: no two have the same guard behavior, and no
two admit the same summary algebra up to renaming of constants. One family,
quota allocation, is not independently defined here: it is E2's frozen
family, used by reference once E2 has frozen it, so the two experiments
corroborate each other on one common family. The grammar and rule library —
the exact set of summary shapes the synthesis process is allowed to try, and
the rules that propose and check a candidate against a family's IR — are
frozen and independently reviewed afterward, before any synthesis attempt.
Either freeze changing after families are registered invalidates every
family registered before that change, and the experiment restarts its gate
for those families.

The manifest records, before the gate runs, the exact hardware the
ten-minute budget is measured on and the rule library's retry policy, if
any — the ten-minute figure is meaningless unpinned to both.

### Gate

For each family, the rule library has ten minutes of wall-clock time to
propose and check an admitted summary. A family not admitted within ten
minutes is a miss, not a retry with more budget.

### Rejection check

For every family where a summary is admitted, the builder constructs
semantic mutants of that family — the same transaction with one deliberately
wrong behavior (an off-by-one bound, a swapped comparison, a dropped
overflow check, an altered response) — and confirms the checker
(`docs/concept.md` §6) rejects every mutant's proposed summary against the
fixed, unmutated reference. An admitted family whose checker fails to reject
even one tested mutant is a **checker-soundness failure**, reported
separately from a synthesis miss as the more serious finding.

### Conjunctive success condition

E3 passes if and only if both hold: at least 4 of the 6 families are
admitted under the gate, within the admissible bound class E2 defines
(summary size and combine/apply cost independent of batch size, measured);
and zero false acceptances occur over the preregistered invalid candidates —
mutated summaries or certificates tested against the fixed reference. A
single false acceptance fails E3 regardless of how many families were
admitted.

### Benchmarking

Only a family whose summary is both gated and mutant-rejection-clean is
benchmarked, feeding into or corroborating E2's measurement for that family.
Benchmarking an unchecked summary, or one that admitted even a single
mutant, is not evidence for or against the thesis and is not reported as a
performance result.

### Deferred to its own item

The exact grammar, rule library, and the five families beyond the
quota-allocation family shared with E2 — all named above — are that item's
first deliverable, under the freeze order and authorship rule above.
