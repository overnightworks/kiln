# kiln

kiln is a research database engine built on a single flipped assumption:
instead of a database that defends itself at runtime against an unknown set
of queries, kiln assumes one application owns the database and its complete
set of queries, transactions, and invariants is known when the engine is
built — so a compiler, not a runtime lock manager, decides at build time
which transactions need no range or table resource at all, which need only
a small keyed or typed reservation, and which need to stay fully ordered.

kiln is in its research phase: no engine exists yet. The current work is the
concept, its prior-art record, and the three experiments (E1, E2, E3) that
will decide whether the thesis survives contact with real transaction
workloads.

- [docs/concept.md](docs/concept.md) — the thesis, the isolation-kernel
  proposal, the exit door, the kill criteria, the engine-phase obligations
  not yet specified, and the open questions for the independent critique.
- [docs/experiments.md](docs/experiments.md) — each experiment's gate
  rules for E1, E2, and E3: hypothesis, gating predicate, baselines,
  aggregation, and audits; the concrete inputs are each experiment's own
  item's frozen deliverable.
- [docs/research/](docs/research/) — the prior-art report, the detailed
  research notes it was built from, and the ideation that produced kiln's
  design options.
- [docs/verification.md](docs/verification.md) — the five-layer verification
  strategy (design proofs, code proofs, differential testing, deterministic
  simulation, a thin real-world layer), the named anomaly and regression
  scenarios, and the verified precedents it rests on.
