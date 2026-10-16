# `kiln-e2`

`crates/kiln-e2` owns E2's oracle: the three frozen families as `kiln-ir`
programs, whose serial replay through `kiln_ir::replay` is the reference the
serial baseline and the summary algebras are checked against, and the seeded
batch generators. This file owns the observable behavior of that crate. It
cites the freeze rather than restating it:
`experiments/e2-parallel-ordered-transactions/inputs.md` (sections 1 to 5). A
test claims a criterion by living in the module named for it: `mod kiln_e2_03
{ #[test] fn ... }` claims `KILN-E2-03`. One criterion per module; several
tests may share it. The crate times nothing and runs nothing in parallel; the only summary algebra
it builds is quota's.

## Quota allocation

- [KILN-E2-01] `allocate(q)` grants up to `q` of what remains and returns the quantity granted: `q` when `remaining >= q`, otherwise what remained (a partial grant, leaving 0). With nothing remaining it aborts `quota_exhausted`.
- [KILN-E2-02] `refill(q)` adds up to `q` below the ceiling 1000 and returns the quantity added: `q` when `remaining + q <= 1000`, otherwise `1000 - remaining` (a partial refill, leaving 1000). At the ceiling it aborts `quota_full`.
- [KILN-E2-03] A quota quantity below 1 or above 50 aborts `invalid_quantity` before the quota is read, even when the quota is exhausted or full.
- [KILN-E2-04] Worked examples Q1 to Q4 replay with their stated per-call outcomes and states, and Q3's two orders of the same calls leave 993 and 996.

## Conditional inventory

- [KILN-E2-05] `order_line(q)` returns the stock it found, before its own update. The stock falls to `s - q` when that stays at least 10 ("by 10 or more"), and otherwise to `s - q + 91`.
- [KILN-E2-06] `stock_check(t)` returns 1 when the stock is below `t` and 0 otherwise, and leaves the stock unchanged.
- [KILN-E2-07] An `order_line` quantity outside 1 to 10 aborts `invalid_quantity`, and a `stock_check` threshold outside 10 to 20 aborts `invalid_threshold`, each leaving the stock unchanged.
- [KILN-E2-08] Worked examples I1 to I3 replay with their stated per-call outcomes and states, and I3's two orders of the same calls leave the same stock 98 with different responses.

## Mixed account operations

- [KILN-E2-09] `deposit` and `withdraw` return the balance after the call and move it by the amount; a withdrawal of exactly the balance commits 0, and one beyond it aborts `insufficient_funds`.
- [KILN-E2-10] `transfer(a, b, x)` moves `x` from `a` to `b`, returns the balance of `a` after it, and aborts `same_account` when `a = b`.
- [KILN-E2-11] `freeze` and `unfreeze` return the balance and set the status; freezing a frozen account aborts `already_frozen`, unfreezing an open one `not_frozen`.
- [KILN-E2-12] Every account call tests in the frozen order and the first failing test decides: argument tests (amount, then `same_account`), then presence (source before target), then status (source before target), then funds. A malformed call therefore reads nothing, and a frozen account with short funds reports `account_frozen`.
- [KILN-E2-13] A call naming an account outside keys 0 to 3 aborts `account_not_found` and changes nothing.
- [KILN-E2-14] Worked examples A1 to A5 replay with their stated per-call outcomes and states.

## All families

- [KILN-E2-15] An aborted call changes no state and does not stop the calls after it.
- [KILN-E2-16] Each program declares the frozen schema: its table, the column widths (quota I32, stock I16, account balance I64 and status I8) and the frozen check constraints, and the schema validates.
- [KILN-E2-17] An argument outside its parameter's width aborts with an argument overflow before the body runs.

## Generators

- [KILN-E2-18] SplitMix64 reproduces the frozen test vectors: from state 0 and from state `(1 << 48) | (64 << 32) | 0`, the first three outputs are those the freeze lists.
- [KILN-E2-19] `uniform(lo, hi)` is `lo + (next mod (hi - lo + 1))`.
- [KILN-E2-20] Batch `j` of size `n` for family `f` starts from the state `(f << 48) | (n << 32) | j` with quota 1, inventory 2 and accounts 3; a size outside 1 to 256 is refused.
- [KILN-E2-21] A generator is deterministic: the same family, size and batch index give the same batch, and the seed derivation is the frozen one of KILN-E2-20.
- [KILN-E2-22] The quota generator draws the initial remaining, then per call the kind, the quantity and the malformed share, in that order, with the frozen mix, ranges and malformed replacement; its batches match a reference derivation of the frozen draws.
- [KILN-E2-23] The inventory generator does the same with the frozen mix of 45 order lines to 4 stock checks, ranges and malformed replacements.
- [KILN-E2-24] The accounts generator draws each account's balance then status, then per call the kind and the arguments in the listed order and the malformed share, with the frozen mix and ranges; it never produces `same_account` or an absent account in a money operation, and its malformed calls carry amount 0 or account 4.
- [KILN-E2-25] Within generated accounts batches, every balance at every point of the replay lies in 0 to `BALANCE_BOUND` = 50 000 + 256 x 10 000 = 2 610 000, and the worst-case batch reaches that bound exactly.
- [KILN-E2-26] On the listed small batches (sizes 4, 12 and 16) the generated calls replay on the oracle to the full ordered responses and abort names and the final state of an independent reference model of the frozen outcome tables, and those batches together cover every abort reason the generators can reach (`same_account` is excluded by the generators); on the larger listed batches the replay is checked by aggregate digests and final state.

## Serial folds and the comparator

- [KILN-E2-27] Each family's hand-written serial fold (`serial_fold`) yields, on every prefix of the protocol's corpus (the first 1, 2, 4, 8, 16, 32, 64, 128 and 256 calls of `generate(256, seed)` for seeds 0 to 99, each prefix keeping the batch's initial state) and on batches generated independently at those sizes, the same outcome for every call and the same final state as the oracle's replay, and also on hand-written batches the generators do not produce (`same_account`, absent accounts in every position, quantities far outside their range). Arguments outside the declared parameter widths are outside the folds' domain: the replay reports them as an argument overflow, and the generators never produce them.
- [KILN-E2-28] Those prefix batches exercise every row of each family's frozen outcome table at least once (`same_account` and an absent account in a money operation excluded, as the generators never produce them): full and partial quota grants and refills, in-place and restocking order lines, both stock-check answers, and every abort.
- [KILN-E2-29] The comparator reports the first diverging call's index with the expected and the actual outcome and what differs (outcome kind, response, abort reason or abort payload); equal calls with unequal call counts report the counts, and equal calls with unequal final states report both states. An oracle abort the family does not declare, or a declared name it does not know, converts to a structural abort and so diverges from any fold.
- [KILN-E2-30] A fold that raises the quota ceiling by one is reported at the first refill that reaches the ceiling, with the oracle's and the fold's response.

## Quota summary, scan and counter

- [KILN-E2-31] Quota's `combine` is associative on values (not merely through `apply`): over atomic summaries of consecutive generated calls and over composite summaries of consecutive segments of `generate(256, seed)` for seeds 0 to 99.
- [KILN-E2-32] `epsilon` (an `invalid_quantity` call, whichever kind) is an identity of `combine` on either side through `apply` on every state in `[0, 1000]`; the laws are checked semantically, never by comparing values.
- [KILN-E2-33] The sequential scan derives each call's pre-state as `apply(P_{i-1}, x0)` from the prefix summaries and yields the serial fold's response for every call (full and partial grants and refills, both declared aborts) and its final state, on every prefix of the protocol's corpus.
- [KILN-E2-34] On the protocol's prefix corpus the batch summary is 24 bytes, the largest combine of the balanced pairwise fold executes 5 counted operations (3 additions, 2 clamps; none for a single call) and applying the batch summary executes 2 (1 addition, 1 clamp), counted where they execute; `cost_report` gives these per protocol batch size.
