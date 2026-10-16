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

- the **summary** `S_i` of one call: a fixed-size value denoting `F_i`;
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

**Compactness rule.** Each family's summary has a fixed size in bytes and
fixed combine and apply costs, stated in its section and independent of
batch size. A summary whose measured size, or whose measured combine or
apply cost, grows with batch size fails its family. It is reported as
failing and never replaced by a fallback (head ruling on #3;
`docs/experiments.md`, "Admissible bound class"). The representations
below are the frozen ones: a call list, a closure tree, or any table whose
length depends on the number of calls summarized is not one of them. If an
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
withdrawals, transfers, and compliance freezes. Every operation's response
reveals a balance, and a freeze changes what later operations may do.

### 4.1 Constants and IR shape

| Constant | Value |
|---|---|
| `K`, the number of accounts | 4 (keys `0..=3`) |
| `X_MAX`, the largest valid amount | 10 000 |
| status values | `0` = open, `1` = frozen |

Schema: table `accounts`, columns `balance` `Integer { width: I64, domain:
None }` and `status` `Integer { width: I8, domain: None }`; check
constraints `status AtLeast 0` and `status AtMost 1`. The initial store
holds exactly the rows `0..=3`. Balances may be negative: this is a ledger
account with no overdraft check (see §4.6 and open question 1).

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
  -- as deposit, with `-` in place of `+` in the update and the return

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

**Guard shape.** Calls naming disjoint accounts do not overlap. Within E2,
a batch is the class-3 batch over all `K` rows, acquired as one footprint
at dispatch (`docs/concept.md` §4); the key-equality guard of §3, class 1,
is not what E2 measures. The in-body guards are the argument tests, account
presence, and the status tests, in the order listed.

**Class.** Every committed call returns a balance read from the contended
row, and every money movement is gated by status reads that a freeze or
unfreeze in the same batch changes. Neither a claim, a pin, nor an escrow
draw describes either dependency.

### 4.2 Outcomes

With `β` the balances and `σ` the statuses before the call:

| Call | Condition, tested in this order | Outcome | Effect |
|---|---|---|---|
| `deposit(a, x)` | `x < 1` or `x > 10000` | abort `invalid_amount` | none |
| | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` | abort `account_frozen` | none |
| | otherwise | commit `β_a + x` | `β_a += x` |
| `withdraw(a, x)` | as `deposit` | as `deposit`, committing `β_a - x` | `β_a -= x` |
| `transfer(a, b, x)` | `x < 1` or `x > 10000` | abort `invalid_amount` | none |
| | `a = b` | abort `same_account` | none |
| | `a` not in `0..=3`, then `b` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` or `σ_b = 1` | abort `account_frozen` | none |
| | otherwise | commit `β_a - x` | `β_a -= x`, `β_b += x` |
| `freeze(a)` | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 1` | abort `already_frozen` | none |
| | otherwise | commit `β_a` | `σ_a = 1` |
| `unfreeze(a)` | `a` not in `0..=3` | abort `account_not_found` | none |
| | `σ_a = 0` | abort `not_frozen` | none |
| | otherwise | commit `β_a` | `σ_a = 0` |

**Bounds and overflow.** Each call changes any one balance by at most
`10 000`. From initial balances in `[0, 1 000 000]` (§4.5), every balance
after at most 256 calls lies in `[-2 560 000, 3 560 000]`, and so does every
intermediate and response value. No `i64` overflow is reachable within the
frozen inputs. The summary below does not model `i64` overflow; its
exactness claim is limited to batches whose balances stay within `i64`,
which the frozen generator guarantees. A status write is a literal `0` or
`1` and stays inside `I8` and the check constraints.

### 4.3 Summary, combine and apply

The state is `(σ, β)` with `σ` in `{0,1}^4` and `β` in `Z^4`. Every call's
transformer has the shape `σ' = g(σ)`, `β' = β + δ(σ)`, where each bit of
`g` either keeps `σ_i` or sets it to a constant, and `δ` depends on `σ`
alone, never on `β`:

- `deposit(a, x)`: gate `(1 - σ_a)`, so `δ_a = x - x σ_a`;
- `withdraw(a, x)`: `δ_a = -x + x σ_a`;
- `transfer(a, b, x)`: gate `(1 - σ_a)(1 - σ_b) = 1 - σ_a - σ_b + σ_a σ_b`,
  so `δ_a = -x·gate` and `δ_b = +x·gate`;
- `freeze(a)`: `g` sets bit `a` to `1`. The `already_frozen` abort leaves it
  `1`, which agrees with the formula.
- `unfreeze(a)`: `g` sets bit `a` to `0`. The `not_frozen` abort leaves it
  `0`, which agrees with the formula.
- `account_frozen` aborts are the gate's zero.
- `invalid_amount`, `same_account` and `account_not_found` are decided by
  the arguments, since the key set is fixed (§1), and have summary `ε`.

**Representation.** `S = (keep, set, c)`:

- `keep`: a `u8` bit mask; bit `i` set means `σ'_i = σ_i`.
- `set`: a `u8` bit mask giving `σ'_i` for every bit not kept, normalised so
  that `set & keep = 0`.
- `c`: 44 `i64` coefficients. For each account `j` in `0..=3`, it holds 11
  coefficients of a multilinear polynomial in `σ`, over the monomials `1,
  σ0, σ1, σ2, σ3, σ0σ1, σ0σ2, σ0σ3, σ1σ2, σ1σ3, σ2σ3`, stored in that order
  and account-major.

Size: 2 + 44 × 8 = **354 bytes**. `ε = (0b1111, 0, all zero)`.

**Apply.** `σ'_i = σ_i` if bit `i` of `keep` is set, else bit `i` of `set`;
`β'_j = β_j + Σ_M c_{j,M} · Π_{i ∈ M} σ_i`, evaluated at the input `σ`.

**Combine.** `S1 ⊕ S2`, with `S1` first:

- `keep = keep1 & keep2`;
- `set = ((set1 & keep2) | set2) & ~keep`;
- `c = c1 + subst(c2, S1)`. `subst` rewrites each term `c_{j,M} Π_{i∈M} σ_i`
  of `S2`, a polynomial in the state *after* `S1`, into the state before
  it: each `σ_i` with `i` not kept by `S1` is replaced by the constant
  `set1_i`. A term with any such `set1_i = 0` vanishes. Otherwise its
  coefficient is added to the monomial `M` minus those replaced bits.

*Closure.* Substituting constants never raises a monomial's degree, and every
atomic `δ` has degree at most 2, so every combined `δ` again has degree at most 2 and fits the
11 monomials. `g2 ∘ g1` keeps a bit only if both keep it, and otherwise
takes the last constant written, which is the mask rule above.

*Correctness.* After `S1`, the state is `(g1(σ), β + δ1(σ))`. Applying `S2`
gives `(g2(g1(σ)), β + δ1(σ) + δ2(g1(σ)))`, and `subst(c2, S1)` is exactly
`δ2 ∘ g1` written over the original `σ`.

*Associativity.* Both groupings denote the same transformer, since function
composition and vector addition are associative:
`δ1(σ) + δ2(g1 σ) + δ3(g2 g1 σ)` either way. The representation is
canonical: the normalised masks are determined by `g`; and a function from
`{0,1}^4` to `Z` has exactly one multilinear polynomial, by Möbius
inversion. Equal transformers therefore have equal representations, so
both groupings give the same value.

**Bounds.** 354 bytes. Combine visits each of S2's 44 terms once, resolving
up to 2 substituted bits each, and performs 44 additions with `c1`; apply
evaluates 44 terms. Each coefficient of a summary of `n` calls is a sum of
at most 4 terms of magnitude at most `10 000` per call, so `|c| <= 40 000 n`, at most
`10 240 000` for `n <= 256`. None of this depends on batch size. How
combine is computed (for example, skipping zero coefficients) is the
implementer's choice; its result is fixed by the definition above.

**Response rule.** From the prefix state `(σ, β)`, §4.2's table.

### 4.4 Worked examples

**A1** — every outcome. Initial balances `β = (100, 50, 0, 20)`, statuses
`σ = (0, 0, 1, 0)` (account 2 frozen).

| # | Call | Outcome | State after (`β`; `σ`) |
|---|---|---|---|
| 1 | `deposit(0, 30)` | commit 130 | `(130, 50, 0, 20)`; `(0,0,1,0)` |
| 2 | `transfer(0, 2, 40)` | abort `account_frozen` | unchanged |
| 3 | `unfreeze(2)` | commit 0 | `(130, 50, 0, 20)`; `(0,0,0,0)` |
| 4 | `transfer(0, 2, 40)` | commit 90 | `(90, 50, 40, 20)`; `(0,0,0,0)` |
| 5 | `withdraw(1, 70)` | commit -20 | `(90, -20, 40, 20)`; `(0,0,0,0)` |
| 6 | `freeze(1)` | commit -20 | `(90, -20, 40, 20)`; `(0,1,0,0)` |
| 7 | `deposit(1, 10)` | abort `account_frozen` | unchanged |
| 8 | `freeze(1)` | abort `already_frozen` | unchanged |
| 9 | `unfreeze(3)` | abort `not_frozen` | unchanged |
| 10 | `transfer(3, 3, 5)` | abort `same_account` | unchanged |
| 11 | `deposit(0, 0)` | abort `invalid_amount` | unchanged |
| 12 | `withdraw(4, 5)` | abort `account_not_found` | unchanged |

Final `β = (90, -20, 40, 20)`, `σ = (0, 1, 0, 0)`.

Summary derivation, writing only non-zero coefficients and the bits each
summary overwrites:

- `S1`: `δ0 = 30 - 30σ0`.
- `S2` and `S4`: `δ0 = -40 + 40σ0 + 40σ2 - 40σ0σ2`, `δ2 = 40 - 40σ0 -
  40σ2 + 40σ0σ2`.
- `S3`: bit 2 set to `0`.
- `S1 ⊕ S2`: `δ0 = -10 + 10σ0 + 40σ2 - 40σ0σ2`; `δ2 = 40 - 40σ0 - 40σ2 +
  40σ0σ2`.
- `⊕ S3`: the same `δ`, with bit 2 set to `0`.
- `⊕ S4`: substituting `σ2 = 0` into `S4` gives `δ0 = -40 + 40σ0` and `δ2 = 40 - 40σ0`.
  The sum is `P = S1 ⊕ … ⊕ S4`: `δ0 = -50 + 50σ0 + 40σ2 - 40σ0σ2`, `δ2 = 80
  - 80σ0 - 40σ2 + 40σ0σ2`, bit 2 set to `0`.
- At `σ = (0,0,1,0)`: `δ0 = -50 + 40 = -10` and `δ2 = 80 - 40 = 40`, giving
  `β = (90, 50, 40, 20)` and `σ = (0,0,0,0)`, which matches row 4. At `σ =
  (0,0,0,0)` it would give `δ0 = -50` and `δ2 = 80`, the result if account 2 had
  been open, where both transfers commit.
- `S5 ⊕ … ⊕ S12`: `S5` gives `δ1 = -70 + 70σ1`; `S6` sets bit 1 to `1`;
  `S7` (`δ1 = 10 - 10σ1`) vanishes under `σ1 = 1`; `S8` sets bit 1 to `1`;
  `S9` sets bit 3 to `0`; `S10`-`S12` are `ε`. Result: `δ1 = -70 + 70σ1`,
  bit 1 set to `1`, bit 3 set to `0`.
- `P ⊕ (S5 ⊕ … ⊕ S12)`: `P` overwrites only bit 2, which does not occur in `δ1`.
  Total: `δ0 = -50 + 50σ0 + 40σ2 - 40σ0σ2`, `δ1 = -70 + 70σ1`, `δ2 = 80 -
  80σ0 - 40σ2 + 40σ0σ2`, `δ3 = 0`; `keep = {0}`, `set1 = 1`, `set2 = 0`,
  `set3 = 0`.
- Applied to the initial state, it gives `β = (90, -20, 40, 20)` and `σ =
  (0, 1, 0, 0)`, the final state above.

**A2** — order dependence. Initial `β = (100, 50, 0, 20)`, `σ = (0,0,0,0)`.

| Batch | Outcomes | Final `β0`, `σ0` |
|---|---|---|
| A2a `[freeze(0), deposit(0, 10)]` | commit 100, abort `account_frozen` | 100, 1 |
| A2b `[deposit(0, 10), freeze(0)]` | commit 110, commit 110 | 110, 1 |

Summaries: for A2a, `(bit 0 set to 1) ⊕ (δ0 = 10 - 10σ0)`. Substituting
`σ0 = 1` makes the deposit term vanish, so the combined summary is "bit 0
set to `1`, `δ = 0`". For A2b, it is `δ0 = 10 - 10σ0` with bit 0 set to
`1`. At `σ0 = 0` they give `β0 = 100` and `β0 = 110` respectively.

### 4.5 Input distribution

- **Initial state:** for each account `j = 0..=3` in order, `β_j =
  uniform(0, 1 000 000)`, then `σ_j = 1` if `uniform(0, 3) = 0`, else `0`.
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

### 4.6 What the account family deliberately leaves out

An insufficient-funds rule ("withdraw or transfer only if `β_a >= x`, else
abort") is not part of this family. With a blind response, it is the
escrow of `docs/concept.md` §3, class 2 (`balance >= 0` "becomes an
escrow"), so it belongs to class 2, not to E2. With a balance-revealing
response, as here, it is the all-or-nothing reserve that the source
ideation calls "much harder": "arbitrary quantities create complicated
state-dependent success patterns." The exact transformer of a chunk then
has breakpoints throughout `[0, Σ amounts)`. Already for repeated
`withdraw(·, 2)`, the piecewise-offset form has one piece per call.
No constant-size exact representation of that variant is known to this
freeze. Whether E2 should carry it anyway is open question 1.

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
| Accounts | `(keep, set, c[44])`, status masks and a degree-2 multilinear `δ` | 354 | 44 terms substituted and added | 44 terms evaluated |

Each family is measured against its row, in bytes and operation counts. A
measured size or cost that grows with batch size fails that family (§1,
"Compactness rule").

## 7. Open questions for the reviewer

1. **Accounts and insufficient funds (§4.6).** The frozen default omits an
   insufficient-funds rule, for two reasons: its blind form is class 2, and
   its revealing form has no known constant-size summary. The alternative
   adds the rule to `withdraw` and `transfer`. The family would then be more
   realistic, and E2 would expect it to fail the size bound under its own
   rule, which would fail E2 as a whole. Recommendation: keep the default,
   and record the alternative's expected non-compactness against
   `docs/concept.md` §13's second kill criterion. Not measuring the
   alternative is a scope decision for the head, not a result.
2. **Reading of "conditional inventory" (§3).** The default is TPC-C's
   conditional restock rule. The alternative reading is an all-or-nothing
   "sell only if in stock", which is the same hard variant as question 1.
   Recommendation: keep the default, which is externally pinned and not
   chosen by this author.
3. **Overflow outside the frozen inputs (§4.2).** The account summary is
   exact only while balances stay within `i64`, which the frozen generator
   guarantees with balances below `3 560 000` in magnitude, far inside
   `i64`. Recommendation: accept this, and
   have slice 3's generator test assert the balance bound.
