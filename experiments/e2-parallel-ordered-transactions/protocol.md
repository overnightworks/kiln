# E2 — frozen part B: compactness measurement only

Status: proposed, not yet frozen. It becomes frozen under the freeze
protocol of `docs/experiments.md` ("Freeze protocol"), and from then on
nothing below changes after a summary has been measured. Part A is
`inputs.md` (families, summaries, generator); this file is part B and owns
one decision: when a family's summary "grows with batch size".

## 1. Scope

This part freezes how "grows with batch size" (`docs/experiments.md`,
"Admissible bound class"; `inputs.md` §1, "Compactness rule") is decided,
for quota, inventory and accounts alike, from bytes and operation counts
only. It answers `inputs.md` §7, question 2. The throughput protocol
(hardware, threads, timed region, repeats, noise limits, verdict
arithmetic) is not frozen here; it is added as further sections, and
nothing below depends on it. This part uses no clock and no hardware.

## 2. Batches

| Setting | Value | Reason |
|---|---|---|
| Batch sizes `n` | 1, 2, 4, 8, 16, 32, 64, 128, 256 | Powers of two cover the whole progression 1-256 at even spacing in `log n`, and include the three gating sizes of `docs/experiments.md` ("Gating sizes and aggregation"). |
| Batches per size | 100, indices `j = 0 … 99` | A maximum over 100 draws reaches the upper tail of each family's input distribution while the whole measurement stays a few thousand small folds. |
| Seed | `batch_generator(family, n, j)`: `state = (f << 48) \| (n << 32) \| j` | Part A's frozen derivation (`inputs.md` §5, implemented as `kiln_e2::batch_generator`), so every batch is reproducible from `(family, n, j)`. |

The generator draws each batch's initial state first, then its calls, in
each family's draw order (`inputs.md` §2.6, §3.5, §4.5). Because `n` is part
of the seed, the batch of size 128 is not an extension of a batch of size
64: each size is an independent sample of the same distribution. The family
numbers are part A's (`f = 1` quota, `2` inventory, `3` accounts).

## 3. Quantities per batch

For a batch `c_1 … c_n` the measurement builds the atomic summaries
`S_1 … S_n` and folds them in order. Three quantities are recorded per
family and batch. None is a duration.

**Size, `B`.** The bytes of the batch summary `S = S_1 ⊕ … ⊕ S_n`, counted
on the frozen representation, not on an in-memory layout:

- quota: 24, the three `i64` of `inputs.md` §2.4;
- inventory: 1, the `u8` of `inputs.md` §3.3;
- accounts: `bytes(S)` of `inputs.md` §4.3, the length of the serialised
  canonical value (two masks and four canonically named reduced diagrams).
  The value is the same under every grouping of the fold, since combine is
  associative on values, so `B` does not depend on how the fold is grouped.

**Combine cost, `C`.** The fold is the balanced pairwise fold: adjacent
summaries are combined in pairs, an odd one out carried to the next level,
until one summary is left. `C` is the largest operation count of any single
combine in this fold. The balanced fold is used because it includes
combines of two large summaries, the case a parallel scan produces and the
one a growing summary makes expensive. A batch of one call has no combine,
and `C` is then not defined. The operations counted:

- quota: each addition and each clamp counts 1, so 5 per combine
  (`inputs.md` §2.4, §6);
- inventory: each addition and each conditional subtraction counts 1, so 2
  per combine (`inputs.md` §3.3, §6);
- accounts: the inner nodes plus the edges of the four diagrams of both
  operands and of the four diagrams of the result.

The accounts count is a function of the three summaries' values. It is
used instead of the nodes and edges an implementation happens to visit
because how combine is computed is the implementer's choice (`inputs.md`
§4.3, "Combine"), and a verdict must not depend on that choice. Any combine
must read both operands and write its result, so the count is a lower bound
on the work. The visited counts that `inputs.md` §4.3 asks to be reported
are reported alongside; they never enter the decision.

**Apply cost, `A`.** The operation count of applying the batch summary `S`
to the batch's initial state:

- quota: one addition and one clamp, 2 (`inputs.md` §2.4, §6);
- inventory: two subtractions and one conditional addition, 3
  (`inputs.md` §3.3, §6);
- accounts: over the four diagrams, the nodes visited when evaluating at
  the initial state, plus per visited node `⌈log2(edge count)⌉` comparisons,
  the binary search `inputs.md` §4.3 fixes. A diagram that is a single leaf
  costs 0.

Chunk summaries of a parallel scan depend on the chunking, which the
throughput part will fix; they are not measured here.

## 4. Statistic per size

For each family, quantity `Q ∈ {B, C, A}` and size `n`, the statistic
`m_Q(n)` is the **maximum** of `Q` over the 100 batches of that size.

Reason: the admissible bound class is a bound, "stays within a fixed,
stated bound" (`docs/experiments.md`), and a bound fails if any batch
breaks it. A mean or median would hide the rare long chains that make an
account summary large; the maximum is the worst case over the sample. Its
price is that it depends on the sample size, which is why the sample size
is fixed in §2 and the decision rule in §5 has a tolerance.

The statistics for all sizes 1-256 are reported as context, as in
`docs/experiments.md` ("Gating sizes and aggregation"). The decision rule
reads only the three gating sizes.

## 5. Decision rule

For one family and one quantity `Q`, let `lo` and `hi` be the smallest and
the largest of `m_Q(64)`, `m_Q(128)`, `m_Q(256)`.

> `Q` **does not grow with batch size** if and only if `hi ≤ 1.10 · lo`.
> The family **grows with batch size** if any of `B`, `C`, `A` grows.

The comparison is exact on integers (`10 · hi ≤ 11 · lo`), so no
floating-point arithmetic is involved. The family passes the compactness
bound if and only if none of its three quantities grows. The rule is the
same for all three families; nothing in it names a family.

**Quota and inventory.** Their `B`, `C` and `A` are the constants of §3
(24, 5, 2 and 1, 2, 3), so `hi = lo` and the rule passes them. This is by
construction, but they are still measured. A measured value that differs
from its constant is not a growth finding: it means the implementation does
not realise the frozen representation, the document is defective, and it is
re-frozen, voiding prior results (`inputs.md` §1).

**Accounts.** The family has no fixed size (`inputs.md` §4.3, §6), so the
rule decides it only by what the measurement shows. Nothing in this
document predicts the outcome.

**A quantity that rises and then saturates.** The rule judges by position:

- Saturation reached at or before size 64 passes. From 64 to 256 the
  statistic is flat within 10%, so it does not depend on batch size over
  the sizes the verdict is judged at. A small value domain can cause such a
  plateau (balances and amounts are bounded by the frozen distribution, so
  the number of distinct thresholds is bounded). That is a property of the
  frozen inputs, which are the inputs the hypothesis is stated for, and the
  result is reported with its plateau value and the domain that bounds it.
  It is not evidence for larger domains. A measurement over a larger domain
  is a different measurement: E3 draws from each family's full domain for
  this reason (§6).
- Still rising between 64 and 256 fails, however the curve flattens
  afterwards. A plateau first reached inside the gating range, or after it,
  cannot be told apart from continued growth by this measurement, and the
  verdict is judged at all three sizes, not at the last one.
- Sizes below 64 never decide. A rise from 1 to 64 is the expected shape of
  a saturating summary and is reported as context only.

**Tolerance.** The 10% allows for the maximum of 100 draws moving by chance
when the true distribution has saturated. It is a setting fixed in advance,
not calibrated on a result. A quantity growing linearly shows a ratio of 4
across the window, and one growing like `√n` a ratio of 2; growth below 10%
over a fourfold increase in batch size is, for this experiment, not growth.

**Alternatives rejected.**

- *Ratio of the largest size to one middle size* (for example `m(256) /
  m(64)`): looks at two points, so a rise at 128 that falls back is missed,
  and the middle size is an arbitrary choice. The window rule uses all three
  gating sizes and no point is privileged.
- *Fitted log-log slope over 1-256*: a saturating curve has a positive
  slope over the whole range because of its rise at small sizes, so the
  slope judges the rise instead of the plateau. It also needs a slope
  threshold, a fitting procedure and floating-point arithmetic, and each of
  them is a further setting with no better reason than 10%.
- *Any strict increase from some size onwards*: the maximum of 100 random
  draws rises by chance even when the distribution has saturated, so the
  rule would fail saturated summaries on noise, and "from some size" is a
  free parameter that can be chosen after seeing sizes.
- *Comparison with size 1 or with the smallest sizes*: every summary of more
  than one call is larger than the atomic one, so any saturating quantity
  would fail it, and constant-size summaries would be separated from
  growing ones only by the choice of the reference.
- *Judging the full progression 1-256 instead of the gating window*:
  contradicts the aggregation `docs/experiments.md` fixes, where sizes below
  64 are context and never enter the verdict.
- *Wall time for cost*: `docs/experiments.md` allows micro-benchmarks as
  well as operation counts; this part uses counts only, because a timing
  needs hardware and noise limits that the throughput part owns, and the
  growth rule must not depend on them.

## 6. What a fail means, and how E3 cites the rule

A family whose quantity grows is reported as failing the admissible bound
class, with its three statistics per size. It is never replaced by a
fallback: no capped or approximate summary, no call list, no serial path
stands in for it (`inputs.md` §1, §4.3). For accounts this makes E2 fail as
a whole ("one family failing fails E2 overall") and the result is recorded
against `docs/concept.md` §13's second kill criterion (`inputs.md` §4.3).
Throughput and correctness are measured and reported for a failing family
either way.

E3 does not reuse E2's batches. It measures its own batches, drawn from each
family's full domain (`experiments/e3-summary-synthesis/inputs.md` §1, §10
item 4), on the representation of each candidate, and applies §5 unchanged
to the per-size maxima of the quantities it measures. For that the sizes 64,
128 and 256 must be among its measured sizes. Its other choices (the
remaining sizes, the number of batches, the quantity definitions for its own
representations) are E3's to freeze, using the same kind of definitions as
§3. The rule has no memory of any domain: it judges the batches it is given,
which is why E3's batches come from the family domain. A candidate that is
admitted on the checking domain but grows on family-domain batches fails,
and counts as a miss in E3.

## 7. No measurement before the freeze

Sizes, batch counts, the statistic, the tolerance and the window are fixed
here before any summary of any family has been measured. No summary has
been built at any batch size to choose them. Choosing the rule after seeing
sizes would tune it against the result, and a result measured under a rule
that changed afterwards is void. A change to any value above re-freezes this
part and voids the results measured under the earlier one.
