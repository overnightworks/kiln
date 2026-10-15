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
