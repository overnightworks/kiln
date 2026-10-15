# Fable blind ideation (summary of the returned report, key mechanisms verbatim in substance)

Core flip: "the database does not know its program" -> closed-world: one service owns the DB; the complete
set of transactions is a closed, typed, analyzable program. "A borrow checker for ACID": static conflict
analysis plus a small runtime (OCC validation, reservations) where analysis cannot decide.

Other assumptions listed: interactive transactions; runtime concurrency control; constraints as runtime
index checks; one totally ordered log; block-device storage; fsync as durability; serializable too
expensive; generality worth its cost; schema changes rare; recovery rare; correctness tested after the fact.

Mechanism:
- Schema, invariants, transactions declared in Rust via proc macros; data access through a restricted
  query DSL so the macro extracts a footprint per table (predicate over keys in parameters, constants,
  earlier reads). Footprints: static / one-hop / dynamic.
- Static conflict graph over transaction classes (chopped where safe); commutativity from abstract
  operation types (increment, insert fresh key, claim, pin, append). Compiler derives coordination domains
  (partitions along a natural ownership key: tenant, warehouse, account).
- Runtime: one single-threaded executor per domain, per core, shared-nothing, own durable log.
  Single-domain txn: no lock/latch/version check. Multi-domain: ticket from per-group sequencer,
  deterministic Calvin-style application, no 2PC vote. Proven-commutative classes bypass ordering.
  Dynamic footprint: reconnaissance read, resubmit, abort+retry if footprint changed.
- Typed reservations: uniqueness = claim(k, owner) in claims index hash-partitioned by value;
  foreign key = pin (reference count on parent); balance >= 0 = escrow counter with quota split.
  Compiler orders abortable pieces (claims, existence checks, escrow) before non-abortable ones.
- Domain logs with commit vectors (Taurus-like); embedded mode = same design in one file.
- Runtime domain-tag check on every access catches unsound analysis; deterministic simulation harness.

Expected gain (its estimate): TPC-C 5-20x Postgres serializable same cores; vs SQLite WAL multi-writer
bounded by core count; sign-ups 3-5x.
Costs: no interactive/ad-hoc writes; hot domain = serial bottleneck (VoltDB wall); unsound analysis =
silent isolation violation; snapshot readers need version chains; moving the ownership key = serial migration.
Killer failure mode: navigational transactions (key of 2nd access depends on 1st read). If most classes
are dynamic -> Calvin with extra steps, loses to Postgres on latency.

Prior art named: transaction chopping (1995), IC3 (2016), Homeostasis (2015, closest in spirit, geo,
linear arithmetic only), I-confluence (2014), Indigo (2015), Hamsaz (2019), Quelea, Carol, escrow (1986),
Weihl ADT commutativity (1988), Doppel (2014), Calvin/Bohm/Aria/SLOG/Detock, H-Store/VoltDB, Hekaton,
Taurus (2020, multi-log recovery with LSN vectors), SpacetimeDB/Convex/Durable Objects (believed runtime
OCC/locking, uncertain), LeanStore/Umbra, DBSP/Feldera, LegoBase.
Claimed uncovered combination: host-language type system whose effects are DB footprints and invariants;
compiler derives domains, commutativity classes, reservations, minimal cross-domain order; serializable
only; one artifact from embedded file to distributed; AI toolchain rewrites transactions against compiler
feedback ("PlaceOrder and Restock conflict on stock.qty; use an escrow counter").

Hypotheses:
- H1 (make-or-break, cheapest, run first): footprint classifier assigns >=70% of transaction instances
  (frequency-weighted) of TPC-C and three real open-source CRUD services to static or one-hop
  single-domain footprints along one ownership key. <50% falsifies. Only extractor+classifier, no runtime.
- H2: 8 warehouses / 8 cores, serializable TPC-C NewOrder+Payment >=5x Postgres 17 serializable,
  within 2x of H-Store, multi-warehouse 10% costs <=30%.
- H3: sign-up workload (unique email, FK to org) near-linear to 32 cores, abort <0.1% at 1% duplicates, 3x Postgres.
- H4: dm-flakey / fault-injecting FS, injected fsync errors and torn writes: zero lost acknowledged
  commits over 10,000 crash cycles; recovery proportional to largest domain.
Ranking: 1 static coordination from closed-world typed program; 2 typed reservations; 3 domain logs (component only).
