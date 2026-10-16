# E3 — frozen inputs: the six summary-synthesis families

Status: **frozen**, pending independent review by a different provider. This
document is E3's first deliverable under the freeze protocol in
[`docs/experiments.md`](../../docs/experiments.md) ("Freeze protocol", "E3",
"Freeze order and authorship"): it fixes the six families E3 synthesizes
summaries for. Quota allocation is E2's frozen family, used by reference; the
other five are defined here. For each family it fixes the IR shape, every
response and semantic failure with its bounds and overflow, a finite checking
domain with its size, worked examples, the expected guard and algebraic
character, and a preregistered list of invalid candidates (mutants) that a
sound checker must reject.

It states no summary for any new family (summaries are what E3 synthesizes),
no grammar, no candidate language, no checking method and no result. The
grammar, the candidate language, the checker's soundness contract and the
manifest are a separate, later freeze by a different author (#5, slice S2).
No code was written for this document. Every worked example and every mutant
witness was derived by hand from the semantics stated here. Nothing here may
change after a result is seen: a change voids every family registered before
it (`docs/experiments.md`, "Freeze order and authorship").

## 1. Conventions shared by all six families

**The IR.** "Today's IR" means `crates/kiln-ir` on `main` at commit
`1671bd0`, interpreted under `docs/concept.md` §2.2. Each family is one
`Program`: one `Schema` and the family's `Function`s. All six are expressible
in today's IR. Where a more natural formulation would need a construct the IR
lacks, the family section names that construct and says what the frozen shape
does instead; no family bends a construct to mean something else.

**Function notation.** E2's notation
([E2 inputs §1](../e2-parallel-ordered-transactions/inputs.md), "Function
notation") applies unchanged, with three additions:

| Notation | `kiln-ir` constructor |
|---|---|
| `insert T[k] {c: e}` | `Effect::Insert { table: T, key: k, columns: {c: e} }` |
| `delete T[k]` | `Effect::Delete { table: T, key: k }` |
| `!=` | `Condition::Compare` with `Relation::NotEqual` |

A function with no parameters is written `fn name():` and declares an empty
parameter list.

**Outcomes.** A call's outcome is a `kiln_ir::Outcome`: `Committed {
response: Value::Integer(n) }` or `Aborted { reason }`. A declared failure is
`AbortReason::Declared(name)`, written `abort name`. Two families also reach
a structural failure, and a third reaches one only outside its measured
batches, each written out in full where it occurs:
`AbortReason::Overflow(OverflowReason::Width { .. })` (wallet, §7),
`AbortReason::CheckViolation { .. }` (only inside a mutant, §5.5) and
`AbortReason::Overflow(OverflowReason::Arithmetic)` (sequence, §4.2, only
after about `9.2 · 10^18` calls). An aborted
call changes no state and does not stop later calls (D4, D6).

**Check order.** Each function tests its failures in the order its body lists
them, and the first failing test decides the outcome. Argument-only tests come
first, so a malformed call reads nothing.

**What equivalence means.** For an initial store `x_0` and a batch `c_1 ..
c_n`, `kiln_ir::replay` gives each call's outcome and the final store. A
candidate is correct for that pair only if every outcome (the response value,
or the full `AbortReason` value, so a wrong abort name is a divergence) and
the final store equal `replay`'s. The access log is not compared. This is the
equivalence the head ruling on #5 names ("final state, every response and
abort").

**Family domain and checking domain.** Each family declares two domains.

- The **family domain** is every initial store and argument the family's
  schema and functions accept: for example, every `balance` in `[0, 32767]`
  for the wallet. The admissible bound class (`docs/experiments.md`, "E2 —
  Admissible bound class") is judged here: a summary's size and its combine
  and apply costs must not grow with batch size on batches drawn from the
  family domain.
- The **checking domain** is a finite subset, small enough for an exhaustive
  checker: a set `X` of initial stores, a call alphabet `A` (each element one
  function with concrete arguments, malformed ones included), and every batch
  of `1` to `L = 4` calls from `A`. Its size is `P = |X| · (|A| + |A|^2 +
  |A|^3 + |A|^4)`, the number of (initial store, batch) pairs. States reached
  from `X` during a batch may lie outside `X`; `replay` judges them exactly.

**Tabulation versus finite state** (head ruling on #77, 16.10.2026). Every
family's checking domain is finite, so on that domain alone every family has
a constant-size exact table of its transformer. E3's expected misses (§7,
§8) and the bound class are therefore meaningful only under this rule, which
classifies each state component:

- A candidate summary **may** enumerate the values of a categorical or
  control state component with at most 16 named values: the order
  lifecycle's status (4 values, §6) and the dispatcher's cursor position
  among its `N = 5` workers (§5).
  A key component whose value set is a fixed semantic constant of at most
  16 named values is categorical too: the lease table's `K = 2` resources
  (§8), so per-resource structure is enumerable.
- A candidate **may never** enumerate the values of a quantity or identifier
  component (balances, quotas, counts, sequence values, owner identifiers,
  keys whose value set is not a fixed small constant), even when the family's semantics fixes a finite range for it (quota
  `[0, 1000]`, owners `[1, 1 000 000]`). Encoding such a lookup through
  literals, branches or schema-derived domains counts as enumeration.
- Semantic constants **may** appear in a candidate's arithmetic and guards:
  `N = 5`, the wallet's overflow threshold 32 767, the quota ceiling 1000,
  the owner validation bound 1 000 000. Using a threshold is not enumerating
  the values below it.
- Enumeration means a candidate whose size or structure grows with the
  number of values of a component. S2's grammar enforces this (§10,
  question 1).

Classification of every family's components under the rule:

- Quota (§3): `remaining` is a quantity (never enumerable; 1000 is a usable
  constant).
- Sequence (§4): `last_value` is a quantity (not enumerable; 1 000 000 is a
  usable constant).
- Dispatcher (§5): `cursor` is a control component (enumerable, 5 values).
- Order lifecycle (§6): `status` is categorical (enumerable, 4 values).
- Wallet (§7): `balance` is a quantity (not enumerable; 32 767 and 100 are
  usable constants).
- Lease (§8): row presence per resource and the resource key (`K = 2`) are
  categorical/control, enumerable; the stored `owner` is an identifier, not
  enumerable; 1 000 000 is a usable constant.

**Mutants.** Each family's list is the preregistered set of invalid
candidates against which E3's "zero false acceptances" condition is judged
(`docs/experiments.md`, "Conjunctive success condition"). A mutant is defined
by its **behaviour**, not by a representation: either the exact semantics of
the family's program with one named change, or a named deviation from
`replay` on composite batches. Every mutant carries a **witness**, an initial
store and a batch inside the checking domain, with `replay`'s outcome and the
mutant's divergent outcome derived by hand; the witness proves the mutant is
not equivalent to the family on the checking domain, so a sound checker can
and must reject it. (For example, changing quota's `s0.remaining >= quantity`
to `>` is an equivalent mutant: at `remaining = quantity` the else branch also
writes `0` and returns `quantity`. It is therefore not listed.)

Mutant codes, used in every family:

| Code | Kind | Definition |
|---|---|---|
| `B` | boundary | one guard's comparison moved by one, or its relation swapped |
| `A` | abort | a failure dropped, added, or reported under another reason |
| `R` | response | one call's response altered while every final store stays right |
| `V` | overflow | an overflow check dropped, or replaced by saturation |
| `C` | clamp order | saturation applied in a different place than the program applies it |
| `O` | order | the effect of "first `S`, then `T`" computed as "first `T`, then `S`" when both are non-empty |
| `N` | non-associative | combining two summaries is exact whenever at least one operand summarizes a single call, but deviates as stated when both operands summarize two or more calls; so every left-to-right fold is exact and only another bracketing diverges |
| `T` | small batches only | exact on every batch of at most 3 calls; on a longer batch, the batch's effect on the store is that of its first 3 calls (the rest dropped), while every prefix of at most 3 calls stays exact |
| `F` | family-specific | stated in the family's section |

An `N` witness names its bracketing, for example `((c1, c2), (c3, c4))`, and
compares the final store that bracketing yields with `replay`'s. For `O` and
`T` witnesses the divergence shown is the final store; an `R` witness shows
an outcome that diverges while the final store agrees.

If S2's candidate language cannot express a mutant, slice S4 records it as
"not expressible" with the reason. Such a mutant cannot be falsely accepted;
it is reported, never dropped silently.

## 2. The six families as a set

| § | Family | Guard behaviour | Expected algebraic character | Expected against the bound class |
|---|---|---|---|---|
| 3 | Quota allocation (E2, by reference) | two-sided data-dependent saturation that shortens the grant, plus aborts at exactly empty and exactly full | clipped translation (E2 §2.4) | admitted (E2's hand summary is fixed-size) |
| 4 | Sequence with restart | no data-dependent guard on the declared domain (§4.2): only an argument range test | translate-or-overwrite on the declared domain: each batch shifts the value or overwrites it and then shifts it; non-commutative, not invertible | admitted |
| 5 | Round-robin dispatcher | data-dependent wrap branch that chooses the written value and never aborts | cyclic group of order 5: commutative, invertible | admitted |
| 6 | Order lifecycle | equality tests on an enumerated status, each selecting a named abort; no arithmetic | transformation monoid of a 4-element set: non-commutative, non-invertible, finite | admitted |
| 7 | Strict wallet | all-or-nothing threshold comparing the stored balance with an argument, plus a width-overflow abort at the top | piecewise translation whose breakpoints come from the arguments and grow in number with batch length | **expected miss** |
| 8 | Lease table | row presence on an argument-chosen key, and equality of a stored value with an argument; inserts and deletes rows | per-key maps on `{free} ∪ owners` that pin each released owner value as an exception | **expected miss** (§8.5, conditional on the tabulation rule of §1) |

No two rows share a guard behaviour, and no two share an algebra up to
renaming of constants: the dispatcher is the only group, the order lifecycle
the only finite non-numeric monoid, the sequence the only unguarded family,
quota the only saturating one, and the two expected misses fail for different
reasons (ordered thresholds on a number versus an exception set of equality
values). E2's inventory and accounts families are not among the six.

No family was chosen because a summary for it is known. Each comes from an
ordinary OLTP pattern named in its section. Two are expected to miss, so E3's
"at least 4 of 6" holds only if all four others are admitted (§10,
question 2).

## 3. Quota allocation (by reference to E2)

**Semantics.** Exactly E2's frozen family: constants, schema, the functions
`allocate` and `refill`, outcomes, bounds and overflow, as
[E2 inputs §2.1-§2.2](../e2-parallel-ordered-transactions/inputs.md) fix
them; E2's worked examples Q1-Q4 (§2.5) are this family's examples. E2's
hand-written summary (§2.4) is hidden from the synthesizer and compared only
afterwards (head ruling on #5). Nothing below changes E2's family.

**Guard.** Two-sided data-dependent saturation (a partial grant at the floor,
a partial refill at the ceiling) and aborts at exactly `remaining = 0` and
`remaining = 1000`. **Character.** Clipped translation.

### 3.1 Checking domain

- `X`: `remaining` in `{0, 1, 2, 6, 7, 8, 49, 50, 51, 500, 950, 992, 993,
  994, 999, 1000}`, 16 stores.
- `A`: `allocate(q)` and `refill(q)` for `q` in `{0, 1, 2, 7, 50, 51}`, 12
  calls (`0` and `51` are malformed).
- `P = 16 · 22 620 = 361 920`.

### 3.2 Mutants

| ID | Code | Mutant | Witness | `replay` | Mutant |
|---|---|---|---|---|---|
| Q-1 | `B` | `refill` aborts `quota_full` when `remaining >= 999` instead of `= 1000` | `999`, `[refill(1)]` | commit 1; final 1000 | abort `quota_full`; final 999 |
| Q-2 | `B` | `allocate` aborts `quota_exhausted` when `remaining <= 1` instead of `= 0` | `1`, `[allocate(1)]` | commit 1; final 0 | abort `quota_exhausted`; final 1 |
| Q-3 | `B` | `allocate` accepts `quantity = 51` (`quantity > 51`) | `500`, `[allocate(51)]` | abort `invalid_quantity`; final 500 | commit 51; final 449 |
| Q-4 | `A` | all-or-nothing allocate: `0 < remaining < quantity` aborts `quota_exhausted` instead of granting `remaining` | `2`, `[allocate(7)]` | commit 2; final 0 | abort `quota_exhausted`; final 2 |
| Q-5 | `R` | a partial refill responds `quantity` instead of `1000 - remaining` | `994`, `[refill(7)]` | commit 6; final 1000 | commit 7; final 1000 |
| Q-6 | `C` | a batch's translations are summed and clamped into `[0, 1000]` once at the end, ignoring every intermediate saturation | `994`, `[refill(7), allocate(7)]` | commit 6, commit 7; final 993 | final 994 |
| Q-7 | `O` | order swap | `2`, `[allocate(7), refill(7)]` | commit 2, commit 7; final 7 | final 2 |
| Q-8 | `N` | when both operands are composite, the right operand's net translation is applied without its intermediate saturation, then clamped into `[0, 1000]` | `0`, `((refill(1), refill(1)), (allocate(7), refill(7)))` | commit 1, 1, 2, 7; final 7 | final 2 |
| Q-9 | `T` | small batches only | `7`, `[allocate(1) × 4]` | commit 1 four times; final 3 | final 4 |

## 4. Sequence with restart

A database sequence generator: `next_value` advances the sequence and returns
the new value, like PostgreSQL's `nextval`. `restart(value)` overwrites the
sequence's last value and returns the value it replaced, an atomic
get-and-set. This deliberately differs from PostgreSQL's `setval`, which
returns its own argument: returning the replaced value makes the response
depend on the prefix of the batch.

### 4.1 Constants and IR shape

| Constant | Value |
|---|---|
| `V_MAX`, the largest valid restart value | 1 000 000 |

Schema: table `sequence`, one column `last_value`, `Integer { width: I64,
domain: None }`; check constraint `last_value AtLeast 0`. The initial store
holds exactly one `sequence` row, key `0`.

```
fn next_value():
  read sequence[0] -> s0
  update sequence[0].last_value = s0.last_value + 1
  return s0.last_value + 1

fn restart(value: Integer(I64)):
  if value < 1:                    abort invalid_value
  if value > 1000000:              abort invalid_value
  read sequence[0] -> s0
  update sequence[0].last_value = value
  return s0.last_value
```

**Guard.** None that depends on the store on the declared domain: from an
initial value in `[0, 1 000 000]` and within a batch of at most 256 calls,
every `next_value` commits, and `restart` fails only on its argument. The one
store-dependent failure, the arithmetic overflow of §4.2, lies outside that
domain. Every call reads and writes
`sequence[0]`. **Character.** Translate-or-overwrite.

### 4.2 Outcomes

With `v` the last value before the call:

| Call | Condition, tested in this order | Outcome | `last_value` after |
|---|---|---|---|
| `next_value()` | `v = i64::MAX` | abort `Overflow(Arithmetic)` | `v` |
| | otherwise | commit `v + 1` | `v + 1` |
| `restart(w)` | `w < 1` or `w > 1000000` | abort `invalid_value` | `v` |
| | otherwise | commit `v` | `w` |

**Bounds and overflow.** The family domain is an initial `last_value` in
`[0, 1 000 000]`. `next_value` has no ceiling, so the value may exceed
`V_MAX`. Today's IR adds with checked `i64` arithmetic, so the `next_value`
call that would exceed `i64::MAX` aborts with
`AbortReason::Overflow(OverflowReason::Arithmetic)`, changes no state, and
every later `next_value` aborts the same way until a `restart` commits. The
column is `I64`, so no width overflow precedes it. That abort lies outside
the declared domain: from an initial value in `[0, 1 000 000]`, a batch of at
most 256 calls leaves the value at most `1 000 256`, so there every
`next_value` commits; reaching `i64::MAX` would take about `9.2 · 10^18`
calls. The translate-or-overwrite character of §2 holds on that declared
domain; on unrestricted batches the translation is partial, undefined (an
abort) for a step past `i64::MAX`, and a summary must reproduce that abort
to be exact there. The check constraint never fires: no call writes a
negative value. An argument outside `I64` cannot be expressed as a
`Value::Integer`, and none is generated.

### 4.3 Checking domain

- `X`: `last_value` in `{0, 1, 2, 5, 999 999, 1 000 000}`, 6 stores.
- `A`: `next_value()`, and `restart(w)` for `w` in `{0, 1, 2, 7, 1 000 000,
  1 000 001}`, 7 calls (`0` and `1 000 001` are malformed).
- `P = 6 · 2 800 = 16 800`.

### 4.4 Worked examples

**S1** — every outcome. Initial `last_value = 5`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `next_value()` | 5 | commit 6 | 6 |
| 2 | `next_value()` | 6 | commit 7 | 7 |
| 3 | `restart(2)` | 7 | commit 7 | 2 |
| 4 | `next_value()` | 2 | commit 3 | 3 |
| 5 | `restart(0)` | 3 | abort `invalid_value` | 3 |
| 6 | `restart(1000001)` | 3 | abort `invalid_value` | 3 |

Final `3`.

**S2** — order dependence of state and responses. Initial `last_value = 0`.

| Batch | Outcomes | Final |
|---|---|---|
| S2a `[restart(7), next_value()]` | commit 0, commit 8 | 8 |
| S2b `[next_value(), restart(7)]` | commit 1, commit 1 | 7 |

**S3** — no ceiling. Initial `999 999`: `[next_value(), next_value(),
restart(1000000)]` gives commit 1 000 000, commit 1 000 001, commit
1 000 001; final `1 000 000`.

### 4.5 Mutants

| ID | Code | Mutant | Witness | `replay` | Mutant |
|---|---|---|---|---|---|
| S-1 | `B` | `restart` accepts `0` (`value < 0`) | `5`, `[restart(0)]` | abort `invalid_value`; final 5 | commit 5; final 0 |
| S-2 | `B` | `restart` rejects `1 000 000` (`value >= 1000000`) | `5`, `[restart(1000000)]` | commit 5; final 1 000 000 | abort `invalid_value`; final 5 |
| S-3 | `R` | `next_value` responds the value before its increment | `5`, `[next_value()]` | commit 6; final 6 | commit 5; final 6 |
| S-4 | `R` | `restart(w)` responds `w` (PostgreSQL's `setval`) | `5`, `[restart(7)]` | commit 5; final 7 | commit 7; final 7 |
| S-5 | `F` | a `next_value` that is the `k`-th `next_value` after the most recent committed `restart(w)` in its batch responds `w + k - 1` instead of `w + k`; final stores stay right | `0`, `[restart(7), next_value()]` | commit 0, commit 8; final 8 | commit 0, commit 7; final 8 |
| S-6 | `O` | order swap | `0`, `[restart(7), next_value()]` | commit 0, commit 8; final 8 | final 7 |
| S-7 | `N` | when both operands are composite, every `restart` in the right operand is ignored and only its `next_value`s count | `0`, `((next_value(), next_value()), (restart(7), next_value()))` | commit 1, 2, 2, 8; final 8 | final 3 |
| S-8 | `T` | small batches only | `0`, `[next_value() × 4]` | commit 1, 2, 3, 4; final 4 | final 3 |

## 5. Round-robin dispatcher

Work items are assigned to `N = 5` workers in turn. `assign` returns the
worker whose turn it is and passes the turn on; `skip(count)` passes the turn
over `count` workers (for example, workers going offline) and returns the
worker whose turn it now is.

### 5.1 Constants and IR shape

| Constant | Value |
|---|---|
| `N`, the number of workers | 5 (cursor values `0..=4`) |
| largest valid skip | 4 |

Schema: table `dispatcher`, one column `cursor`, `Integer { width: I8,
domain: None }`; check constraints `cursor AtLeast 0` and `cursor AtMost 4`.
The initial store holds exactly one `dispatcher` row, key `0`.

```
fn assign():
  read dispatcher[0] -> s0
  if s0.cursor = 4:
    update dispatcher[0].cursor = 0
    return 4
  update dispatcher[0].cursor = s0.cursor + 1
  return s0.cursor

fn skip(count: Integer(I8)):
  if count < 1:                    abort invalid_count
  if count > 4:                    abort invalid_count
  read dispatcher[0] -> s0
  if s0.cursor + count >= 5:
    update dispatcher[0].cursor = s0.cursor + count - 5
    return s0.cursor + count - 5
  update dispatcher[0].cursor = s0.cursor + count
  return s0.cursor + count
```

**Missing construct, named.** A remainder operator would write the wrap as
`(cursor + count) mod 5`. Today's IR has only `Add` and `Subtract`
(`ArithmeticOp`), so the wrap is the explicit branch above; one subtraction
suffices because `cursor + count <= 8 < 10`.

**Guard.** A data-dependent wrap branch that chooses the written value and
never aborts; only the argument test on `skip` aborts. **Character.** Cyclic
group of order 5.

### 5.2 Outcomes

With `c` the cursor before the call:

| Call | Condition, tested in this order | Outcome | `cursor` after |
|---|---|---|---|
| `assign()` | `c = 4` | commit `4` | `0` |
| | otherwise | commit `c` | `c + 1` |
| `skip(k)` | `k < 1` or `k > 4` | abort `invalid_count` | `c` |
| | `c + k >= 5` | commit `c + k - 5` | `c + k - 5` |
| | otherwise | commit `c + k` | `c + k` |

**Bounds and overflow.** The family domain is a cursor in `[0, 4]`, which is
also the checking domain's `X`. Every write lies in `[0, 4]`, so neither check
constraint fires; the largest intermediate value is `8`, inside `I8`. An
argument outside `I8` aborts with `Overflow(Argument)` before the body (D3)
and is not in the checking domain.

### 5.3 Checking domain

- `X`: `cursor` in `{0, 1, 2, 3, 4}`, 5 stores.
- `A`: `assign()`, and `skip(k)` for `k` in `{0, 1, 2, 3, 4, 5}`, 7 calls
  (`0` and `5` are malformed).
- `P = 5 · 2 800 = 14 000`.

### 5.4 Worked examples

**D1** — every outcome. Initial `cursor = 3`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `assign()` | 3 | commit 3 | 4 |
| 2 | `assign()` | 4 | commit 4 | 0 |
| 3 | `skip(4)` | 0 | commit 4 | 4 |
| 4 | `skip(2)` | 4 | commit 1 (`4 + 2 - 5`) | 1 |
| 5 | `skip(5)` | 1 | abort `invalid_count` | 1 |
| 6 | `skip(0)` | 1 | abort `invalid_count` | 1 |
| 7 | `assign()` | 1 | commit 1 | 2 |

Final `2`.

**D2** — order-independent state, order-dependent responses. Initial `0`.

| Batch | Outcomes | Final |
|---|---|---|
| D2a `[skip(3), assign()]` | commit 3, commit 3 | 4 |
| D2b `[assign(), skip(3)]` | commit 0, commit 4 | 4 |

**D3** — two wraps. Initial `2`: `[skip(4), skip(4), skip(2)]` gives commit 1
(`2 + 4 - 5`), commit 0 (`1 + 4 - 5`), commit 2; final `2`.

### 5.5 Mutants

The order mutant `O` is deliberately absent: every call's transformer here is
a rotation, rotations commute, so a swapped combine denotes the same
transformer on every prefix and is a valid candidate, not a mutant.

| ID | Code | Mutant | Witness | `replay` | Mutant |
|---|---|---|---|---|---|
| D-1 | `B` | `skip`'s wrap test is `s0.cursor + count > 5` | `4`, `[skip(1)]` | commit 0; final 0 | writes 5, so `abort CheckViolation { cursor AtMost 4, key 0 }`; final 4 |
| D-2 | `B` | `skip` accepts `count = 5` (`count > 5`) | `0`, `[skip(5)]` | abort `invalid_count`; final 0 | commit 0; final 0 |
| D-3 | `B` | `skip` rejects `count = 4` (`count > 3`) | `0`, `[skip(4)]` | commit 4; final 4 | abort `invalid_count`; final 0 |
| D-4 | `R` | `assign` responds the cursor after its move | `1`, `[assign()]` | commit 1; final 2 | commit 2; final 2 |
| D-5 | `R` | `skip` responds the cursor before its move | `2`, `[skip(1)]` | commit 3; final 3 | commit 2; final 3 |
| D-6 | `F` | `assign` at cursor 4 commits 4 and leaves the cursor at 4 (saturates instead of wrapping) | `4`, `[assign(), assign()]` | commit 4, commit 0; final 1 | commit 4, commit 4; final 4 |
| D-7 | `F` | a batch's net advance is reduced by 5 at most once: final cursor `c + a - 5` if `c + a >= 5`, else `c + a`, for net advance `a` | `0`, `[skip(4), skip(4), skip(4)]` | commit 4, 3, 2; final 2 | final 7 |
| D-8 | `N` | when both operands are composite, the right operand's net advance is subtracted instead of added | `0`, `((skip(1), skip(1)), (skip(1), skip(1)))` | commit 1, 2, 3, 4; final 4 | final 0 |
| D-9 | `T` | small batches only | `0`, `[assign() × 4]` | commit 0, 1, 2, 3; final 4 | final 3 |

## 6. Order lifecycle

One order's status moves through a fixed state machine: an order is created,
paid, shipped, or cancelled; a created or paid order can be cancelled, and a
cancelled order can be reopened as a fresh created order. Every committed call
returns the status it found, so `cancel` tells its caller whether a refund is
due (`1`, paid) or not (`0`, created).

### 6.1 Constants and IR shape

| Status | Value |
|---|---|
| created | 0 |
| paid | 1 |
| shipped | 2 |
| cancelled | 3 |

Schema: table `orders`, one column `status`, `Integer { width: I8, domain:
None }`; check constraints `status AtLeast 0` and `status AtMost 3`. The
initial store holds exactly one `orders` row, key `0`.

```
fn pay():
  read orders[0] -> s0
  if s0.status = 1:                abort already_paid
  if s0.status = 2:                abort already_paid
  if s0.status = 3:                abort order_cancelled
  update orders[0].status = 1
  return s0.status

fn ship():
  read orders[0] -> s0
  if s0.status = 0:                abort not_paid
  if s0.status = 2:                abort already_shipped
  if s0.status = 3:                abort order_cancelled
  update orders[0].status = 2
  return s0.status

fn cancel():
  read orders[0] -> s0
  if s0.status = 2:                abort already_shipped
  if s0.status = 3:                abort already_cancelled
  update orders[0].status = 3
  return s0.status

fn reopen():
  read orders[0] -> s0
  if s0.status != 3:               abort not_cancelled
  update orders[0].status = 0
  return s0.status
```

**Guard.** Equality tests on an enumerated status, each selecting a named
abort; no function has an argument, and no arithmetic is performed.
**Character.** Transformation monoid of a 4-element set.

### 6.2 Outcomes

With `s` the status before the call:

| Call | Condition, tested in this order | Outcome | `status` after |
|---|---|---|---|
| `pay()` | `s = 1` or `s = 2` | abort `already_paid` | `s` |
| | `s = 3` | abort `order_cancelled` | `3` |
| | `s = 0` | commit `0` | `1` |
| `ship()` | `s = 0` | abort `not_paid` | `0` |
| | `s = 2` | abort `already_shipped` | `2` |
| | `s = 3` | abort `order_cancelled` | `3` |
| | `s = 1` | commit `1` | `2` |
| `cancel()` | `s = 2` | abort `already_shipped` | `2` |
| | `s = 3` | abort `already_cancelled` | `3` |
| | `s = 0` or `s = 1` | commit `s` | `3` |
| `reopen()` | `s != 3` | abort `not_cancelled` | `s` |
| | `s = 3` | commit `3` | `0` |

**Bounds and overflow.** Every write is a literal in `[0, 3]`; the check
constraints never fire and no arithmetic exists to overflow. The family
domain is the status set `{0, 1, 2, 3}`, which is also `X`.

### 6.3 Checking domain

- `X`: `status` in `{0, 1, 2, 3}`, 4 stores.
- `A`: `pay()`, `ship()`, `cancel()`, `reopen()`, 4 calls (none malformed:
  no function has an argument).
- `P = 4 · 340 = 1 360`.

### 6.4 Worked examples

**O1** — forward path and its failures. Initial `status = 0`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `ship()` | 0 | abort `not_paid` | 0 |
| 2 | `pay()` | 0 | commit 0 | 1 |
| 3 | `pay()` | 1 | abort `already_paid` | 1 |
| 4 | `ship()` | 1 | commit 1 | 2 |
| 5 | `cancel()` | 2 | abort `already_shipped` | 2 |
| 6 | `reopen()` | 2 | abort `not_cancelled` | 2 |

Final `2`.

**O2** — cancellation and reopening. Initial `status = 1`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `cancel()` | 1 | commit 1 (refund due) | 3 |
| 2 | `ship()` | 3 | abort `order_cancelled` | 3 |
| 3 | `cancel()` | 3 | abort `already_cancelled` | 3 |
| 4 | `pay()` | 3 | abort `order_cancelled` | 3 |
| 5 | `reopen()` | 3 | commit 3 | 0 |
| 6 | `pay()` | 0 | commit 0 | 1 |

Final `1`.

**O3** — order dependence. Initial `0`.

| Batch | Outcomes | Final |
|---|---|---|
| O3a `[pay(), cancel()]` | commit 0, commit 1 | 3 |
| O3b `[cancel(), pay()]` | commit 0, abort `order_cancelled` | 3 |
| O3c `[cancel(), reopen()]` | commit 0, commit 3 | 0 |
| O3d `[reopen(), cancel()]` | abort `not_cancelled`, commit 0 | 3 |

### 6.5 Mutants

There is no numeric boundary in this family; its `B`-like mutants drop or
move an equality guard and are coded `A`.

| ID | Code | Mutant | Witness | `replay` | Mutant |
|---|---|---|---|---|---|
| O-1 | `A` | `cancel` succeeds on a shipped order (its `s0.status = 2` test dropped) | `2`, `[cancel()]` | abort `already_shipped`; final 2 | commit 2; final 3 |
| O-2 | `A` | `pay` succeeds on a cancelled order (its `s0.status = 3` test dropped) | `3`, `[pay()]` | abort `order_cancelled`; final 3 | commit 3; final 1 |
| O-3 | `A` | `ship` on a cancelled order reports `not_paid` | `3`, `[ship()]` | abort `order_cancelled`; final 3 | abort `not_paid`; final 3 |
| O-4 | `R` | `cancel` responds the new status `3` | `1`, `[cancel()]` | commit 1; final 3 | commit 3; final 3 |
| O-5 | `F` | `reopen` moves a cancelled order to paid instead of created | `3`, `[reopen(), pay()]` | commit 3, commit 0; final 1 | commit 3, abort `already_paid`; final 1 |
| O-6 | `O` | order swap | `0`, `[cancel(), reopen()]` | commit 0, commit 3; final 0 | final 3 |
| O-7 | `N` | when both operands are composite, the result is the right operand alone (the left operand's effect is lost) | `0`, `((pay(), ship()), (cancel(), reopen()))` | commit 0, 1, abort `already_shipped`, abort `not_cancelled`; final 2 | final 0 |
| O-8 | `T` | small batches only | `0`, `[cancel(), reopen(), pay(), ship()]` | commit 0, 3, 0, 1; final 2 | final 1 |

O-5's witness agrees on the final store and diverges on the second response.

## 7. Strict wallet — expected miss

A prepaid balance with all-or-nothing debits of arbitrary amounts: a debit
succeeds in full if the balance covers it and otherwise aborts
`insufficient_funds`, and every committed call reveals the new balance. This
is the case the source ideation calls "much harder" than partial allocation:
"Arbitrary quantities create complicated state-dependent success patterns"
(`docs/research/ideation/codex-astra-ideation.md`). The balance column is
`I16`, so a credit near the top overflows and aborts.

### 7.1 Constants and IR shape

| Constant | Value |
|---|---|
| `X_MAX`, the largest valid amount | 100 |
| balance width | `I16`, so at most 32 767 |

Schema: table `wallet`, one column `balance`, `Integer { width: I16, domain:
None }`; check constraint `balance AtLeast 0`. The initial store holds
exactly one `wallet` row, key `0`.

```
fn debit(amount: Integer(I16)):
  if amount < 1:                   abort invalid_amount
  if amount > 100:                 abort invalid_amount
  read wallet[0] -> s0
  if s0.balance < amount:          abort insufficient_funds
  update wallet[0].balance = s0.balance - amount
  return s0.balance - amount

fn credit(amount: Integer(I16)):
  if amount < 1:                   abort invalid_amount
  if amount > 100:                 abort invalid_amount
  read wallet[0] -> s0
  update wallet[0].balance = s0.balance + amount
  return s0.balance + amount
```

**Guard.** An all-or-nothing threshold comparing the stored balance with an
argument (`s0.balance < amount` aborts), plus a width-overflow abort when a
credit's write leaves `I16`. **Character.** Piecewise translation whose
breakpoints come from the arguments.

### 7.2 Outcomes

With `b` the balance before the call:

| Call | Condition, tested in this order | Outcome | `balance` after |
|---|---|---|---|
| `debit(a)` | `a < 1` or `a > 100` | abort `invalid_amount` | `b` |
| | `b < a` | abort `insufficient_funds` | `b` |
| | otherwise | commit `b - a` | `b - a` |
| `credit(a)` | `a < 1` or `a > 100` | abort `invalid_amount` | `b` |
| | `b + a > 32767` | abort `Overflow(Width { table: wallet, column: balance, key: 0 })` | `b` |
| | otherwise | commit `b + a` | `b + a` |

**Bounds and overflow.** The family domain is a balance in `[0, 32767]`. A
committed debit leaves `b - a >= 0`, so the check constraint never fires. A
credit's write `b + a` is checked against `I16` (D3): above `32 767` the
update aborts before the `return` is reached, which is the overflow row
above; the column declares no clamping domain, so nothing saturates.
Intermediate values lie in `[-100, 32 867]`, far inside `i64`. Arguments
outside `I16` abort with `Overflow(Argument)` before the body and are not in
the checking domain.

### 7.3 Checking domain

- `X`: `balance` in `{0, 1, 2, 3, 4, 5, 6, 7, 99, 100, 101, 200, 32 667,
  32 668, 32 766, 32 767}`, 16 stores.
- `A`: `debit(a)` and `credit(a)` for `a` in `{0, 1, 2, 3, 100, 101}`, 12
  calls (`0` and `101` are malformed).
- `P = 16 · 22 620 = 361 920`.

### 7.4 Worked examples

**W1** — every debit outcome. Initial `balance = 5`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `debit(3)` | 5 | commit 2 | 2 |
| 2 | `debit(3)` | 2 | abort `insufficient_funds` | 2 |
| 3 | `credit(1)` | 2 | commit 3 | 3 |
| 4 | `debit(3)` | 3 | commit 0 (`3 >= 3`) | 0 |
| 5 | `debit(101)` | 0 | abort `invalid_amount` | 0 |
| 6 | `credit(0)` | 0 | abort `invalid_amount` | 0 |

Final `0`.

**W2** — the top. Initial `balance = 32 766`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `credit(1)` | 32 766 | commit 32 767 | 32 767 |
| 2 | `credit(1)` | 32 767 | abort `Overflow(Width { wallet, balance, 0 })` | 32 767 |
| 3 | `debit(100)` | 32 767 | commit 32 667 | 32 667 |
| 4 | `credit(100)` | 32 667 | commit 32 767 | 32 767 |

Final `32 767`.

**W3** — order dependence. Initial `1`.

| Batch | Outcomes | Final |
|---|---|---|
| W3a `[credit(2), debit(3)]` | commit 3, commit 0 | 0 |
| W3b `[debit(3), credit(2)]` | abort `insufficient_funds`, commit 3 | 3 |

**W4** — the number of pieces grows with batch length. A batch of `n` calls
`debit(2)` from balance `b` commits exactly `min(n, ⌊b / 2⌋)` of them: each
succeeds while at least 2 remain. Its final balance is `b - 2 · min(n, ⌊b /
2⌋)`, a translation that is constant on the pieces `[0, 2)`, `[2, 4)`, …,
`[2n - 2, 2n)`, `[2n, 32767]`: `n + 1` pieces with pairwise different
translations `0, -2, …, -2n`. From `b = 5` and `n = 3`: commit 3, commit 1,
abort `insufficient_funds`; final `1`.

### 7.5 Why this family is expected to miss

W4 shows that the number of pieces of a batch's state transformer grows with
batch length. Piece count alone does not prove a miss: W4's own batch has the
short closed form above, because all its amounts are equal. With mixed
amounts, which piece a balance falls in depends on the whole order of the
amounts, and every committed response reveals the exact balance, so no
escrow-style yes/no admission stands in for it (E2 §2.3 makes the same
distinction for quota). E2 froze the multi-account version of this guard and
found no exact representation of fixed size (E2 §4.3, "Size behaviour: not
fixed, measured"); this freeze knows none either. The expectation is
therefore a miss against the bound class. It is an expectation, not a proof:
an admission would be a result worth recording, provided the summary passes
the bound-class measurement on family-domain batches with mixed amounts.

### 7.6 Mutants

| ID | Code | Mutant | Witness | `replay` | Mutant |
|---|---|---|---|---|---|
| W-1 | `B` | the funds test is `s0.balance <= amount` | `3`, `[debit(3)]` | commit 0; final 0 | abort `insufficient_funds`; final 3 |
| W-2 | `B` | `debit` accepts `amount = 101` (`amount > 101`) | `200`, `[debit(101)]` | abort `invalid_amount`; final 200 | commit 99; final 99 |
| W-3 | `A` | partial debit: when `b < a`, commit `0` and set the balance to `0` (quota's partial grant) | `2`, `[debit(3)]` | abort `insufficient_funds`; final 2 | commit 0; final 0 |
| W-4 | `V` | overflow check dropped: a credit above `32 767` commits | `32 767`, `[credit(1)]` | abort `Overflow(Width { wallet, balance, 0 })`; final 32 767 | commit 32 768; final 32 768 |
| W-5 | `V` | overflow saturates, as if `balance` declared the clamping domain `[-32768, 32767]`: the write stores `32 767`, the response is `32 768` | `32 767`, `[credit(1)]` | abort `Overflow(Width { wallet, balance, 0 })`; final 32 767 | commit 32 768; final 32 767 |
| W-6 | `R` | `debit` responds the balance before the debit | `5`, `[debit(2)]` | commit 3; final 3 | commit 5; final 3 |
| W-7 | `O` | order swap | `1`, `[credit(2), debit(3)]` | commit 3, commit 0; final 0 | final 3 |
| W-8 | `N` | when both operands are composite, every debit in the right operand is applied without its funds test | `0`, `((credit(1), credit(1)), (debit(3), credit(1)))` | commit 1, 2, abort `insufficient_funds`, commit 3; final 3 | final 0 |
| W-9 | `T` | small batches only | `7`, `[debit(1) × 4]` | commit 6, 5, 4, 3; final 3 | final 4 |

## 8. Lease table — expected miss

An advisory lease (lock) table: `acquire` takes a free resource for an owner,
`release` frees it if the caller holds it, and `inspect` reports the holder.
A held resource is a row; a free one has no row, so `acquire` inserts and
`release` deletes.

### 8.1 Constants and IR shape

| Constant | Value |
|---|---|
| `K`, the number of resources | 2 (keys `0..=1`) |
| `O_MAX`, the largest valid owner | 1 000 000 |

Schema: table `lease`, one column `owner`, `Integer { width: I32, domain:
None }`; check constraints `owner AtLeast 1` and `owner AtMost 1000000`. The
initial store holds a `lease` row at key `r` exactly when resource `r` is
held, with `owner` its holder; it holds no row at any other key.

```
fn acquire(resource: Integer(I8), owner: Integer(I32)):
  if resource < 0:                 abort invalid_resource
  if resource > 1:                 abort invalid_resource
  if owner < 1:                    abort invalid_owner
  if owner > 1000000:              abort invalid_owner
  read lease[resource] -> s0
  if absent s0:
    insert lease[resource] {owner: owner}
    return owner
  if s0.owner = owner:             abort already_held
  abort held_by_other

fn release(resource: Integer(I8), owner: Integer(I32)):
  if resource < 0:                 abort invalid_resource
  if resource > 1:                 abort invalid_resource
  if owner < 1:                    abort invalid_owner
  if owner > 1000000:              abort invalid_owner
  read lease[resource] -> s0
  if absent s0:                    abort not_held
  if s0.owner != owner:            abort not_owner
  delete lease[resource]
  return owner

fn inspect(resource: Integer(I8)):
  if resource < 0:                 abort invalid_resource
  if resource > 1:                 abort invalid_resource
  read lease[resource] -> s0
  if absent s0:                    abort not_held
  return s0.owner
```

`if absent s0: X` followed by more lines is E2's notation for
`Terminal::Branch { condition: RowPresent(s0), when_present: rest,
when_absent: X }`. Because presence is tested before every insert, delete and
projection, the structural failures `DuplicateKey`, `MissingRow` and
`ProjectionSourceAbsent` are unreachable.

**Missing construct, named.** A natural `acquire` would abort with a record
naming the current holder (`docs/concept.md` §2.2 D9, "An abort carries its
name and a record of values computed before it"). Today's `AbortReason::
Declared` carries only a name, so the frozen shape reveals the holder through
`inspect` instead, and `held_by_other` names no owner.

**Guard.** Row presence on an argument-chosen key, and equality of the stored
owner with the owner argument. **Character.** Per-key maps on `{free} ∪
owners` that pin each released owner value as an exception.

### 8.2 Outcomes

With `h_r` the holder of resource `r` before the call (`free` if no row):

| Call | Condition, tested in this order | Outcome | Effect |
|---|---|---|---|
| `acquire(r, o)` | `r < 0` or `r > 1` | abort `invalid_resource` | none |
| | `o < 1` or `o > 1000000` | abort `invalid_owner` | none |
| | `h_r = free` | commit `o` | insert row `r` with owner `o` |
| | `h_r = o` | abort `already_held` | none |
| | otherwise | abort `held_by_other` | none |
| `release(r, o)` | `r < 0` or `r > 1` | abort `invalid_resource` | none |
| | `o < 1` or `o > 1000000` | abort `invalid_owner` | none |
| | `h_r = free` | abort `not_held` | none |
| | `h_r != o` | abort `not_owner` | none |
| | otherwise | commit `o` | delete row `r` |
| `inspect(r)` | `r < 0` or `r > 1` | abort `invalid_resource` | none |
| | `h_r = free` | abort `not_held` | none |
| | otherwise | commit `h_r` | none |

**Bounds and overflow.** No function computes arithmetic. Every inserted
owner passed the argument test, so it lies in `[1, 1 000 000]`, inside `I32`,
and neither check constraint fires. Arguments outside `I8` or `I32` abort with
`Overflow(Argument)` before the body and are not in the checking domain. The
family domain is every assignment of `free` or an owner in `[1, 1 000 000]`
to each of the two resources.

### 8.3 Checking domain

- `X`: each of resources `0` and `1` is `free` or held by owner `1`, `2` or
  `3`, 16 stores. A store is written `(h_0, h_1)`.
- `A`: `acquire(r, o)` and `release(r, o)` for `r` in `{0, 1}` and `o` in
  `{1, 2, 3}` (12 calls); `inspect(0)` and `inspect(1)`; and the malformed
  `acquire(2, 1)` and `release(0, 0)`; 16 calls.
- `P = 16 · 69 904 = 1 118 464`.

### 8.4 Worked examples

**L1** — every outcome. Initial `(free, 2)`.

| # | Call | Outcome | State after |
|---|---|---|---|
| 1 | `acquire(0, 1)` | commit 1 | `(1, 2)` |
| 2 | `acquire(0, 1)` | abort `already_held` | `(1, 2)` |
| 3 | `acquire(0, 3)` | abort `held_by_other` | `(1, 2)` |
| 4 | `inspect(1)` | commit 2 | `(1, 2)` |
| 5 | `release(1, 3)` | abort `not_owner` | `(1, 2)` |
| 6 | `release(1, 2)` | commit 2 | `(1, free)` |
| 7 | `release(1, 2)` | abort `not_held` | `(1, free)` |
| 8 | `inspect(1)` | abort `not_held` | `(1, free)` |
| 9 | `acquire(2, 1)` | abort `invalid_resource` | `(1, free)` |
| 10 | `release(0, 0)` | abort `invalid_owner` | `(1, free)` |

Final `(1, free)`.

**L2** — an exception set. The batch `[release(0, 1), acquire(0, 2)]` from
each holder of resource 0 (resource 1 free throughout):

| Initial `h_0` | Outcomes | Final `h_0` |
|---|---|---|
| `free` | abort `not_held`, commit 2 | 2 |
| 1 | commit 1, commit 2 | 2 |
| 2 | abort `not_owner`, abort `already_held` | 2 |
| 3 | abort `not_owner`, abort `held_by_other` | 3 |

The released owner `1` behaves like `free`; owner `3` keeps the resource.

### 8.5 Why this family is expected to miss

Take, on the family domain, the batch `B(o_1, …, o_k; o) = [release(0, o_1),
…, release(0, o_k), acquire(0, o)]` with distinct owners `o_1 … o_k` and an
owner `o` not among them. From initial holder `h_0`:

- `h_0 = free`: every release aborts `not_held`; the acquire commits; final
  `o`.
- `h_0 = o_i`: releases before position `i` abort `not_owner`, release `i`
  commits, the later releases abort `not_held`, the acquire commits; final
  `o`.
- `h_0 = o`: every release aborts `not_owner`, the acquire aborts
  `already_held`; final `o`.
- any other owner: every release aborts `not_owner`, the acquire aborts
  `held_by_other`; final unchanged.

So the batch's transformer on resource 0 maps exactly `{free, o, o_1, …,
o_k}` to `o` and fixes every other holder. Two such batches with different
sets `{o_1, …, o_k}` therefore denote different transformers, and batches of
`k + 1` calls denote at least `C(999 999, k)` distinct transformers. An exact
summary must distinguish them, so its size is at least `log2 C(999 999, k)`
bits, more than `11 k` bits for every `k <= 256` (since `C(m, k) >= (m /
k)^k` and `999 999 / 256 > 3 900 > 2^11`).

That counting argument has a precise scope: it lower-bounds the number of
distinguishable transformers that batches of `k + 1` calls denote, and so
the information an exact summary of such a batch must carry. It does not by
itself exclude a representation of fixed size. The owner universe is finite,
so a full-domain transformation table over `{free} ∪ [1, 1 000 000]` per
resource has a size and a composition cost independent of batch length, and
the growing lower bound stays below that table's size. Such a table
enumerates owners, an identifier component, and §1's tabulation rule
forbids that even though the family's semantics fixes the range
`[1, 1 000 000]`. The miss
is therefore an expectation that rests on that rule and on the concrete
representation restrictions S2's grammar imposes, not a proof. Under them,
the growing number of transformers is why this freeze expects no admissible
summary; an admission would be a result worth recording, and would also
call S2's enforcement of the rule into question. The checking domain's three
owners cannot show this growth by themselves; that is why the family-domain
rule of §1 matters here.

### 8.6 Mutants

| ID | Code | Mutant | Witness | `replay` | Mutant |
|---|---|---|---|---|---|
| L-1 | `B` | `acquire` rejects resource 1 (`resource > 0`) | `(free, free)`, `[acquire(1, 1)]` | commit 1; final `(free, 1)` | abort `invalid_resource`; final `(free, free)` |
| L-2 | `B` | `release`'s owner test swapped to `s0.owner = owner` aborting `not_owner` | `(1, free)`, `[release(0, 2)]` | abort `not_owner`; final `(1, free)` | commit 2; final `(free, free)` |
| L-3 | `A` | `acquire` by the current holder reports `held_by_other` | `(1, free)`, `[acquire(0, 1)]` | abort `already_held` | abort `held_by_other` |
| L-4 | `R` | `release` responds the resource instead of the owner | `(2, free)`, `[release(0, 2)]` | commit 2; final `(free, free)` | commit 0; final `(free, free)` |
| L-5 | `F` | cross-key leak: a committed `release(0, o)` also frees resource 1 if `o` holds it | `(1, 1)`, `[release(0, 1)]` | commit 1; final `(free, 1)` | final `(free, free)` |
| L-6 | `F` | only the last release of a resource in the batch takes effect; every earlier release of it aborts `not_owner` with no effect | `(1, free)`, `[release(0, 1), release(0, 2), acquire(0, 3)]` | commit 1, abort `not_held`, commit 3; final `(3, free)` | abort `not_owner`, abort `not_owner`, abort `held_by_other`; final `(1, free)` |
| L-7 | `O` | order swap | `(free, free)`, `[acquire(0, 1), release(0, 1)]` | commit 1, commit 1; final `(free, free)` | final `(1, free)` |
| L-8 | `N` | when both operands are composite, every `release` in the right operand is a no-op | `(free, free)`, `((acquire(0, 1), inspect(0)), (release(0, 1), acquire(0, 2)))` | commit 1, 1, 1, 2; final `(2, free)` | final `(1, free)` |
| L-9 | `T` | small batches only | `(free, free)`, `[acquire(0, 1), release(0, 1), acquire(0, 2), acquire(1, 3)]` | commit 1, 1, 2, 3; final `(2, 3)` | final `(2, free)` |

L-6 is this family's "right on small batches only" in its natural form: it
is exact on every batch with at most one release per resource, which is
what a summary keeping one exception owner per resource would get right.

## 9. Summary of the frozen domains and mutants

| Family | States `X` | Calls `A` | `L` | `P` | Mutants | Expected |
|---|---|---|---|---|---|---|
| Quota | 16 | 12 | 4 | 361 920 | 9 (Q-1 … Q-9) | admitted |
| Sequence | 6 | 7 | 4 | 16 800 | 8 (S-1 … S-8) | admitted |
| Dispatcher | 5 | 7 | 4 | 14 000 | 9 (D-1 … D-9) | admitted |
| Order lifecycle | 4 | 4 | 4 | 1 360 | 8 (O-1 … O-8) | admitted |
| Strict wallet | 16 | 12 | 4 | 361 920 | 9 (W-1 … W-9) | miss |
| Lease table | 16 | 16 | 4 | 1 118 464 | 9 (L-1 … L-9) | miss (conditional on §1's tabulation rule, §8.5) |

Fifty-two preregistered mutants in total, each with a hand-derived witness
inside its family's checking domain.

## 10. Open questions for the reviewer

1. **No tabulation over the checking domain (§1).** On a finite checking
   domain every family has a constant-size exact table, so the bound class
   and both expected misses are meaningful only under the head's ruling
   stated in §1: categorical or control components with at most 16 named
   values (order status, dispatcher cursor) may be enumerated; quantity and
   identifier components (balances, quotas, counts, sequence values, owners,
   keys) may not, even where their semantics fixes a finite range, while
   semantic constants stay usable in arithmetic and guards. S2 owns its
   enforcement in the grammar.
   Recommendation: S2's candidate language takes no checking-domain value as
   an input and offers no construct that encodes such a lookup through
   literals, branches or a schema-derived domain, a candidate's size is
   measured on its representation, and the bound-class measurement batches
   are drawn from the family domain (for the lease, with owners from the
   full `[1, 1 000 000]`).
2. **Two expected misses (§2).** With the wallet and the lease expected to
   miss, E3's "at least 4 of 6" requires all four other families to be
   admitted. Recommendation: keep both. Both are expectations, and neither
   predetermines the measured outcome: E3 records whatever the measurement
   shows. The lease's expectation rests on the counting argument of §8.5 and
   on the tabulation rule, so it also checks that the bound-class
   measurement catches growth (a candidate valid over the checking domain
   but growing on family-domain batches must count as a miss); the wallet is
   the open case the source ideation names. Keeping both leaves no expected
   spare admission. Replacing the lease with a bounded-owner variant would
   make it an admit, but its algebra would then collide with the order
   lifecycle's finite monoid.
3. **Which mutants are tested (§1).** `docs/experiments.md` asks for mutants
   of every admitted family ("Rejection check") and for zero false
   acceptances over the preregistered invalid candidates ("Conjunctive
   success condition"). Recommendation: S4 tests every preregistered mutant
   of all six families against the checker, admitted or not, because the
   checker's soundness does not depend on what the synthesizer found.
4. **Measurement batches for the bound class.** This document freezes the
   families and their checking domains, not the batches the bound class is
   measured on. Recommendation: S2 freezes them, using E2's SplitMix64
   generator and seed derivation (E2 §5) with family numbers `4` to `8` in
   the order of §4 to §8, and E2's growth rule from #3's measurement protocol
   once it is frozen.
