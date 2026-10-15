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

Every log is committed next to its configuration.

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
concept is silent the rule is conflict (see "Concept gaps").

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
the relation, `NoWaitCycle` its acyclicity. Programs must register in
canonical order (non-decreasing lowest key; `ASSUME CanonicalOrder`); TLC
refuses a scenario that violates it.

**Serializability, checked for real.** `Serializable`: for the transactions
that have ended (committed or rejected), some permutation, executed
serially from `InitDb` with the same `ApplyOp`, returns exactly the
responses each caller received and produces exactly the committed state.
A rejected transaction takes part with its failure response and no
effect, so a premature failure (one no serial order would produce) is a
violation. Permutations are enumerated (at most 6 for 3 transactions).

**Other properties.** `MutualExclusion` (no two live transactions hold
incompatible resources), `Uniqueness` (per key, committed successful
inserts never exceed committed deletes plus one), `ReferentialIntegrity`
(a present row's parent is present), `BalanceNonNegative`, `TypeOK`, and
TLC's deadlock check (a state with a live transaction and no enabled step;
`Terminating` self-loops once every transaction has ended).

**Bounds.** 2-3 transactions with programs of 1-2 operations, 2-3 keys,
fixed programs per scenario. Not covered: longer programs, lock upgrades
(a program takes the strongest mode it needs at a key's first touch, as
§4's "covering resource before access" prescribes), more than one table,
crash and recovery, any liveness.

## Scenarios and results

All properties above are checked in every scenario; TLC found no violation
and no deadlock. Each run finished in under one second with 4 workers.

| Scenario | What runs | Distinct states | Depth | Log |
|---|---|---|---|---|
| `Composed` | t1: `Update(1,0,0)`, `Scan(2,3)`; t2: `Buy(1,1)`, `Insert(2,1)`; t3: `Delete(3)`, `Scan(1,3)` (table) — X meets DRAW on key 1, PIN meets PIN, CLAIM and DEL meet a range and a table scan | 39 | 10 | `Composed.tlc.log` |
| `Phantom` | t1: `Scan(2,3)` twice; t2: `Insert(2,0)` — round-1 phantom | 11 | 6 | `Phantom.tlc.log` |
| `EventClose` | t1: `Update(1,10,0)` on the event row (sold 10, quota 1); t2, t3: `Buy(1,1)` | 20 | 6 | `EventClose.tlc.log` |
| `ClaimAbort` | t1: `Insert(2,0)`, `Buy(3,1)` against an empty escrow; t2, t3: `Insert(2,0)` — tentative claim whose holder is rejected while claimants wait | 19 | 6 | `ClaimAbort.tlc.log` |
| `ParentDelete` | t1: `Delete(1)`; t2: `Insert(2,1)` — parent delete concurrent with child insert | 7 | 4 | `ParentDelete.tlc.log` |
| `OutOfOrder` | t1: `Buy(1,1)`, `Insert(2,0)`; t2: `Insert(2,0)`, `Buy(1,1)` — only runs under the `NoCanonicalOrder` mutant | refused by `ASSUME CanonicalOrder` | – | `OutOfOrder.tlc.log` |

**Reachability witnesses.** Each scenario names a negated state predicate
(`*Unreached`); TLC's counterexample is the proof that the scenario's
decisive state is reachable. Logs: `<Scenario>.witness.<Predicate>.tlc.log`.

| Witness | State reached | Trace length |
|---|---|---|
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
| `NoAdmissionGuard` — `Conflicting` is empty, nothing ever waits | `Composed` | `Serializable` (t2 draws tentatively, t1 observes quota 1 and closes, both commit: no serial order yields both responses) | 7 |
| `NoAdmissionGuard` | `Composed` | `MutualExclusion` | 3 |
| `RangeCompatibleWithClaim` — `RS`/`CLAIM` compatible | `Phantom` | `Serializable` (second scan sees the inserted key) | 6 |
| `DrawCompatibleWithX` — `X`/`DRAW` compatible | `EventClose` | `Serializable` | 5 |
| `DrawIgnoresTentative` — allowance ignores live draws | `EventClose` | `BalanceNonNegative` (quota −1) | 5 |
| `ClaimCompatibleWithClaim` | `ClaimAbort` | `Uniqueness` (two committed inserts of key 2) | 7 |
| `ClaimLoserTrustsTentativeHolder` — the loser sees the tentative row and fails at once | `ClaimAbort` | `Serializable` (`exists` returned though no serial order produces it) | 3 |
| `PinCompatibleWithDelete` — `PIN`/`DEL` compatible | `ParentDelete` | `ReferentialIntegrity` | 5 |
| `NoCanonicalOrder` — `ASSUME CanonicalOrder` dropped | `OutOfOrder` | `NoWaitCycle` (t1 waits on t2's claim, t2 waits on t1's draw) | 3 |
| `NoCanonicalOrder` | `OutOfOrder` | TLC deadlock check | 3 |

`TypeOK` has no mutant: it is a typing check with no protocol content.

## Concept gaps and findings (for the head to route)

Found while encoding, not by a violation; the model takes the conservative
side of each silence and the concept should say which it means.

1. **Silent compatibility pairs.** §4/§5 do not state whether a `PIN` or a
   `DRAW` is compatible with an `S` point lock on the same key, whether a
   `PIN` is compatible with an `X` update of the parent row that is not a
   delete, or whether a `PIN` and a `DRAW` on one key are compatible. The
   model conflicts in all four cases. Any of them could be relaxed with a
   corresponding mutant run showing the properties still hold.
2. **Escrow wait versus failure.** §5 says a tentative draw reduces what a
   concurrent draw may reserve and that only committed state may justify a
   failure, but not what a draw does when the committed balance suffices
   and the live tentative draws exhaust it. The model waits on those
   holders (the same rule as a claim loser) and rejects only when the
   committed balance is short. If the concept intends immediate rejection
   there, that is a different protocol.
3. **A child delete's resource on its parent.** §5 ties the reverse
   reference to the child row's lifetime but names no resource for the
   unlink. The model takes a `PIN` on the parent while deleting the child.
4. **§9.5 within these bounds.** With draws registered in canonical order
   like every other keyed resource, no wait cycle and no deadlock arose in
   any scenario where draws wait on tentative holders (`EventClose`,
   `Composed`, `ClaimAbort`); the cycle §9.5 fears appears exactly when
   canonical order is dropped (`OutOfOrder` under `NoCanonicalOrder`). That
   is evidence for "canonical order extends to escrow", not a proof: two
   transactions, two keys, and no fairness (slice 3).

No concept defect was found: every TLC violation during the build was a
model error (a row overwrite where the concept says delta; a witness
predicate satisfiable by an unintended route) and was fixed in the model.
