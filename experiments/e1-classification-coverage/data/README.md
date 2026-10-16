# TPC-C classified units

**Unaudited refusal scaffolding.** These files reflect the current limits of the IR
(each transaction is refused in full) and are unsuitable for measurement. Do not pass
them to `kiln-e1 verdict`: a share computed from them is not a result. They are to be
regenerated after the IR extensions and the audits of #2 slice 6.

Written by `kiln-e1 tpcc` from the hand-lowering in `crates/kiln-e1/src/tpcc`, under
the rules of `inputs.md` §5. Each line is one classified unit: its category (or
refusal reason) with evidence. The files state no coverage figure and no verdict.

- `tpcc-classified.jsonl`: one unit per transaction type, in the order of the frozen mix.
- `tpcc-by-id-sensitivity.jsonl`: the by-id-only variants of Payment and Order-Status
  (`inputs.md` §5, Q1.5). Sensitivity only.

A unit's position is the clause of the transaction profile in TPC-C Revision 5.11
(`file`, after `#clause-`) and the printed page it starts on (`line`).

Regenerate from a clean checkout of the lane tip:

```sh
cargo build --release -p kiln-e1
target/release/kiln-e1 tpcc \
  --out experiments/e1-classification-coverage/data/tpcc-classified.jsonl \
  --sensitivity-out experiments/e1-classification-coverage/data/tpcc-by-id-sensitivity.jsonl
```

Produced with the release binary of source commit `d1b727c902e55e62c2aef29d4a67dbf7057cb78f`,
SHA-256 `62eaf8cee3a17f6128ecedac114397c0b8e45fd5c9442b66e47b0ea44727149a`. The
output is deterministic, so a rebuilt binary must write identical bytes.
