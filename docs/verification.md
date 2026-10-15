# kiln: verification strategy

This document answers one question: how does kiln prove its correctness
claims, instead of merely hoping tests happen to find bugs? It is written for
two readers. A **builder agent** implementing kiln's engine needs to know,
for any piece of code it is about to write, which verification layer covers
it, what tool that layer uses, and when in the project that layer starts. The
**operator** needs to understand, without a database background, what each
layer can and cannot promise, so a claim like "kiln is correct" can be
checked against what was actually proved rather than taken on faith.

Every technical term below is defined once, at its first use, in plain
language. `docs/concept.md` is the architecture this document verifies;
section references below (`§3`, `§5`, …) point there unless stated
otherwise. `docs/experiments.md` is the protocol for E1, E2, and E3; this
document states only how those three experiments are checked, not what they
measure.

## 1. The five verification layers

kiln uses five layers, from the strongest and narrowest guarantee to the
broadest and weakest. No single layer is sufficient alone: a design proof
(layer 1) can be sound about an abstract model and still miss a bug in the
real code; a differential test (layer 3) can run for years without
generating the one interleaving that breaks an invariant. The layers are
deliberately redundant where they overlap, because redundancy across
independent techniques is what catches a bug that one technique's blind spot
would otherwise hide.

### Layer 1 — Design proofs (TLA+)

**What it proves.** That an abstract model of a protocol — a finite-state
description of what messages, steps, or crash points are possible — never
reaches a bad state, for *every* interleaving and crash point the model
admits, not just the ones a human thought to test. A **model checker**
(TLA+'s tool, TLC) explores the model's entire reachable state space
exhaustively for a small enough model.

**What it cannot prove.** Anything about the real Rust code. The abstract
model is written by a human (or agent) who decides what to include; a
protocol step the model simplifies away, or an implementation detail the
model never represented, is invisible to this layer. A design proof is a
proof about the design, not about the code that is supposed to implement it.

**What it covers.** The isolation kernel's locking and ordering protocol
(`docs/concept.md` §4: strict two-phase locking over one shared resource
namespace, canonical lock ordering, lock-expansion restart), including
§9.5's liveness-of-tentative-waits obligation (whether a wait behind a
non-exclusive escrow holder is deadlock-free, and every waiter eventually
proceeds under stated fairness assumptions — this is where that question
and those assumptions are settled, per §9.5's own status line); the commit
and
recovery protocol (§5, §9.1, §9.2: atomicity across a commit group, the
visible-version pointer, recovery after a crash mid-install); and the
migration switch (§8, §9.3: backfill watermark, catch-up phase, barrier
commit, rollback window).

**Tool.** TLA+, model-checked with TLC over small, finite instances (a
handful of rows, a handful of concurrent transactions, a handful of crash
points) — large enough to exhibit the interesting interleavings, small
enough for exhaustive search to finish.

**When it starts.** Before any engine code exists. §9 of `docs/concept.md`
names the publication boundary, storage failure model, and migration
watermark protocol as "not yet specified" engine-phase obligations; writing
each one precisely enough to become a TLA+ model *is* that specification
work, so layer 1 is the first verification activity in the project, not an
afterthought bolted onto finished code.

### Layer 2 — Code proofs for a small critical kernel (Verus, Kani)

**What it proves.** That specific Rust functions meet a stated specification
— not "probably correct," a mathematical proof checked by a tool, for every
input the function's type allows, including inputs no test happened to try.
**Verus** is a tool that lets a proof live inside ordinary Rust source code,
as annotations the compiler-adjacent verifier checks; the proof and the code
it proves cannot drift apart because they are the same file. **Kani** is a
separate tool for a narrower job: proving a Rust `unsafe` block (code that
asks the compiler to trust the programmer instead of checking memory safety
itself) cannot violate memory safety — within a harness that states the
domain of inputs checked and a bound on loop unwinding, and only for the
operations Kani's verifier supports.

**What it cannot prove.** Anything about code outside what was proved. A
Verus proof of one function says nothing about a different function, even an
adjacent one, unless that function is separately proved. Kani proves memory
safety of an `unsafe` block within its harness's stated domain and unwind
bound; it does not prove the block does the right thing semantically, and it
does not establish anything for a value outside that domain or a loop
iteration past that bound unless the harness's own unwinding assertions pass
(Kani fails the check named "unwinding assertion" when the bound was too
low to see a violation past it, rather than silently under-proving). Kani's
own documented limitations include concurrency and data races: "concurrent
features are currently out of scope for Kani," and data-race freedom is
explicitly unsupported
([feature support](https://model-checking.github.io/kani/rust-feature-support.html)),
so a block whose safety argument depends on how it is used concurrently, or
on aliasing across separate calls, is not established by a Kani harness
alone
([loop unwinding](https://model-checking.github.io/kani/tutorial-loop-unwinding.html)).

**What it covers, and why not more.** Three pieces, proved in Verus: log
record framing and recovery truncation (§9.2 — a torn or corrupt
**unacknowledged tail** record must be detected and discarded without
touching previously acknowledged bytes; corruption discovered in an
already-acknowledged record is the separate case §9.2 and layer 4 below
treat as a surfaced recovery failure, never a silent discard);
transaction-summary composition's associativity (§3.3 — the algebraic
property the parallel prefix scan depends on for correctness, not merely
performance); and lock-order deadlock freedom (§4's canonical ordering — the
property that makes deadlock structurally impossible rather than merely
detected and retried). Kani covers every `unsafe` block anywhere in the
engine with no exception — each gets its own harness with a stated domain
and unwind bound, and the harness's unwinding assertions must pass before
the proof counts. A block whose safety argument rests on concurrency or on
aliasing across separate calls is outside what a single Kani harness checks;
that block's safety is established another way (a Verus proof, or
documented manual review) or the block is redesigned to avoid needing a
concurrency- or aliasing-dependent argument at all.

The whole engine is **not** verified this way, and should not be: proof
effort here is reserved for code whose failure mode is a subtle, silent
violation of a correctness argument that testing is poorly suited to find —
a deadlock that only a specific lock-acquisition order triggers, a
compaction edge case in associativity that only shows up after many
combines, a byte-range miscalculation that only corrupts data already
promised durable. Everything else in the engine — the compiler's front
ends (§2), the certificate checker's search heuristics, the export
adapters (§10), the benchmark harness — fails, when it fails, by producing a
wrong-but-plausible answer or an outright crash, which layers 3 and 4 below
catch more cheaply by comparing against a reference than a full functional
proof would.

**When it starts.** From the first engine slice that implements one of the
three covered pieces — there is nothing to prove before that code exists.
Kani starts the moment the first `unsafe` block is written, whenever in the
project that happens.

### Layer 3 — The workhorse: property-based differential tests

**What it proves.** That kiln's public API and a simple, obviously-correct
**serial reference model** (a small, unoptimized implementation that applies
every transaction one at a time, with no parallelism and no reservation
machinery, kept solely to be compared against) produce identical results
when the reference model replays the engine's **actual commit order** — the
real sequence in which calls in a batch committed, as the engine itself
emits it, including every call that failed or was aborted — rather than
some independently chosen serial order. This is why a legally ambiguous
outcome (two sign-ups racing for the same email, where either could legally
win) is still a sound check: the comparison target is whichever order the
engine itself actually produced, replayed call for call, never a canonical
order picked in advance that could reject a result no less legal than the
one it expected. An outcome still unresolved at comparison time (an
`fsync`-failure case, `docs/concept.md` §9.2) is compared only after
recovery resolves it to a known commit or a known abort. A **property-based
testing library** generates many random workloads automatically instead of
a human writing each test case by hand. When a generated workload finds a
disagreement, the library **shrinks** it to the smallest workload that
still fails, far easier for a person to read than the original random
case.

**What it cannot prove.** Anything about crash behavior — this layer injects
no faults, so it says nothing about what happens if the process dies
mid-commit. It also gives no exhaustiveness guarantee: a random generator
can run for a long time without ever producing the one interleaving that
would expose a given bug, unlike layer 1's exhaustive search of a small
model.

**What it covers.** Every compatibility class from `docs/concept.md` §3
(independent, reservation-compatible, ordered-batch, and conservative
fallback), the reservation mechanisms of §3.2/§5 (claim, pin, escrow,
including the tentative-versus-committed distinction), and every
certificate-admitted specialization from §6 — each is run against the
reference model and must match on every response, every failure, and the
final state, not merely "look right."

**Tool.** A property-based testing library (for example, `proptest` in
Rust), driving kiln's public API as the entry point, never re-implementing
kiln's logic inside the test.

**When it starts.** From the first engine slice — this is the workhorse
layer and the first one that exercises real, running code.

### Layer 4 — Deterministic simulation with fault injection

**What it proves.** That across many seeded, deterministic replays — a
sample, not an exhaustive search of every possible interleaving and fault
timing, which is layer 1's job over a smaller abstract model — kiln's
durability and serializability promises still hold after any fault the
simulator injects. A **deterministic simulator** replaces the real disk,
clock, and thread scheduler with simulated versions driven entirely by a
single random seed, so the exact same run — same timing, same fault, same
interleaving — can be replayed exactly from that seed whenever a failure
needs to be reproduced or a fix needs to be checked.

Faults injected fall into two classes, because the storage failure model of
`docs/concept.md` §9.2 draws exactly this line. **Inside the declared
storage contract** — power loss at any write, a torn write into a record
not yet acknowledged (part of a new, unacknowledged write reaches storage,
part does not), an `fsync` error (the flush that is supposed to guarantee
durability reports failure), and disk full — every previously acknowledged
commit must survive, and the harness checks exactly that; "discarded"
without a trace is correct only for an unacknowledged tail record (one
never told to a caller as durable) torn by a crash before it was ever
acknowledged. **Outside the contract** — a bit flip in already-acknowledged
bytes, or a torn write that reaches already-acknowledged bytes — §9.2
states plainly that the engine cannot repair this; the harness checks that
recovery *detects* the damage via the record's framing and surfaces it as a
recovery failure, never silently skipping the damaged record or reporting a
clean recovery over lost data. A lying flush (the device reports success
for a write that did not actually reach storage) leaves no framing
corruption for recovery to detect when the loss is clean (no bytes
written); a lying flush can instead tear the new record's own bytes, which
framing validation detects like any other torn write — only the clean,
wholesale loss of an acknowledged append is framing-invisible and left to
the external harness
([SQLite's hardware assumptions](https://www.sqlite.org/atomiccommit.html#_hardware_assumptions)).
The engine guarantees only that its recovered history is a prefix of its
own recorded commit order, excluding any call still unresolved at the
crash (§5); separately, the harness, which independently keeps its own
ledger of acknowledged appends, compares that recovered prefix against it
and reports any acknowledged append missing as a loss.

After every injected crash, the harness checks: every commit the engine had
already acknowledged is present, for every inside-the-contract fault; an
outside-the-contract fault other than a clean lying-flush loss is surfaced
as a detected recovery failure, never silently absorbed; a clean
lying-flush loss is surfaced only by the external acknowledgment check
(§9.2); no transaction is left half-applied; every declared invariant
(uniqueness, foreign key, numeric bound) still holds; and full-history
replay, scoped to calls not already reported missing by that check,
matches layer 3's **commit-order replay** contract, with each call in the
trace carrying its engine outcome class: a semantic rejection replays
normally, an infrastructure-forced abort (disk full, or an `fsync` failure
resolved to abort) replays as a discarded no-op and the check is that the
engine's final state shows no effect of it, and any outcome still
unresolved at the crash is compared only after recovery resolves it. An
**Elle-style anomaly checker** — a tool that reconstructs the version
history of each piece of data from what reads actually returned and reports
a concrete cycle (a provable contradiction) when no valid serial order
exists — runs as an independent cross-check on histories shaped for it
(list-append-style keys Elle's inference supports), labelled **no predicate
coverage**: Elle detects every anomaly in Adya's formalism except predicate
anomalies, so a predicate-based invariant violation (the "doctors on call"
write-skew case) is caught by the invariant checks above plus the
commit-order replay, never relied on from Elle alone.

**What it cannot prove.** General correctness of a specialization's business
logic — that is layer 3's job; simulation explores timing and faults, not an
open-ended space of transaction semantics. It also cannot prove the
simulated disk model matches how a real disk actually fails; that is layer
5's job, and layer 5 itself only validates the model through a real OS
block-interface stack, not against arbitrary physical hardware failure
modes.

**What it covers.** Durability and recovery (§5, §9.1, §9.2), the
publication boundary's atomic visibility (§9.1), reservation atomicity
across a crash (§5), and migration crash safety (§8, §9.3) — anything whose
correctness depends on what happens when a fault strikes at an inconvenient
moment.

**Tool.** A custom deterministic simulator (the same technique used by
FoundationDB and TigerBeetle: simulate time, disk, and scheduling under one
seed so a failure is always reproducible) plus an Elle-style history
checker for the serializability check.

**When it starts.** From the first engine slice — specifically, once there
is a commit path and at least one derived structure that a crash can land
in the middle of.

### Layer 5 — A thin real-world layer

**What it proves.** That the layers above are testing something real, not
only an idealized model of it. Four distinct checks, each thin and
narrowly scoped on purpose: crash loops on a real **fault-injecting block
device** (`dm-flakey` or an equivalent — a kernel device-mapper layer that
injects dropped writes, I/O errors, and corrupted bytes into a real OS
block-interface stack
([dm-flakey documentation](https://docs.kernel.org/admin-guide/device-mapper/dm-flakey.html)))
to establish that the engine survives the fault classes layer 4 models when
they are delivered through that real block stack, not merely through the
simulator's own code — a narrower claim than confirming how a physical disk
actually fails, which would need separately obtained hardware evidence this
layer does not supply; differential runs against SQLite, comparing results
on the same workload, as an independent check beyond kiln's own reference
model; the benchmark harness that measures the performance claims of
`docs/concept.md` §12; and, only if the SQL front end named as an option in
§11 is actually built, reuse of **sqllogictest** (SQLite's own suite of
SQL-result correctness tests) to check SQL-level behavior specifically —
not before then, because kiln has no SQL layer to test until that option is
exercised.

**What it cannot prove.** Anything beyond the specific faults and workloads
actually run on real hardware; it is evidence that the model is realistic,
not an exhaustive check the way layer 1's model checking is — layer 4 is
seeded sampling too, not exhaustive. It is deliberately thin because real
hardware runs are slow and expensive compared to the simulator, so this
layer validates the model rather than replacing layers 3–4.

**Tool.** A real fault-injecting block device (for example, `dm-flakey` or
an equivalent), SQLite itself as a differential oracle, a benchmark harness,
and `sqllogictest` conditionally.

**When it starts.** Block-device crash loops start once layer 4's simulator
exists and needs validating against reality. SQLite differential runs and
the benchmark harness start once the first engine slice produces real,
measurable commits. `sqllogictest` reuse starts only if and when the SQL
front end option is taken.

## 2. E1, E2, and E3's lighter checks

E1, E2, and E3 (`docs/experiments.md`) do not get the full five-layer
treatment, because none of them has a runtime, a storage engine, or a kernel
to simulate or crash-test — each is scoped, by its own protocol, to exclude
exactly what layers 1, 2, 4, and 5 would check. Each uses a narrower,
already-specified check instead:

- **E1** uses the evidence requirement and the **lowering** and
  **containment audits** already defined in `docs/experiments.md`'s E1
  section: the classifier's output must carry checkable evidence, and both
  audits run before the first result counts and again on every change that
  reclassifies transactions — the lowering audit re-checking a sample of
  the newly reclassified transactions against the reference semantics of
  `docs/concept.md` §2.1/§6, the containment audit confirming the
  resources each one actually touched stay within its inferred footprint.
  This is a lighter, sampled form of layer 3 (differential checking against
  reference semantics) — lighter because E1 has no engine to run the
  classified transactions against beyond that sample.
- **E2** uses the **correctness-comparison** requirement already defined in
  `docs/experiments.md`'s E2 section: every call's response and the final
  state, compared element by element between the class-3 path and the
  serial-fold baseline, on every batch tested. This is layer 3's technique
  applied in memory only, with no persistence and no fault injection,
  because E2's own scope statement excludes persistence and crash recovery.
- **E3** uses the **mutant-rejection check** already defined in
  `docs/experiments.md`'s E3 section: for every family whose summary is
  admitted, construct semantic mutants (a deliberately wrong version of the
  same family) and confirm the checker of `docs/concept.md` §6 rejects every
  one. This is again layer 3's technique, applied to the certificate
  checker itself rather than to the engine.

## 3. The scenario matrix: data, not code

The matrix is **workload family × fault profile × size**:

- **Workload family** (5): transfers, sign-ups with a unique email, doctors
  on call, ticket-quota contention, navigational lookups.
- **Fault profile** (5): none, power loss, torn write, fsync error, disk
  full — all inside the declared storage contract (§9.2); an
  outside-contract fault (bit flip, lying flush) is not randomized here but
  runs as a mandatory named preset in §4 with its own oracle.
- **Size** (2): small, large.

That is 5 × 5 × 2 = 50 cells. Each cell is a row of **data** — a
configuration naming which family, which fault, which size — not a
hand-written test function. One generator reads a cell and randomizes
everything the cell does not pin down: the number of concurrent callers, the
concrete argument values, the order and timing of operations, and, for a
nonzero fault profile, which specific operation the fault lands on. One
**oracle** — the serial reference model of layer 3, replaying the engine's
own recorded commit order, with layer 4's invariant checks and Elle-style
checker as additional independent cross-checks — is reused unchanged across
every cell; only the generator's parameters change from one cell to the
next.

This is what avoids test explosion. The alternative — one hand-written test
per (family, fault, size, interleaving) combination — has no natural upper
bound, because the interleaving space is unbounded; that is how a suite
balloons to many times the size of the code it covers. Instead, one shared
harness runs the fixed 50-cell table with many random seeds per cell,
shrinking any failure to the smallest workload, interleaving, and fault
timing that still reproduces it; that shrunk failure is then pinned
permanently as a new entry in the named scenario catalogue below, which is
how coverage accumulates while the matrix itself stays fixed at 50 cells.

## 4. Named anomaly and regression scenarios

Each scenario names a concrete bad outcome, the invariant the harness checks
to catch it, and the `docs/concept.md` section whose correctness argument it
exists to guard. The first group is the classic isolation anomalies named in
the item body; the second group is the concrete counterexamples the
independent critique raised against earlier drafts of the concept, each
promoted to a permanent regression scenario so the fix it drove can never
silently regress. Unlike the matrix's random generator in §3, every scenario
here is a **mandatory, named preset**: each one runs every time,
deterministically, regardless of what a given run's random seeds happen to
cover — the matrix's randomization reduces duplicated test code across many
interleavings of the same few families, it does not by itself guarantee
that any specific named scenario below actually occurs.

| Scenario | What happens | Invariant checked after it | Guards (concept.md) |
|---|---|---|---|
| Lost update | Two transactions read-modify-write the same value concurrently with no coordination; one's write is silently overwritten by the other's. | Final state matches some valid serial order's result. | §5 (consistency/serializable isolation) |
| Dirty read | A transaction observes another transaction's uncommitted write. | Every observed read belongs to a committed transaction's state at some point in a valid serial order (Elle check). | §5 (atomicity: no reservation outcome visible before commit) |
| Read skew | A transaction reads two related values at two inconsistent points in time. | The full history admits a valid serial order (Elle check); readers never observe a commit's effects partially. | §9.1 (publication boundary), §5 |
| Write skew | Two transactions each check a precondition over disjoint rows and both commit, jointly violating a cross-row invariant (the "doctors on call" example). | The invariant holds after every batch the kernel evaluates with a transaction summary. | §3.3 (associative summary + prefix scan), §5 |
| Phantom from a range read against a reservation insert | A class-4 reader holds a range lock while a class-2 insertion, using only its claim, inserts a new row inside that range; repeating the range read sees the new row. | A range read returns the same row set if repeated within one transaction; an insertion into a locked range is ordered behind the range lock (next-key locking). | §4 (one resource namespace, next-key locking) |
| Uniqueness race: a tentative claim whose holder later aborts | Two sign-ups race for the same email; the loser must not fail with "already exists" until the winner's claim is committed, not merely held. | A uniqueness failure returned to a caller is always justified by a real committed order; if the holder aborts, the waiter proceeds as if the value was never claimed. | §5 (tentative vs. committed reservations) |
| Foreign-key race: parent delete against child insert | A child row is inserted and committed, and its insert transaction (and its tentative pin) is long finished, before a parent-deletion transaction runs. | A parent row is never deleted while a durable reverse reference naming a live child still exists. | §5 (durable reverse references) |
| Foreign-key race: concurrent parent delete and child insert | A parent-deletion transaction and a child-insert transaction for that same parent start together, each observing the state before the other's effect. | The two transactions never both commit: either the delete is ordered behind the child's pin and fails to find an empty reverse-reference set, or the insert is ordered behind the delete and fails to find the parent live. | §5 (durable reverse references), §4 (one resource namespace) |
| Escrow at exactly zero | A draw that would take the remaining quantity to exactly zero, not below it. | The sum of committed escrow draws never exceeds the ceiling; the exact-boundary case is exercised, not only overflow past it. | §3.2, §5 |
| Partial live publication of a multi-key commit | A commit touching two derived structures, A and B, becomes visible to a concurrent reader as (new A, old B). | For any commit touching more than one derived structure, no reader ever observes some but not all of its effects. | §9.1 (publication boundary) |
| A specialization that matches its footprint but computes the wrong value | An AI-proposed specialization (for example, subtracting two instead of one) touches exactly its declared footprint, so the footprint-tag check passes, but its result differs from the reference semantics. | The specialization's response and resulting state equal the reference model's on every differential run; a passing footprint-tag check is never by itself treated as evidence of correctness. | §6 (trust model and certificate) |
| Crash during an append | A crash lands while a new record is being appended, with an earlier record already acknowledged as durable sitting in the log. | A new record never lands inside a byte range already told to a caller as durable. | §9.2 (storage failure model) |
| A bit flip in already-acknowledged bytes | A fault injector flips a byte inside a record already acknowledged as durable, after the acknowledgment, independent of any new write. | Recovery detects the corruption via the record's framing checksum and surfaces it as a recovery failure; the engine never silently serves, or silently skips, the corrupted record as if it were intact. | §9.2 (storage failure model) |
| A lying flush loses an acknowledged append | The injector reports flush success for a targeted write that never reaches storage, then crashes. The harness keeps its own ledger of acknowledged appends. | The recovered history is a prefix of the recorded commit order; every present record passes framing validation (a torn record is a detected-corruption failure, not a loss); the only acknowledged appends missing are a contiguous suffix ending at the targeted write, which the harness reports as "detected loss, N appends", distinct from corruption. | §9.2 (storage failure model) |
| A backfill missing a commit after the snapshot point | A migration's backfill replays the log to its recorded watermark, but a commit lands before the catch-up phase starts tracking it and is silently absent from the new structures. | After the barrier commit, the new structures reflect every commit up to and including the barrier, with none missing and none duplicated. | §9.3 (migration watermark) |
| Rollback after a relaxed invariant | V2 relaxes `balance ≥ 0` and commits a negative balance; the operator then rolls back to V1, whose invariant forbids that balance. | Rollback is refused once a post-relaxation commit violates the old invariant, or is only offered inside the defined window before such a commit occurs — never a silent "switch back" that leaves an invalid committed row. | §9.3 (rollback window) |
| Crash at every commit step | The fault injector crashes at every distinguishable point in the commit sequence: before log append, after append before fsync, after fsync before derived-structure install, mid-install, and after install before the visible-version pointer advances. | After each crash and recovery: every acknowledged commit is present, no transaction is partial, every invariant holds, and the full history is serializable. | §5, §9.1, §9.2 |
| Crash in the middle of a migration | The fault injector crashes during backfill, during the catch-up phase, and exactly at the barrier commit. | After recovery, the migration either resumes correctly to the same end state, or the system is left cleanly on the pre-migration version with no reader ever observing a mix of old and new structures outside the backfill's declared catch-up contract. | §8, §9.3 |

Which of these need an injected fault to trigger and which do not is stated
by each scenario's own "What happens" description above, not by a blanket
count: lost update, write skew, the phantom from a range read against a
reservation insert, both foreign-key-race presets, the uniqueness race,
escrow at exactly zero, partial live publication of a multi-key commit, the
specialization-correctness scenario, the backfill race (an ordinary timing
race in the catch-up phase, not a fault), and rollback after a relaxed
invariant all need no injected fault — each is caught by the invariant
check named in its own row above, most of them via layer 3's commit-order
replay; dirty read and read skew are caught primarily by layer 4's
Elle-style checker; the remaining, fault-driven scenarios (crash during an
append, the bit flip, the lying flush, crash at every commit step, and
crash in the middle of a migration) are caught primarily by layer 4's
deterministic simulation. The kernel-ordering and publication-boundary
scenarios are additionally modeled in layer 1 before any code exists, so a
design flaw in those two is caught before it is ever implemented.

## 5. What kiln borrows from SQLite, and why not its test files

kiln borrows SQLite's **test categories**, confirmed directly from
[SQLite's testing documentation](https://www.sqlite.org/testing.html):

- **Crash testing** — simulating a process or power crash mid-write and
  checking the database is not corrupted on restart.
- **I/O error testing** — injecting a simulated I/O error at a chosen point
  and checking the database still passes an integrity check afterward.
- **Out-of-memory testing** — failing a chosen memory allocation and
  checking the engine fails that operation cleanly rather than corrupting
  state.
- **Malformed-file fuzzing** — starting from a well-formed database, flipping
  bytes outside the engine's own control, and checking the engine reports a
  clean error rather than overflowing a buffer or dereferencing a null
  pointer.
- **Boundary value testing** — pushing values to the edge of every declared
  limit and checking behavior there is still correct.
- **Branch-coverage discipline** — treating every branch in the core code as
  something a test must actually exercise, not merely a line-coverage
  percentage.

kiln does not borrow SQLite's **test files**, for two reasons the same page
states directly. First, SQLite's most thorough suite is not public: "The
[TH3](https://www.sqlite.org/th3.html) test harness is a set of proprietary
tests, written in C that provide 100% branch test coverage... to the core
SQLite library" — there is nothing to reuse. Second, SQLite's public test
suite targets SQL-language conformance details (parser edge cases, specific
`PRAGMA` behavior, SQL function semantics) that do not apply to kiln's core
engine, which has no SQL layer unless the optional SQL front end of
`docs/concept.md` §11 is built — and even then, `sqllogictest` (layer 5)
already covers that case specifically, rather than SQLite's own test files.

## 6. Precedents

Each precedent below was checked against its actual source before being
cited here. Two of the four item-body precedents needed a correction; both
corrections are stated below rather than silently fixed.

**VeriBetrKV, OSDI 2020 — confirmed as described.** Hance, Lattuada,
Hawblitzel, Howell, Johnson, and Parno, "Storage Systems are Distributed
Systems (So Verify Them That Way!)," USENIX OSDI 2020
([PDF](https://www.andrew.cmu.edu/user/bparno/papers/veribetrkv.pdf)).
VeriBetrKV is a crash-safe key-value store built on a B<sup>ε</sup>-tree,
verified in Dafny: "TLA-based reasoning lets us prove VeriBetrKV's correct
behavior under process crashes and under disk sector corruptions, while
Hoare logic allows us to reason independently about the implementation-level
optimizations needed for performance." This matches what the item body
assumed — a Dafny-verified, crash-safe key-value store — exactly, and is a
direct precedent for kiln's layer 1 (crash-safety modeling) plus layer 2
(code-level proof of a storage kernel).

**Amazon's use of TLA+ and of P — confirmed, but documented a decade
apart, not as one source.** TLA+: Newcombe, Rath, Zhang, Munteanu, Brooker,
and Deardeuff, "Use of Formal Methods at Amazon Web Services" (2014),
condensed as "How Amazon Web Services Uses Formal Methods," *Communications
of the ACM* (2015) — summarized with links at
[lamport.org/tla/industrial-use.html](https://lamport.org/tla/industrial-use.html)
— the original account of AWS using TLA+ model checking since 2011 to find
design bugs other techniques missed. The P language is not covered by that
paper; it is documented separately, a decade later, in Brooker and Desai,
["Systems Correctness Practices at AWS,"](https://cacm.acm.org/practice/systems-correctness-practices-at-amazon-web-services)
*Communications of the ACM*, Vol. 68, No. 6 (June 2025), pp. 38–42, which
surveys AWS's broader correctness toolkit, including P, property-based
testing, and fuzzing. The item body cites "Amazon's use of TLA+ and of P" as
one bundled precedent; the two techniques are in fact documented in two
separate papers published roughly ten years apart, not a single combined
source.

**Anvil, OSDI 2024, "with Verus" — corrected.** Anvil itself is real: Sun,
Ma, Gu, Ma, Chajed, Howell, Lattuada, Padon, Suresh, Szekeres, and Xu,
"Anvil: Verifying Liveness of Cluster Management Controllers," OSDI 2024,
pp. 649–666 (Jay Lepreau Best Paper Award;
[program listing](https://atcosdi24.sched.com/event/1fLhu/anvil-verifying-liveness-of-cluster-management-controllers)),
and it is built on Verus. But Anvil verifies **liveness** (a property called
"eventually stable reconciliation") of Kubernetes cluster-management
controllers written in Rust — verified case studies for ZooKeeper, RabbitMQ,
and FluentBit operators — not storage or crash safety. It is not a precedent
for kiln's planned use of Verus on log framing, summary associativity, or
lock-order deadlock freedom. The paper that actually contains the relevant
precedent — a crash-safe storage case study — is a different paper at a
different venue: Lattuada et al., "Verus: A Practical Foundation for Systems
Verification,"
[SOSP 2024](https://www.microsoft.com/en-us/research/publication/verus-a-practical-foundation-for-systems-verification/)
(not OSDI), whose evaluation's case studies include a crash-safe storage
system alongside an OS page table, a NUMA-aware concurrent data-structure
replication library, and a concurrent allocator, totaling 6.1K lines of
implementation and 31K lines of proof. Two corrections, both worth keeping
straight: Anvil (OSDI 2024) verifies controller liveness, not storage; the
Verus paper with the storage case study is at SOSP 2024, not OSDI.

**Elle, cited as "VLDB 2020" — corrected: the PVLDB issue was published
November 2020, a different, both-true fact from "VLDB 2021," the correct
citation for the conference proceedings volume it belongs to, held the
following year
([front matter PDF](https://www.vldb.org/pvldb/vol14/FrontMatterVol14No3.pdf)).**
Kingsbury and Alvaro, "Elle: Inferring Isolation Anomalies from
Experimental Observations," *Proceedings of the VLDB Endowment*, Vol. 14,
No. 3
([paper page](https://www.vldb.org/pvldb/volumes/14/paper/Elle%3A%20Inferring%20Isolation%20Anomalies%20from%20Experimental%20Observations)).
What the paper confirms, independent of the year: Elle infers a dependency graph of
client-observed transactions (in Adya's formalism) purely from the values
reads return, detects every anomaly in that formalism except predicate
anomalies, and was evaluated against real databases including a case study
of four systems. This matches what this document's layer 4 needs an
"Elle-style anomaly checker" to do.

**SQLite's testing page — confirmed, quoted in §5 above.**
[sqlite.org/testing.html](https://www.sqlite.org/testing.html) states TH3's
proprietary status and documents each borrowed test category directly; see
§5 for the exact quotations.
