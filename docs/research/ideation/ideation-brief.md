# Brief: first-principles ideation for a new database engine

## Context
A learning and research project: a new database engine, written in Rust, built by AI agents.
The operator wants a genuinely new foundational concept, close to scientific novelty, not a
reassembly of existing systems. "Better because the base concepts are better", not because
of micro-optimizations.

## Hard requirements the design must still meet
- Full ACID: atomicity, consistency, serializable isolation (no write skew), durability.
- Primary keys, uniqueness constraints ("user with this email already exists"),
  referential integrity.
- Crash safety under power loss, torn writes, fsync failure.
- Scales from a single embedded file (SQLite class) up to very large data volumes and high
  read and write throughput.
- Aim: outperform existing systems on the workloads it targets.

## What to do
1. List the 8-12 deepest implicit assumptions that nearly all databases make (SQLite,
   Postgres, MySQL, MongoDB, RocksDB, FoundationDB, Spanner, ...) that may no longer hold in
   2026: hardware, how software is written (AI-generated code, one service owning its
   database), deployment, workloads.
2. For each: what changes if you flip it.
3. Pick the 1-3 most promising flips and combine them into one coherent architecture. For
   each: the concrete mechanism (data structures, protocols), which bottleneck disappears
   and why, the expected gain (order of magnitude, with reasoning), what it costs and what
   gets worse, and the failure mode that would kill the idea.
4. Prior art: name the closest systems and papers you know and state precisely what they do
   not cover. Mark uncertainty. Claim no novelty you cannot argue.
5. One falsifiable hypothesis per idea and the cheapest experiment that would test it
   (for example TPC-C or YCSB against SQLite and Postgres).
6. How ACID and uniqueness are guaranteed in your design.

## Constraints
- Known building blocks (deterministic execution, log-as-database, LSM trees, MVCC, CRDTs,
  bitemporal facts, learned indexes, columnar storage) are not novel by themselves. Use them
  only as parts; the novelty must be in a flipped assumption.
- Rank your ideas by novelty x expected impact x feasibility for a small AI-driven team.
- Dense, no filler, 1500-2500 words. Answer in English.
- Read-only: do not create or modify any files.
