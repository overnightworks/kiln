# kiln

kiln is a research database engine built on a single flipped assumption:
instead of a database that defends itself at runtime against an unknown set
of queries, kiln assumes one application owns the database and its complete
set of queries, transactions, and invariants is known when the engine is
built — so a compiler, not a runtime lock manager, decides at build time
which transactions can run in parallel, which need a typed reservation, and
which need to stay fully ordered.

kiln is in its research phase: no engine exists yet. The current work is the
concept, its prior-art record, and the two experiments (E1, E2) that will
decide whether the thesis survives contact with real transaction workloads.

- [docs/concept.md](docs/concept.md) — the thesis, the isolation-kernel
  proposal, the kill criteria, and the open questions for the independent
  critique.
- [docs/research/](docs/research/) — the prior-art report, the detailed
  research notes it was built from, and the ideation that produced kiln's
  design options.
