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
TLC's code: 0 for a pass, 10 when an assumption refuses the scenario, 12
on a property violation and 11 on a deadlock — the expected outcome of
every witness run, of `AllowanceCycle`, and of every mutant run.

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

**Protocol.** A transaction executes its program in order. `DoOp` registers
the operation's resources atomically with the compatibility check (§4,
"atomically at dispatch") and applies the operation to the transaction's
view — committed rows overlaid with its own tentative writes and draws. A
rejection is a constraint failure returned to the caller: the transaction
ends as `rejected`, releases everything and leaves no effect (§5, known
abort). `Commit` makes the view the committed state and releases every
resource (strict two-phase locking). Escrow draws are deltas, not
overwrites: two committed draws subtract twice. There is no other abort:
the only ways a transaction ends are commit and constraint rejection.

**Waiting.** `WaitsOn(t)` is derived from the state: the live transactions
holding a resource incompatible with the next operation's resources, or,
for a draw that fits the committed balance but not the remaining
allowance, the holders of the tentative draws on that key (§5: a tentative
draw already reduces what a concurrent draw may reserve; §9.5: a
non-exclusive wait). A transaction steps only when it waits on nobody, so
a loser behind a tentative claim waits for the holder's outcome and is
rejected with `exists` only once the holder has committed. `WaitFor` is
the relation, `NoWaitCycle` its acyclicity.

**Canonical order, over the complete footprint.** `Footprint(op)` is the
set of resources an operation registers, computed statically: only a
delete's resources depend on the view (the pin on the parent its row
names), so a delete's footprint pins every parent the row can have in the
scenario (`InitDb`'s and every `Insert` of that key in any program) — an
over-approximation that can refuse a program but never admit one out of
order. Two assumptions, both checked by TLC before any state is explored:

- `ASSUME CanonicalOrder`: every key an operation covers for the first
  time lies above every key the program covered before it. Example:
  `Buy(1,1); Insert(2,3)` covers 1, then 2 and 3 — admitted.
  `Insert(3,1); Update(2,0,0)` covers 1 and 3, then 2 — refused
  (`FootprintGap`).
- `ASSUME NoUpgrade`: on a key the program already holds, a later operation
  may only take a mode the held one dominates, where `m` dominates `w` when
  every kind compatible with `m` is also compatible with `w` (so the later
  request can never be refused while the earlier hold stands). Example:
  `Delete(1); Scan(1,3)` holds `DEL` on 1 and later covers 1 under `RS` —
  admitted, `DEL` dominates everything. `Scan(1,2); Update(1,0,0)` holds
  `RS` on 1 and later wants `X` — refused (`ScanUpgrade`). Whether the
  protocol detects, restarts or forbids such an upgrade is decided in #24;
  the model only refuses it.

Together they make the wait-for relation over exclusive resources acyclic:
a waiter's blocked key is new to it (a held key could only be re-requested
in a dominated mode, which no live holder can refuse), hence above every
key it holds, while the holder it waits on already covers that key; a
cycle would need a key below itself. The argument does not cover a wait on
the escrow *allowance* for a key the waiter already draws on — that is
`AllowanceCycle` below. Within one program, a transaction may not both
pin and draw on one key: neither mode dominates the other while the
concept leaves `PIN`/`DRAW` unspecified (gap 1).

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
incompatible resources), `Uniqueness` (per key, committed successful
inserts never exceed committed deletes plus one), `ReferentialIntegrity`
(a present row's parent is present), `BalanceNonNegative`, `TypeOK`, and
TLC's deadlock check (a state with a live transaction and no enabled step;
`Terminating` self-loops once every transaction has ended).

**Bounds.** 2-3 transactions with programs of 1-2 operations, 1-3 keys,
fixed programs per scenario. A program takes the strongest mode it needs
at a key's first touch (`ASSUME NoUpgrade`, as §4's "covering resource
before access" prescribes). Not covered: longer programs, more than one
table, crash and recovery, any liveness.

## Scenarios and results

Three outcomes exist. A *checked* scenario is admitted by both assumptions
and TLC explores its whole state space against every property above and
the deadlock check. A *refused* scenario fails an assumption, so TLC
explores nothing: the log proves only that the assumption rejects that
program. A *documented failure* is admitted and violates a property; its
log is the counterexample and it is not part of the passing set. Every
checked scenario passes with no violation and no deadlock; each run
finished in under one second with 4 workers.

| Scenario | What runs | Outcome | Distinct states | Depth | Log |
|---|---|---|---|---|---|
| `Composed` | t1: `Buy(1,1)`, `Insert(2,3)`; t2: `Buy(1,1)`, `Delete(3)`; t3: `Scan(1,3)` (table) — two draws share key 1's allowance (quota 2), the child insert's pin on 3 meets its delete while both draws are live, claim, delete and draws meet the table scan; insert first rejects the delete with `referenced`, delete first the insert with `noparent` | checked, passes | 39 | 8 | `Composed.tlc.log` |
| `Pins` | t1: `Delete(1)`, `Scan(1,3)` (table); t2: `Insert(2,1)`; t3: `Insert(3,1)` — two pins on parent 1 coexist against its delete, two claims meet the table scan | checked, passes | 20 | 6 | `Pins.tlc.log` |
| `RangePoint` | t1: `Scan(1,3)` (table); t2: `Read(2)`, `Buy(3,1)`; t3: `Update(2,1,0)` — a table scan and a point read coexist against an update, the draw meets the table scan | checked, passes | 38 | 8 | `RangePoint.tlc.log` |
| `Phantom` | t1: `Scan(2,3)` twice; t2: `Insert(2,0)` — round-1 phantom | checked, passes | 11 | 6 | `Phantom.tlc.log` |
| `EventClose` | t1: `Update(1,10,0)` on the event row (sold 10, quota 1); t2, t3: `Buy(1,1)` | checked, passes | 20 | 6 | `EventClose.tlc.log` |
| `ClaimAbort` | t1: `Insert(2,0)`, `Buy(3,1)` against an empty escrow; t2, t3: `Insert(2,0)` — tentative claim whose holder is rejected while claimants wait | checked, passes | 19 | 6 | `ClaimAbort.tlc.log` |
| `ParentDelete` | t1: `Delete(1)`; t2: `Insert(2,1)` — parent delete concurrent with child insert | checked, passes | 7 | 4 | `ParentDelete.tlc.log` |
| `OutOfOrder` | t1: `Buy(1,1)`, `Insert(2,0)`; t2: `Insert(2,0)`, `Buy(1,1)` — t2 covers key 1 after key 2; runs only under the `NoCanonicalOrder` mutant | refused by `ASSUME CanonicalOrder` | – | – | `OutOfOrder.tlc.log` |
| `FootprintGap` | t1: `Insert(3,1)`, `Update(2,0,0)`; t2: `Update(2,1,0)`, `Delete(3)` — in order by lowest key per operation (1, 2; 2, 3), out of order over the footprint (t1 covers 2 after 3, t2 covers 1 after 2); deadlocked under the previous ordering | refused by `ASSUME CanonicalOrder` | – | – | `FootprintGap.tlc.log` |
| `ScanUpgrade` | t1: `Scan(1,2)`, `Update(1,0,0)`; t2: `Scan(1,2)`, `Update(2,0,0)` — each wants `X` on a key it holds under `RS`; deadlocked while upgrades were excluded only in prose | refused by `ASSUME NoUpgrade` | – | – | `ScanUpgrade.tlc.log` |
| `AllowanceCycle` | t1, t2: `Buy(1,1)`, `Buy(1,1)` against quota 2 — canonical and upgrade-free; the first draws coexist, each second draw fits its own view but not the allowance left by the other's tentative draw, and each waits on the other | documented failure: `NoWaitCycle` violated at state 3, then deadlock — concept defect, [#24](https://github.com/overnightworks/kiln/issues/24) | 9 | 5 | `AllowanceCycle.tlc.log` |

**Reachability witnesses.** Each checked scenario names negated state
predicates (`*Unreached`); TLC's counterexample is the proof that the
scenario's decisive state is reachable. Logs:
`<Scenario>.witness.<Predicate>.tlc.log`.

| Witness | State reached | Trace length |
|---|---|---|
| `ComposedDrawsUnreached` | t1 and t2 both hold a tentative draw on 1; t3's table scan waits on both | 3 |
| `ComposedPinDeleteUnreached` | t1 holds its pin on 3; t2, still holding its draw, waits on t1 for the delete | 4 |
| `ComposedReferencedUnreached` | t2 was told `ok, referenced`, t1 committed key 2 under parent 3, and key 1's quota is 1: the rejected draw left no effect | 6 |
| `PinsConcurrentUnreached` | t2 and t3 both hold a pin on 1; t1's delete waits on both | 3 |
| `PinsTableUnreached` | t1 holds the table scan after deleting 1; both claims wait on t1 | 3 |
| `PinsReferencedUnreached` | t1 was told `referenced`; keys 2 and 3 are committed under parent 1 | 6 |
| `RangePointUnreached` | t1 holds the table scan and t2 the point read on 2; t3's update waits on both, t2's draw waits on t1 | 3 |
| `PhantomUnreached` | t1 holds `RS 2..3`, t2's claim on 2 waits on t1 | 2 |
| `EventCloseUnreached` | t2 holds its tentative draw; t1's X lock and t3's draw both wait on t2 | 2 |
| `ClaimAbortWaitUnreached` | t1 holds the tentative claim on 2, t2 waits on it | 2 |
| `ClaimAbortUnreached` | t1 answered `ok, insufficient` (claim released by the rejection), t2 committed key 2, t3 was told `exists` | 6 |
| `ParentDeleteUnreached` | t2 holds its pin on 1, t1's delete waits on it | 2 |

## Mutants (specs/kernel/mutants/)

Each mutant is a patch against `Kernel.tla` that removes one named guard;
`run.sh` applies it to a temporary copy and checks exactly one property.
TLC refutes every one; the log is `<Mutant>.<Scenario>.<Property>.tlc.log`.

| Mutant (guard removed) | Scenario | Property refuted | Trace length |
|---|---|---|---|
| `NoAdmissionGuard` — `Conflicting` is empty, nothing ever waits | `Composed` | `Serializable` (t1 inserts the child of 3 and t2 deletes 3, both told `ok` and both committed: no serial order yields both) | 7 |
| `NoAdmissionGuard` | `Composed` | `MutualExclusion` | 3 |
| `InsufficientCommitted` — a constraint rejection is recorded as `committed` | `EventClose` | `Serializable` (t2 was told `insufficient` and ended `committed`; every serial order ends it `rejected`) | 4 |
| `RangeCompatibleWithClaim` — `RS`/`CLAIM` compatible | `Phantom` | `Serializable` (second scan sees the inserted key) | 6 |
| `DrawCompatibleWithX` — `X`/`DRAW` compatible | `EventClose` | `Serializable` | 5 |
| `DrawIgnoresTentative` — allowance ignores live draws | `EventClose` | `BalanceNonNegative` (quota −1) | 5 |
| `ClaimCompatibleWithClaim` | `ClaimAbort` | `Uniqueness` (two committed inserts of key 2) | 7 |
| `ClaimLoserTrustsTentativeHolder` — the loser sees the tentative row and fails at once | `ClaimAbort` | `Serializable` (`exists` returned though no serial order produces it) | 3 |
| `PinCompatibleWithDelete` — `PIN`/`DEL` compatible | `ParentDelete` | `ReferentialIntegrity` | 5 |
| `NoCanonicalOrder` — `ASSUME CanonicalOrder` dropped (`NoUpgrade` kept) | `OutOfOrder` | `NoWaitCycle` (t1 waits on t2's claim, t2 waits on t1's draw) | 3 |
| `NoCanonicalOrder` | `OutOfOrder` | TLC deadlock check | 3 |

`TypeOK` has no mutant: it is a typing check with no protocol content.

## Concept findings

**Concept defect, routed to [#24](https://github.com/overnightworks/kiln/issues/24).**
`AllowanceCycle` is §9.5's unresolved problem made concrete: two
transactions inside canonical order, each already holding a tentative
draw on one key, each wanting a second draw that its own view affords but
the shared allowance does not, wait on each other. Canonical order rules
out the cycle for exclusive resources, and `OutOfOrder` under
`NoCanonicalOrder` shows a draw/claim cycle that order does prevent; the
allowance wait on a key the waiter already draws on is outside the
argument. The protocol decision (aggregate reservation, arbitration, or
another mechanism) belongs to #24, together with the protocol rule for
lock upgrades the model only refuses (`ScanUpgrade`).

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
   which no scenario here drives. Until decided, a program cannot both
   pin and draw on one key (see `NoUpgrade`).
2. **A child delete's resource on its parent.** §5 ties the reverse
   reference to the child row's lifetime but names no resource for the
   unlink; durable reverse-reference protection is required either way.
   The model takes a `PIN` on the parent while deleting the child.

Every other TLC violation during the build was a model error (a row
overwrite where the concept says delta; a witness predicate satisfiable by
an unintended route; an ordering over per-operation minima instead of the
complete footprint, `FootprintGap`) and was fixed in the model.
