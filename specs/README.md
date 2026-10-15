# TLA+ model of kiln's kernel

Verification layer 1 of `docs/verification.md` (design proofs, checked with
TLC). This is slice 1 (issue #16, cut from #6): resources, admission and
typed reservations. Crash, recovery and publication are slice 2; liveness
under stated fairness is slice 3.

## Tool

TLC from the `tla2tools.jar` release bundled by the TLA+ Foundation.

- Version: TLC2 2.19 of 08 August 2024 (rev: 5a47802)
- SHA-256: `936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88`
- Runtime: Java 21 (Temurin 21.0.12.1)

Run a model from `specs/kernel/`:

```sh
java -jar /path/to/tla2tools.jar -workers 4 -config <Module>.cfg <Module>.tla
```

`-workers` is capped at 4 per the repository's machine-load rule. Each run's
log (including its final state count) is committed next to its module as
`<Module>.tlc.log`.

## specs/kernel/Resources.tla — resources and admission

Models `docs/concept.md` §4's resource namespace and admission protocol in
isolation, before any typed reservation exists.

**Abstractions and bounds:**

- One resource namespace, 3 keys (`1..3`), 3 transactions (`t1`, `t2`,
  `t3`) — the "2-3 transactions, 2-3 keys" bound from issue #6/#16.
- A resource's compatibility is modeled purely by the set of keys it
  covers (`Overlaps(a, b) == a \cap b # {}`): a point is a singleton, a
  range an interval, a table the whole key space. §4's three named rules
  ("equal keys conflict"; "a point conflicts with any range or table
  resource covering it") are the two instances of this one coverage-
  intersection rule that this model's scenario exercises directly; the
  table case is the same rule at `Coverage = Keys` and is not given its
  own transaction in this 3-transaction scenario, since the compatibility
  operator does not distinguish it from any other covering range.
- All resources are acquired in exclusive mode. The model does not
  represent shared/read-mode locks; every stated compatibility question
  for this slice is about key overlap, not read/write mode, so collapsing
  to one exclusive mode is sound for what §4 and §5 ask this slice to
  check.
- `t1`'s footprint is `<<{1}, {2,3}>>` (point key 1, then the range
  covering keys 2-3) — acquired in two steps, in ascending canonical
  order. `t2` and `t3` both need only the point on key 2: identical
  footprints, so they exercise "equal keys conflict" directly against each
  other, and both race `t1`'s pending range acquisition the moment `t1`
  has acquired its point and is trying to acquire the range that covers
  key 2 — this is the dispatch-time race named in #16 ("a point
  registration while a covering range is being acquired").
- Canonical order is checked by an `ASSUME` that every footprint is sorted
  ascending by its minimal key, rather than computed at runtime; a real
  engine computes this order from the resources a call's arguments name,
  this model fixes it once per scenario.
- Commit releases a transaction's resources instantly (strict two-phase
  locking: held from acquisition through commit, nothing held after). The
  model has no abort path in this slice — a transaction's resources are
  never released early — because admission and ordering, not recovery, are
  what this module checks; abort appears in `Reservations.tla` where a
  typed reservation's tentative/committed distinction makes it meaningful.

**Serializability method.** A `precedes` relation records a happens-before
edge `u -> t` exactly when `t`'s acquisition of a resource overlapping one
of `u`'s could only have succeeded because `u` had already committed and
released it (`MutualExclusion` rules out any other way an overlap
resolves). The `Serializable` invariant checks this relation never closes a
cycle through its transitive closure: an acyclic happens-before graph over
every pair of transactions whose footprints actually overlapped is the
standard conflict-serializability certificate — commit order is a valid
serial order for every such pair. This slice has no read/write values or
caller-visible responses (that starts in `Reservations.tla`), so there is
no separate "responses" dimension to check here; `Serializable` is the
whole of what serializability means for pure resource admission.

**Checked:** `TypeOK`, `MutualExclusion` (mutual exclusion of incompatible
holders), `Serializable` (as above), and absence of deadlock (TLC's default
deadlock check — no `-deadlock` flag is passed, and the spec adds an
explicit `Terminating` self-loop so a finished run is never mistaken for a
deadlock).

**Result:** no violation found. 66 states generated, 40 distinct, search
depth 8, finished in under 1 second. Log: `specs/kernel/Resources.tlc.log`.

## Typed reservations

`docs/concept.md` §3.2/§5's three reservation kinds are each modeled in
their own small module rather than as one combined scenario, so that each
mechanism's own contention case stays small enough to design and check on
its own within the "2-3 transactions" bound. Each is a resource inside
`Resources.tla`'s one namespace in the real design; this slice checks each
kind's own compatibility and commit-effect rules in isolation, which is
sound because §4 states those rules are the same kernel protocol
end to end — nothing about acquiring a claim, a pin, or an escrow draw
changes how acquisition, waiting, and release work, only what a resource
*is* and what committing it durably does.

**Serializability method for all three (claims, pins, escrow).** Each
transaction in these scenarios holds exactly one reservation, so there is
no multi-resource acquisition order to reason about the way
`Resources.tla`'s `precedes` graph does; the serializability content
collapses to "the committed set is consistent with some serial order",
which each kind's own safety invariant already states directly: for a
claim, at most one committed winner per value (`Uniqueness`); for a pin,
no committed deletion coexists with a live reference, now or later
(`ReferentialIntegrity`); for escrow, the sum of committed draws never
exceeds the ceiling (`BalanceNonNegative`). Each of these is true in every
reachable state exactly because every reservation is held exclusively (or,
for escrow, within its admitted allowance) until it is released by commit
or abort — strict two-phase locking, carried over unchanged from
`Resources.tla`. A transaction that must wait on a tentative holder is not
owed a position in the serial order at all if that holder aborts (concept
§5: "the waiter proceeds as if the value had never been claimed"), so no
precedence edge is recorded for it, unlike a `Resources.tla` acquisition
that always eventually succeeds against a releasing holder.

### specs/kernel/Claims.tla — unique claim

**Abstractions and bounds:**

- 3 transactions, 2 claim values (`"x"`, `"y"`). `t1` and `t2` both claim
  `"x"` (direct contention); `t3` claims the independent `"y"`.
- A value already in `committedValues` can never be acquired again — a
  claim's uniqueness is permanent, unlike a `Resources.tla` lock, which
  frees the moment its holder commits.
- A claim's permanent failure (value already committed by someone else)
  is not modeled as a distinct error transition; it forces that
  transaction toward the model's general "abort for any reason" action
  (concept §5), since the specific claim can then never be acquired.
  Distinguishing the error payload a caller would see is out of scope
  (this model has no response/payload type at all).
- No abort reason is distinguished from any other (all aborts are the one
  `AbortClaim` action) — this slice does not model *why* a transaction
  aborts, only that it may, at any time, for any reason.

**Checked:** `TypeOK`, `Uniqueness`, `MutualExclusion` (no two active
transactions tentatively hold the same value), no deadlock.

**Result:** no violation found. 297 states generated, 105 distinct, search
depth 7, finished in under 1 second. Log: `specs/kernel/Claims.tlc.log`.

### specs/kernel/Pins.tla — foreign-key pin

**Abstractions and bounds:**

- 3 transactions, 1 parent. `c1` and `c2` each insert a child row pinning
  that parent (concurrent pins must not conflict with each other); `d1`
  deletes it (a delete must conflict with every pin on it, tentative or
  durable).
- The reverse-reference set (`liveChildren`) never shrinks in this model
  — there is no child-deletion action — because the property under test
  is whether a live reference blocks a parent's deletion, not child
  lifecycle; a child is never removed once it commits.
- A separate `parentDeleted` flag marks the parent gone for good once its
  deletion commits. **This was added after TLC found a real gap in the
  first version of this model**, not a concept defect: the first version
  only checked `status = "active"` to decide whether a delete or a pin
  was "held", so once `d1` *committed*, nothing stopped a brand-new child
  from acquiring a pin referencing the now-deleted parent — `d1` aborted,
  `c1` aborted, `d1` acquired and committed its deletion, and only then
  did `c2` acquire and commit a pin on the deleted parent, violating
  `ReferentialIntegrity` (trace: `Init` → `AbortChild(c1)` →
  `AcquireDelete(d1)` → `CommitDelete(d1)` → `AcquirePin(c2)` →
  `CommitChild(c2)`). `docs/concept.md` §3.2/§5 only specifies that a pin
  blocks deletion while live, not that a deleted parent can never be
  referenced again; that second half is ordinary foreign-key semantics my
  first pass simply forgot to encode, not something the concept gets
  wrong, so the model was fixed (not the concept) by adding `parentDeleted`
  and gating `AcquirePin` on it.
- This model does not check that a parent exists *before* any pin is ever
  taken on it (there is no "parent created" transaction) — only that an
  already-committed deletion is never followed by a new live reference.

**Checked:** `TypeOK`, `ReferentialIntegrity` (a committed deletion never
coexists with a live reference, and none ever reappears after),
`NoConflictingHolds` (a delete reservation and a pin on the same parent
are never both held), no deadlock.

**Result:** first run found the gap above; after adding `parentDeleted`,
no violation found. 207 states generated, 93 distinct, search depth 7,
finished in under 1 second. Log: `specs/kernel/Pins.tlc.log`.

### specs/kernel/Escrow.tla — numeric bound

**Abstractions and bounds:**

- 3 transactions drawing against one shared ceiling of 3: amounts 2, 2,
  and 1. Any two of the three can exceed or exactly exhaust the ceiling
  (2+2=4, 2+1=3), forcing real admission contention within the bound.
- Admission is pessimistic: a draw is granted tentatively only if the
  ceiling still has room for every other currently-admitted draw plus
  this one, so a concurrent draw already sees the reduced remaining
  allowance before either commits (concept §5).
- A committed draw is never released back to the pool in this model
  (there is no refund/rollback-of-a-committed-draw action) - the same
  "committed is forever" durability as `Claims.committedValues` and
  `Pins.parentDeleted`. An aborted draw's tentative hold is released.
- Only one draw per transaction; the model does not represent a
  transaction drawing against more than one escrow or adjusting its draw
  amount after acquiring it.

**Checked:** `TypeOK`, `BalanceNonNegative` (the ceiling never lets the
sum of committed allowances exceed it - the check constraint itself),
`NoOveradmission` (the stronger invariant that tentative holds are also
never admitted past the ceiling), no deadlock.

**Result:** no violation found. 297 states generated, 105 distinct,
search depth 7, finished in under 1 second. Log:
`specs/kernel/Escrow.tlc.log`.

## Not covered by this slice

Crash, recovery and atomic publication (slice 2, docs/concept.md §9.1,
§9.2); the migration watermark protocol (§8, §9.3); and liveness - whether
every tentative-reservation waiter eventually proceeds under a stated
fairness assumption (§9.5, slice 3). This slice's models also do not
represent read/write lock modes (every resource is exclusive), multiple
reservations per transaction for the typed-reservation kinds, or any
caller-visible response payload.
