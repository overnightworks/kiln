# TLA+ model of kiln's kernel

Verification layer 1 of `docs/verification.md` (design proofs, checked with
TLC). Slice 1 (issue #16, cut from #6) built resources, admission and
typed reservations in one composed state space; #24 added escrow
admission by aggregate budget after slice 1 found a wait cycle among
escrow draws. Liveness under stated weak fairness is #38. Crash, recovery and
unknown commit outcomes are #51 (`Durable.tla`); the publication pointer of
concept §9.1 is a later slice of #6.

## Tool

TLC from the `tla2tools.jar` release bundled by the TLA+ Foundation,
fetched outside the repository and verified by checksum before use.

- Version: TLC2 2.19 of 08 August 2024 (rev: 5a47802)
- SHA-256: `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`
- Runtime: Java 21 (Temurin 21.0.12.1)

Run a scenario from anywhere (`-workers 4` is the repository's cap; TLC's
default deadlock check stays on):

```sh
export TLA2TOOLS=/path/to/tla2tools.jar
specs/kernel/check.sh Composed                          # -> Composed.tlc.log
specs/kernel/check.sh Phantom Phantom.witness.PhantomUnreached.cfg
specs/kernel/mutants/run.sh NoAdmissionGuard Composed Serializable
```

Every log is committed next to its configuration. `check.sh` exits with
TLC's code: 0 for a pass, 12 on a property violation and 11 on a
deadlock — the expected outcome of every witness run and of every mutant
run.

## specs/kernel/Kernel.tla — the model

One module holds the protocol; a scenario module (`Composed.tla`,
`Phantom.tla`, ...) instantiates its two constants, `Prog` (each
transaction's fixed program) and `InitDb` (the committed rows before any
transaction runs).

**One namespace, one table.** Keys `1..3` name rows of a single table
whose row carries `val` (a plain column, e.g. tickets sold), `bal` (the
escrow-guarded quantity, e.g. remaining quota) and `parent` (a foreign key
into the same table, 0 for none). The reverse-reference set of §5 is
derived from the rows (`Refs`), so it is durable exactly as long as the
child row exists.

**Resources.** `[kind, cov]`, where `cov` is the set of keys covered: a
point `{k}`, a range `lo..hi`, the whole key space for a table resource.
Kinds: `S` and `X` point locks, `RS` shared range/table lock, `CLAIM`
(unique insert), `PIN` (foreign-key reference), `DEL` (exclusive against a
parent's reverse-reference set), `DRAW` (escrow). Two resources with
disjoint coverage never contend; with overlapping coverage (equal keys, or
a point under a covering range or table resource) only these pairs are
compatible: `S`/`S`, `S`/`RS`, `RS`/`RS`, `PIN`/`PIN`, `DRAW`/`DRAW`. Every
other overlapping pair conflicts. §4 and §5 state the rules this encodes:
equal keys conflict, a point conflicts with a covering range or table
resource, a claim is checked against any range covering the point it
inserts into, pins count references and so coexist, draws share a balance
through their reservations, a delete is incompatible with the pins on the
parent. Where the concept is silent the rule is conflict (see "Concept
findings").

**Operations** (`ApplyOp`, one pure function used by both the live protocol
and the serial reference): `Read(k)` under `S`; `Scan(lo, hi)` under `RS`,
returning the present keys and values; `Update(k, val, bal)` under `X`,
returning the previous row; `Insert(k, p)` under `CLAIM` on `k` plus `PIN`
on `p`, rejected with `exists` or `noparent`; `Delete(k)` under `DEL` on
`k` plus `PIN` on the row's own parent while it unlinks, rejected with
`referenced`; `Buy(k, a)` under `DRAW`, drawing `a` from `bal` and adding
one to `val`, rejected with `insufficient`; `BuyUnder(k, a, cap)`, the
same draw under `DRAW` and `S` on `k`, taken only while `val < cap` and
answered `skipped` otherwise (a branch on the row); `BuyVal(k, j)` under `DRAW` on `k` and `S` on `j`, drawing row
`j`'s `val` from `k` (an amount the program does not know statically).

**Protocol.** A transaction starts `pending`, holding nothing. `Dispatch`
gives it the next sequence number: it is now queued for admission.
`Acquire` takes its complete footprint — every resource its program needs
(`Footprint(t)`, below) together with its escrow reservations — in one
step, atomically with the compatibility check against every live holder
and the escrow admission check. This is the model's abstraction of §4's
atomic-at-dispatch registration: a keyed resource is registered
"atomically at dispatch, held through commit", and a covering resource is
acquired "before any read or write touches the underlying state". An
implementation may acquire the same set piece by piece; the step abstracts
that sequence, so the model never holds a partial footprint. The transaction is then `active` and `DoOp` executes
its program in order on its view — committed rows overlaid with its own
tentative writes and draws — acquiring nothing more and waiting for
nothing (§4, rule 1: an admitted transaction never waits again). A
rejection is a constraint failure returned to the caller: the transaction
ends as `rejected`, releases everything, its unused reservation included,
and leaves no effect (§5, known abort). `Commit` makes the view the
committed state and releases every resource and reservation (strict
two-phase locking, admission to commit). Escrow draws are deltas, not
overwrites: two committed draws subtract twice. There is no other abort:
the only ways a transaction ends are commit and constraint rejection.

**Escrow budgets and reservations (§4, rules 1 and 2).** `Budget(t, k)` is a
program's statically known total draw on key `k`, the sum of its draw
amounts there; a branch counts in full whether or not it is taken, and a
`BuyVal` makes the budget unknown, which counts as unbounded. At admission
the transaction reserves `min(budget, committed balance)` on each of its
escrow keys (`Reservation`). Live reservations are funded from the committed
balance, so a transaction is admitted only while every other live
reservation on each of its escrow keys leaves room for its own
(`EscrowBlocked`); a reservation capped at the committed balance — an
unfundable or unknown budget — is therefore admitted only once every other
reservation on that key has resolved, and no reservation at all is taken
until the whole footprint, every escrow key included, can be funded. A draw
consumes the transaction's own reservation: the balance a draw sees
(`DrawView`) is what remains of that reservation, never the committed
balance other holders draw from, so a transaction that does not lower the
balance itself never fails an uncapped draw (its own `Update` can lower it
before a draw, a valid failure) and a capped one fails only on committed
state, exactly as the serial order "earlier holders, then this transaction"
does. One exception, one rule (`SoleDrawHolder`, shared by `DrawView` and
`DrawsWithinReservation`): a transaction holding a covering resource on the
key incompatible with `DRAW` (`X`, a claim, a delete, a read) is the key's
only draw holder and draws on its own view; that is how an `Update` that
raised the balance in the same transaction is drawn from, the one case a
draw exceeds the reservation when draw amounts are nonnegative (every
scenario's are; the model does not constrain the parameter). Reservations
cover blind draws only: `BuyUnder` branches on the row's `val`, so it needs
`S` beside `DRAW` and is ordered against every other draw on the key
(`ConcurrentBranch`).

**Admission order (§4, rule 3).** Queued transactions are ordered by
dispatch sequence: a queued transaction waits on every older queued
transaction whose footprint shares a resource with its own (overlapping
coverage, compatible or not), so a later request never bypasses an older
queued request on a resource they share, while disjoint footprints
proceed independently. The model checks only that this order is safe
(no cycle, no deadlock, serializable); that every queued transaction is
eventually admitted is the liveness obligation below.

**Waiting.** `WaitsOn(t)` is derived from the state. A queued transaction
waits on every live holder of a resource incompatible with its footprint,
on the reservation holders that leave no room for its own reservation, and
on the older queued transactions sharing a resource with it. An active
transaction waits on nobody. A transaction steps only when it waits on
nobody, so a loser behind a tentative claim waits for the holder's outcome
and is rejected with `exists` only once the holder has committed.
`WaitFor` is the relation, `NoWaitCycle` its acyclicity. A cycle is
impossible by construction: a waiter holds nothing, an active transaction
never waits, and waits between queued transactions point from younger to
older. The `ActiveAllowanceWait` mutant restores slice 1's wait of an
active draw holder on another holder's tentative draw; TLC brings the
`AllowanceCycle` cycle back.

**The footprint, computed statically.** `Footprint(t)` is the union of the
resources each operation of `t`'s program needs, so a program needing two
modes on one key (`Scan(1,2); Update(1,0,0)`: `RS` over 1..2 and `X` on
1) holds both, which is the strongest mode it needs. Only a delete's
resources depend on the view (the pin on the parent its row names); a
parent is set by `InitDb` or by an `Insert` in some program, so a delete's
footprint pins every parent the row can have in the scenario. That
over-approximation costs at most a spurious conflict, never a missed
resource: the invariant `FootprintCovers` checks on every reachable state
that an active transaction holds everything its next operation needs on
the view it runs on. The order of a program's operations is immaterial to
admission — `OutOfOrder`, `FootprintGap` and `ScanUpgrade`, which deadlock
when resources are registered as each operation runs, are ordinary
checked scenarios here, and `DeleteReinsert` is the program on which an
earlier version of this model, registering per operation and ordering
over the static footprint, credited a transaction with a pin it never
held and admitted a deadlock. The `PerOperationAcquisition` mutant
reintroduces that registration; TLC refutes it on all four.

**Widen-and-restart (§4, "lock expansion restarts safely", #46).**
`Follow(k)` reads row `k` and the row its `val` names, so the second
resource is decided by data. `Footprint(t)` guesses it from `InitDb`;
`widened[t]` holds what earlier attempts found missing, and admission
requests `Requested(t) = Footprint(t) \cup widened[t]`. `DoOp` runs only
when `Covered(t)`: the held footprint contains `Need(NextOp(t), View(t))`.
Otherwise `WidenAndRestart(t)` is the only step of the running transaction:

```
WidenAndRestart(t) ==
  /\ Running(t) /\ ~Covered(t)
  /\ widened' = [widened EXCEPT ![t] = @ \cup Need(NextOp(t), View(t))]
  /\ status' = pending, seq' = 1 + (number of queued), pc' = 0, resp' = <<>>
  /\ held', reserved', wr', draws' = nothing; db unchanged
```

The call holds nothing afterwards and joins the admission queue at its
tail, so queue order, conflicts and escrow reservations apply to it as to
any request. Its `resp` is cleared, so the response it finally gives is
the one of its last attempt, which `Serializable` checks exactly. Every
restart adds a resource not yet requested (the held footprint contains
`widened`), a subset of a finite set, so restarts are finite; the step is
weakly fair like the others. `FootprintCovers` now exempts `Follow`
only. `Rearrive` clears `widened`.

**Serializability, checked for real.** `Serializable`: for the transactions
that have ended (committed or rejected), some permutation, executed
serially from `InitDb` with the same `ApplyOp`, returns exactly the
responses each caller received, ends each transaction with exactly the
terminal status it reached (committed, or rejected with its failure as
the last response), and produces exactly the committed state. A rejected
transaction takes part with its failure response and no effect, so a
premature failure (one no serial order would produce) is a violation
(`PrematureFailure`), and so is a rejection recorded as a commit
(`InsufficientCommitted`). Permutations are enumerated (at most 6 for 3
transactions).

**What the oracle cannot see.** The serial reference runs the same
`ApplyOp` as the live protocol, so it catches every divergence caused by
interleaving but cannot validate the operation semantics themselves: a
wrong `ApplyOp` is wrong on both sides and passes. Those semantics are
pinned only by the state invariants below (`Uniqueness`,
`ReferentialIntegrity`, `BalanceNonNegative`) and by the witnesses, which
name the concrete responses they expect. `resp` entries of a still-active
transaction are buffered observations the caller has not been shown yet;
publication is a later slice, and `Serializable` compares only ended
transactions.

**Other properties.** `MutualExclusion` (no two live transactions hold
incompatible resources), `FootprintCovers` (above), `ReservationsFunded`
(per key, the live reservations sum to at most the committed balance),
`DrawsWithinReservation` (a transaction never draws more than it reserved
unless it is the key's sole draw holder, `SoleDrawHolder` in concept §4),
`Uniqueness` (per key, committed successful inserts never exceed committed
deletes plus one), `ReferentialIntegrity` (a present row's parent is
present), `BalanceNonNegative`, `TypeOK`, and TLC's deadlock check (a state
with a live transaction and no enabled step; `Terminating` self-loops once
every transaction has ended).

**Liveness under weak fairness (#38).** `Spec` is `Init /\ [][Next]_Vars /\
Fairness` with

```
Fairness == \A t \in Txns : WF_Vars(Acquire(t)) /\ WF_Vars(DoOp(t))
                      /\ WF_Vars(WidenAndRestart(t)) /\ WF_Vars(Commit(t))
```

A step of a transaction that stays enabled is eventually taken: admission,
execution, completion. Dispatch is the caller's request, not a step the
protocol owes, so it is not fair. Every scenario with a program of finite
length is checked against

```
RequestedLeadsToTerminal == \A t \in Txns : Queued(t) ~> ~Live(t)
```

a request, once dispatched, ends committed or rejected (a justified
rejection is an end: fairness cannot manufacture missing funds). It holds
on every scenario below. On finite programs that run once, though, it holds
for any deadlock-free protocol, bypass included: every transaction ends once
nothing blocks the last one. So the `Starvation` scenario closes the cycle:
`RepeatSpec` adds `Rearrive`, by which a finished transaction requests its
program again, so a transaction that is overtaken forever is a lasso. Its
history is unbounded, and `Rearrive` resets the transaction histories
that `Serializable` and `Uniqueness` read (`Serializable` compares a
history with `InitDb`, `Uniqueness` counts the retained committed
insert and delete responses), so these history-based checks do not suit
repeated executions and `Starvation` checks neither. Rearrival leaves a
transaction pending and undispatched; only the next `Dispatch` queues it
and starts the `RequestedLeadsToTerminal` obligation. The `NoBypass` mutant drops `OlderQueued` from
`WaitsOn` (rule 3) and TLC returns the lasso: t3 is admitted, commits and
requests again, over and over, while t1, queued before each of t3's
requests, is admitted in no state of the loop.

Queued sequence numbers are dense (1..n): `Acquire` closes the gap it
leaves, so the numbers stay bounded when requests repeat. This changed the
encoding, not the behaviour, of every earlier scenario; their state counts
below are from the rerun.

**Bounds.** 2-3 transactions with programs of 1-2 operations, 1-3 keys,
fixed programs per scenario; `Starvation` repeats its three single-operation programs without bound. The footprint is acquired as one step; the
order in which an implementation acquires it piecewise is not modelled.
Widening is modelled for one data-dependent read (`Follow`), not for a
widened range or table resource. Not covered: longer programs, more than
one table, the publication pointer. Liveness is checked only for the
fair steps above, on scenarios of 2-3 transactions; it makes no claim
beyond those bounds.

## specs/kernel/Durable.tla — crash, recovery and unknown outcomes

`Durable.tla` extends `Kernel.tla` with the failure model of concept §5 and
§9.2 (#51). `Kernel.tla` changes only by one status value (`aborted`, a
transaction discarded by recovery) and by `Ended` naming committed and
rejected transactions; no earlier scenario changes, and all were rerun
with identical state counts. Commit is split into the steps an engine
takes: `Enqueue(t)` puts the transaction's commit record (its tentative
rows and escrow deltas) in the volatile tail; `Flush` makes the oldest
record durable, and only then are its effects committed and its footprint
and reservations released; `Ack(t)` tells the caller, after `Flush`. The
durable `log` is a sequence of such records; `history` is what the engine
serves and `db = Replay(history)`.

Failures, at most `Failures` per run (a constant of the scenario), each
enabled only while the engine is up:

- `Crash`: the engine stops; of the tail, a prefix of any length
  (including none) reached storage and joins the log, the rest is lost.
- `FsyncFail` (tail not empty): the writer is fenced and every transaction
  in the tail is answered `unknown`; any prefix of the tail may have
  reached storage anyway. Those transactions keep their footprint and
  reservations (`status` stays `active`).
- `Recover` (engine fenced or down): `history` and `db` are rebuilt from
  the log alone. Every transaction with a durable record becomes
  `committed`, acknowledged or not; every other dispatched transaction
  becomes `aborted`; a request not yet made stays pending and runs on the
  recovered engine. A transaction that had run its program and was not
  acknowledged, or whose flush completed before the crash, is answered
  `unknown`.

A fenced or crashed engine does nothing but `Recover`: §5 resumes serving
only after recovery resolves the outcome.

Properties, checked as invariants of every crash scenario beside every
Kernel invariant (`Serializable` now ranges over the recovered committed
state and every ended transaction's exact responses):

| Property | Meaning |
|---|---|
| `AcknowledgedSurvive` | every transaction answered `committed` is in the engine's history and in the log |
| `RecoveredPrefix` | the engine's history is a prefix of the log, and equal to it while the engine is up |
| `NoKnownAbortAfterFsyncFailure` | no transaction whose commit met an fsync failure is answered `aborted` |
| `KnownAbortNotInLog` | nothing answered or recorded `aborted` has a durable commit record |
| `UnknownKeepsReservations` | until recovery, an unknown-outcome transaction is still `active`, holds its whole footprint and holds exactly the escrow reservations it held when the flush failed (recorded at the failure, so an unused part and a legitimate zero reservation are covered) |

`CrashFree` instantiates `Durable` with `Failures = 0` on the `Composed`
programs: every Kernel invariant and `RequestedLeadsToTerminal` hold when
commit is three steps and nothing fails. `Recovery` runs up to two
failures over t1: `Buy(1,1)`; t2: `Buy(1,1)`, `Insert(2,0)`; t3:
`Update(3,7,0)` (quota 2, key 2 absent): the two draws coexist, so two
records can sit in the tail at once. `RecoveryEscrow` holds a reservation
larger than its draw and a zero one through an fsync failure.
`RecoveryDependent` has three transactions on one key, so a response
depends on a predecessor whose commit survived or was lost.

**Bounds.** One log, one writer, three transactions, at most two
failures; a failure discards every volatile transaction. Recovery
discards every queued or active request; only a request not yet
dispatched stays runnable.

**Not proved.** `DFairness` covers the protocol steps and `Flush`, but
neither `Ack` nor `Recover`: a failure-free run may flush everything and
leave a caller unanswered forever, and a fenced engine may stay fenced.
`RequestedLeadsToTerminal` (checked on `CrashFree` only) speaks of
terminal transaction status, not of the caller's answer. Eventual
acknowledgement and eventual recovery are unproved, and no temporal
claim is made for them. Not covered: the
publication pointer (§9.1), torn writes that reach acknowledged bytes and
a device that lies about a flush (outside §9.2's model), retried commits
after an unknown outcome, storage layout. Records are abstract effect
deltas, not bytes: framing and checksums are not modelled.

## Scenarios and results

For every scenario TLC explores the whole state space against every
invariant its configuration lists and the deadlock check; every scenario
passes with no violation and no deadlock, with 4 workers. Each scenario
also passes `RequestedLeadsToTerminal` under `Fairness`. `Starvation`
(under `RepeatSpec`) checks `TypeOK`, `MutualExclusion`,
`FootprintCovers`, `ReservationsFunded`, `DrawsWithinReservation`,
`NoWaitCycle`, `ReferentialIntegrity`, `BalanceNonNegative` and
`RequestedLeadsToTerminal`; it is exempt from `Serializable` and
`Uniqueness` because `Rearrive` resets the histories they read. Every
other scenario checks `Serializable` and `Uniqueness` too. `CrashFree` and the three
`Recovery` scenarios check `DTypeOK` (`TypeOK` plus the durable state) in
place of `TypeOK`, and the `Recovery` scenarios check no liveness property (failures are not owed by
fairness). `CrashFree` checks `RequestedLeadsToTerminal` under `DSpec`'s
fairness.
Depth is the length of the longest shortest path (breadth-first).

| Scenario | What runs | Distinct states | Depth | Log |
|---|---|---|---|---|
| `Composed` | t1: `Buy(1,1)`, `Insert(2,3)`; t2: `Buy(1,1)`, `Delete(3)`; t3: `Scan(1,3)` (table) — claim, pin, delete, two draws on key 1 (quota 2) and a table scan in one key space; the pin on 3 conflicts with its delete and the scan with everything, so the three serialize: insert first rejects the delete with `referenced`, delete first the insert with `noparent`, and a draw from a later-rejected transaction leaves no effect | 127 | 14 | `Composed.tlc.log` |
| `Pins` | t1: `Delete(1)`, `Scan(1,3)` (table); t2: `Insert(2,1)`; t3: `Insert(3,1)` — two pins on parent 1 coexist against its delete, two claims meet the table scan | 110 | 12 | `Pins.tlc.log` |
| `RangePoint` | t1: `Scan(1,2)`; t2: `Read(2)`, `Buy(3,1)`; t3: `Update(2,1,0)` — a range scan and a point read coexist against an update; the draw on 3 lies outside the range | 138 | 14 | `RangePoint.tlc.log` |
| `Phantom` | t1: `Scan(2,3)` twice; t2: `Insert(2,0)` — round-1 phantom | 26 | 10 | `Phantom.tlc.log` |
| `EventClose` | t1: `Update(1,10,0)` on the event row (sold 10, quota 1); t2, t3: `Buy(1,1)` — the second draw's reservation waits on the first's, the update waits on both | 101 | 12 | `EventClose.tlc.log` |
| `ClaimAbort` | t1: `Insert(2,0)`, `Buy(3,1)` against an empty escrow; t2, t3: `Insert(2,0)` — tentative claim whose holder is rejected while claimants wait | 99 | 12 | `ClaimAbort.tlc.log` |
| `ParentDelete` | t1: `Delete(1)`; t2: `Insert(2,1)` — parent delete concurrent with child insert | 21 | 8 | `ParentDelete.tlc.log` |
| `OutOfOrder` | t1: `Buy(1,1)`, `Insert(2,0)`; t2: `Insert(2,0)`, `Buy(1,1)` — the programs touch keys 1 and 2 in opposite orders (quota 1); the claims conflict at dispatch and the second draw is rejected with `insufficient` | 25 | 9 | `OutOfOrder.tlc.log` |
| `FootprintGap` | t1: `Insert(3,1)`, `Update(2,0,0)`; t2: `Update(2,1,0)`, `Delete(3)` — in order by lowest key per operation (1, 2; 2, 3), out of order over the footprint; the `X` locks on 2 conflict at dispatch | 29 | 11 | `FootprintGap.tlc.log` |
| `ScanUpgrade` | t1: `Scan(1,2)`, `Update(1,0,0)`; t2: `Scan(1,2)`, `Update(2,0,0)` — each needs `X` on a key its own scan covers; range and `X` lock are taken together, and each `X` lock conflicts with the other's range | 28 | 11 | `ScanUpgrade.tlc.log` |
| `DeleteReinsert` | rows 1 and 2 present, no parents; t1: `Delete(2)`, `Insert(2,1)`; t2: `Delete(1)`, `Update(2,0,0)` — the reinsert makes 1 a possible parent of 2, so t1's footprint pins 1; `DEL 1` against `PIN 1` and `DEL 2` against `X 2` conflict at dispatch | 26 | 10 | `DeleteReinsert.tlc.log` |
| `AllowanceCycle` | t1, t2: `Buy(1,1)`, `Buy(1,1)` against quota 2 — slice 1's documented failure (each second draw waited on the other's tentative draw): now each reserves 2 at admission, the second is admitted once the first has resolved, capped at the remaining 0, and fails its first draw on committed state | 25 | 9 | `AllowanceCycle.tlc.log` |
| `BranchBudget` | t1: `BuyUnder(1,1,1)` twice against quota 2 (the second branch is never taken, the budget is 2); t2: `Buy(1,1)` — the untaken branch keeps the balance reserved until t1 commits; t2 then takes the ticket it left | 26 | 10 | `BranchBudget.tlc.log` |
| `ConcurrentBranch` | quota 2; t1, t2: `BuyUnder(1,1,1)` once each — both reservations are funded, but the branch reads the sold count, so the `S` lock orders them: one draws, the other skips | 23 | 9 | `ConcurrentBranch.tlc.log` |
| `Unfundable` | t1: `Buy(1,2)`, `Buy(1,1)` (budget 3) against quota 2; t2: `Buy(1,1)` — t1's reservation is capped at the balance and admitted only with no other reservation live; it draws its reservation and fails the rest with `insufficient` on committed state | 22 | 9 | `Unfundable.tlc.log` |
| `TwoEscrows` | keys 1 and 2 with quota 1 each; t1: `Buy(1,1)`, `Buy(2,1)`; t2: `Buy(2,1)`; t3: `Buy(1,1)` — admission is all-or-nothing across escrows: t1 waits on both holders reserving nothing, then is capped at 0 on both | 113 | 13 | `TwoEscrows.tlc.log` |
| `HolderAbort` | key 2 present; t1: `Buy(1,1)`, `Insert(2,0)` (rejected with `exists`, releasing its reservation); t2: `Buy(1,1)` twice against quota 2 — t2 waits instead of reserving the remainder and failing on it; after t1's rejection it reserves 2 and commits both draws | 25 | 10 | `HolderAbort.tlc.log` |
| `MixedModes` | quota 1; t1: `Update(1,0,2)`, `Buy(1,1)`; t2: `Buy(1,1)`, `Update(1,5,0)` — `X` and `DRAW` on one key in each footprint; the `X` locks serialize them; after t2 commits, t1 is capped at 0 and draws from the balance its own update raised | 29 | 11 | `MixedModes.tlc.log` |
| `UnknownBudget` | quota 2, row 2 with `val` 1; t1: `BuyVal(1,2)`; t2: `Buy(1,1)`; t3: `Update(2,3,0)` — an unknown budget reserves the whole balance; the draw amount t3 raises fails on committed state | 112 | 13 | `UnknownBudget.tlc.log` |
| `Starvation` | t1, t2, t3: `Update(1,0,0)` each, requested again after finishing (`RepeatSpec`, no `Serializable`) — three transactions contend for one `X` lock forever; dispatch order keeps every request from being overtaken | 98 | 13 | `Starvation.tlc.log` |
| `Widen` | key 1 points at 2; t1: `Follow(1)`; t2: `Update(1,3,0)`; t3: `Update(3,9,0)` — t1's guessed footprint is `S 1, S 2`; if t2 commits first t1 needs `S 3`, restarts, and waits for t3 if t3 holds key 3 | 135 | 15 | `Widen.tlc.log` |
| `WidenQueue` | the same pointer; t1: `Follow(1)`; t2: `Update(1,3,0)`; t3: `Update(2,5,0)` — t3 queues behind t1; when t1 restarts it re-enters behind t3 and t3 is admitted first | 129 | 15 | `WidenQueue.tlc.log` |
| `WidenCycle` | no pointers; t1: `Update(2,1,0)`, `Follow(2)`; t2: `Update(1,2,0)`, `Follow(1)` — each discovers the key the other holds from a pointer it just wrote; restarting discards the pointer and the two serialize | 65 | 17 | `WidenCycle.tlc.log` |
| `WidenEscrow` | quota 1; key 3 has no pointer; t1: `Buy(1,1)`, `Follow(3)`; t2: `Update(3,2,0)`; t3: `Buy(1,1)` — if t2 commits first, t1 has reserved and drawn the ticket when its follow needs `S 2`; the restart discards both, t3 (waiting on the reservation) takes the ticket and t1's buy then fails on committed state | 130 | 16 | `WidenEscrow.tlc.log` |
| `CrashFree` | the `Composed` programs on `Durable` with no failure — commit is append, flush, acknowledge; the Kernel invariants and `RequestedLeadsToTerminal` still hold | 273 | 18 | `CrashFree.tlc.log` |
| `Recovery` | t1: `Buy(1,1)`; t2: `Buy(1,1)`, `Insert(2,0)`; t3: `Update(3,7,0)` (quota 2) with up to two crashes or fsync failures at any step; checks the Kernel invariants and the five durability properties | 10891 | 24 | `Recovery.tlc.log` |
| `RecoveryEscrow` | balances 2 (key 1) and 0 (key 2); t1: `BuyUnder(1,1,1)` twice (reservation 2, draw 1); t2: `BuyUnder(2,1,0)` (skipped on an empty balance, reservation 0), one failure — the reservations an unknown outcome keeps are compared exactly | 337 | 16 | `RecoveryEscrow.tlc.log` |
| `RecoveryDependent` | key 1 present; t1: `Update(1,5,0)`; t2: `Update(1,6,0)`; t3: `Read(1)`, with up to two failures — t2's and t3's responses depend on t1, whose commit may be durable but unacknowledged, or lost; `Serializable` compares them exactly | 6864 | 23 | `RecoveryDependent.tlc.log` |

**Reachability witnesses.** Each scenario names negated state predicates
(`*Unreached`); TLC's counterexample is the proof that the scenario's
decisive state is reachable. Logs: `<Scenario>.witness.<Predicate>.tlc.log`.
Witnesses and mutants run with one worker and a fixed fingerprint
function, so their traces reproduce; trace lengths are the number of
states in the committed log.
Two witnesses (`ComposedPinDeleteUnreached`, `PinsTableUnreached`) once
asserted that two queued transactions each wait *only* on the holder;
under rule 3 the later-dispatched one also waits on the earlier, so they
assert membership.

| Witness | State reached | Trace length |
|---|---|---|
| `ComposedPinDeleteUnreached` | t1 holds its draw on 1 and its pin on 3; t2's delete and t3's table scan both wait on t1 | 5 |
| `ComposedReferencedUnreached` | t2 was told `ok, referenced`, t1 committed key 2 under parent 3, and key 1's quota is 1: the draw from the later-rejected transaction left no effect | 10 |
| `ComposedNoParentUnreached` | t1 was told `ok, noparent`, t2 committed the delete of 3, and key 1's quota is 1 | 10 |
| `PinsConcurrentUnreached` | t2 and t3 both hold a pin on 1; t1's delete waits on both | 6 |
| `PinsTableUnreached` | t1 holds the table scan with its delete; both claims wait on t1 | 5 |
| `PinsReferencedUnreached` | t1 was told `referenced`; keys 2 and 3 are committed under parent 1 | 12 |
| `RangePointUnreached` | t1 holds the range scan over 1..2, t2 the point read on 2 and the draw on 3; t3's update waits on both | 6 |
| `PhantomUnreached` | t1 holds `RS 2..3`, t2's claim on 2 waits on t1 | 4 |
| `EventCloseUnreached` | t2 holds its tentative draw; t1's X lock and t3's draw both wait on t2 | 5 |
| `ClaimAbortWaitUnreached` | t1 holds the tentative claim on 2, t2 waits on it | 4 |
| `ClaimAbortUnreached` | t1 answered `ok, insufficient` (claim released by the rejection), t2 committed key 2, t3 was told `exists` | 12 |
| `ParentDeleteUnreached` | t2 holds its pin on 1, t1's delete waits on it | 4 |
| `AllowanceCycleQueuedUnreached` | the queued case: t1 holds a reservation of 2 with nothing drawn, t2 is queued behind it on the escrow alone | 4 |
| `ConcurrentBranchSkippedUnreached` | both committed; one was told `ok`, the other `skipped`; quota 1 | 9 |
| `BranchBudgetOverReservedUnreached` | t1 skipped its second branch and still holds a reservation of 2 against a draw of 1; t2 waits on it | 6 |
| `BranchBudgetReleasedUnreached` | both committed; the skipped branch's ticket went to t2 | 10 |
| `UnfundableCappedUnreached` | the capped case: t1 holds a reservation equal to the committed balance 2 and below its budget 3; t2 is queued behind it | 4 |
| `UnfundableInsufficientUnreached` | t1 was told `ok, insufficient` after drawing its reservation; t2 committed the ticket the rejection released | 9 |
| `TwoEscrowsAllOrNothingUnreached` | t2 and t3 each hold one key's reservation; t1 waits on both holding nothing | 6 |
| `HolderAbortReleasedUnreached` | t1 was rejected with `ok, exists`; t2, which waited on it, holds the whole balance | 7 |
| `MixedModesRaisedUnreached` | t1 drew one against a reservation of 0 under its X lock | 10 |
| `UnknownBudgetUnboundedUnreached` | t1 holds the whole balance under an unknown budget; t2 waits on it | 4 |
| `StarvationQueuedBehindUnreached` | t1 holds the key; t2 and t3 are queued behind it, t2 first | 5 |
| `WidenRestartedUnreached` | t2 committed the repointing; t1 restarted with a widened footprint and holds `S 3` | 9 |
| `WidenConflictUnreached` | t1 restarted and is queued waiting on t3, which is active and holds `X 3` | 10 |
| `WidenQueueBehindUnreached` | t1 restarted and is queued behind t3, which was waiting on it | 9 |
| `WidenCycleBothUnreached` | both calls widened and both ended | 17 |
| `WidenEscrowDrawnUnreached` | t1 is about to restart with its whole balance reserved and drawn (reservation 1, draw 1) while t3 waits on that reservation | 9 |
| `WidenEscrowDiscardedUnreached` | t1 restarted: it is queued and widened with reservation 0 and draw 0 | 9 |
| `UnknownBudgetInsufficientUnreached` | after t3 raised the amount to 3, t1's draw failed on the committed balance 2 | 8 |
| `RecoveryDurableUnackedUnreached` | the engine crashed after t1's flush made its record durable and before t1 was acknowledged | 7 |
| `RecoveryUnackedCommittedUnreached` | recovery serves t1 as committed and answers `unknown`, with no fsync failure: t1's record was in the tail when the crash persisted it (Enqueue, Crash, Recover; no Flush) | 7 |
| `RecoveryFencedUnreached` | a flush failed: the engine is fenced, t1 is answered `unknown`, still active and holds its escrow reservation | 6 |
| `RecoveryResolvedCommittedUnreached` | an fsync failure, then recovery: t1's unknown outcome resolved to committed, its record had reached storage | 7 |
| `RecoveryResolvedAbortedUnreached` | an fsync failure, then recovery: t1's unknown outcome resolved to aborted, its record had not | 7 |
| `RecoveryEscrowFencedUnreached` | the engine is fenced with both outcomes unknown; t1 holds a reservation of 2 against a draw of 1, t2 a reservation of 0 after a skipped branch | 11 |
| `RecoveryDependentSeesUnackedUnreached` | t1 durable and unacknowledged (`unknown`) after a failure; t2 committed and was told the row t1 wrote | 12 |
| `RecoveryDependentIgnoresLostUnreached` | t1's record was lost (`aborted`); t2 committed and was told the initial row | 8 |

## Mutants (specs/kernel/mutants/)

Each mutant is a patch against `Kernel.tla` or `Durable.tla` that removes one named guard;
`run.sh` applies it to a temporary copy and checks exactly one property
under the scenario's own constants.
TLC refutes every one; the log is `<Mutant>.<Scenario>.<Property>.tlc.log`.
`run.sh` uses one worker and a fixed fingerprint function; the trace
lengths below are the states of the committed logs.

| Mutant (guard removed) | Scenario | Property refuted | Trace length |
|---|---|---|---|
| `NoAdmissionGuard` — `Conflicting` is empty, nothing ever waits | `Composed` | `Serializable` (t1 inserts the child of 3 and t2 deletes 3, both told `ok` and both committed: no serial order yields both) | 11 |
| `NoAdmissionGuard` | `Composed` | `MutualExclusion` | 5 |
| `InsufficientCommitted` — a constraint rejection is recorded as `committed` | `EventClose` | `Serializable` (t2 was told `insufficient` and ended `committed`; every serial order ends it `rejected`) | 8 |
| `BranchReadUnprotected` — `BuyUnder` takes `DRAW` without `S`: the branch reads the row unprotected | `ConcurrentBranch` | `Serializable` (both told `ok`, quota 0 and 2 sold; every serial order answers `ok, skipped`) | 9 |
| `RangeCompatibleWithClaim` — `RS`/`CLAIM` compatible | `Phantom` | `Serializable` (second scan sees the inserted key) | 10 |
| `DrawCompatibleWithX` — `X`/`DRAW` compatible | `EventClose` | `Serializable` | 9 |
| `ClaimCompatibleWithClaim` | `ClaimAbort` | `Uniqueness` (two committed inserts of key 2) | 9 |
| `ClaimLoserTrustsTentativeHolder` — the loser sees the tentative row and fails at once | `ClaimAbort` | `Serializable` (`exists` returned though no serial order produces it) | 7 |
| `PinCompatibleWithDelete` — `PIN`/`DEL` compatible | `ParentDelete` | `ReferentialIntegrity` | 9 |
| `PerOperationAcquisition` — slice 1's registration per operation: nothing is acquired at dispatch, `DoOp` registers each operation's resources as it runs, a draw sees the committed balance and waits for the allowance | `DeleteReinsert` | `NoWaitCycle` (t1 holds `DEL 2` waiting for `PIN 1`, t2 holds `DEL 1` waiting for `X 2`) | 3 |
| `PerOperationAcquisition` | `DeleteReinsert` | TLC deadlock check | 3 |
| `PerOperationAcquisition` | `OutOfOrder` | `NoWaitCycle` (t1 waits on t2's claim, t2 waits on t1's draw) | 3 |
| `PerOperationAcquisition` | `FootprintGap` | `NoWaitCycle` (t1 holds `CLAIM 3` waiting for `X 2`, t2 holds `X 2` waiting for `DEL 3`) | 3 |
| `PerOperationAcquisition` | `ScanUpgrade` | `NoWaitCycle` (each holds its range and waits for `X` on a key the other's range covers) | 3 |
| `ActiveAllowanceWait` — slice 1's escrow restored: no reservation at admission, a draw sees the committed balance, an active transaction waits for the allowance left by other tentative draws | `AllowanceCycle` | `NoWaitCycle` (each holds a tentative draw and waits on the other's) | 7 |
| `ActiveAllowanceWait` | `AllowanceCycle` | TLC deadlock check | 7 |
| `CapIgnored` — the budget is reserved in full, never capped at the committed balance | `Unfundable` | `ReservationsFunded` (t1 reserves 3 on a balance of 2) | 3 |
| `CapIgnored` | `Unfundable` | `BalanceNonNegative` (t1 draws 3 and commits: quota −1) | 6 |
| `PrematureFailure` — a capped reservation takes what other holders leave instead of waiting for them to resolve | `HolderAbort` | `Serializable` (t2 holds a reservation of 2, t1 takes the remainder 0 and its response is `insufficient` while t2 is still live; run alone, t1's serial response is `ok, exists` and it ends rejected) | 6 |
| `DrawIgnoresReservation` — admission accounts what other holders have drawn, not what they reserved | `AllowanceCycle` | `ReservationsFunded` (both admitted with 2 on a balance of 2) | 5 |
| `DrawIgnoresReservation` | `AllowanceCycle` | `BalanceNonNegative` (quota −2) | 11 |
| `BudgetUndercounts` — the budget is the largest single draw, not the total | `AllowanceCycle` | `Serializable` (t1 reserves 1 and is told `insufficient` on its second draw; no serial order rejects t1) | 5 |
| `NoBypass` — a later request may overtake an older queued request on a shared resource (`OlderQueued` dropped from `WaitsOn`) | `Starvation` | `RequestedLeadsToTerminal` (liveness: a lasso in which t3 is admitted, commits and requests again forever while t1, queued first, is never admitted) | 8, loop back to 4 |
| `WidenInPlace` — a call acquires the widened resource on top of its held footprint and waits for it instead of restarting | `WidenCycle` | `NoWaitCycle` (each holds `X` on the key the other's pointer names and waits for `S` on it) | 7 |
| `WidenInPlace` | `WidenCycle` | TLC deadlock check | 7 |
| `KeepTentativeOnRestart` — a restart keeps the tentative writes and draws | `WidenCycle` | `Serializable` (the re-run `Update` returns the call's own earlier write; no serial order yields it) | 9 |
| `KeepTentativeOnRestart` | `WidenEscrow` | `Serializable` (the kept draw leaves the re-run `Buy` no balance in its new reservation: t1 is told `insufficient` while t3, which could have taken the ticket, has not run; no serial order of the ended calls yields it) | 11 |
| `FsyncFailureAsAbort` — an fsync failure answers `aborted`, marks the transactions aborted and releases their footprint and reservations | `Recovery` | `NoKnownAbortAfterFsyncFailure` | 6 |
| `FsyncFailureAsAbort` | `Recovery` | `KnownAbortNotInLog` (the record had reached storage: recovery finds it for a transaction told `aborted`) | 6 |
| `FsyncFailureAsAbort` | `Recovery` | `UnknownKeepsReservations` (the footprint is released while the outcome is unknown) | 6 |
| `FsyncFailureShrinksReservation` — an fsync failure releases one unit of any reservation above 1 and keeps the rest | `RecoveryEscrow` | `UnknownKeepsReservations` (t1's reservation falls from 2 to 1, still positive: only the exact comparison sees it) | 7 |
| `ReleaseAtEnqueue` — a transaction releases its footprint and reservations when its record enters the tail, before the flush | `RecoveryDependent` | `Serializable` (t2 passes t1's lock and is told the initial row although t1's record precedes its own in the log; no serial order yields both responses) | 11 |
| `AckBeforeDurable` — the caller is acknowledged when the record is appended, before the flush | `Recovery` | `AcknowledgedSurvive` (t1 answered `committed`, its record in no log) | 5 |
| `RecoverSkipsTail` — recovery drops the last durable record | `Recovery` | `AcknowledgedSurvive` (an acknowledged commit missing after recovery) | 9 |
| `RecoverSkipsTail` | `Recovery` | `RecoveredPrefix` (the served history is shorter than the log) | 7 |
| `RecoverSkipsTail` | `Recovery` | `Serializable` (a committed transaction's effects are missing from the recovered state) | 8 |

`TypeOK` has no mutant: it is a typing check with no protocol content.

## Concept findings

**Concept defect found by slice 1, fixed by [#24](https://github.com/overnightworks/kiln/issues/24).**
`AllowanceCycle` was §9.5's unresolved problem made concrete: two
transactions whose footprints coexisted (one `DRAW` each), each already
holding a tentative draw on one key, each wanting a second draw that its
own view afforded but the shared allowance did not, waited on each other.
Acquiring the footprint at dispatch rules out every cycle over exclusive
resources — a waiter holds nothing — but a draw's allowance was checked
when the draw ran, after the footprint was held, so the allowance wait was
a wait between holders. §4's rules 1-4 (aggregate escrow budgets reserved
at admission, no premature failure, admission in dispatch order, no
wait-die) are what this model now encodes; the mutant table shows what
each rule prevents.

**Decided by the concept, encoded as is.** A transaction whose reservation
cannot be funded because of other *tentative* reservations waits for them
to resolve; it does not fail. §5 states both halves: a tentative draw
already reduces what a concurrent draw may reserve, and only committed
state may justify a failure. A draw is rejected only when it exceeds a
reservation that equals the committed balance.

**Open details, conservative choice taken (for the head to route).**

1. **Silent compatibility pairs.** §4/§5 do not state whether a `PIN` or a
   `DRAW` is compatible with an `S` point lock on the same key, whether a
   `PIN` is compatible with an `X` update of the parent row that is not a
   delete, or whether a `PIN` and a `DRAW` on one key are compatible. The
   model conflicts in all four cases. A relaxation needs its own proof,
   not a rerun of the existing scenarios: `DRAW`/`S` compatible, for
   example, would let `Read(1); Read(1)` straddle a concurrent purchase,
   which no scenario here drives — and a transaction holding `S` and
   `DRAW` on one key would no longer be the key's only draw holder, so it
   would have to draw on its reservation rather than its view.
2. **A child delete's resource on its parent.** §5 ties the reverse
   reference to the child row's lifetime but names no resource for the
   unlink; durable reverse-reference protection is required either way.
   The model takes a `PIN` on the parent while deleting the child.
3. **What "shares a resource" means for admission order.** Rule 3 is
   encoded as overlapping coverage, compatible or not, so a reader queued
   behind a writer also holds back a later reader on the same key (no
   writer starvation). A narrower reading (incompatible resources only)
   would admit the later reader; the concept does not say which.

Every other TLC violation during the build was a model error (a row
overwrite where the concept says delta; a witness predicate satisfiable by
an unintended route; resources registered per operation under static
ordering assumptions, which admitted the deadlocks of `FootprintGap` and
`DeleteReinsert`; an unknown budget encoded as a token TLC cannot compare
with an integer) and was fixed in the model.
