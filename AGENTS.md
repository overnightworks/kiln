The global contract owns the code policy and the coordination workflow; this
file adds only kiln's own facts.

Entry points (pointers only; the facts live there): `README.md`,
`docs/concept.md` (the idea this repository tests), `docs/experiments.md`
(the experiment catalogue) and `docs/verification.md` (how a claim gets
checked). Read the one that owns the question before editing.

## Kiln

- The root-layout rule of the global contract is not a checked invariant here:
  kiln's CI has no root allowlist check yet, so it currently relies on review.
- `rust-toolchain.toml` pins the one Rust toolchain every lane and CI use;
  the runner's and the agent's rustup both read it, so there is no second
  version to track.
- Run tests targeted, not with the full suite: `cargo test -p <crate>`, with
  `CARGO_BUILD_JOBS=4` set in the shell. CI runs the same checks
  (`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`) once per tracked Rust workspace root.
- A benchmark runs only under its experiment's frozen protocol (see
  `experiments/*/protocol.md`); never ad hoc, because an unprotocolled number
  is not evidence.
- `docs/research/` holds the record of what was already investigated. It is
  read, never edited — a new finding gets a new file or a new experiment, not
  a rewrite of the record.
- Each dispatched slice of an experiment or lane is its own work item; this
  file names no slice by name because that inventory would go stale the
  moment one lands.
