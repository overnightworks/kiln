# E2 — frozen part B: compactness measurement only

Status: frozen. Record: written and independently reviewed in #81 on
2026-10-16 under the freeze protocol of `docs/experiments.md` ("Freeze
protocol"); nothing below changes after a summary has been measured. Part
A is `inputs.md` (families, summaries, generator); this file is part B and
owns one decision: when a family's summary "grows with batch size".

## 1. Scope and claim

This part freezes how "grows with batch size" (`docs/experiments.md`,
"Admissible bound class"; `inputs.md` §1, "Compactness rule") is decided,
for quota, inventory and accounts alike, from bytes and executed operation
counts. It answers `inputs.md` §7, question 2. The throughput protocol
(hardware, threads, timed region, repeats, noise limits, verdict arithmetic)
is added as further sections and nothing below depends on it. This part uses
no clock and no hardware.

The verdict this part can give is **"no growth detected over the window
64-256 under this protocol"**, or "growth detected". It is an empirical
window result. It decides the owner's fail condition, growth, and it does not
by itself establish the owner's "fixed, stated bound": quota and inventory
have their frozen constants, checked by measurement, and accounts has no
stated bound (`inputs.md` §6), so a pass there claims no batch-independent
bound. Growth slower than `τ` over the window, or beginning beyond 256, is
outside what it can see (§5 states the blind spots). For accounts, "no growth
detected" is necessary but not sufficient for E2's overall pass: the owner's
fixed, stated bound must still be stated before measurement and checked. The
part of this protocol that freezes throughput states it; until then an
accounts pass cannot make E2 pass.

## 2. Batches

| Setting | Value | Reason |
|---|---|---|
| Batch sizes `n` | 1, 2, 4, 8, 16, 32, 64, 128, 256 | Powers of two cover 1-256 at even spacing in `log n` and include the three gating sizes of `docs/experiments.md` ("Gating sizes and aggregation"). |
| Seeds | `j = 0 … 99`, 100 per family | §5: a rise share of 30% with no falls is missed with probability 0.0089. |
| Batch of seed `j` | `generate(256, j)`: state `(f << 48) \| (256 << 32) \| j` | Part A's frozen derivation (`inputs.md` §5, `kiln_e2::batch_generator`) at the largest size; one batch per seed. |
| Batch of size `n` for seed `j` | the first `n` calls of that batch, with its initial state | The **paired design**: every size is measured on the same calls, so no between-sample difference enters a comparison across sizes. |

Family numbers are part A's (`f = 1` quota, `2` inventory, `3` accounts).

**Why a prefix is a batch of that size.** The generator draws the initial
state first, then the calls in order (`inputs.md` §5). Each call's draws
follow its family's draw order (§2.6, §3.5, §4.5) and its value depends only
on the stream position, never on the state or an earlier call. The size `n`
enters only the seed. So, given the initial state, the first `n` calls of a
256-call batch have the distribution of a batch of `n` calls under the frozen
inputs **if the seeded draws are modelled as independent samples**. They are
not mathematically independent: the SplitMix64 streams are fixed and the
seeds consecutive. Every probability in this document is conditional on that
model. `crates/kiln-e2/src/{quota,inventory,accounts}/generator.rs` implement
the structure: `generate` draws the initial state, then maps `size` calls
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

**What is counted.** A cost is the number of size-dependent work units the
implementation **executes**, measured by instrumenting it, never computed
from sizes (`docs/experiments.md` asks for "actual operation counts";
`inputs.md` §4.3 asks for visited nodes and edges). A proxy would miss work
absent from the result, such as product construction and reduction. These
boundaries are frozen before any implementation exists, so the counted unit
cannot be chosen after a result. The counter increases by 1 for each of:

- a node constructed, including intermediates later discarded or merged;
- an edge constructed;
- a node or an edge read or scanned, each entry counted, in an operand, an
  intermediate diagram or the result;
- a comparison executed, in an edge search or in reduction alike, each
  counted (not an estimate such as `⌈log2(edge count)⌉`);
- a key element hashed or compared inside a unique, memo or computed table
  lookup, per element, hit or miss, so a long key costs its length.

Quota (`inputs.md` §2.4, §6) and inventory (§3.3, §6) count every addition,
subtraction, clamp and conditional adjustment executed, once whether or not
its condition fires; the counts equal the frozen 5 and 2 and 2 and 3.

**Boundaries and cache lifecycle.** A combine starts when `combine` is
entered with the two summaries and ends when it returns the reduced,
canonically named summary; an apply starts when `apply` is entered with the
summary and the state and ends when it returns. Every unique, memo or
computed table starts empty at the entry of each combine and each apply, and
nothing persists from one operation to the next, so no work can be moved
into a warm cache or a prewarm. Any preparation an operation relies on runs
inside it and is counted there. Outside the boundaries only three things run:
building one call's atomic summary, serialising for `B`, and generation.
Nothing else may.

Size-derived figures (nodes plus edges of the operands and the result) may be
reported as context beside the counts and never decide. Chunk summaries of a
parallel scan depend on the chunking, which the throughput part will fix;
they are not measured here.

## 4. Per seed: does a quantity rise or fall?

For family, quantity `Q ∈ {B, C, A}` and seed `j`, let `Q_j(n)` be the value
on the prefix of size `n`. Seed `j` **rises** if and only if

> `Q_j(256) > Q_j(64)` and `5 · Q_j(256) ≥ 6 · Q_j(64)`,

and **falls** if and only if

> `Q_j(64) > Q_j(256)` and `5 · Q_j(64) ≥ 6 · Q_j(256)`.

Otherwise it is **neutral**. `τ = 6/5 = 1.20`; the comparison is exact on
integers. `Q_j(64) = 0` with `Q_j(256) > 0` rises, and `0` against `0` is
neutral. The point 128 is reported with 64 and 256 and does not decide: the
test is directional over the window.

Why `τ = 1.20`. The test must catch the slowest growth class worth calling
growth, and a family that grows logarithmically is that class. A summary
that grows like `c · log2 n` goes from `6c` to `8c` over 64-256, a ratio of
`1.33`. `1.20` leaves a margin of `1.33 / 1.20 ≈ 1.11` below it, so
logarithmic growth is caught as long as its additive offset `a` in
`c · log2 n + a` stays at most `4c` (`8c + a ≥ 1.2 · (6c + a)`). As a power
law `n^α`, a fourfold range gives the ratio `4^α`, and `τ = 1.20` detects
every exponent `α ≥ ln 1.2 / ln 4 ≈ 0.13`. Linear growth (ratio 4) and `√n`
(ratio 2) are caught by a wide margin.

## 5. Per family: does it grow?

A quantity **grows** if and only if (number of rising seeds) − (number of
falling seeds) is at least 20 over the 100 seeds. The family **grows with
batch size** if any of `B`, `C`, `A` grows. The rule is the same for all
three families and names none. The counts of rising, falling and neutral
seeds are reported for every quantity.

Why a sign test on the same pairs. Growth need not touch every call
sequence, so the seeds are a sample. Counting falls against rises makes a stationary plateau balance by itself,
where counting rises alone would fail it whenever the endpoints are noisy
(paired costs of 100 or 130 with equal chance at both sizes give a rise
share of ¼ and no growth).

All probabilities below are conditional on modelling the seeded draws as
independent samples (§2).

**False fail.** If each seed's endpoint pair is exchangeable (a plateau whose
two endpoints are identically distributed and interchangeable), a rise and a
fall are equally likely. The difference is at least 20 with no neutral seeds
exactly when at least 60 of 100 seeds rise, so the false-fail probability is
at most `P(Bin(100, ½) ≥ 60) = 0.0284` per quantity (neutral seeds lower it).
A family has three quantities, so its false-fail probability is at most
three times that (union bound), about 0.085, under the same assumptions.
Without exchangeability it is **not controlled**; extra calls can change a
summary's final evaluation path, so endpoints need not be interchangeable.

**Detection.** A share `s` of rising seeds and no falling seeds is missed with
probability `P(Bin(100, s) ≤ 19)`. Falling seeds subtract from the
difference and reduce detection.

| Rising share `s`, no falling seeds | Probability of a miss |
|---|---|
| 0.5 | 1.4 · 10^-10 |
| 0.4 | 5.9 · 10^-6 |
| 0.3 | 0.0089 |
| 0.2 | 0.46 |

The design target is `s ≥ 0.3` (miss below 1%); a share under about a fifth
is likely to go undetected.

**What a pass does not show.** It does not detect:

- a rise below `τ` over the window (an exponent below about 0.13, or
  logarithmic growth with offset above `4c`);
- growth that appears only in fewer than about a fifth of the seeds, or that
  falling seeds offset;
- growth beyond 256, or on inputs outside the frozen distribution.

It is therefore not evidence of a batch-independent bound. A pass is reported
as "no growth detected over the window 64-256 under this protocol", with
these limits.

**Quota and inventory.** Their stated bound is the frozen constant of §3 (`B`,
`C`, `A` of 24, 5, 2 and 1, 2, 3), checked by measurement, so no seed rises or
falls. A measured value that differs from its constant is not a growth
finding: the implementation does not realise the frozen representation, the
document is defective, and it is re-frozen, voiding prior results
(`inputs.md` §1).

**Accounts.** No bound is stated in advance (`inputs.md` §4.3, §6), so the
rule decides only whether growth is detected. Nothing in this document
predicts the outcome, and a pass claims no batch-independent bound.

**A quantity that rises and then saturates.** Saturation is an observed
pattern, not a consequence of the frozen bounds: the threshold bound of
`inputs.md` §4.3 itself depends on the maximum batch length. The predicate
decides it and nothing else: a plateau passes when its rises and falls
balance within the criterion above (difference below 20). Sizes below 64
never decide; a rise from 1 to 64 is the expected shape of a saturating
summary and is reported as context only. A quantity whose difference reaches
20 fails, whether or not the curve flattens after 256, since the measurement
cannot tell a later plateau from continued growth. A pass is scoped to E2's
frozen domain (four accounts, balances up to 50 000, amounts up to 10 000)
and says nothing about larger domains.

**Alternatives rejected.**

- *The maximum over independent batches per size, compared across the
  window:* detects variation, not growth. A falling maximum would fail and a
  rise of 9% would pass, and different sizes would use different samples.
- *Fitted log-log slope over 1-256:* a saturating curve has a positive slope
  over the range because of its early rise, so the slope judges the rise and
  not the plateau, and it adds a fitting procedure, floating point and a
  slope threshold with no better reason than `τ`.
- *At least 20 of 100 seeds rise, ignoring falls:* a noisy plateau fails often.
- *Any strict increase from some size onwards:* "from some size" is a free
  parameter chosen after seeing sizes.
- *Comparison with size 1 or the smallest sizes:* a quantity whose early
  growth is large enough to cross `τ` would fail against an atomic summary.
- *Judging 1-256:* contradicts the aggregation `docs/experiments.md` fixes.
- *Wall time for cost:* needs hardware limits the throughput part owns.
- *A size-derived cost proxy (nodes plus edges of operands and result):* a
  lower bound on work, blind to product construction and reduction, and it
  ignores shortcuts and sharing; it is reported as context only (§3).

## 6. What a fail means, and how E3 cites the rule

A family whose quantity grows is reported as growth detected, failing the
owner's compactness condition, with its three quantities at every size and
its rising and falling seed counts. It
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
256 among its sizes, `τ` and the sign-test criterion of §5, with the counting
boundaries and cache lifecycle of §3 restated for its representations. Its
other choices are E3's to freeze. Where its generator lacks the pairing
property it states its own error criterion. E3 must state and freeze a bound
for every family it admits, as `docs/experiments.md` requires.

This rule does **not** replace E3 §1's separate prohibition on enumerating
quantity and identifier components of the checking domain. A full-domain
table could pass the growth test and is still forbidden by that gate, which
S2 enforces in the grammar. An admissible measured pass is also not
contradicted by E3's expected misses, which are expectations, not outcomes.

## 7. Freeze order

Sizes, seeds, the paired design, the counted operations and their boundaries,
the cache lifecycle, `τ`, the sign-test criterion and the window are fixed in
this document before any accounts summary implementation has landed in the
repository. A change to any value above re-freezes this part and voids the
results measured under the earlier one.
