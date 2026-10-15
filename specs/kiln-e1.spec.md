# `kiln-e1`

`crates/kiln-e1` owns E1's record contract, coverage arithmetic, evidence gate
and verdict. This file owns the observable behavior of that crate. It cites the
frozen protocol rather than restating it: `docs/experiments.md` E1 ("Footprint
categories", "Gating numerator and verdict", "Audits") and
`experiments/e1-classification-coverage/inputs.md` §1, §3 and §4. A test claims
a criterion by living in the module named for it: `mod kiln_e1_03 { #[test] fn
stub_without_evidence_is_rejected() {...} }` claims `KILN-E1-03`. One criterion
per module; several tests may share it.

## Records and evidence gate

- [KILN-E1-01] A classified unit written as a JSON line reads back unchanged, with its application, position, unit rule (S2, S3 or S5), parameters, absorbed sites and classification; any other unit rule is rejected.
- [KILN-E1-02] A unit labelled static, one-hop or domain-bounded without evidence (no IR node path, or a blank one) is rejected, so an all-static stub carries no verdict.
- [KILN-E1-03] A unit labelled unknown carries its reason and is rejected without one; a refusal reason on a unit labelled static, one-hop or domain-bounded is rejected as contradictory.
- [KILN-E1-04] An S4 exclusion is accepted only with affirmative evidence: a resolved receiver type that is not `Any`, the called method, and the reason that method has no path to an ORM operation. A missing or blank part, or an `Any` receiver, is rejected, as is an exclusion that also carries a category.
- [KILN-E1-05] A malformed JSONL line fails loudly naming the source and the 1-based line number, and no units are returned.

## Manifest

- [KILN-E1-06] The frozen set is the `owner_repo` of every row of `corpus-manifest.csv` (110 applications); a row with the wrong number of fields, a quoted field, an empty or duplicate id, or a header without `owner_repo` is rejected naming its line.

## Coverage arithmetic

- [KILN-E1-07] An application's share is its units labelled static or one-hop over all its units; domain-bounded and unknown units count in the denominator only, and the domain-bounded share is reported separately.
- [KILN-E1-08] A refused unit counts as unknown and stays in the denominator. Example: 3 static units and 1 refused unit give 75 %.
- [KILN-E1-09] A unit with an S4 exclusion leaves the denominator, and the application reports how many it excluded.
- [KILN-E1-10] The median over the frozen set is the middle share for an odd count and the mean of the two middle shares for an even count.
- [KILN-E1-11] TPC-C's share weights each transaction type's share by the frozen mix (New-Order 45 %, Payment 43 %, Order-Status 4 %, Delivery 4 %, Stock-Level 4 %). Example: only Payment fully static and the rest not gives 43 %.

## Verdict

- [KILN-E1-12] The verdict passes when both shares are at least 70 %, fails when either share is below 50 %, and is otherwise inconclusive; 70 and 50 themselves are not below their threshold, and 69.99 and 49.99 are below it. A fail outranks a pass condition on the other share.

## Refusals

- [KILN-E1-13] A manifest application with no unit left in its denominator makes the evaluation refuse and name it; it is never dropped and never counted as 0 %.
- [KILN-E1-14] A classified unit naming an application outside the manifest makes the evaluation refuse and name it.
- [KILN-E1-15] A TPC-C unit naming no TPC-C transaction type, or a TPC-C transaction type with no unit left in its denominator, makes the evaluation refuse and name it.

## Entry point

- [KILN-E1-16] `kiln-e1 verdict --manifest M --tpcc T --applications A` prints each application's share, TPC-C's weighted share, the median labelled "unweighted — no trace source exists", and the verdict, then exits 0; a refusal or an invalid input prints the reason to stderr and exits non-zero.
