**I would investigate a database whose computational primitive is a verified transformation of an ordered batch of transactions.** Its central question is: can transactions that depend on one another’s results nevertheless be evaluated in parallel, while preserving every caller’s result, failure, and database constraint?

The promising research target is **automatically discovering compact representations that make this possible for useful transaction families**. Batching, compilation, semantic concurrency control, and parallel prefix algorithms already exist. Their combination alone would not establish scientific novelty.

Target workloads: service-owned OLTP with recurring transaction families, contention on shared business state, and enough concurrent requests to batch. General-purpose analytics and isolated, latency-sensitive writes are weaker targets.

The assumptions worth challenging are these. They are widespread design defaults, with substantial exceptions—not properties shared universally by every named system.

| Implicit assumption | Why reconsider it now? | What changes when flipped? |
|---|---|---|
| **1. Future transactions are unknown to the engine.** | One service often owns nearly all mutations. | A versioned transaction catalog becomes optimization input; unknown transactions use a conservative execution path. |
| **2. One transaction is the natural compilation unit.** | Many simultaneous requests execute the same application code. | Compile a batch into one computation, preserving individual outcomes. Shared work can disappear. |
| **3. A dependency chain requires sequential computation.** | Some state transitions have compact associative composition. | Preserve transaction order while evaluating its prefixes in parallel. The hard problem becomes representation discovery. |
| **4. Read/write overlap adequately describes interference.** | Application code exposes guards, arithmetic, and returned values. | Derive interference from semantics. Two writes may combine; apparently harmless operations may conflict through their results. |
| **5. Each committed update deserves separate physical maintenance.** | Numerous intermediate versions are never observed. | Persist sufficient effects and outcomes, then coalesce representation maintenance across unobserved states. |
| **6. Aggressive engine optimizations must be handwritten and trusted.** | AI agents can cheaply propose many specializations, but their reliability is insufficient. | Treat generated optimizations as untrusted proposals accepted only through checked rules or proofs. |
| **7. Application deployment and engine specialization are independent.** | A service’s transaction vocabulary changes at identifiable deployment boundaries. | Deploy schema, transaction semantics, and specialization certificates as one versioned package. |
| **8. The database process boundary is architecturally fundamental.** | A single owner can embed execution and avoid repeated request exchanges. | Share semantics between embedded and server modes; choose process isolation for operational reasons. |
| **9. Storage latency dominates transaction cost.** | Communication and coordination can dominate modern OLTP. | Optimize messages, repeated interpretation, and serialized shared-state work before storage micro-optimizations. |
| **10. Keys or tables are the natural placement units.** | Transactions connect rows through invariants and references. | Place frequently interacting constraint domains together, accepting imbalance when it avoids distributed coordination. |
| **11. Embedded and distributed databases need different semantic cores.** | Transaction evaluation can be separated from persistence and replication. | Reuse semantics and batch formats while retaining distinct local and distributed commit protocols. |

The hardware argument is workload-dependent: a recent whole-stack study identifies communication as the dominant cost in its OLTP experiments. That supports assumption 9’s reversal, not a universal claim that storage has stopped mattering. [OLTP Through the Looking Glass, CIDR 2025](https://www.vldb.org/cidrdb/2025/oltp-through-the-looking-glass-16-years-later-communication-is-the-new-bottleneck.html)

I would combine three flips, ranked below. Scores are subjective, from 1–5; “novelty” means potential research contribution, conditional on further prior-art review.

| Rank | Idea | Novelty × impact × small-team feasibility | Role |
|---|---|---:|---|
| 1 | Automatically derive compact, ordered transaction composition | 4 × 5 × 2 = **40** | Scientific bet |
| 2 | Admit application-specific execution through checked certificates | 2 × 4 × 4 = **32** | Makes specialization manageable |
| 3 | Materialize state according to observation requirements | 1 × 3 × 4 = **12** | Supporting storage design |

**The first flip changes what execution means.** Represent invocation \(i\) by a state transformation \(F_i\) and a response function \(R_i\). Responses include application results and semantic failures. A rejected operation leaves state unchanged.

For a chosen serial order:

\[
s_i=F_i(s_{i-1}),\qquad r_i=R_i(s_{i-1})
\]

Search offline for a compact summary of each \(F_i\), with an associative composition operator. A parallel prefix scan then computes the state preceding every invocation, allowing its response to be evaluated.

For eligible summaries with constant-cost composition and application, this takes \(O(n)\) work and \(O(\log n)\) dependency depth. Producing \(n\) responses still requires \(O(n)\) work.

A concrete example is a quota supporting capped refill and partial allocation:

- Refill adds capacity up to a ceiling.
- Allocate grants up to the available capacity and returns the quantity granted.

These transitions can use clipped translations, \(F(x)=\operatorname{clamp}(x+a,l,u)\), whose compositions remain compact. Refill and allocation generally do not commute, but their fixed-order composition is associative. Every caller’s granted quantity follows from its prefix state.

By contrast, `reserve(q)` that either grants **all** requested units or fails is much harder. Arbitrary quantities create complicated state-dependent success patterns. Simply storing a composition tree preserves the original computation; it does not automatically parallelize it.

The proposed compiler would recognize tractable families, generate bulk relational effects, and reject unsupported combinations. A transaction touching several rows retains one atomic outcome; the compiler cannot independently accept its component updates.

The disappearing bottleneck is repeated serialized evaluation and mutation of hot state. A batch might replace 64 shared-record updates with one final update plus parallel response computation.

An **estimated 3–10× gain on the affected bottleneck** is plausible. End-to-end gains are smaller: accelerating 80% of execution by 8× yields approximately 3.3× overall. Logging, network traffic, and unique inserted rows remain.

Costs include batching latency, compilation complexity, and limited eligibility. The killer failure is summary growth: if realistic guarded transactions require large representations or mostly fall back, this becomes a narrow optimizer rather than a foundational engine.

**The second flip changes who supplies optimization—and who must be trusted.** A deployment package contains transaction definitions, schema and collation versions, derived footprints, permitted composition rules, and correctness certificates.

AI agents may propose representations and rewrites. A small checker admits only transformations proven equivalent to the reference semantics. Initially, execute checked intermediate representations through trusted Rust primitives; unchecked generated Rust must not gain arbitrary access to engine state.

Certificates must cover state changes, responses, constraint failures, arithmetic overflow, and applicable ordering requirements. Runtime guards establish that a specialization’s preconditions hold. Unknown cases enter ordinary serializable execution.

This removes repeated interpretation and conservative metadata work where those costs matter. If these consume 20–50% of CPU time, eliminating most of them offers roughly **1.2–2×** improvement over the unspecialized path. This estimate overlaps with the first idea’s gains.

Costs move into deployment: proof generation, version compatibility, and invalidation when transaction semantics change. The killer is annotation burden. If every workload requires an expert to handwrite its merged execution, the proposed automation has failed.

**The third flip separates durable state from immediate physical materialization.** Keep a canonical logical state represented by immutable indexed runs plus committed delta overlays. A batch records its final effects and required per-call outcomes. Background work merges overlays into read-oriented structures.

Readers must see a complete committed state: they either query the overlay correctly or wait for necessary materialization. Arbitrary future queries remain possible because logical data is preserved. Only unnecessary intermediate physical versions disappear; retained snapshots and declared change streams constrain coalescing.

With 64 updates to one record, maintenance of that record can fall from 64 operations to one. Total storage traffic will improve much less when receipts, new rows, or replication dominate. Expect **large reductions in hot-record maintenance, perhaps 1.1–2× overall** on suitable workloads.

Costs are read amplification, checkpoint work, retained history, and potentially worse read-tail latency. Frequent observations or CDC requirements can eliminate the opportunity. This component has low novelty and should remain conventional.

**The nearest prior art substantially narrows the claim.**

- **Beta** already compiles whole database application programs and specializes their execution. **Cozy** synthesizes data structures from operations and queries. Neither cited work supplies the proposed certified algebra for parallel evaluation of guarded ACID transaction batches. [Beta](https://arxiv.org/abs/1807.09887), [Cozy](https://homes.cs.washington.edu/~mernst/pubs/generalized-synthesis-icse2018.pdf)
- **IC3** uses static transaction analysis and runtime dependencies to expose serializable concurrency. Its contribution is constrained interleaving, rather than synthesizing compact compositions of transaction effects. [IC3](https://cs.nyu.edu/media/publications/TR2016-981.pdf)
- **Homeostasis** already derives predicates and partially evaluated transactions, allowing distributed execution while preserving observable behavior. Consequently, guarded semantic compilation and observational equivalence are established ideas. Its objective is reducing distributed coordination, rather than fixed-order parallel batch evaluation. [Homeostasis](https://www.cs.cornell.edu/~jnfoster/papers/homeostasis.pdf)
- **Doppel** parallelizes contended updates using split representations and reconciliation. Its split operations have restricted commutativity and return-value requirements; the proposed target includes ordered, noncommuting operations with individual responses. [Doppel](https://pdos.csail.mit.edu/papers/doppel:osdi14.pdf)
- **TransactionMerger**, submitted in January 2026, is the closest direct collision. It merges transactions, aggregates contended effects, and reconstructs per-request results—including allocated IDs. Difficult merged procedures remain developer-written. Therefore even “merge transactions while preserving outputs” is already occupied. [TransactionMerger](https://arxiv.org/html/2601.10596v1)
- **Parallel combining** already turns supplied parallel batch algorithms into concurrent data structures. It assumes the batch algorithm that this proposal would need to discover. [Parallel Combining](https://arxiv.org/abs/1710.07588)

The potentially defensible contribution is therefore **a characterization and synthesis method for useful, compact transaction-composition classes, with checked preservation of relational constraints and individual outcomes**. I have not established that this exact contribution is unprecedented. A collection of familiar components would not satisfy the novelty requirement.

**ACID requires an explicit kernel beneath these optimizations.** The following is a proposed correctness argument, not an implemented guarantee.

For isolation, start with strict two-phase locking over logical resources, held through durable commit. Resources include rows, index ranges, absent keys, unique values, and parent-reference guards. Every specialized and fallback path participates in the same protocol. Unknown footprints use conservative range or domain locks.

A batch holds the resources needed by its constituent transactions and proves equivalence to a serial sequence inside that protected interval. Other conflicting batches cannot interleave. This gives a serial order of batches, expandable into a serial order of individual transactions. Predicate protection prevents write skew and phantoms. Real-time precedence can also be respected by admitting and publishing batches accordingly.

For primary keys and uniqueness, maintain an authoritative mapping:

`(unique_index, canonical_value) → row_id`

Inserting or changing a value requires exclusive ownership of that logical resource. Competing inserts are evaluated in the batch’s serial order; losers receive the appropriate constraint failure. Canonicalization and collation belong to the versioned semantics. Hashes route requests; full values determine equality.

For referential integrity, child insertion protects the parent’s existence while atomically adding the child and reverse-reference entry. Parent deletion takes incompatible protection and checks the child range. Cascades execute atomically. Within a batch, constraints are checked at the boundaries required by their declared semantics—not merely against the batch’s final state.

For atomicity and durability, embedded mode uses one file containing append-only commit records and copy-on-write checkpoint extents:

1. Append framed canonical effects, outcomes, identifiers, and a commit footer covering their lengths and digests.
2. Successfully synchronize the file before acknowledging or exposing committed effects.
3. Recover only complete, validated committed groups.
4. Flush checkpoint contents before publishing a checksummed checkpoint root; reclaim older records only afterward.

Writes must avoid damaging previously committed data within the storage stack’s stated failure granularity. Checksums detect torn records; retained older state enables recovery. File creation also requires durable directory metadata.

An `fsync` failure stops the writer and leaves the transaction outcome **unknown**: data may have reached storage. Recovery resolves that outcome; a stable transaction identifier supports safe retries. Retrying `fsync` and assuming success repaired everything is insufficient. [Can Applications Recover from fsync Failures?](https://www.usenix.org/system/files/atc20-rebello.pdf)

These guarantees assume truthful successful flushes and a specified corruption boundary. Arbitrary loss of previously durable bytes cannot be repaired by single-file software alone. SQLite documents analogous storage assumptions. [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html)

At large scale, partition data and unique-value ownership across consensus-replicated groups. Cross-group transactions use replicated prepare/commit records and atomic commit, retaining locks until the decision is resolved. This preserves the same semantic kernel while adding network and availability costs. Same-value uniqueness races, global predicates, and heavily connected foreign keys remain coordination bottlenecks. Large volume scales through partitioned storage; throughput scales only where workload dependencies permit it.

**Three experiments should decide whether to proceed.**

1. **Composition hypothesis:** at least three useful guarded transaction families admit bounded summaries and achieve ≥3× execution throughput on 16 cores versus optimized serial batching, with identical per-call outcomes. First implement only the algebra and reference interpreter. Test quota allocation, conditional inventory, and mixed account operations across batches of 1–256. Measure summary size, work, and response correctness before adding persistence.

2. **Certification hypothesis:** a fixed rule library automatically admits at least four of six preregistered transaction families within ten minutes each, without workload-specific handwritten batch procedures. Mutate constraints, overflow behavior, and output semantics to test rejection. Failure means specialization remains artisanal.

3. **Materialization hypothesis:** with 10% reads, coalescing reduces hot-record maintenance by ≥8× while keeping read p99 below twice the eager implementation. Compare identical batching and durability settings, changing only materialization. Inject torn writes and flush failures at every persistence boundary.

Only after those pass should the prototype face SQLite with durable settings, PostgreSQL at `SERIALIZABLE`, and a manually specialized batched baseline. Use YCSB for storage controls and TPC-C-derived workloads for relational behavior; report throughput, latency, aborts, bytes written, and recovery correctness. Keep full TPC-C results distinct from modified workloads.

The first implementation should be the falsifier for compact composition. Its result determines whether there is a new engine concept worth building.