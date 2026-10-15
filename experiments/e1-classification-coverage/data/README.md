# TPC-C classified units

Written by `kiln-e1 tpcc` from the hand-lowering in `crates/kiln-e1/src/tpcc`, under
the rules of `inputs.md` §5. Each line is one classified unit: its category (or
refusal reason) with evidence. The files state no coverage figure and no verdict.

- `tpcc-classified.jsonl`: one unit per transaction type, in the order of the frozen
  mix. This is the file `kiln-e1 verdict --tpcc` reads.
- `tpcc-by-id-sensitivity.jsonl`: the by-id-only variants of Payment and Order-Status
  (`inputs.md` §5, Q1.5). Sensitivity only; never pass it to `kiln-e1 verdict`.

A unit's position is the clause of the transaction profile in TPC-C Revision 5.11
(`file`, after `#clause-`) and the printed page it starts on (`line`).

Regenerate from a clean checkout of the lane tip:

```sh
cargo build --release -p kiln-e1
target/release/kiln-e1 tpcc \
  --out experiments/e1-classification-coverage/data/tpcc-classified.jsonl \
  --sensitivity-out experiments/e1-classification-coverage/data/tpcc-by-id-sensitivity.jsonl
```

Produced with the release binary of source commit `27fb07f30823fc918a10603c1d81e9862ef664f3`,
SHA-256 `0d2a85e847ba4aa73d7aeccd6c981bbf3cd269ff99d6202c417766292b9b6f63`. The
output is deterministic, so a rebuilt binary must write identical bytes.
