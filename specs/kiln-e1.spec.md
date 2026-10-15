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

- [KILN-E1-01] A classified unit is read from a JSON line with its subject, position, unit rule (S2, S3 or S5), parameters, absorbed sites and classification; the subject is either a manifest application id or one of TPC-C's five transaction types, and any other subject or unit rule is rejected.
- [KILN-E1-02] A unit labelled static, one-hop or domain-bounded without evidence (no IR node path, or a blank one) is rejected, so an all-static stub carries no verdict.
- [KILN-E1-03] A unit labelled unknown carries its reason and is rejected without one; it may also carry evidence, which must not be blank. A refusal reason on a unit labelled static, one-hop or domain-bounded is rejected as contradictory.
- [KILN-E1-04] An S4 exclusion is accepted only with affirmative evidence: a resolved receiver type that is not `Any`, the called method, and the reason that method has no path to an ORM operation. A missing or blank part, or an `Any` receiver, is rejected, as is an exclusion that also carries a category.
- [KILN-E1-05] A malformed JSONL line fails loudly naming the source and the 1-based line number, and no units are returned.

## Manifest

- [KILN-E1-06] The frozen set is the `owner_repo` of every row of `corpus-manifest.csv` (110 applications); a row with the wrong number of fields, a quoted field, an empty or duplicate id, or a header without `owner_repo` is rejected naming its line.

## Coverage arithmetic

- [KILN-E1-07] An application's share is its units labelled static or one-hop over all its units; domain-bounded and unknown units count in the denominator only, and the domain-bounded share is reported separately.
- [KILN-E1-08] A refused unit counts as unknown and stays in the denominator. Example: 3 static units and 1 refused unit give 75 %.
- [KILN-E1-09] A unit with an S4 exclusion leaves the denominator, and the application reports how many it excluded.
- [KILN-E1-10] The median over the frozen set is the middle share for an odd count and the mean of the two middle shares for an even count, computed exactly: a mean that is exactly 70 % or 50 % is exactly that.
- [KILN-E1-11] TPC-C's share weights each transaction type's share by the frozen mix (New-Order 45 %, Payment 43 %, Order-Status 4 %, Delivery 4 %, Stock-Level 4 %). Example: only Payment fully static and the rest not gives 43 %. The weighting is exact: transaction shares of 100 %, 7/15, 14/15, 0 and 3/10 weigh to exactly 70 %.

- [KILN-E1-17] Shares are exact fractions: they compare without rounding, and a share or sum that does not fit 128 bits makes the evaluation refuse instead of rounding; a share is shown in percent to four decimals.

## Verdict

- [KILN-E1-12] The verdict passes when both shares are at least 70 %, fails when either share is below 50 %, and is otherwise inconclusive; 70 and 50 themselves are not below their threshold, and 69.99 and 49.99 are below it, also when TPC-C's share or the median reaches 70 or 50 exactly only through its fractions. A fail outranks a pass condition on the other share.

## Refusals

- [KILN-E1-13] A manifest application with no unit left in its denominator makes the evaluation refuse and name it; it is never dropped and never counted as 0 %.
- [KILN-E1-14] A classified unit naming an application outside the manifest makes the evaluation refuse and name it.
- [KILN-E1-15] A unit given as TPC-C's that names an application, a unit given as an application's that names a TPC-C transaction type, or a TPC-C transaction type with no unit left in its denominator, makes the evaluation refuse and name it.

## Entry point

- [KILN-E1-16] `kiln-e1 verdict --manifest M --tpcc T --applications A` prints each application's share, TPC-C's weighted share, the median labelled "unweighted — no trace source exists", and the verdict, then exits 0; a refusal or an invalid input prints the reason to stderr and exits non-zero.

## Footprint classifier

The classifier assigns a transaction function of a `kiln-ir` program its footprint category under `docs/experiments.md` ("Footprint categories") and `inputs.md` §5 (Q1-Q4). A resolving read is a read whose result determines an access target of the function: a key, a range bound, or a key to insert. A function is classified by its most demanding feasible path.

- [KILN-E1-18] A function whose every access, over every feasible branch, has a key or range bound that is a pure function of its arguments and literals is static, with the IR node path of every access of every branch and of every branch's terminal as evidence. The primary key of a point-read row is the key that read used. A function that touches nothing is static.
- [KILN-E1-19] A branch composes by its arms: the function is classified as the most demanding of its feasible arms, and a continuation that contradicts a presence test already established on its path is not feasible. A branch is decided by the read it tests; that read is a resolution when the arms reach different targets (a different table, key or range), and is none when both arms reach the same targets, so a branch whose arms touch the same rows keeps a static function static. An arm that holds a resolving read makes the function at least one-hop, and the selector and a resolving read inside an arm count once when they are the same read (Q1.3, Q3.2).
- [KILN-E1-20] A read whose result only reaches the returned value or a written column, and no access target, is no resolving read (Q3.2).
- [KILN-E1-21] A function with exactly one resolving read on its most demanding path is one-hop when that read's result reaches the later access through a declared foreign key; the evidence names the read and each access it resolves, and every other access of every branch. One resolving result may determine any number of later accesses, also in a loop over its rows, without adding a hop (Q3.1, Q3.4).
- [KILN-E1-22] A resolving read whose result reaches a later access through a column with no declared foreign key into the accessed table, or through a primary key into another table, or into the bound of a primary-key range, makes the function unknown with that reason (Q2.5, `docs/experiments.md` "Unknown"). A key of an accessed table that is arithmetic over a read result is no read's column or key: it keeps neither foreign-key identity nor range containment, so it is unknown with that reason.
- [KILN-E1-23] A key to insert derived from a read is that function's one resolution, whichever column it projects; a second resolving read still exceeds one (Q3.5).
- [KILN-E1-24] A range over a declared index whose bounds are pure functions of arguments is static; resolving its rows into base-row accesses of the index's table, or through a foreign key, is one hop when it is the only resolving read. The evidence names, for each index range read of every branch, which applies: index-only, feeding another index range's bounds, or followed by base-row accesses; a read that only decides a branch is index-only. A range over an index the schema does not declare is unknown, and a primary-key range is never taken for an index (Q2.1-Q2.5).
- [KILN-E1-25] Two or more resolving reads exceed one hop whether they are independent or chained, so such a function is never one-hop; the unknown reason says which (Q3.3, Q3.7, Q3.8).
- [KILN-E1-26] A function whose two or more resolving reads are chained is domain-bounded only when it is provably confined to one partition the schema declares (Q3.7). kiln-ir declares no partition, so such a function is unknown with the reason "no declared partition", whatever its arguments look like, and the classifier never labels a function domain-bounded.
- [KILN-E1-27] Every proven label carries non-blank evidence and every unknown label its reason, so the result is a classification the evidence gate (`KILN-E1-02`, `KILN-E1-03`) accepts. A name the program does not declare as a function, and a read of a slot no earlier read bound, are reported instead of classified.
