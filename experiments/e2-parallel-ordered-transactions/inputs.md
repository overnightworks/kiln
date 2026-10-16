# E2 — frozen inputs, part A: family semantics and summaries

Status: **frozen**, pending independent review. This document is the first
half of #3's first deliverable under the freeze protocol in
`docs/experiments.md` ("Freeze protocol", "E2 — Deferred to #3"): for each
of E2's three families it fixes the IR shape, every response and semantic
failure, the numeric bounds, the guard shape, the summary representation
with its combine and apply operators, worked examples, and the input
distributions. The measurement protocol (hardware, threads, timed region,
repeats, noise limits, verdict arithmetic) is part B, `protocol.md`, a
separate freeze.

No code was written for this document and no measurement exists. Every
number below was chosen before any implementation, and nothing here may
change after a result is seen: a change after implementation has started
voids every result produced under the prior version (`docs/experiments.md`,
"Freeze protocol"). Every worked example was derived by hand from the
semantics stated here. External facts are cited to their primary source.

## 1. Conventions shared by all three families

**The IR.** "Today's IR" means `crates/kiln-ir` on `main` at commit
`c722dff`, interpreted under `docs/concept.md` §2.2 (D1-D7). Each family is
one `Program`: one `Schema` and the family's `Function`s. All three
families are expressible in today's IR; no family needs a construct the IR
lacks (§2.1, §3.1 and §4.1 show each shape).

**Function notation.** Function bodies are written in a pseudo-notation that
maps one-to-one onto `kiln-ir` constructors:

| Notation | `kiln-ir` constructor |
|---|---|
| `read T[k] -> sN` | `Effect::Read { table: T, key: k, into: Slot(N) }` |
| `update T[k].c = e` | `Effect::Update { table: T, key: k, columns: {c: e} }` |
| `if <cond>: X` followed by the rest of the body | `Terminal::Branch { condition, when_present: X, when_absent: rest }` |
| `if absent sN: X` | `Terminal::Branch { condition: RowPresent(Slot(N)), when_present: rest, when_absent: X }` |
| `return e` | `Terminal::Return(e)` |
| `abort name` | `Terminal::Abort(AbortName("name"))` |
| `sN.c` | `Expr::Projection { slot: Slot(N), column: c }` |
| `a + b`, `a - b` | `Expr::Arithmetic(Add` / `Subtract, a, b)` |
| `<`, `<=`, `=`, `>=`, `>` | `Condition::Compare` with the matching `Relation` |
| a bare number | `Expr::Literal(Value::Integer(n))` |
| a parameter name | `Expr::Parameter(name)` |

Effects listed before an `if` belong to the same `Block` as that `if`'s
`Terminal`; the arms of an `if` are `Block`s of their own.

**Outcomes.** A call's outcome is a `kiln_ir::Outcome`: `Committed {
response: Value::Integer(n) }`, or `Aborted { reason }`. Every semantic
failure in this document is a declared abort, `AbortReason::Declared(name)`,
with the name given in each family's outcome table. A call aborted for any
reason changes no state and does not stop later calls (D4, D6).

**Check order.** Each function tests its failures in the order its body
lists them, and the first failing test decides the outcome. Argument-only
tests come first, so a malformed call reads nothing.

**Hot record.** Every quota and inventory call reads and writes one row,
key `0`, of one table. Every account call touches one or two of `K = 4`
rows, keys `0..=3`, of one table (head ruling on #3: one hot record per
family, a small fixed K for accounts). No function of any family inserts or
deletes a row, so the set of present keys is fixed by the initial state.

**State, transformer, response.** For a batch of calls `c_1 .. c_n` from an
initial state `x_0`, serial replay (`kiln_ir::replay`) defines
`x_i = F_i(x_{i-1})` and the outcome `o_i = R_i(x_{i-1})`. For each family
this document fixes:

- the **summary** `S_i` of one call: a value denoting `F_i`;
- **combine** `S ⊕ T`: the summary of "first `S`, then `T`", denoting
  `F_T ∘ F_S`;
- **apply** `apply(S, x)`: the state `F_S(x)`;
- the **response rule** `R_i`: the outcome of call `i` as a pure function of
  the state just before it.

The state before call `i` is `apply(S_1 ⊕ ... ⊕ S_{i-1}, x_0)` and the final
state is `apply(S_1 ⊕ ... ⊕ S_n, x_0)`; both, and every outcome, must equal
`replay`'s (`docs/experiments.md`, "Correctness comparison"). The identity
summary `ε` denotes the identity transformer. A call whose outcome is an
abort decided by its arguments alone has summary `ε`. A call whose abort
depends on the state is covered by its summary's formula, which maps that
state to itself; each family states this explicitly.

**Combine is associative on representations.** For each family the proof
shows that `(S ⊕ T) ⊕ U` and `S ⊕ (T ⊕ U)` are the *same value*, not merely
two values denoting the same function, because the slice-5/6 property test
compares values.

**Compactness rule.** A summary whose measured size, or whose measured
combine or apply cost, grows with batch size fails its family. It is
reported as failing and never replaced by a fallback (head ruling on #3;
`docs/experiments.md`, "Admissible bound class"). The quota and inventory
summaries have a fixed size in bytes and fixed combine and apply costs,
stated in their sections and independent of batch size. The account
summary has no size fixed in advance: it is the best exact representation
this freeze knows for the guarded family, its size can grow with batch
length, and that size is a quantity to be measured, never inferred (§4.3,
head ruling on #70). For every family, size and costs are measured as
§6 lists them. The representations below are the frozen ones; a call list
or a closure tree that replays the calls is not one of them. If an
implementation cannot realise a representation exactly as frozen here,
this document is defective and is re-frozen, voiding prior results; it is
not silently changed.

## 2. Quota allocation

A shared quota of at most `C` units supports two operations, as defined by
the source ideation (`docs/research/ideation/codex-astra-ideation.md`,
"A concrete example is a quota supporting capped refill and partial
allocation"): **allocate** grants up to the requested quantity from what
remains and returns the quantity granted; **refill** returns units to the
quota up to its ceiling and returns the quantity actually added.

### 2.1 Constants and IR shape

| Constant | Value |
|---|---|
| `C`, the quota's ceiling | 1000 |
| `Q_MAX`, the largest valid allocate or refill quantity | 50 |

Schema: table `quota`, one column `remaining`, `Integer { width: I32,
domain: None }`; check constraints `remaining AtLeast 0` and `remaining
AtMost 1000`. The initial store holds exactly one `quota` row, key `0`.

```
fn allocate(quantity: Integer(I32)):
  if quantity < 1:                 abort invalid_quantity
  if quantity > 50:                abort invalid_quantity
  read quota[0] -> s0
  if s0.remaining = 0:             abort quota_exhausted
  if s0.remaining >= quantity:
    update quota[0].remaining = s0.remaining - quantity
    return quantity
  else:
    update quota[0].remaining = 0
    return s0.remaining

fn refill(quantity: Integer(I32)):
  if quantity < 1:                 abort invalid_quantity
  if quantity > 50:                abort invalid_quantity
  read quota[0] -> s0
  if s0.remaining = 1000:          abort quota_full
  if s0.remaining + quantity <= 1000:
    update quota[0].remaining = s0.remaining + quantity
    return quantity
  else:
    update quota[0].remaining = 1000
    return 1000 - s0.remaining
```

**Guard shape.** Every call reads and writes `quota[0]`: any two calls
overlap unconditionally, with no argument guard that could separate them.
The in-body guards are the four comparisons above, in that order.

### 2.2 Outcomes

With `r` the remaining quantity before the call:

| Call | Condition, tested in this order | Outcome | `remaining` after |
|---|---|---|---|
| `allocate(q)` | `q < 1` or `q > 50` | abort `invalid_quantity` | `r` |
| | `r = 0` | abort `quota_exhausted` | `0` |
| | `r >= q` | commit `q` | `r - q` |
| | `0 < r < q` | commit `r` (partial grant) | `0` |
| `refill(q)` | `q < 1` or `q > 50` | abort `invalid_quantity` | `r` |
| | `r = 1000` | abort `quota_full` | `1000` |
| | `r + q <= 1000` | commit `q` | `r + q` |
| | `r + q > 1000` | commit `1000 - r` (partial refill) | `1000` |

**Bounds and overflow.** The check constraints keep every committed
`remaining` in `[0, 1000]`, and the guards above never write outside it, so
neither check ever fires; they are declared because they state the
domain the summary below relies on. The largest intermediate value is
`r + q <= 1050`, far inside `i64` and inside `I32`, so no arithmetic or
width overflow is reachable. An argument outside `I32` aborts with
`AbortReason::Overflow(OverflowReason::Argument)` before the body runs
(D3); the generator (§2.6) never produces one.

### 2.3 Why this family is not a class-2 escrow draw

Class 2 serves a numeric bound such as `remaining >= 0` with an escrow
(`docs/concept.md` §3, class 2; §4, "Escrow draws are admitted by
budget"): a draw is admitted against a reservation of `min(budget,
committed balance)` and then either succeeds in full or fails, and the
whole mechanism rests on two properties. Allocate and refill violate both.

1. **An escrow draw's result is a yes/no test; a partial grant reads the
   exact balance.** §4: "Reservations cover *blind* draws only: an
   operation whose result depends on the escrowed row beyond the
   sufficient-balance test … also reads that row, and the read is ordered
   against every other draw on it." `allocate(q)` returns `min(q, r)`,
   which equals `r` exactly whenever `r < q`: its response is a function
   of the precise prefix of every earlier grant and refill, not of an
   admission decision. Concretely, from `r = 10` the batch
   `[allocate(7), allocate(7)]` gives outcomes `(7, 3)` and final `0`. An
   escrow draw-until-empty gives `(7, fail)` and final `3`: admitted by
   budget, the first call reserves 7, the second waits until it commits,
   then reserves `min(7, 3) = 3` and its draw of 7 fails (§4's own
   example). Escrow cannot produce the response `3`.
2. **Escrow's final state does not depend on the order of the committed
   calls; a capped refill makes it depend.** Under escrow the final
   balance is the initial one minus the committed draws plus the credits,
   whatever their order. Here, from `r = 998`, `[refill(5), allocate(7)]`
   commits both calls with outcomes `(2, 7)` and final `993`, while
   `[allocate(7), refill(5)]` commits both with outcomes `(7, 5)` and final
   `996` (worked example Q3). The same two calls, both committing, leave
   different states: the ceiling discards part of a refill depending on
   what ran before it.

The all-or-nothing variant, "grant `q` if `r >= q`, else fail" with a blind
response, is exactly §3's class-2 ticket example and is therefore not
E2's family: measuring it would measure the wrong class. The per-caller
sequence-number variant named on #70 is not frozen; partial allocation is
chosen because the source ideation defines the family that way.

### 2.4 Summary, combine and apply

On the domain `[0, 1000]`, every call's transformer is a *clipped
translation* `x ↦ clamp(x + a, l, u)` with `clamp(z, l, u) = min(max(z, l),
u)`:

- `allocate(q)` with `1 <= q <= 50` is `clamp(x - q, 0, 1000)`. Check
  against §2.2: for `x >= q` it gives `x - q`; for `0 < x < q` it gives
  `0`; for `x = 0`, the `quota_exhausted` case, it gives `0 = x`.
- `refill(q)` with `1 <= q <= 50` is `clamp(x + q, 0, 1000)`. For
  `x + q <= 1000` it gives `x + q`; above, `1000`; for `x = 1000`, the
  `quota_full` case, `1000 = x`.
- a call aborted for `invalid_quantity` is `ε`.

**Representation.** `S = (a, l, u)`, three `i64`, 24 bytes, with `0 <= l
<= u <= 1000`. Atomic summaries: `allocate(q) = (-q, 0, 1000)`,
`refill(q) = (q, 0, 1000)`, `ε = (0, 0, 1000)`.

This representation is not canonical: distinct triples can denote the same
transformer on `[0, 1000]`. For example `ε ⊕ allocate(7) = (-7, 0, 993)`
while `allocate(7) = (-7, 0, 1000)`; both map every `x` in `[0, 1000]` to
`clamp(x - 7, 0, 1000)`. The identity laws `ε ⊕ S = S ⊕ ε = S` therefore
hold semantically (same transformer on the domain), not byte for byte, and
a test checks them through `apply` on the domain, never by comparing
values. Associativity, proven below, does hold on the values themselves.

**Apply.** `apply((a, l, u), x) = clamp(x + a, l, u)`.

**Combine.** `(a1, l1, u1) ⊕ (a2, l2, u2) = (a1 + a2, clamp(l1 + a2, l2,
u2), clamp(u1 + a2, l2, u2))`.

*Correctness.* `clamp(clamp(x + a1, l1, u1) + a2, l2, u2) = clamp(clamp(x +
a1 + a2, l1 + a2, u1 + a2), l2, u2)`, since adding a constant commutes
with clamping when the bounds shift with it. The **nested-clamp lemma**
gives the rest: for `L <= U` and `l <= u`, `clamp(clamp(z, L, U), l, u) =
clamp(z, clamp(L, l, u), clamp(U, l, u))`. Proof by cases on `z`: for `z <=
L` both sides equal `clamp(L, l, u)`; for `z >= U` both equal `clamp(U, l,
u)`; for `L <= z <= U` the left side is `clamp(z, l, u)`, and the right side
agrees in each sub-case: if `z < l` then `clamp(L, l, u) = l`, so it gives `l`;
if `z > u` then `clamp(U, l, u) = u`, so it gives `u`; if `l <= z <= u`
then `clamp(L, l, u) <= z <= clamp(U, l, u)`, so it gives `z`. The
invariant `l <= u` is preserved, because `clamp` is monotone and `l1 <= u1`.

*Associativity.* Both groupings of `S1 ⊕ S2 ⊕ S3` have first component
`a1 + a2 + a3`. For the second component, `(S1 ⊕ S2) ⊕ S3` gives
`clamp(clamp(l1 + a2, l2, u2) + a3, l3, u3)`, which equals
`clamp(clamp(l1 + a2 + a3, l2 + a3, u2 + a3), l3, u3)` and, by the lemma,
`clamp(l1 + a2 + a3, clamp(l2 + a3, l3, u3), clamp(u2 + a3, l3, u3))`.
That is exactly what `S1 ⊕ (S2 ⊕ S3)` computes, since `S2 ⊕ S3 = (a2 + a3,
clamp(l2 + a3, l3, u3), clamp(u2 + a3, l3, u3))`. The third component is
the same argument with `u1` in place of `l1`. The triples are equal as
values.

**Bounds.** `l` and `u` stay in `[0, 1000]`: each is produced by a clamp
into the later summary's `[l2, u2]`. `|a| <= 50 n`, at most 12 800 for
`n <= 256`. Combine costs 3 additions and 2 clamps; apply costs 1 addition
and 1 clamp. Neither depends on batch size.

**Response rule.** From the prefix state `r`, §2.2's table.

### 2.5 Worked examples

Each row gives the state before the call, the outcome, and the state after.

**Q1** — exhaustion and partial grant. Initial `remaining = 10`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `allocate(7)` | 10 | commit 7 | 3 |
| 2 | `allocate(7)` | 3 | commit 3 | 0 |
| 3 | `allocate(5)` | 0 | abort `quota_exhausted` | 0 |
| 4 | `refill(20)` | 0 | commit 20 | 20 |
| 5 | `allocate(0)` | 20 | abort `invalid_quantity` | 20 |

Final `remaining = 20`. Summaries: `(-7,0,1000)`, `(-7,0,1000)`,
`(-5,0,1000)`, `(20,0,1000)`, `ε`. Left to right: `S1 ⊕ S2 = (-14, 0,
993)`; `⊕ S3 = (-19, 0, 988)`; `⊕ S4 = (1, 20, 1000)`; `⊕ ε = (1, 20,
1000)`. `apply((1, 20, 1000), 10) = clamp(11, 20, 1000) = 20`. Other
grouping: `S3 ⊕ S4 = (15, 20, 1000)`, and `(-14, 0, 993) ⊕ (15, 20, 1000) =
(1, clamp(15, 20, 1000), clamp(1008, 20, 1000)) = (1, 20, 1000)`, the same
value.

**Q2** — the ceiling. Initial `remaining = 990`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `refill(5)` | 990 | commit 5 | 995 |
| 2 | `refill(50)` | 995 | commit 5 | 1000 |
| 3 | `refill(1)` | 1000 | abort `quota_full` | 1000 |
| 4 | `allocate(51)` | 1000 | abort `invalid_quantity` | 1000 |
| 5 | `allocate(50)` | 1000 | commit 50 | 950 |

Final `950`. Summaries left to right: `(5,0,1000) ⊕ (50,0,1000) = (55, 50,
1000)`; `⊕ (1,0,1000) = (56, 51, 1000)`; `⊕ ε = (56, 51, 1000)`; `⊕
(-50,0,1000) = (6, 1, 950)`. `apply((6, 1, 950), 990) = clamp(996, 1, 950)
= 950`.

**Q3** — order dependence of state and responses. Initial `remaining =
998`.

| Batch | Outcomes | Final | Batch summary | `apply(·, 998)` |
|---|---|---|---|---|
| Q3a `[refill(5), allocate(7)]` | commit 2, commit 7 | 993 | `(5,0,1000) ⊕ (-7,0,1000) = (-2, 0, 993)` | 993 |
| Q3b `[allocate(7), refill(5)]` | commit 7, commit 5 | 996 | `(-7,0,1000) ⊕ (5,0,1000) = (-2, 5, 1000)` | 996 |

**Q4** — the escrow contrast of §2.3. Initial `remaining = 10`, batch
`[allocate(7), allocate(7)]`: commit 7 (`10 → 3`), commit 3 (`3 → 0`);
final `0`; summary `(-14, 0, 993)`, `apply(·, 10) = 0`.

### 2.6 Input distribution

- **Initial state:** `remaining = uniform(0, 1000)`.
- **Call mix:** `k = uniform(0, 99)`; `k < 75` is `allocate`, otherwise
  `refill` (3 : 1).
- **Argument:** `quantity = uniform(1, 50)`.
- **Malformed share:** `m = uniform(0, 99)`; if `m = 0`, the quantity is
  replaced by `0` when the drawn quantity is at most 25, and by `51`
  otherwise.
- **Draw order per call:** `k`, `quantity`, `m`.

Why these numbers: allocation outweighs refill so that, within the gating
sizes 64-256, a batch typically both exhausts the quota and refills it,
which exercises every row of §2.2. The 1% malformed share exercises the
argument failure. Neither number was chosen against a result.

## 3. Conditional inventory

One stock record whose sale rule branches on the stock it reads. The rule
is TPC-C's New-Order stock update, TPC Benchmark C Revision 5.11, Clause
2.4.2.2 (p. 30), the specification E1 pins
(`experiments/e1-classification-coverage/inputs.md` §1; PDF fetched
2026-10-16 and read with `pdftotext`): "If the retrieved value for
S_QUANTITY exceeds OL_QUANTITY by 10 or more, then S_QUANTITY is decreased
by OL_QUANTITY; otherwise S_QUANTITY is updated to (S_QUANTITY -
OL_QUANTITY)+91." The normative clause text is frozen. The Appendix A
sample code tests `s_quantity > ol_quantity` instead, which is a different
rule: it can drive the stock below 10. That sample code is not normative
and is not used.

### 3.1 Constants and IR shape

| Constant | Value | Source |
|---|---|---|
| stock domain | `[10, 100]` | initial `S_QUANTITY` random within `[10 .. 100]`, Clause 4.3.3.1; preserved by the rule (§3.2) |
| `Q_MAX`, the largest valid order quantity | 10 | `OL_QUANTITY` random within `[1 .. 10]`, Clause 2.4.1.5 |
| restock amount | 91 | Clause 2.4.2.2 |
| reorder margin | 10 | Clause 2.4.2.2, "by 10 or more" |
| threshold range | `[10, 20]` | Stock-Level threshold, Clause 2.8.1.2 |

Schema: table `stock`, one column `quantity`, `Integer { width: I16,
domain: None }`; check constraints `quantity AtLeast 10` and `quantity
AtMost 100`. The initial store holds one `stock` row, key `0`.

```
fn order_line(quantity: Integer(I16)):
  if quantity < 1:                       abort invalid_quantity
  if quantity > 10:                      abort invalid_quantity
  read stock[0] -> s0
  if s0.quantity - quantity >= 10:
    update stock[0].quantity = s0.quantity - quantity
    return s0.quantity
  else:
    update stock[0].quantity = s0.quantity - quantity + 91
    return s0.quantity

fn stock_check(threshold: Integer(I16)):
  if threshold < 10:                     abort invalid_threshold
  if threshold > 20:                     abort invalid_threshold
  read stock[0] -> s0
  if s0.quantity < threshold:            return 1
  return 0
```

`order_line` returns the stock it retrieved, before its own update, which is
the `S_QUANTITY` Clause 2.4.2.2 retrieves. `stock_check` is TPC-C's
Stock-Level test ("S_QUANTITY lower than threshold", Clause 2.8.2.2)
reduced to the one hot item, so it answers `1` (below threshold) or `0`.

**Guard shape.** Every call reads `stock[0]` and every `order_line` writes it,
so all calls overlap unconditionally. The in-body guards are the argument
tests, the data-dependent `s0.quantity - quantity >= 10`, and
`s0.quantity < threshold`.

**Class.** The overlap is a read-modify-write whose written value is chosen
by a branch on the value read, and whose response reveals that value. It is
not a claim, a pin, or an escrow draw: there is no bound to draw against,
and no call can fail for lack of stock. This family's batch *state* is
order-independent (§3.3 shows the transformers commute), so its order
dependence lies wholly in its responses: each caller sees the stock as left
by exactly its predecessors (example I3).

### 3.2 Outcomes

With `s` the stock before the call:

| Call | Condition, tested in this order | Outcome | `quantity` after |
|---|---|---|---|
| `order_line(q)` | `q < 1` or `q > 10` | abort `invalid_quantity` | `s` |
| | `s - q >= 10` | commit `s` | `s - q` |
| | `s - q < 10` | commit `s` | `s - q + 91` |
| `stock_check(t)` | `t < 10` or `t > 20` | abort `invalid_threshold` | `s` |
| | `s < t` | commit `1` | `s` |
| | `s >= t` | commit `0` | `s` |

**Bounds and overflow.** For `s` in `[10, 100]` and `q` in `[1, 10]`: the
first branch writes `s - q` in `[10, 99]`; the second runs only when `s <=
q + 9`, writing `s - q + 91` in `[91, 100]`. Every write stays in `[10,
100]`, so the check constraints never fire and the domain is preserved by
induction from a valid initial state. Intermediate values lie in `[0, 190]`:
no `i64` or `I16` overflow is reachable. Out-of-width arguments abort
before the body (D3) and are never generated.

### 3.3 Summary, combine and apply

Write `t = s - 10`, in `[0, 90]`. A valid `order_line(q)` maps `t` to
`t - q` when `t >= q`, and to `t - q + 91` when `t < q`. Since `1 <= q <=
10 < 91`, both cases equal `(t - q) mod 91`, taking the non-negative residue:
in the first case `0 <= t - q <= 90`; in the second `-q <= t - q < 0`.
`stock_check` and every aborted call leave `t` unchanged.

**Representation.** `S = d`, one `u8` (1 byte) with `0 <= d <= 90`, denoting
`t ↦ (t - d) mod 91`. Atomic summaries: valid `order_line(q)` is `q`;
`stock_check` and every aborted call are `ε = 0`.

**Apply.** `apply(d, s) = 10 + ((s - 10 - d) mod 91)`. Since `s - 10 - d` lies in
`[-90, 90]`, this is `s - d` when `s - 10 >= d`, and `s - d + 91` otherwise.

**Combine.** `d1 ⊕ d2 = (d1 + d2) mod 91`.

*Correctness and associativity.* `((t - d1) mod 91 - d2) mod 91 = (t - (d1 +
d2)) mod 91`. Combine is addition in the integers mod 91, which is
associative (and commutative), so both groupings give the same residue.

**Bounds.** 1 byte. Combine costs 1 addition and 1 conditional subtraction;
apply costs 2 subtractions and 1 conditional addition. Neither depends on
batch size.

**Response rule.** From the prefix stock `s`, §3.2's table.

### 3.4 Worked examples

**I1** — both branches, a read-only check, and both argument failures. Initial
`quantity = 25`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `order_line(10)` | 25 | commit 25 | 15 |
| 2 | `order_line(6)` | 15 | commit 15 | 100 |
| 3 | `stock_check(20)` | 100 | commit 0 | 100 |
| 4 | `order_line(10)` | 100 | commit 100 | 90 |
| 5 | `order_line(11)` | 90 | abort `invalid_quantity` | 90 |
| 6 | `stock_check(9)` | 90 | abort `invalid_threshold` | 90 |

Final `90`. Summaries `10, 6, 0, 10, 0, 0`; combined `26`;
`apply(26, 25) = 10 + ((15 - 26) mod 91) = 10 + 80 = 90`.

**I2** — the exact margin. Initial `quantity = 20`.

| # | Call | Before | Outcome | After |
|---|---|---|---|---|
| 1 | `order_line(10)` | 20 | commit 20 | 10 (`20 - 10 = 10`, "by 10 or more") |
| 2 | `stock_check(11)` | 10 | commit 1 | 10 |
| 3 | `order_line(1)` | 10 | commit 10 | 100 (`10 - 1 + 91`) |

Final `100`. Combined summary `11`; `apply(11, 20) = 10 + ((10 - 11) mod
91) = 100`.

**I3** — order-dependent responses with order-independent state. Initial
`quantity = 15`.

| Batch | Outcomes | Final | Summary |
|---|---|---|---|
| I3a `[order_line(5), order_line(3)]` | commit 15 (`→ 10`), commit 10 (`→ 98`) | 98 | `8` |
| I3b `[order_line(3), order_line(5)]` | commit 15 (`→ 12`), commit 12 (`→ 98`) | 98 | `8` |

`apply(8, 15) = 10 + ((5 - 8) mod 91) = 98`.

### 3.5 Input distribution

- **Initial state:** `quantity = uniform(10, 100)` (Clause 4.3.3.1).
- **Call mix:** `k = uniform(0, 48)`; `k < 45` is `order_line`, otherwise
  `stock_check`. This is the 45 : 4 ratio of New-Order to Stock-Level in
  E1's frozen TPC-C mix (`experiments/e1-classification-coverage/inputs.md`
  §1).
- **Arguments:** `order_line`: `quantity = uniform(1, 10)` (Clause 2.4.1.5);
  `stock_check`: `threshold = uniform(10, 20)` (Clause 2.8.1.2).
- **Malformed share:** `m = uniform(0, 99)`; if `m = 0`, an `order_line`
  quantity becomes `0` when the drawn value is at most 5, and `11` otherwise;
  a `stock_check` threshold becomes `9` when the drawn value is at most 15,
  and `21` otherwise. This 1% share exercises the argument failures. It is not
  TPC-C's 1% rollback, which concerns an unused item number and has no
  counterpart on one hot record.
- **Draw order per call:** `k`, the argument, `m`.

## 4. Mixed account operations

Heterogeneous operations on a small fixed set of accounts: deposits,
withdrawals, transfers, and compliance freezes. A withdrawal or transfer
succeeds only if its source account holds at least the amount, and
otherwise aborts with the named reason `insufficient_funds` (head ruling on
#70; `docs/concept.md` §2.1 uses exactly this abort as the IR's example).
Every committed money movement reveals a balance, and a freeze changes what
later operations may do.

### 4.1 Constants and IR shape

| Constant | Value |
|---|---|
| `K`, the number of accounts | 4 (keys `0..=3`) |
| `X_MAX`, the largest valid amount | 10 000 |
| status values | `0` = open, `1` = frozen |

Schema: table `accounts`, columns `balance` `Integer { width: I64, domain:
None }` and `status` `Integer { width: I8, domain: None }`; check
constraints `balance AtLeast 0`, `status AtLeast 0` and `status AtMost 1`.
The initial store holds exactly the rows `0..=3`, every balance at least
`0`. The guard keeps every committed balance non-negative, so the balance
check never fires; it is declared because it states the domain the summary
below relies on.

```
fn deposit(account: Integer(I64), amount: Integer(I64)):
  if amount < 1:                    abort invalid_amount
  if amount > 10000:                abort invalid_amount
  read accounts[account] -> s0
  if absent s0:                     abort account_not_found
  if s0.status = 1:                 abort account_frozen
  update accounts[account].balance = s0.balance + amount
  return s0.balance + amount

fn withdraw(account: Integer(I64), amount: Integer(I64)):
  if amount < 1:                    abort invalid_amount
  if amount > 10000:                abort invalid_amount
  read accounts[account] -> s0
  if absent s0:                     abort account_not_found
  if s0.status = 1:                 abort account_frozen
  if s0.balance < amount:           abort insufficient_funds
  update accounts[account].balance = s0.balance - amount
  return s0.balance - amount

fn transfer(from: Integer(I64), to: Integer(I64), amount: Integer(I64)):
  if amount < 1:                    abort invalid_amount
  if amount > 10000:                abort invalid_amount
  if from = to:                     abort same_account
  read accounts[from] -> s0
  if absent s0:                     abort account_not_found
  read accounts[to] -> s1
  if absent s1:                     abort account_not_found
  if s0.status = 1:                 abort account_frozen
  if s1.status = 1:                 abort account_frozen
  if s0.balance < amount:           abort insufficient_funds
  update accounts[from].balance = s0.balance - amount
  update accounts[to].balance = s1.balance + amount
  return s0.balance - amount

fn freeze(account: Integer(I64)):
  read accounts[account] -> s0
  if absent s0:                     abort account_not_found
  if s0.status = 1:                 abort already_frozen
  update accounts[account].status = 1
  return s0.balance

fn unfreeze(account: Integer(I64)):
  read accounts[account] -> s0
  if absent s0:                     abort account_not_found
  if s0.status = 0:                 abort not_frozen
  update accounts[account].status = 0
  return s0.balance
```

`s0.balance < amount` compares a projection with a parameter, a
`Condition::Compare` with `Relation::Less`; the guard needs no construct
today's IR lacks. A frozen source is reported as `account_frozen` even when
its balance is also short, because the status test comes first.

**Guard shape.** Calls naming disjoint accounts do not overlap. Within E2,
a batch is the class-3 batch over all `K` rows, acquired as one footprint
at dispatch (`docs/concept.md` §4); the key-equality guard of §3, class 1,
is not what E2 measures. The in-body guards are the argument tests, account
presence, the status tests, and the funds test, in the order listed.

**Class.** Every committed call returns a balance read from the contended
row, every money movement is gated by status reads that a freeze or
unfreeze in the same batch changes, and every debit is gated by the exact
source balance. A claim or a pin describes none of these dependencies. Nor
does an escrow draw: "Reservations cover *blind* draws only" (§4), and a
withdrawal or transfer here returns `β_a - x`, the exact balance after it,
so the read is ordered against every other draw on that row. The funds
test with a blind response would be §3's class-2 escrow (`balance >= 0`
"becomes an escrow"); with the revealing response frozen here it is not.

### 4.2 Outcomes

With `β` the balances and `σ` the statuses before the call:

| Call | Condition, tested in this order | Outcome | Effect |
|---|---|---|---|
| `deposit(a, x)` | `x < 1` or `x > 10000` | abort `invalid_amount` | none |
| | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` | abort `account_frozen` | none |
| | otherwise | commit `β_a + x` | `β_a += x` |
| `withdraw(a, x)` | `x < 1` or `x > 10000` | abort `invalid_amount` | none |
| | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` | abort `account_frozen` | none |
| | `β_a < x` | abort `insufficient_funds` | none |
| | otherwise | commit `β_a - x` | `β_a -= x` |
| `transfer(a, b, x)` | `x < 1` or `x > 10000` | abort `invalid_amount` | none |
| | `a = b` | abort `same_account` | none |
| | `a` not in `0..=3`, then `b` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` or `σ_b = 1` | abort `account_frozen` | none |
| | `β_a < x` | abort `insufficient_funds` | none |
| | otherwise | commit `β_a - x` | `β_a -= x`, `β_b += x` |
| `freeze(a)` | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` | abort `already_frozen` | none |
| | otherwise | commit `β_a` | `σ_a = 1` |
| `unfreeze(a)` | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 0` | abort `not_frozen` | none |
| | otherwise | commit `β_a` | `σ_a = 0` |

A committed debit leaves exactly `β_a - x >= 0`; `withdraw(a, β_a)`
commits `0`.

**Bounds and overflow.** No committed call makes a balance negative, by the
funds test, and each call raises any one balance by at most `10 000`. From
initial balances in `[0, 50 000]` (§4.5), every balance after at most 256
calls lies in `[0, 2 610 000]`, and so does every intermediate and response
value. No `i64` overflow is reachable within the frozen inputs. The summary
below does not model `i64` overflow; its exactness claim is limited to
batches whose balances stay within `i64`, which the frozen generator
guarantees, and slice 3's generator test asserts that balance bound (head
ruling on #70). A status write is a literal `0` or `1` and stays inside
`I8` and the check constraints.

### 4.3 Summary, combine and apply

The state is `(σ, β)` with `σ` in `{0,1}^4` and `β` in `N^4`, `N` the
non-negative integers (§4.1's balance check). Every call's transformer has
the shape `σ' = g(σ)`, `β' = β + t(σ, β)`, where each bit of `g` either
keeps `σ_i` or sets it to a constant, and the translation `t(σ, β)` in
`Z^4` is piecewise constant, its pieces cut by thresholds on single
balances:

- `deposit(a, x)`: `t_a = x` if `σ_a = 0`, else `0`.
- `withdraw(a, x)`: `t_a = -x` if `σ_a = 0` and `β_a >= x`, else `0`. The
  `account_frozen` and `insufficient_funds` aborts are the `else`: they map
  the state to itself.
- `transfer(a, b, x)`: `t_a = -x` and `t_b = +x` if `σ_a = σ_b = 0` and
  `β_a >= x`, else `0`.
- `freeze(a)`: `g` sets bit `a` to `1`, `t = 0`. The `already_frozen` abort
  leaves it `1`, which agrees. `unfreeze(a)`: `g` sets bit `a` to `0`,
  `t = 0`; the `not_frozen` abort agrees likewise.
- `invalid_amount`, `same_account` and `account_not_found` are decided by
  the arguments, since the key set is fixed (§1), and have summary `ε`.

A composition keeps this shape: a later call's test `β_a + t_a(σ, β) >= x`,
on the state its prefix left, is a threshold on `β_a` within each piece of
the prefix. The pieces are not separable per account. A transfer from `a`
credits `b` only if `β_a >= x`, so after it a debit of `b` succeeds or not
depending on `β_a` (example A3): `t_b` is a function of `σ` and the whole
of `β`, not of `(σ_b, β_b)` alone.

**Interval diagrams.** The variables are ordered `σ0, σ1, σ2, σ3, β0, β1,
β2, β3`. An interval diagram is a rooted acyclic graph whose leaves carry
an integer and whose inner nodes each test one variable. A node's out-edges
partition its variable's domain into intervals listed in increasing order:
`{0}` and `{1}` for a status variable, and `[lo_1 = 0, lo_2), …, [lo_e,
∞)` for a balance variable. Along every path the tested variables strictly
follow the order. The diagram denotes the function whose value at `(σ, β)`
is the leaf reached by taking, at each node, the edge whose interval holds
that variable's value. A diagram is **reduced** when (R1) no two adjacent
edges of a node lead to the same target, (R2) no node has a single edge,
and (R3) no two distinct nodes denote the same function.

For a fixed variable order, every function `{0,1}^4 × N^4 → Z` that is
constant on the cells of a finite grid has exactly one reduced diagram, up
to the naming of its nodes. This is Bryant's canonicity argument for
reduced ordered binary decision diagrams (R. E. Bryant, "Graph-Based
Algorithms for Boolean Function Manipulation", IEEE Transactions on
Computers C-35(8), 1986), with intervals in place of the two Boolean edges.
By induction from the leaves: a function is tested at the first variable it
depends on (R2); a value `c` starts a new interval exactly when the
residual functions at `c - 1` and at `c` differ (a boundary that is not
forced is forbidden by R1, and a forced one cannot be dropped because an
edge's target denotes one function); and each edge's target is the node of
its residual function, which R3 makes unique. Nodes are named canonically
by numbering them `0, 1, …` in order of first visit in a depth-first
traversal from the root that follows each node's edges in increasing
interval order.

**Representation.** `S = (keep, set, D_0, D_1, D_2, D_3)`:

- `keep`, `set`: `u8` bit masks; bit `i` of `keep` set means `σ'_i = σ_i`,
  and `set` gives `σ'_i` for every bit not kept, normalised so that `set &
  keep = 0`.
- `D_j`: the canonically named reduced interval diagram of `t_j`.

The serialised value, which the property test compares and whose length is
the summary's size, is the two masks followed by each `D_j` in turn: a
`u32` node count; for a diagram with no inner node, its leaf value as one
`i64`; otherwise, per node in canonical order, the tested variable (`u8`),
the edge count (`u32`), and per edge its interval's lower bound (`i64`; the
upper bound is the next edge's lower bound), a target tag (`u8`: node or
leaf) and the node index or leaf value (`i64`). Its size is

```
bytes(S)   = 2 + bytes(D_0) + bytes(D_1) + bytes(D_2) + bytes(D_3)
bytes(D)   = 12                                  if D is a single leaf
           = 4 + Σ over nodes (5 + 17 · edges)   otherwise
```

`ε = (0b1111, 0, leaf 0, leaf 0, leaf 0, leaf 0)`, 50 bytes.

**Atomic summaries.** All masks are `keep = 0b1111`, `set = 0` and all
diagrams are `leaf 0`, except:

- `deposit(a, x)`: `D_a` = node `σ_a`: `{0}` → leaf `x`, `{1}` → leaf `0`.
- `withdraw(a, x)`: `D_a` = node `σ_a`: `{0}` → node `β_a` (`[0, x)` → leaf
  `0`, `[x, ∞)` → leaf `-x`), `{1}` → leaf `0`.
- `transfer(a, b, x)`, with `p = min(a, b)` and `q = max(a, b)`: `D_a` =
  node `σ_p`: `{0}` → node `σ_q` (`{0}` → node `β_a` (`[0, x)` → leaf `0`,
  `[x, ∞)` → leaf `-x`), `{1}` → leaf `0`), `{1}` → leaf `0`. `D_b` is the
  same with leaf `+x` in place of `-x`.
- `freeze(a)`: bit `a` cleared in `keep` and set in `set`.
- `unfreeze(a)`: bit `a` cleared in `keep`, and clear in `set`.

**Apply.** `σ'_i = σ_i` if bit `i` of `keep` is set, else bit `i` of `set`;
`β'_j = β_j + D_j(σ, β)`, each diagram evaluated at the input `(σ, β)`. An
evaluation visits at most 8 nodes and picks each edge by binary search over
that node's intervals.

**Combine.** `S1 ⊕ S2`, with `S1` first:

- `keep = keep1 & keep2`;
- `set = ((set1 & keep2) | set2) & ~keep`;
- `D_j` = the canonically named reduced diagram of `t1_j(σ, β) + t2_j(g1(σ),
  β + t1(σ, β))`, where `t1`, `g1` are denoted by `S1` and `t2` by `S2`.

*Correctness.* After `S1` the state is `(g1(σ), β + t1(σ, β))`. Applying
`S2` gives `(g2(g1(σ)), β + t1(σ, β) + t2(g1(σ), β + t1(σ, β)))`: the mask
rule is `g2 ∘ g1` (a bit is kept only if both keep it, and otherwise takes
the last constant written), and the diagrams
denote exactly the translation above. Within each grid cell of `t1` the
thresholds of `t2` on `β_a + t1_a` are thresholds on `β_a`, so the
composite is again constant on the cells of a finite grid and has a reduced
diagram.

*Associativity.* Both groupings of `S1 ⊕ S2 ⊕ S3` denote the same
transformer, since function composition is associative. The masks are
determined by `g`, and the canonically named reduced diagrams by `t`, so
equal transformers have equal serialised values, and both groupings give
the same value.

How combine is computed (for example a memoised recursive descent over the
two diagrams in the manner of Bryant's `apply`) is the implementer's
choice; its result is fixed by the definition above.

**Value bounds.** Within the frozen inputs every interval lower bound lies
in `[0, 2 570 000]` (a funds threshold of at most `10 000` shifted by a net
prefix translation of at most `2 560 000`), and every leaf in `[-2 560 000,
2 560 000]`; `i64` holds both.

**Size behaviour: not fixed, measured.** Unlike §2.4 and §3.3, this
representation has no size fixed in advance: its size depends on the calls
it summarizes, and it **can grow with batch length**. The summary of `n`
calls `withdraw(0, 2)` has a `D_0` whose `β0` node has `n + 1` edges, one
per distinct number of successful withdrawals (example A4), so
`bytes(S)` grows by at least 17 bytes per call. Apply cost grows with the
edge counts (through the binary searches), and combine cost with the
diagram sizes. No constant-size exact representation of this family is
known to this freeze; the source ideation calls the all-or-nothing case
"much harder": "arbitrary quantities create complicated state-dependent
success patterns." This freeze states no upper bound on the growth.

Whether the generated batches make it grow is a quantity to measure, not
to infer from this example: slice 6 reports, for every batch summary and
every chunk summary the scan builds, `bytes(S)`, the total edge count, and
the combine and apply operation counts (nodes and edges visited), at every
batch size. Part B (`protocol.md`) fixes how growth with batch size is
decided from those measurements. If the measured size or cost grows with
batch size, the accounts family fails the admissible bound class, E2 fails
as a whole ("one family failing fails E2 overall"), and the result is
recorded against `docs/concept.md` §13's second kill criterion. There is
no fallback: no capped or approximate diagram, no call list, and no serial
path stands in for the summary. Throughput and correctness are measured and
reported for the family either way.

**Response rule.** From the prefix state `(σ, β)`, §4.2's table.

### 4.4 Worked examples

Masks are written with bit 0 rightmost. A diagram is written as nested
nodes, `node v: interval → target, …`; `D_j = 0` is the single leaf `0`.
Unless stated, `D_1` to `D_3` are `0`.

**A1** — every outcome. Initial balances `β = (100, 50, 0, 20)`, statuses
`σ = (0, 0, 1, 0)` (account 2 frozen).

| # | Call | Outcome | State after (`β`; `σ`) |
|---|---|---|---|
| 1 | `deposit(0, 30)` | commit 130 | `(130, 50, 0, 20)`; `(0,0,1,0)` |
| 2 | `transfer(0, 2, 40)` | abort `account_frozen` | unchanged |
| 3 | `unfreeze(2)` | commit 0 | `(130, 50, 0, 20)`; `(0,0,0,0)` |
| 4 | `transfer(0, 2, 40)` | commit 90 | `(90, 50, 40, 20)`; `(0,0,0,0)` |
| 5 | `withdraw(1, 70)` | abort `insufficient_funds` (`50 < 70`) | unchanged |
| 6 | `withdraw(1, 50)` | commit 0 (`50 >= 50`) | `(90, 0, 40, 20)`; `(0,0,0,0)` |
| 7 | `transfer(2, 3, 41)` | abort `insufficient_funds` (`40 < 41`) | unchanged |
| 8 | `freeze(1)` | commit 0 | `(90, 0, 40, 20)`; `(0,1,0,0)` |
| 9 | `deposit(1, 10)` | abort `account_frozen` | unchanged |
| 10 | `withdraw(1, 20000)` | abort `invalid_amount` | unchanged |
| 11 | `freeze(1)` | abort `already_frozen` | unchanged |
| 12 | `unfreeze(3)` | abort `not_frozen` | unchanged |
| 13 | `transfer(3, 3, 5)` | abort `same_account` | unchanged |
| 14 | `deposit(0, 0)` | abort `invalid_amount` | unchanged |
| 15 | `withdraw(4, 5)` | abort `account_not_found` | unchanged |

Final `β = (90, 0, 40, 20)`, `σ = (0, 1, 0, 0)`. Row 10 shows the argument
test ahead of the status test: account 1 is frozen, yet the outcome is
`invalid_amount`.

**A2** — a withdrawal fails before a deposit and succeeds after it.
Initial `β = (30, 0, 0, 0)`, `σ = (0, 0, 0, 0)`.

| Batch | Outcomes | Final `β0` |
|---|---|---|
| A2a `[withdraw(0, 50), deposit(0, 40), withdraw(0, 50)]` | abort `insufficient_funds` (`30 < 50`), commit 70, commit 20 | 20 |
| A2b `[deposit(0, 40), withdraw(0, 50), withdraw(0, 50)]` | commit 70, commit 20, abort `insufficient_funds` (`20 < 50`) | 20 |
| A2c `[withdraw(0, 50), withdraw(0, 50), deposit(0, 40)]` | abort `insufficient_funds`, abort `insufficient_funds`, commit 70 | 70 |

The same three calls give three response sequences and two final states.

Summaries of A2a, all with masks `keep = 0b1111`, `set = 0`:

- `S1 = S3` (`withdraw(0, 50)`): `D_0 = node σ0: {0} → node β0: ([0, 50) → 0,
  [50, ∞) → -50), {1} → 0`; 120 bytes.
- `S2` (`deposit(0, 40)`): `D_0 = node σ0: {0} → 40, {1} → 0`; 81 bytes.
- `S1 ⊕ S2`: under `σ0 = 1` both translations are `0`. Under `σ0 = 0`, the
  deposit adds `40` on every piece: `[0, 50) → 40`, `[50, ∞) → -10`.
- `(S1 ⊕ S2) ⊕ S3`, under `σ0 = 0`: on `[0, 50)` the state before `S3` is
  `β0 + 40`, and `S3` succeeds iff `β0 + 40 >= 50`, iff `β0 >= 10`: `[0, 10)
  → 40`, `[10, 50) → -10`. On `[50, ∞)` the state is `β0 - 10`, and `S3`
  succeeds iff `β0 >= 60`: `[50, 60) → -10`, `[60, ∞) → -60`. R1 merges the
  adjacent `-10` intervals.
- Result: `D_0 = node σ0: {0} → node β0: ([0, 10) → 40, [10, 60) → -10,
  [60, ∞) → -60), {1} → 0`. Size `2 + (4 + 39 + 56) + 3 · 12 = 137` bytes.
- Other grouping: `S2 ⊕ S3` under `σ0 = 0` is `[0, 10) → 40`, `[10, ∞) →
  -10`. Then `S1 ⊕ (S2 ⊕ S3)`: on `[0, 50)`, `S1` adds `0` and `S2 ⊕ S3`
  reads `β0`, giving `[0, 10) → 40`, `[10, 50) → -10`; on `[50, ∞)`, `S1`
  adds `-50` and `S2 ⊕ S3` reads `β0 - 50`, giving `[50, 60) → -10`, `[60,
  ∞) → -60`. After merging, the same value.
- Responses: the state before call 2 is `apply(S1, ·)`, with `β0 = 30` in
  `[0, 50)`: `30`, so the deposit commits `70`. Before call 3 it is
  `apply(S1 ⊕ S2, ·)`: `30 + 40 = 70`, so the withdrawal commits `20`.
  Final: `30` lies in `[10, 60)`, so `β0 = 30 - 10 = 20`.

**A3** — a transfer couples two accounts. `σ = (0, 0, 0, 0)` throughout.

| Batch | Initial `β` | Outcomes | Final `β` |
|---|---|---|---|
| A3a `[transfer(0, 1, 20), withdraw(1, 20)]` | `(30, 0, 0, 0)` | commit 10, commit 0 | `(10, 0, 0, 0)` |
| A3b the same calls | `(15, 0, 0, 0)` | abort `insufficient_funds`, abort `insufficient_funds` (`0 < 20`) | `(15, 0, 0, 0)` |

The withdrawal from account 1 succeeds or fails according to account 0's
balance. The combined summary, masks unchanged:

- `D_0` (the transfer alone): `node σ0: {0} → node σ1: ({0} → node β0:
  ([0, 20) → 0, [20, ∞) → -20), {1} → 0), {1} → 0`.
- `D_1`, by cases. With `σ1 = 1` both calls abort: `0`. With `σ1 = 0` and
  `σ0 = 1`, only the withdrawal runs: `W = node β1: [0, 20) → 0, [20, ∞) →
  -20`. With `σ0 = σ1 = 0` and `β0 < 20`, likewise `W`; with `β0 >= 20`,
  account 1 holds `β1 + 20 >= 20`, so the withdrawal always succeeds: `+20
  - 20 = 0`.
- Canonically named: node 0 `σ0: {0} → 1, {1} → 4`; node 1 `σ1: {0} → 2,
  {1} → 0`; node 2 `β0: [0, 20) → 3, [20, ∞) → 0`; node 3 `β1: [0, 20) → 0,
  [20, ∞) → -20`; node 4 `σ1: {0} → 3, {1} → 0`. Node 3 is shared. Size `2 +
  (4 + 3 · 39) + (4 + 5 · 39) + 2 · 12 = 346` bytes.
- Apply at A3a's `β = (30, 0, 0, 0)`: `D_0` reaches `β0 = 30 >= 20`, `-20`;
  `D_1` takes nodes 0, 1, 2 and `β0 = 30` gives `0`. Final `(10, 0, 0, 0)`.
  At A3b's `β0 = 15`: `D_0` gives `0`; `D_1` reaches node 3 with `β1 = 0`,
  `0`. Final `(15, 0, 0, 0)`.

**A4** — growth with batch length. Initial `β = (5, 0, 0, 0)`, `σ = (0, 0,
0, 0)`, batch `[withdraw(0, 2), withdraw(0, 2), withdraw(0, 2)]`: commit 3,
commit 1, abort `insufficient_funds` (`1 < 2`); final `β0 = 1`.

After `k` calls `withdraw(0, 2)`, under `σ0 = 0`, `t_0 = -2 · min(k,
⌊β0 / 2⌋)`: if `k < ⌊β0 / 2⌋` the balance `β0 - 2k` is at least `2` and
the next call succeeds; otherwise the balance is `β0 mod 2 < 2` and it
fails. So the summary of `n` such calls has `D_0 = node σ0: {0} → node β0:
([0, 2) → 0, [2, 4) → -2, …, [2n - 2, 2n) → -2(n - 1), [2n, ∞) → -2n), {1}
→ 0`, with `n + 1` edges whose adjacent leaves differ, so R1 merges none.
Its size is `103 + 17 n` bytes: 154 for `n = 3`, 4455 for `n = 256`. For
this batch: `5` lies in `[4, 6)`, so `β0 = 5 - 4 = 1`.

**A5** — status order. Initial `β = (100, 50, 0, 20)`, `σ = (0,0,0,0)`.

| Batch | Outcomes | Final `β0`, `σ0` |
|---|---|---|
| A5a `[freeze(0), deposit(0, 10)]` | commit 100, abort `account_frozen` | 100, 1 |
| A5b `[deposit(0, 10), freeze(0)]` | commit 110, commit 110 | 110, 1 |

Both summaries have `keep = 0b1110`, `set = 0b0001`. For A5a the deposit
is read under `σ0 = 1`, so `D_0 = 0`; for A5b, `D_0 = node σ0: {0} → 10,
{1} → 0`. At `σ0 = 0` they give `β0 = 100` and `β0 = 110`.

### 4.5 Input distribution

- **Initial state:** for each account `j = 0..=3` in order, `β_j =
  uniform(0, 50 000)`, then `σ_j = 1` if `uniform(0, 3) = 0`, else `0`.
- **Call mix:** `k = uniform(0, 99)`. `[0, 30)` is `deposit`, `[30, 60)` is
  `withdraw`, `[60, 90)` is `transfer`, `[90, 95)` is `freeze`, and `[95,
  100)` is `unfreeze`.
- **Arguments:** `deposit` and `withdraw`: `account = uniform(0, 3)`,
  `amount = uniform(1, 10 000)`. `transfer`: `from = uniform(0, 3)`; `v =
  uniform(0, 2)`; `to = v` if `v < from`, else `v + 1` (so `to ≠ from`);
  `amount = uniform(1, 10 000)`. `freeze` and `unfreeze`: `account =
  uniform(0, 3)`.
- **Malformed share:** `m = uniform(0, 99)`. If `m = 0`, `deposit`,
  `withdraw` and `transfer` get `amount = 0`, and `freeze` and `unfreeze`
  get `account = 4`, an absent key.
- **Draw order per call:** `k`, the arguments in the order listed, `m`.

The generator therefore never produces `same_account`, and never an absent
account in a money operation; worked example A1 covers both. A quarter of
the accounts start frozen, and freezes and unfreezes recur, so that status
gating is exercised at every batch size.

**Why the initial balances are at most `50 000`.** The funds guard needs
the balances near its threshold, because a funds test that never fails is
not exercised. Each call touches a
given account's balance with probability `0.3` (a deposit to it, a
withdrawal from it, a transfer from it, or a transfer to it, each `0.075`),
so a batch of 256 draws about 77 candidate moves on it, of
root-mean-square size about `5 800` (`amount = uniform(1, 10 000)`), and
roughly balanced in sign. Fewer of them write: malformed amounts, frozen
accounts and insufficient funds suppress some. As an illustration only, the
balance wanders by at most about `√77 · 5 800 ≈ 50 000` over a batch of
256, and by about half that over 64. Starting balances up to `1 000 000`
would leave most accounts far from `0`, so `insufficient_funds` would
rarely occur at the gating sizes; `uniform(0, 50 000)`, five times
`X_MAX`, encourages the funds test to fall on both sides of its threshold.
It does not guarantee both outcomes at every gating size or in every
batch. This is an estimate from the stated distribution, not a
measurement, and the distribution is frozen as stated.
The summary's size does not depend on the initial state, because a summary
denotes its calls' transformer on every state.

## 5. Generator

All randomness comes from **SplitMix64**, with `u64` wrapping arithmetic:

```
state = state + 0x9E3779B97F4A7C15
z = state
z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
z = (z ^ (z >> 27)) * 0x94D049BB133111EB
next = z ^ (z >> 31)
```

Test vectors: from `state = 0`, the first three outputs are
`16294208416658607535`, `7960286522194355700`, `487617019471545679`. From
`state = (1 << 48) | (64 << 32) | 0`, they are `14475248428079389989`,
`3002686106093258379`, `170901929721367164`.

`uniform(lo, hi) = lo + (next mod (hi - lo + 1))`, computed in `u64`. Its
modulo bias is below `2^-40` for every range used here, and it is part of
the frozen definition, not an error to correct.

**Seed derivation.** Batch `j` (a `u32`) of size `n` (1 to 256) for family
`f` starts from `state = (f << 48) | (n << 32) | j`, where `f = 1` for
quota, `2` for inventory, and `3` for accounts. The initial state is drawn
first, then the calls in order, each in the draw order of its family's
input section. Which sizes are run, and how many batches `j` per size, are
part of the measurement protocol (`protocol.md`, part B), not of this
document.

## 6. Summary of the frozen bounds

| Family | Summary | Bytes | Combine | Apply |
|---|---|---|---|---|
| Quota | `(a, l, u)`, a clipped translation | 24 | 3 add, 2 clamp | 1 add, 1 clamp |
| Inventory | `d`, a translation mod 91 | 1 | 1 add, 1 conditional subtract | 2 subtract, 1 conditional add |
| Accounts | `(keep, set, D_0..D_3)`, status masks and four reduced interval diagrams | not fixed: `bytes(S)` of §4.3, at least `103 + 17 n` for A4's batch of `n` | grows with the diagrams' sizes | at most 8 nodes and one binary search per node, per diagram |

Each family is measured against its row, in bytes and operation counts; for
accounts the row names the measured quantities, not a bound. A measured
size or cost that grows with batch size fails that family (§1,
"Compactness rule").

## 7. Open questions for the reviewer

1. **Variable order of the account diagrams (§4.3).** A reduced diagram's
   size depends on its variable order, and the frozen order is `σ0, σ1, σ2,
   σ3, β0, β1, β2, β3`: statuses first, because they gate every movement,
   then balances by key. Another order may give smaller diagrams on the
   generated batches. Recommendation: keep this order, fixed before any
   measurement; choosing an order after seeing sizes would be tuning
   against a result and would void it.
2. **How growth is decided (§4.3).** This document fixes what is measured
   for accounts; the rule that turns sizes at batch sizes 1-256 into
   "grows with batch size" is not fixed here. Recommendation: part B
   (`protocol.md`) freezes it, for all three families alike, before any
   measurement.
