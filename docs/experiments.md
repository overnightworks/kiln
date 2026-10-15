# kiln: experiment protocols (E1, E2, E3)

These are the complete, operational protocols for the three experiments
that test kiln's thesis (`docs/concept.md`). Each section is written so a
fresh builder can start implementing that experiment from this document
alone, without reading the comment history or re-deriving a definition the
thesis only gestures at. `docs/concept.md` keeps only each experiment's
hypothesis, threshold, and a link here; this is where the hypothesis
becomes something a builder can actually run.

The thresholds below are carried over unmodified from the operator's
ruling and the ideation brief
(`docs/research/ideation/codex-astra-ideation.md`,
`docs/research/ideation/fable-ideation.md`): E1 at 70%/50%, E2 at 3× on 16
cores. Nothing in this document changes them; it only states, precisely,
what must be measured to check them honestly.

## E1 — classification coverage (issue #2)

**Hypothesis.** At least 70% of frequency-weighted transaction instances,
measured on TPC-C and on a corpus of real open-source applications,
classify into a footprint category the compiler can analyze rather than
unknown. Below 50% falsifies the thesis.

**Scope.** This experiment is only a footprint extractor plus classifier —
no runtime, no storage engine, no kernel from `docs/concept.md` §4.

### Footprint categories

Define these independently of the compatibility classes in
`docs/concept.md` §3. A category describes what can be statically proven
about a transaction *function's* footprint; a class describes a
relationship between a *pair* of calls. The two overlap in practice — a
one-hop function will usually end up needing class-4 treatment for some
pairs — but they answer different questions, and this experiment reports
them as two separate numbers, never conflated into one.

- **Static.** The complete set of rows, ranges, or keys the function can
  touch is a pure function of its arguments and the schema, derivable with
  no read of current data. Example: `read(accounts, key = from_id)`.
- **One-hop.** The footprint requires exactly one data-dependent read whose
  result determines a further access, and that one hop resolves through an
  index or foreign key the compiler can identify ahead of time (not an
  arbitrary secondary-attribute scan). Example: look up an order by
  `order_id`, then touch its line items, which are keyed by `order_id`.
- **Domain-bounded.** Not static or one-hop, but provably confined to one
  named partition of the schema (a tenant id, a warehouse id) that is
  itself a static argument. This category exists for measurement purposes
  regardless of whether the engine implements domain-based execution —
  `docs/concept.md` §4 rejects that as the kernel, but a transaction can
  still be domain-bounded as a fact about its footprint.
- **Unknown.** None of the above. Includes navigational code, a
  secondary-attribute lookup with no usable index hint, and anything the
  source-to-IR boundary (`docs/concept.md` §2.1) refuses to translate.

A transaction function is extracted and categorized, never a running
instance guessed at; "frequency-weighted transaction instances" (below)
is what turns a per-function category into the coverage percentage.

Report two numbers, never one blended figure: the percentage of
frequency-weighted instances that are static, one-hop, or domain-bounded
(the coverage number the 70%/50% thresholds apply to), and, separately, the
percentage of instances whose pairwise execution class (`docs/concept.md`
§3) could actually be resolved to class 1, 2, or 3 against their observed
neighbors in the traced workload. These are different measurements and
must not be presented as the same result.

### Corpus

- **TPC-C.** The standard five-transaction TPC-C workload, transaction
  boundaries as defined by the TPC-C specification (the builder records
  the exact spec version used, e.g. TPC-C v5.11), run unmodified for the
  TPC-C-only result.
- **Applications.** The 111-application Django/TypeORM corpus named in
  `docs/research/notes/open_problems_and_challenged_assumptions.md`
  (citing ACM paper doi 10.1145/3725268). The builder pins the exact
  snapshot — commit hash or release tag — of every repository at
  extraction time, recorded in a manifest file committed alongside the
  experiment's results, so the corpus is reproducible from that manifest
  alone. If the paper's exact 111-repository list cannot be obtained, the
  builder documents a substitute corpus and its selection rule (for
  example, "the top N Django/TypeORM repositories by star count with at
  least X transaction functions") in the same manifest before running
  classification — a substitution is a named deviation, never a silent
  one.

### Extraction rule

A transaction function is extracted from source only through a defined,
checkable rule stated in the manifest before extraction runs — for
example, every function decorated `@transaction.atomic` in Django, every
callback passed to TypeORM's `transaction()`. When extraction fails for a
function — the source-to-IR boundary of `docs/concept.md` §2.1 refuses to
translate it — that function's instances count as **unsupported**, folded
into the **unknown** category's numerator. Unsupported extraction is never
dropped from the denominator; a classifier cannot improve its coverage
number by silently failing to extract the functions it would have trouble
with.

### Frequency weighting

Application source supplies transaction *functions*, not invocation
*frequencies*. "Frequency-weighted transaction instances" requires an
explicit, reproducible weighting source, chosen and documented before
results are reported:

- a captured or synthetic request trace, reproducible from a fixed script
  or a fixed random seed recorded in the manifest; or
- where no trace source exists for a given application, an explicitly
  different, named metric — for example, an unweighted count of one per
  transaction function — reported as a clearly labeled alternative metric,
  never silently presented as "frequency-weighted" when it is not.

### Reporting and evidence

- TPC-C's result and the application corpus's result are reported as two
  separate numbers, never pooled into one blended percentage.
- Any application, or TPC-C as a whole, landing strictly between 50% and
  70% is reported explicitly as **inconclusive** for that unit, not rounded
  into a pass or a fail.
- The classifier's output must carry checkable evidence — which IR nodes
  (`docs/concept.md` §2.1) justified each category assignment — not only
  the final label. Before trusting any coverage number, the builder runs
  the classifier against a deliberately degenerate stub (one that labels
  every transaction "static" with no analysis) and confirms the evaluation
  harness rejects that stub's output for lacking evidence. A harness that
  cannot catch this stub is not ready to report a real number.
- **Lowering-correctness audit.** A classifier can raise its coverage
  number by weakening what "static" or "one-hop" require. Any change that
  increases the measured coverage is paired with an audit step: re-run a
  sample of the newly reclassified transactions against the reference
  semantics (`docs/concept.md` §6) and confirm they are still correctly
  classified. A coverage increase without this audit does not count as
  evidence toward or against the 70%/50% thresholds.

## E2 — parallel evaluation of ordered transactions (issue #3)

**Hypothesis.** At least 3× throughput on 16 cores, against an optimized
serial-batching baseline with identical per-call outcomes, for class-3
transaction families (`docs/concept.md` §3.3), at batch sizes 1–256.

**Scope.** Execution-only: no persistence, no durability, no crash
recovery. This experiment measures the summary algebra and its parallel
evaluation in memory, exactly as the source ideation scoped it.

### Frozen family semantics

Before any code is written, freeze and write down, for each family under
test:

- its inputs — the exact IR shape (`docs/concept.md` §2.1) of every
  transaction function in the family;
- every response and failure it can produce;
- its overflow behavior at any declared numeric bound;
- its **guard shape** — whether admission is a simple clamp or cap
  (monotonic, composes as a bounded accumulator) or an all-or-nothing
  threshold such as `reserve(q)` (grants every requested unit or fails
  entirely). This is stated explicitly because the guard shape determines
  whether the summary representation below is even attemptable: E3 §"Six
  preregistered families" names all-or-nothing reservation as the hard case
  a clamped summary cannot express;
- the exact associative summary representation and the combine/apply
  operators proposed for it.

"Quota allocation, conditional inventory, and mixed account operations" is
not a frozen semantics; a frozen semantics is the actual function
signatures and summary algebra for those exact families, written into the
experiment's manifest before the benchmark runs. At least three families
are required, each independently useful — not a trivial variant of another
family already in the set.

### Admitted summary size and cost bounds

For each family, state and then measure, never merely assume from
associativity alone:

- **Summary size bound.** The compact summary must stay within a fixed,
  stated bound (for example, O(1) words, or O(1) relative to batch size),
  measured in actual bytes per summary. A family whose summary grows with
  batch size fails this bound and is reported as failing — it is not
  silently excluded from the result.
- **Combine/apply cost bound.** Combining two summaries, and applying a
  summary to a concrete state, must each be a fixed, stated cost (for
  example O(1) or O(log batch size)), measured in actual operation counts
  or wall-clock micro-benchmarks. Associativity alone does not imply O(n)
  total work and O(log n) depth: a tree of closures that just replays the
  original sequential computation is associative but not compact, and
  measuring bytes and work — not inferring them from the algebra's shape —
  is what catches that case.

Both measurements are reported alongside throughput for every family and
every batch size tested.

### Baseline

The "optimized serial-batching baseline" is one specific, named
implementation: a straightforward serial fold over the batch — apply each
transaction's effect one at a time, in order, with no summary mechanism —
written with the same data structures and the same execution boundary
(in-process, in-memory, no persistence) as the class-3 path it is compared
against. "Optimized" means ordinary optimization of that serial fold (no
interpretive overhead, no unnecessary allocation); it is not a second,
different parallel mechanism. The 3× figure is the summary-and-scan path's
throughput divided by this exact serial fold's throughput, at the same
batch size, on the same hardware, in the same process.

### Correctness comparison

For every batch tested, every call's response — including semantic
failures — and the final state after the batch are compared element by
element between the class-3 path and the serial-fold baseline. Any
divergence fails that batch's correctness check regardless of its
throughput result; "identical per-call outcomes" is checked on every
batch, never assumed from the algebra's design.

### Aggregation over batch sizes

The 3× threshold is evaluated per batch size, over a stated progression
(for example 1, 2, 4, 8, ..., 256) fixed in the manifest before the
experiment runs. A family's pass/fail verdict requires the 3× ratio to
hold at the stated large-batch sizes — the regime where the mechanism's
O(log n) depth actually matters. Small-batch sizes, where parallel
overhead can dominate and a 3× ratio is not expected, are reported
separately as context and are not folded into the pass/fail aggregate.
Fixing this choice before the run, not after seeing results, is what makes
the aggregation rule honest.

## E3 — automatic synthesis and checking of transaction summaries (new)

This item opens its own board entry when the concept document lands; this
protocol is written in advance so that item can be dispatched directly.

**Hypothesis.** A fixed rule library automatically admits at least 4 of 6
preregistered transaction families within ten minutes each, without
hand-written per-family batch procedures, and correctly rejects semantic
mutants of every family it admits.

**Why this experiment exists.** E2 can pass using hand-written summaries
for hand-picked families; it does not test whether the compiler can
*discover* a compact summary on its own. E3 is the cheapest available
falsifier of that separate claim — the source ideation named a
certification experiment explicitly
(`docs/research/ideation/codex-astra-ideation.md`) — and it must run before
investing in framework front ends or multicore execution: a synthesis-
method failure here makes E2's hand-written results moot as evidence for
automatic discovery, so finding that out early is cheaper than building
around an assumption E3 would have killed.

### Frozen grammar and rule library

Before registering any family, freeze and write down the exact grammar of
summary shapes the synthesis rule library is allowed to try (for example,
clamped linear transformations, bounded counters, fixed-size accumulator
records) and the fixed set of rules that propose and check a candidate
summary against a transaction family's IR (`docs/concept.md` §2.1). The
grammar and rule library do not change once families are registered — a
mid-experiment change to either invalidates every family registered before
the change, and the experiment restarts its gate for those families.

### Six preregistered families

Fixed before any synthesis attempt, recorded in the experiment's manifest:

1. **Partial allocation** — the quota-with-partial-grant family already
   used in E2, shared deliberately so the two experiments can corroborate
   each other on one common family.
2. **All-or-nothing reservation** — `reserve(q)` that grants every
   requested unit or fails entirely. The ideation names this as the hard
   case plain clamped translations cannot handle; whether the rule library
   synthesizes anything for it, or correctly refuses to, is itself part of
   the result, not a precondition for running the experiment.
3–6. **Four further families**, selected from real contended-state
   transaction patterns (for example: conditional inventory, mixed account
   transfer, a rate-limited counter, append-only sequence allocation) and
   named exactly, with their IR, in the manifest before synthesis begins.

### Gate

For each family, the rule library has ten minutes of wall-clock time to
propose and check an admitted summary. A family not admitted within ten
minutes counts as a miss — not a retry with more time budget. The
experiment passes only if at least 4 of the 6 families are admitted under
this gate.

### Rejection check

For every family where a summary is admitted, the builder constructs
semantic mutants: the same transaction family with one deliberately wrong
behavior (an off-by-one bound, a swapped comparison, a dropped overflow
check, an altered response) and confirms the checker
(`docs/concept.md` §6) rejects every mutant's proposed summary. An
admitted family whose checker fails to reject even one tested mutant is
reported as a **checker-soundness failure** — a more serious finding than
a synthesis miss, and reported separately from the gate's 4-of-6 result.

### Benchmarking

Only after a family's summary is both gated and mutant-rejection-clean is
it benchmarked — feeding into or corroborating E2's measurement for that
family. Benchmarking an unchecked summary, or one that admitted even a
single mutant, is not evidence for or against the thesis and is not
reported as a performance result.
