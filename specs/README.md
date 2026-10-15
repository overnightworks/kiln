# TLA+ model of kiln's kernel

Verification layer 1 of `docs/verification.md` (design proofs, checked with
TLC). This is slice 1 (issue #16, cut from #6): resources, admission and
typed reservations in one composed state space. Crash, recovery and
publication are slice 2; liveness under stated fairness is slice 3.

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
deadlock — the expected outcome of every witness run, of
`AllowanceCycle`, and of every mutant run.

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
inserts into, pins count references and so coexist, draws share an
allowance, a delete is incompatible with the pins on the parent. Where the
concept is silent the rule is conflict (see "Concept findings").

**Operations** (`ApplyOp`, one pure function used by both the live protocol
and the serial reference): `Read(k)` under `S`; `Scan(lo, hi)` under `RS`,
returning the present keys and values; `Update(k, val, bal)` under `X`,
returning the previous row; `Insert(k, p)` under `CLAIM` on `k` plus `PIN`
on `p`, rejected with `exists` or `noparent`; `Delete(k)` under `DEL` on
`k` plus `PIN` on the row's own parent while it unlinks, rejected with
`referenced`; `Buy(k, a)` under `DRAW`, drawing `a` from `bal` and adding
one to `val`, rejected with `insufficient`.

**Protocol.** A transaction starts `pending`: dispatched, holding nothing.
`Acquire` takes its complete footprint — every resource its program needs
(`Footprint(t)`, below) — in one step, atomically with the compatibility
check against every live holder, as §4 prescribes: a keyed resource is
registered "atomically at dispatch, held through commit", and a covering
resource is acquired "before any read or write touches the underlying
state". §4's canonical order is how an implementation acquires the same
set piece by piece; the step abstracts that sequence, so the model never
holds a partial footprint. The transaction is then `active` and `DoOp`
executes its program in order on its view — committed rows overlaid with
its own tentative writes and draws — acquiring nothing more. A rejection
is a constraint failure returned to the caller: the transaction ends as
`rejected`, releases everything and leaves no effect (§5, known abort).
`Commit` makes the view the committed state and releases every resource
(strict two-phase locking, dispatch to commit). Escrow draws are deltas,
not overwrites: two committed draws subtract twice. There is no other
abort: the only ways a transaction ends are commit and constraint
rejection.

**Waiting.** `WaitsOn(t)` is derived from the state. A pending transaction
waits on every live holder of a resource incompatible with its footprint.
An active one holds its footprint and waits only for a draw that fits the
committed balance but not the remaining allowance, on the holders of the
tentative draws on that key (§5: a tentative draw already reduces what a
concurrent draw may reserve; §9.5: a non-exclusive wait). A transaction
steps only when it waits on nobody, so a loser behind a tentative claim
waits for the holder's outcome and is rejected with `exists` only once
the holder has committed. `WaitFor` is the relation, `NoWaitCycle` its
acyclicity. Over exclusive resources a cycle is impossible by
construction: a waiter on a resource holds nothing, so nobody waits on
it. What remains is the allowance wait of an active transaction on
another active draw holder — `AllowanceCycle` below.

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

**Serializability, checked for real.** `Serializable`: for the transactions
that have ended (committed or rejected), some permutation, executed
serially from `InitDb` with the same `ApplyOp`, returns exactly the
responses each caller received, ends each transaction with exactly the
terminal status it reached (committed, or rejected with its failure as
the last response), and produces exactly the committed state. A rejected
transaction takes part with its failure response and no effect, so a
premature failure (one no serial order would produce) is a violation, and
so is a rejection recorded as a commit (`InsufficientCommitted`).
Permutations are enumerated (at most 6 for 3 transactions).

**What the oracle cannot see.** The serial reference runs the same
`ApplyOp` as the live protocol, so it catches every divergence caused by
interleaving but cannot validate the operation semantics themselves: a
wrong `ApplyOp` is wrong on both sides and passes. Those semantics are
pinned only by the state invariants below (`Uniqueness`,
`ReferentialIntegrity`, `BalanceNonNegative`) and by the witnesses, which
name the concrete responses they expect. `resp` entries of a still-active
transaction are buffered observations the caller has not been shown yet;
publication is slice 2, and `Serializable` compares only ended
transactions.

**Other properties.** `MutualExclusion` (no two live transactions hold
incompatible resources), `FootprintCovers` (above), `Uniqueness` (per key, committed successful
inserts never exceed committed deletes plus one), `ReferentialIntegrity`
(a present row's parent is present), `BalanceNonNegative`, `TypeOK`, and
TLC's deadlock check (a state with a live transaction and no enabled step;
`Terminating` self-loops once every transaction has ended).

**Bounds.** 2-3 transactions with programs of 1-2 operations, 1-3 keys,
fixed programs per scenario. The footprint is acquired as one step; the
canonical order in which an implementation acquires it piecewise, and §4's
restart of a class-4 call whose covering resource must widen
mid-execution, are not modelled. Not covered: longer programs, more than
one table, crash and recovery, any liveness.

## Scenarios and results

Two outcomes exist. For a *checked* scenario TLC explores the whole state
space against every property above and the deadlock check. A *documented
failure* violates a property; its log is the counterexample and it is not
part of the passing set. Every checked scenario passes with no violation
and no deadlock; each run finished in under one second with 4 workers.
State counts are given only for full explorations; a run that stops at a
counterexample reports how far 4 workers had got, which varies between
runs, while the trace length (breadth-first, hence shortest) is stable.

| Scenario | What runs | Outcome | Distinct states | Depth | Log |
|---|---|---|---|---|---|
| `Composed` | t1: `Buy(1,1)`, `Insert(2,3)`; t2: `Buy(1,1)`, `Delete(3)`; t3: `Scan(1,3)` (table) — claim, pin, delete, two draws on key 1 (quota 2) and a table scan in one key space; the pin on 3 conflicts with its delete and the scan with everything, so the three serialize: insert first rejects the delete with `referenced`, delete first the insert with `noparent`, and a rejected draw leaves no effect | checked, passes | 48 | 11 | `Composed.tlc.log` |
| `Pins` | t1: `Delete(1)`, `Scan(1,3)` (table); t2: `Insert(2,1)`; t3: `Insert(3,1)` — two pins on parent 1 coexist against its delete, two claims meet the table scan | checked, passes | 38 | 9 | `Pins.tlc.log` |
| `RangePoint` | t1: `Scan(1,2)`; t2: `Read(2)`, `Buy(3,1)`; t3: `Update(2,1,0)` — a range scan and a point read coexist against an update; the draw on 3 lies outside the range | checked, passes | 58 | 11 | `RangePoint.tlc.log` |
| `Phantom` | t1: `Scan(2,3)` twice; t2: `Insert(2,0)` — round-1 phantom | checked, passes | 15 | 8 | `Phantom.tlc.log` |
| `EventClose` | t1: `Update(1,10,0)` on the event row (sold 10, quota 1); t2, t3: `Buy(1,1)` — two draws coexist, the second waits on the first's allowance, the update waits on both | checked, passes | 39 | 9 | `EventClose.tlc.log` |
| `ClaimAbort` | t1: `Insert(2,0)`, `Buy(3,1)` against an empty escrow; t2, t3: `Insert(2,0)` — tentative claim whose holder is rejected while claimants wait | checked, passes | 34 | 9 | `ClaimAbort.tlc.log` |
| `ParentDelete` | t1: `Delete(1)`; t2: `Insert(2,1)` — parent delete concurrent with child insert | checked, passes | 11 | 6 | `ParentDelete.tlc.log` |
| `OutOfOrder` | t1: `Buy(1,1)`, `Insert(2,0)`; t2: `Insert(2,0)`, `Buy(1,1)` — the programs touch keys 1 and 2 in opposite orders (quota 1); the claims conflict at dispatch and the second draw is rejected with `insufficient` | checked, passes | 13 | 7 | `OutOfOrder.tlc.log` |
| `FootprintGap` | t1: `Insert(3,1)`, `Update(2,0,0)`; t2: `Update(2,1,0)`, `Delete(3)` — in order by lowest key per operation (1, 2; 2, 3), out of order over the footprint; the `X` locks on 2 conflict at dispatch | checked, passes | 17 | 9 | `FootprintGap.tlc.log` |
| `ScanUpgrade` | t1: `Scan(1,2)`, `Update(1,0,0)`; t2: `Scan(1,2)`, `Update(2,0,0)` — each needs `X` on a key its own scan covers; range and `X` lock are taken together, and each `X` lock conflicts with the other's range | checked, passes | 16 | 9 | `ScanUpgrade.tlc.log` |
| `DeleteReinsert` | rows 1 and 2 present, no parents; t1: `Delete(2)`, `Insert(2,1)`; t2: `Delete(1)`, `Update(2,0,0)` — the reinsert makes 1 a possible parent of 2, so t1's footprint pins 1; `DEL 1` against `PIN 1` and `DEL 2` against `X 2` conflict at dispatch | checked, passes | 14 | 8 | `DeleteReinsert.tlc.log` |
| `AllowanceCycle` | t1, t2: `Buy(1,1)`, `Buy(1,1)` against quota 2 — both footprints are one `DRAW` on 1 and coexist; the first draws coexist, each second draw fits its own view but not the allowance left by the other's tentative draw, and each waits on the other | documented failure: `NoWaitCycle` violated at state 5 (`AllowanceCycle.tlc.log`); with the invariants off, deadlock at state 5 (`AllowanceCycle.deadlock.tlc.log`) — concept defect, [#24](https://github.com/overnightworks/kiln/issues/24) | – | – | `AllowanceCycle.tlc.log` |

**Reachability witnesses.** Each checked scenario names negated state
predicates (`*Unreached`); TLC's counterexample is the proof that the
scenario's decisive state is reachable. Logs:
`<Scenario>.witness.<Predicate>.tlc.log`.

| Witness | State reached | Trace length |
|---|---|---|
| `ComposedPinDeleteUnreached` | t1 holds its draw on 1 and its pin on 3; t2's delete and t3's table scan both wait on t1 | 2 |
| `ComposedReferencedUnreached` | t2 was told `ok, referenced`, t1 committed key 2 under parent 3, and key 1's quota is 1: the rejected draw left no effect | 8 |
| `ComposedNoParentUnreached` | t1 was told `ok, noparent`, t2 committed the delete of 3, and key 1's quota is 1 | 8 |
| `PinsConcurrentUnreached` | t2 and t3 both hold a pin on 1; t1's delete waits on both | 3 |
| `PinsTableUnreached` | t1 holds the table scan with its delete; both claims wait on t1 | 2 |
| `PinsReferencedUnreached` | t1 was told `referenced`; keys 2 and 3 are committed under parent 1 | 9 |
| `RangePointUnreached` | t1 holds the range scan over 1..2, t2 the point read on 2 and the draw on 3; t3's update waits on both | 3 |
| `PhantomUnreached` | t1 holds `RS 2..3`, t2's claim on 2 waits on t1 | 2 |
| `EventCloseUnreached` | t2 holds its tentative draw; t1's X lock and t3's draw both wait on t2 | 4 |
| `ClaimAbortWaitUnreached` | t1 holds the tentative claim on 2, t2 waits on it | 2 |
| `ClaimAbortUnreached` | t1 answered `ok, insufficient` (claim released by the rejection), t2 committed key 2, t3 was told `exists` | 9 |
| `ParentDeleteUnreached` | t2 holds its pin on 1, t1's delete waits on it | 2 |

## Mutants (specs/kernel/mutants/)

Each mutant is a patch against `Kernel.tla` that removes one named guard;
`run.sh` applies it to a temporary copy and checks exactly one property.
TLC refutes every one; the log is `<Mutant>.<Scenario>.<Property>.tlc.log`.

| Mutant (guard removed) | Scenario | Property refuted | Trace length |
|---|---|---|---|
| `NoAdmissionGuard` — `Conflicting` is empty, nothing ever waits | `Composed` | `Serializable` (t1 inserts the child of 3 and t2 deletes 3, both told `ok` and both committed: no serial order yields both) | 9 |
| `NoAdmissionGuard` | `Composed` | `MutualExclusion` | 3 |
| `InsufficientCommitted` — a constraint rejection is recorded as `committed` | `EventClose` | `Serializable` (t2 was told `insufficient` and ended `committed`; every serial order ends it `rejected`) | 6 |
| `RangeCompatibleWithClaim` — `RS`/`CLAIM` compatible | `Phantom` | `Serializable` (second scan sees the inserted key) | 8 |
| `DrawCompatibleWithX` — `X`/`DRAW` compatible | `EventClose` | `Serializable` | 7 |
| `DrawIgnoresTentative` — allowance ignores live draws | `EventClose` | `BalanceNonNegative` (quota −1) | 7 |
| `ClaimCompatibleWithClaim` | `ClaimAbort` | `Uniqueness` (two committed inserts of key 2) | 7 |
| `ClaimLoserTrustsTentativeHolder` — the loser sees the tentative row and fails at once | `ClaimAbort` | `Serializable` (`exists` returned though no serial order produces it) | 5 |
| `PinCompatibleWithDelete` — `PIN`/`DEL` compatible | `ParentDelete` | `ReferentialIntegrity` | 7 |
| `PerOperationAcquisition` — nothing is acquired at dispatch; `DoOp` registers each operation's resources as it runs | `DeleteReinsert` | `NoWaitCycle` (t1 holds `DEL 2` waiting for `PIN 1`, t2 holds `DEL 1` waiting for `X 2`) | 3 |
| `PerOperationAcquisition` | `DeleteReinsert` | TLC deadlock check | 3 |
| `PerOperationAcquisition` | `OutOfOrder` | `NoWaitCycle` (t1 waits on t2's claim, t2 waits on t1's draw) | 3 |
| `PerOperationAcquisition` | `FootprintGap` | `NoWaitCycle` (t1 holds `CLAIM 3` waiting for `X 2`, t2 holds `X 2` waiting for `DEL 3`) | 3 |
| `PerOperationAcquisition` | `ScanUpgrade` | `NoWaitCycle` (each holds its range and waits for `X` on a key the other's range covers) | 3 |

`TypeOK` has no mutant: it is a typing check with no protocol content.

## Concept findings

**Concept defect, routed to [#24](https://github.com/overnightworks/kiln/issues/24).**
`AllowanceCycle` is §9.5's unresolved problem made concrete: two
transactions whose footprints coexist (one `DRAW` each), each already
holding a tentative draw on one key, each wanting a second draw that its
own view affords but the shared allowance does not, wait on each other.
Acquiring the footprint at dispatch rules out every cycle over exclusive
resources — a waiter holds nothing — but a draw's allowance is checked
when the draw runs, after the footprint is held, so the allowance wait is
a wait between holders. The protocol decision (aggregate reservation at
dispatch, arbitration, or another mechanism) belongs to #24.

**Decided by the concept, encoded as is.** A draw whose committed balance
suffices but whose remaining allowance does not *waits* on the tentative
holders; it does not fail. §5 states both halves: a tentative draw already
reduces what a concurrent draw may reserve, and only committed state may
justify a failure. The model rejects a draw only when the committed
balance is short.

**Open details, conservative choice taken (for the head to route).**

1. **Silent compatibility pairs.** §4/§5 do not state whether a `PIN` or a
   `DRAW` is compatible with an `S` point lock on the same key, whether a
   `PIN` is compatible with an `X` update of the parent row that is not a
   delete, or whether a `PIN` and a `DRAW` on one key are compatible. The
   model conflicts in all four cases. A relaxation needs its own proof,
   not a rerun of the existing scenarios: `DRAW`/`S` compatible, for
   example, would let `Read(1); Read(1)` straddle a concurrent purchase,
   which no scenario here drives.
2. **A child delete's resource on its parent.** §5 ties the reverse
   reference to the child row's lifetime but names no resource for the
   unlink; durable reverse-reference protection is required either way.
   The model takes a `PIN` on the parent while deleting the child.

Every other TLC violation during the build was a model error (a row
overwrite where the concept says delta; a witness predicate satisfiable by
an unintended route; resources registered per operation under static
ordering assumptions, which admitted the deadlocks of `FootprintGap` and
`DeleteReinsert`) and was fixed in the model.
