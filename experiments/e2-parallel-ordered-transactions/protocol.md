# E2 — frozen part B: compactness measurement only

Status: proposed, not yet frozen. It becomes frozen under the freeze
protocol of `docs/experiments.md` ("Freeze protocol"), and from then on
nothing below changes after a summary has been measured. Part A is
`inputs.md` (families, summaries, generator); this file is part B and owns
one decision: when a family's summary "grows with batch size".

## 1. Scope and claim

This part freezes how "grows with batch size" (`docs/experiments.md`,
"Admissible bound class"; `inputs.md` §1, "Compactness rule") is decided,
for quota, inventory and accounts alike, from bytes and executed operation
counts. It answers `inputs.md` §7, question 2. The throughput protocol
(hardware, threads, timed region, repeats, noise limits, verdict arithmetic)
is added as further sections and nothing below depends on it. This part uses
no clock and no hardware.

The verdict this part can give is **"no growth detected over the window
64-256 at threshold `τ` on the generated batches"**, or "growth detected".
It is an empirical window result. It is not a proof that a bound holds for
every batch length, and a pass is never reported as one: growth slower than
`τ` over the window, or beginning beyond 256, is outside what it can see
(§5 states the exact blind spots).

## 2. Batches

| Setting | Value | Reason |
|---|---|---|
| Batch sizes `n` | 1, 2, 4, 8, 16, 32, 64, 128, 256 | Powers of two cover 1-256 at even spacing in `log n` and include the three gating sizes of `docs/experiments.md` ("Gating sizes and aggregation"). |
| Seeds | `j = 0 … 99`, 100 per family | §5 derives it: a growth that lifts the ratio past `τ` in 30% of seeds is missed with probability below 1%. |
| Batch of seed `j` | `generate(256, j)`: state `(f << 48) \| (256 << 32) \| j` | Part A's frozen derivation (`inputs.md` §5, `kiln_e2::batch_generator`) at the largest size; one batch per seed. |
| Batch of size `n` for seed `j` | the first `n` calls of that batch, with its initial state | The **paired design**: every size is measured on the same calls, so no between-sample difference enters a comparison across sizes. |

Family numbers are part A's (`f = 1` quota, `2` inventory, `3` accounts).

**Why a prefix is a batch of that size.** The generator draws the initial
state first, then the calls in order (`inputs.md` §5). Each call's draws
follow its family's draw order (§2.6, §3.5, §4.5) and its value depends only
on the stream position, never on the state or an earlier call. The size `n`
enters only the seed. So, given the initial state, the first `n` calls of a
256-call batch are `n` independent draws of the same call distribution, which
is the distribution of a batch of `n` calls under the frozen inputs; the
only departure is the generator's modulo bias below `2^-40` (`inputs.md`
§5). `crates/kiln-e2/src/{quota,inventory,accounts}/generator.rs` implement
exactly that: `generate` draws the initial state, then maps `size` calls
through a `draw_call` that takes only the generator. A prefix is therefore
not bit-equal to `generate(n, j)`; this part does not use `generate(n, j)`
for `n < 256`. If a future change to part A makes a call depend on its
position or on `n`, the pairing is void and this part is re-frozen.

## 3. Quantities per prefix

For a prefix `c_1 … c_n` the measurement builds the atomic summaries
`S_1 … S_n` and folds them in order. Three quantities are recorded per
family, seed and size. None is a duration.

**Size, `B`.** The bytes of the batch summary `S = S_1 ⊕ … ⊕ S_n` on the
frozen representation, not on an in-memory layout: quota 24 (three `i64`,
`inputs.md` §2.4); inventory 1 (one `u8`, §3.3); accounts `bytes(S)` of §4.3,
the length of the serialised canonical value. The value is the same under
every grouping of the fold, since combine is associative on values.

**Combine cost, `C`.** The fold is the balanced pairwise fold: adjacent
summaries are combined in pairs, an odd one out carried to the next level,
until one is left. This includes combines of two large summaries, the case a
parallel scan produces. `C` is the largest operation count of any single
combine in this fold; for `n = 1` there is none and `C` is undefined.

**Apply cost, `A`.** The operation count of applying `S` to the batch's
initial state.

**What is counted.** A cost is the number of operations the summary
implementation **executes**, measured by instrumenting it, never computed
from sizes (`docs/experiments.md` asks for "actual operation counts";
`inputs.md` §4.3 asks for visited nodes and edges). A representation-size
proxy would miss work absent from the result, such as product construction,
intermediate diagrams and reduction. These boundaries are frozen before any
implementation exists, so the counted unit cannot be chosen after a result:

- **Quota** (`inputs.md` §2.4, §6): every addition and every clamp executed.
  A combine starts when `combine(S, S')` is entered and ends when it returns;
  an apply starts when `apply(S, x)` is entered and ends when it returns.
  Executed counts equal the frozen 5 and 2.
- **Inventory** (`inputs.md` §3.3, §6): every addition, every subtraction and
  every conditional subtract or add executed, each counted once whether or
  not its condition fires. Frozen 2 and 3.
- **Accounts** (`inputs.md` §4.3): the counter increases by 1 for each of
  - a diagram node constructed, including intermediate nodes later discarded
    or merged by reduction;
  - a node visited, in an operand, an intermediate diagram or the result,
    each entry counted;
  - a comparison made by an edge search, each executed comparison counted
    (not an estimate such as `⌈log2(edge count)⌉`);
  - a lookup in a unique table or a memo table, hit or miss.

  A combine starts when `combine` is entered with the two summaries and ends
  when it returns the reduced, canonically named summary, so product
  construction, reduction and naming are inside. An apply starts when
  `apply` is entered with the summary and the state and ends when it
  returns the new state. Building the atomic summaries `S_i`, serialising for
  `B`, and the generator are outside every count. Whether tables persist
  across combines is the implementer's choice; the report states it, and the
  executed counts decide whatever it is.

Size-derived figures (nodes plus edges of the operands and the result) may be
reported as context beside the counts and never decide. Chunk summaries of a
parallel scan depend on the chunking, which the throughput part will fix;
they are not measured here.

## 4. Per seed: does a quantity rise?

For family, quantity `Q ∈ {B, C, A}` and seed `j`, let `Q_j(n)` be the value
on the prefix of size `n`. Seed `j` **rises** if and only if

> `Q_j(256) > Q_j(64)` and `5 · Q_j(256) ≥ 6 · Q_j(64)`.

`τ = 6/5 = 1.20`. The comparison is exact on integers. A decrease, or no
change, never counts as a rise; `Q_j(64) = 0` with `Q_j(256) > 0` rises, and
`0` against `0` does not. The point 128 is reported with 64 and 256 and does
not decide: the test is directional over the window, and a middle point that
rises and falls back is not growth at 256.

Why `τ = 1.20`. The test must catch the slowest growth class worth calling
growth, and a family that grows logarithmically is that class. A summary
that grows like `c · log2 n` goes from `6c` to `8c` over 64-256, a ratio of
`1.33`. `1.20` leaves a margin of `1.33 / 1.20 ≈ 1.11` below it, so
logarithmic growth is caught as long as its additive offset `a` in
`c · log2 n + a` stays at most `4c` (`8c + a ≥ 1.2 · (6c + a)`). As a power
law `n^α`, a fourfold range gives the ratio `4^α`, and `τ = 1.20` detects
every exponent `α ≥ ln 1.2 / ln 4 ≈ 0.13`. Linear growth (`α = 1`, ratio 4)
and `√n` (ratio 2) are caught by a wide margin.

## 5. Per family: does it grow?

A quantity **grows** if and only if at least 20 of the 100 seeds rise. The
family **grows with batch size** if any of `B`, `C`, `A` grows, and passes
the compactness bound if none does. The rule is the same for all three
families and names none.

Why 20 of 100. Growth in a family need not touch every call sequence: long
chains that lengthen a diagram occur in some batches only. Pairing leaves no
between-sample difference, but the seeds are still a sample of call
sequences, and the count is its error criterion. If the true share of rising
seeds is `p`, the number of rising seeds is `Binomial(100, p)` and the
quantity is missed when it falls below 20:

| True share `p` of rising seeds | Probability of a miss |
|---|---|
| 0.5 | below 10^-9 |
| 0.4 | 6 · 10^-6 |
| 0.3 | 0.009 |
| 0.2 | 0.46 |

The design target is `p ≥ 0.3` (miss below 1%); 100 seeds with a threshold of 20 meets it, and the cost is a few thousand
small folds. A growth
that lifts the ratio past `τ` in under about a fifth of the seeds is, by the
table, likely to go undetected. A false fail needs 20 seeds to rise past
`τ` on the same calls by chance; with a paired comparison the only
variation is the extension from 64 to 256 calls of the same sequence. The
count of rising seeds is reported with the verdict for every quantity.

**What a pass does not show.** It does not detect:

- a rise below `τ` over the window (an exponent below about 0.13, or
  logarithmic growth with offset above `4c`);
- growth that appears only in fewer than about a fifth of the seeds;
- growth beyond 256, or on inputs outside the frozen distribution.

It is therefore not evidence of a batch-independent bound, and
`docs/experiments.md`'s "fixed, stated bound" is read for E2 as "no growth
detected over the window at `τ`", reported with these limits.

**Quota and inventory.** Their `B`, `C` and `A` are the constants of §3
(24, 5, 2 and 1, 2, 3), so no seed rises. A measured value that differs from
its constant is not a growth finding: the implementation does not realise
the frozen representation, the document is defective, and it is re-frozen,
voiding prior results (`inputs.md` §1).

**Accounts.** The family has no fixed size (`inputs.md` §4.3, §6), so the
rule decides it only by what the measurement shows. Nothing in this document
predicts the outcome.

**A quantity that rises and then saturates.** Saturation is an observed
pattern, not a consequence of the frozen bounds: the threshold bound of
`inputs.md` §4.3 itself depends on the maximum batch length, so bounded
amounts and balances do not imply a plateau by 64. The rule judges by
position:

- A quantity that has stopped rising by size 64 passes: fewer than 20 seeds
  satisfy the predicate of §4. The plateau holds for E2's frozen domain (four
  accounts, balances up to 50 000, amounts up to 10 000), and the result is
  reported with its plateau value and that scope. Independence from batch
  length is a different property from independence from domain size, and
  this measurement says nothing about larger domains.
- A quantity for which 20 or more seeds satisfy §4's predicate fails, whether
  or not the curve flattens after 256: the measurement cannot tell a later
  plateau from continued growth.
- Sizes below 64 never decide. A rise from 1 to 64 is the expected shape of a
  saturating summary and is reported as context only.

**Alternatives rejected.**

- *The maximum over independent batches per size, compared across the
  window:* detects variation, not growth. A falling maximum would fail and a
  rise of 9% would pass, and different sizes would use different samples.
- *Fitted log-log slope over 1-256:* a saturating curve has a positive slope
  over the range because of its early rise, so the slope judges the rise and
  not the plateau, and it adds a fitting procedure, floating point and a
  slope threshold with no better reason than `τ`.
- *Any strict increase from some size onwards:* "from some size" is a free
  parameter chosen after seeing sizes.
- *Comparison with size 1 or the smallest sizes:* every summary of more than
  one call is larger than an atomic one, so any saturating quantity would
  fail, and the reference would decide.
- *Judging 1-256 instead of the gating window:* contradicts the aggregation
  `docs/experiments.md` fixes, where sizes below 64 are context.
- *Wall time for cost:* a timing needs hardware and noise limits that the
  throughput part owns, and the growth rule must not depend on them.
- *A size-derived cost proxy (nodes plus edges of operands and result):* a
  lower bound on work, blind to product construction and reduction, and it
  ignores shortcuts and sharing; it is reported as context only (§3).

## 6. What a fail means, and how E3 cites the rule

A family whose quantity grows is reported as failing the admissible bound
class, with its three quantities at every size and its rising-seed counts. It
is never replaced by a fallback: no capped or approximate summary, no call
list, no serial path stands in for it (`inputs.md` §1, §4.3). For accounts
this makes E2 fail as a whole ("one family failing fails E2 overall") and
the result is recorded against `docs/concept.md` §13's second kill criterion
(`inputs.md` §4.3). Throughput and correctness are measured and reported for
a failing family either way.

E3 does not reuse E2's batches. It measures its own, drawn from each family's
full domain (`experiments/e3-summary-synthesis/inputs.md` §1, §10 item 1), on
the representation of each candidate, and applies §4 and §5 unchanged to the
quantities it measures: the same prefix pairing where its generator has the
property of §2 (calls independent of position and of `n`), the sizes 64 and
256 among its sizes, `τ` and the 20-of-100 criterion, with the counting
boundaries of §3 restated for its representations. Its other choices are
E3's to freeze. Where E3's generator lacks the pairing property it states
its own error criterion.

This rule does **not** replace E3 §1's separate prohibition on enumerating
quantity and identifier components of the checking domain. A full-domain
table could pass the growth test and is still forbidden by that gate, which
S2 enforces in the grammar. An admissible measured pass is also not
contradicted by E3's expected misses, which are expectations, not outcomes.

## 7. Freeze order

Sizes, seeds, the paired design, the counted operations and their boundaries,
`τ`, the 20-of-100 criterion and the window are fixed here before any
summary implementation for accounts has landed in the repository, so none of
them was chosen against a result. A change to any value above re-freezes
this part and voids the results measured under the earlier one.
