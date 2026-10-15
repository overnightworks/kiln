# Systematic prior-art review for kiln's three narrowed claims

This document is a systematic, protocol-first re-examination of prior art against the
three claims named in issue #8, superseding nothing in `docs/research/` (those files
stay unchanged) but extending them: every collision or partial overlap they already
named is re-checked here against its primary source, not repeated from memory, and
the search is widened with fresh queries aimed specifically at the three claims below.
`docs/concept.md` §15 and `docs/research/prior-art-report.md` remain the record of the
earlier, time-boxed pass; where this review's primary-source reading sharpens or
corrects that record (a venue, a mechanism detail, a quote), the correction is stated
explicitly in the relevant section.

**The three claims under test** (verbatim from the issue):

- **C1.** Checked automatic synthesis of compact ordered transaction summaries:
  transactions on contended state that do not commute, represented by summaries whose
  composition is associative and whose size and combine/apply costs do not grow with
  the batch, synthesized automatically and admitted only with a checked equivalence
  certificate, evaluated by parallel prefix while preserving every caller's response.
- **C2.** Parametric per-call footprint derivation: read/write sets derived at build
  time from transaction code as functions of the call's arguments (and at most one
  dependent read), driving admission under one logical-resource protocol.
- **C3.** A measurement of how much real application transaction code admits C2.

This document went through one independent review round after its first draft
(`28c42f9`). The review found ten blocking defects — a missing research lineage for
C1, several primary-source misreadings, two misreported numbers, and a protocol
section that claimed more rigor than it had evidence for — and one follow-up. An
independent verification against the primary sources confirmed, partly confirmed, or
corrected each finding; this revision resolves every finding exactly as verified,
widens §2 into the lineage the verification located, and adds the screening register
and confidence levels the verification required. Primary sources for every new or
corrected claim below were fetched and read in this revision, not carried over from
the verification's prose.

## 1. Search protocol

### 1.1 Sources actually reachable from this session

The tools available to this builder are a general web-search tool and a URL fetcher,
backed by local `curl`/`pdftotext` for PDFs the fetcher could not render (many
database-conference PDFs are scanned or font-subsetted and defeat naive text
extraction; `curl` with a browser user-agent plus local `pdftotext` recovered text
from most of them — see §4 for the ones it did not). **Neither the ACM Digital
Library, DBLP's own crawler/API, nor Google Scholar's UI was independently reachable
as a distinct index** — there is no authenticated or scriptable access to any of them
in this environment. In practice this means "search DBLP" and "search ACM DL" in this
protocol both resolve to targeted web-search queries that surface DBLP listing pages,
author pages, and ACM abstract pages, followed by locating a non-paywalled copy of
the actual paper (arXiv, an author's or university's homepage, a conference's own
open proceedings page, or a technical-report mirror) to read in full. This is a real
limitation on how systematic "systematic" can be here, named in §6, not papered over.

Sources used, in the order tried for each candidate: arXiv (abstract and PDF),
author/university-hosted PDF mirrors, conference open-access proceedings (USENIX,
VLDB.org, EuroSys, OPODIS/Dagstuhl), DBLP bibliographic pages (via search, for
venue/year ground truth), Microsoft Learn and Microsoft Research's own publication
pages (for production-API and Microsoft Research citations), and general web search
as the discovery layer standing in for ACM DL and Google Scholar. Semantic Scholar
was not queried through its own API in either pass; named as a gap in §6.

### 1.2 Venues and date range

SIGMOD, VLDB/PVLDB, CIDR, ICDE, OSDI, SOSP, EuroSys, NSDI, PLDI, POPL, OOPSLA, ASPLOS,
PPoPP, SPAA, 2010–2026, plus the named older foundations the issue itself points at:
O'Neil 1986 (escrow), Amato/Giannotti/Mainetto VLDB 1993, Shasha/Llirbat/Simon/Valduriez
("transaction chopping," ACM TODS 1995, following up on an SOSP precursor). Hits outside
these venues (OOPSLA/SPLASH, LCPC, ICSE, DISC, ECOOP, journal venues such as ACM TODS,
and arXiv preprints not yet at a venue) were not excluded when a targeted query
surfaced them as a plausible collision or partial overlap — the venue list bounds the
*search*, not the admissibility of what the search turns up, consistent with forward/
backward citation chasing from each known collision reaching outside the list.

### 1.3 Query strings run, by claim

**C1** (synthesis of compact ordered/non-commuting summaries, checked certificate,
parallel prefix, per-caller response preservation):
`automatic synthesis commutative batch procedure contended transactions database
program synthesis certificate`; `parallel prefix scan database transaction batch
evaluation associative summary OLTP`; `synthesizing batch algorithm concurrent data
structure parallel scan compiler database contention certificate verification`;
`automatic derivation guarded commutativity condition database operations synthesis
checker`; `"Mergeable Replicated Data Types" synthesize merge function non-commutative
operations`; `"Scalable Commutativity Rule" COMMUTER database transactions`; plus
forward citation chasing from TransactionMerger's and Doppel's own related-work
sections (both read in full, §2).

**C2** (parametric, build-time, per-call footprint derivation from code, one logical-
resource protocol, dependent reads):
`static analysis derive read set write set function of arguments compile time
transaction database 2023 2024 2025`; `database footprint extraction compiler build
time transaction arguments deterministic scheduling new system 2025 2026`; plus
direct re-reading of Calvin, Bohm, Aria, Caracal, IC3, Amato 1993 at the primary
source for the exact "declared vs. discovered vs. derived" wording (§3).

**C3** (measurement of real code's admission of C2):
`"are database system researchers making correct assumptions" 111 applications
Django TypeORM paper full citation venue`; `empirical study static inference read
write sets real world database transactions percentage coverage 2022 2023 2024`;
direct fetch and reading of the paper's methodology and figures (§4).

**Fix-pass queries** (same day, second session, resolving an independent review's
findings against primary sources — §1.6 logs every query below with its disposition):
direct fetch of Farzan & Nicolet's divide-and-conquer synthesis papers (PLDI 2017,
2019, 2021) and their mechanical predecessors (Mytkowicz et al., ASPLOS 2014; Raychev,
Musuvathi & Mytkowicz, SOSP 2015; Fedyukovich & Bodik, SYNT 2016) to open the
loop-parallelization lineage the first pass missed entirely (§2.4); direct fetch of
Bartoletti/Galletta/Murgia (arXiv 1905.04366), Conthereum (arXiv 2504.07280), and a
2025 Ethereum static-conflict-analysis paper (arXiv 2507.04357) to re-check the
blockchain static-key lineage against C2 (§3.2); re-fetch of TransactionMerger's full
text, Beta's §7, Homeostasis's Theorem 3.8, Amato's §6, and QURO's full VLDB PDF to
correct five primary-source misreadings (§2, §3); re-fetch of Nguyen/Niechen/
DeCarolis/Abadi's full methodology (§3.2/§4.1) to correct two misreported numbers
(§4); and re-fetch of the CoqSQL thesis (adam.chlipala.net) to replace a secondary
characterization with a primary-source classification (§2.5). Two further web
searches (Fisher & Ghuloum, PLDI 1994; Morita et al., PLDI 2007) confirmed
bibliographic facts for candidates this pass did not open — carried as unresolved,
§6.

### 1.4 Inclusion / exclusion rules

A work is **read in full** (primary text obtained and read, not just an abstract or a
secondary summary) if it is (a) one of the collisions the issue names as already known
and due for re-checking, or (b) surfaced by a targeted query as a plausible collision
or partial overlap for one of the three claims specifically. A work that surfaces only
as a tangential citation inside another paper's related-work section, and does not
itself independently satisfy (a) or (b), is named and classified at citation level
(bibliographic facts plus the citing paper's characterization of it), not deep-read —
flagged per entry below. This follows the instruction to prioritize depth on
collisions and partial overlaps over breadth of irrelevant hits; §6 states what this
cost in coverage.

### 1.5 Date of search

2026-10-15 (original pass and same-day fix pass).

### 1.6 Screening register

What was actually searched, independent of the narrative in §§2–4. For the general
web-search queries in §1.3 run by the original pass, no per-hit screening count was
preserved as a structured log — named as a gap, not papered over, in §6; the
"disposition" column for those rows lists what the queries promoted to a full read.
Every fix-pass query below was run this session. A direct fetch (rows 4–17) screens
exactly one document, so its count is exact; rows 18–19's web-search counts are
approximate — marked "~" because the hit list was skimmed for the one bibliographic
fact sought, not tallied — and are not claimed as exact.

| # | Query / target | Source | Hits screened | Disposition |
|---|---|---|---|---|
| 1 | Original pass's 3 C1 queries + citation-chasing from TransactionMerger/Doppel | general web search | not logged per-hit (§6) | promoted: TransactionMerger, Parallel Combining, Cozy, Beta, IC3, Callas, Homeostasis, O'Neil, Doppel, ESE, DBSP, Noria, MRDT/Quark, COMMUTER, Koskinen & Bansal |
| 2 | Original pass's 2 C2 queries + direct re-read of Calvin/Bohm/Aria/Caracal/IC3/Amato | general web search + direct fetch | not logged per-hit (§6) | promoted: Calvin, Bohm, Aria, Caracal, Amato, chopping, Block-STM, PWV, QURO, FAFO/ParaLyze, Afek/Korland/Zilberstein, CoqSQL |
| 3 | Original pass's C3 query (111-application study) | general web search + direct fetch | 1 target paper identified | promoted: Nguyen/Niechen/DeCarolis/Abadi, SIGMOD 2025 |
| 4 | Direct fetch: Farzan & Nicolet, "Automated Synthesis of Divide and Conquer Parallelism," arXiv 1701.08345 | arXiv | 1 (the paper) | promoted — anchors loop-parallelization lineage, §2.4 |
| 5 | Direct fetch: Mytkowicz, Musuvathi & Schulte, "Data-Parallel Finite-State Machines," ASPLOS 2014 | Microsoft Research | 1 | promoted — §2.4 |
| 6 | Direct fetch: Raychev, Musuvathi & Mytkowicz, "Parallelizing User-Defined Aggregations using Symbolic Execution" (Symple), SOSP 2015 | Microsoft Research (corrected URL after a first 404) | 1 | promoted — §2.4 |
| 7 | Direct fetch: Fedyukovich & Bodik, "Approaching Symbolic Parallelization by Synthesis of Recurrence Decompositions" (GRASSP), arXiv 1611.07629 | arXiv | 1 | promoted — §2.4 |
| 8 | Direct fetch: Farzan & Nicolet, PLDI 2019 ("Modular Divide-and-Conquer Parallelization of Nested Loops") and PLDI 2021 ("Phased Synthesis of Divide and Conquer Programs") | author homepage | 2 | promoted as lineage descendants — §2.4 |
| 9 | Direct fetch: Bartoletti, Galletta & Murgia, arXiv 1905.04366 | arXiv | 1 | promoted — §3.2, re-opens the declare/discover framing |
| 10 | Direct fetch: Conthereum, arXiv 2504.07280 | arXiv | 1 | promoted — §3.2 |
| 11 | Direct fetch: "Static Analysis for Detecting Transaction Conflicts in Ethereum Smart Contracts," arXiv 2507.04357 | arXiv (HTML) | 1 | promoted — §3.2, confirms per-function-pair granularity |
| 12 | Re-fetch: TransactionMerger full text | arXiv (HTML) | 1 | re-read — corrected §2.2 mechanism description |
| 13 | Re-fetch: Nguyen/Niechen/DeCarolis/Abadi, full PDF, §3.2/§4.1/Fig. 5 | author homepage | 1 | re-read — corrected §4.1 star thresholds and the 0.9% denominator |
| 14 | Re-fetch: QURO full VLDB PDF | vldb.org | 1 | re-read — corrected "deployed" to "implemented and benchmarked," §3.2 |
| 15 | Re-fetch: CoqSQL thesis (Suriyakarn, MIT MEng 2017) | adam.chlipala.net | 1 | promoted from "unverified" to classified — §2.5 |
| 16 | Re-fetch: Beta, §7/§7.4 | arXiv | 1 | re-read — corrected §2.5/§3.2 concurrency-exclusion claim |
| 17 | Re-fetch: Homeostasis, Theorem 3.8/§3.2 | arXiv | 1 | re-read — corrected §2.3 exclusion ground |
| 18 | Web search: "Fisher Ghuloum PLDI 1994 parallelizing complex scans" | web search | ~9 links screened | 1 bibliographic fact confirmed (title/venue/pages); not opened — unresolved, §6 |
| 19 | Web search: "Morita 2007 PLDI automatic parallelization" | web search | ~10 links screened | 1 bibliographic fact confirmed (title/venue/DOI); not opened — unresolved, §6 |
| 20 | Web search: "Farzan Nicolet Freund Yahav PLDI 2021" | web search | 10 links screened | corrected a misattribution: Freund and Yahav were PLDI'21 program chairs, not co-authors — the paper is Farzan & Nicolet alone |

## 2. Claim C1 — checked synthesis of compact non-commuting summaries

### 2.1 Three lineages

The first draft of this review split C1's territory into two lineages and concluded
"no collision." An independent review found this incomplete: a third lineage —
general-purpose loop- and aggregation-parallelization synthesis — supplies exactly
the mechanical core C1 needs (automatic, checked, associative, parallel-prefix-
evaluated summary synthesis), and the first draft's search terms never reached it
because they were all database-flavored. All three lineages are stated here against
primary sources read this pass.

- **Lineage A — database-side merging and escrow** (§2.2): manual declaration gives
  way to automatic reconciliation restricted to commutative updates, and separately to
  application-side hybrid merging of real OLTP transaction code that reconstructs each
  caller's individual response but needs a human-written merge and no checked
  certificate.
- **Lineage B — CRDT / automatic merge synthesis** (§2.3): automatically derives
  composable merge functions, now including for explicitly non-commuting operations,
  but trades serializability for convergence over a replica's final state, in a
  geo-replicated rather than single-node contended-batch setting.
- **Lineage C — loop-parallelization synthesis** (§2.4, newly opened this pass):
  automatically synthesizes an associative combine/join operator for a sequential
  loop or aggregation and checks its correctness by a generated proof or bounded
  verification, evaluated by a parallel-prefix scan or symbolic-summary composition —
  for a single sequential program, never for a database, a transaction, contention, or
  a per-caller response.

### 2.2 Lineage A — database-side merging and escrow

| Work | What it actually does (verified from primary text) | What it does not do, relative to C1 | Classification |
|---|---|---|---|
| **O'Neil escrow**, 1986, [full text read](https://www.cs.umb.edu/~poneil/EscrowTM.pdf) | A manually-specified, per-aggregate-type mechanism; the paper states the database must be made an "expert" about the specific update type (grepped for "automat*"/"static analysis"/"compile" in the full text: one incidental hit, unrelated). | No automatic derivation of anything — the escrow method is hand-declared per aggregate. This is kiln's own class-2 (typed reservation) mechanism, not class-3 (C1). | **Adjacent**. |
| **ESE `JetEscrowUpdate`**, Microsoft, [docs page read in full](https://learn.microsoft.com/en-us/windows/win32/extensible-storage-engine/jetescrowupdate-function) | A single explicit API a developer calls per column, after declaring that column `JET_bitColumnEscrowUpdate`; confirmed shipping, production, single-column atomic-add mechanism with versioned rollback. | Entirely manual and declared, one operation type, no synthesis, no batching, no parallel evaluation. | **Adjacent** — the one real production deployment in the escrow lineage, but of the class-2 mechanism, not C1. |
| **Doppel**, Narula/Cutler/Kohler/Morris, OSDI 2014, [full text read](https://pdos.csail.mit.edu/papers/doppel:osdi14.pdf) | Phase reconciliation: splits a contended value per core, reconciling split per-core states back into the global store. States explicitly: *"all modifications to a contended item must commute"* (line 53). | C1 is specifically about operations that do not commute — Doppel is, by its own stated restriction, outside C1's scope entirely. | **Adjacent** — a mechanism that explicitly excludes the hard case C1 is about. |
| **TransactionMerger**, Ren/Li/Wang, arXiv 2601.10596 (Jan 2026), [full text re-read](https://arxiv.org/html/2601.10596v1) | §IV-A ("Static Analysis to Find Merging Opportunities") is a **static analysis**, not sampling: it relies on **developer-supplied per-statement column annotations** ("relies on the developer to provide statement information, including which columns the statement reads or writes"), groups conflicting statements ("two statements access the same column with at least one write" / "if two groups have a conflict or have an overlap, we merge these two groups"), and argues correctness informally — *"the correctness of this re-ordering can be proved easily using the theory of serialization graph"* — no formal proof is produced, and no execution sampling or runtime isolation testing occurs anywhere in the paper. The merged procedure itself is still **hand-written** by the developer; the paper says automating that step "probably would require complex analysis" (line 424). Reports 2.65× (TPC-C) and 3.52× (Spree) throughput. | Not automatic synthesis of the merge (explicitly future work); no associative/parallel-prefix evaluation (one hand-coded aggregate rewrite); no checked equivalence certificate — the serialization-graph argument is informal prose, not a certificate checked against a reference semantics. *(Corrected from the first draft, which described this as execution sampling rather than static analysis — primary text has no sampling or testing step at all.)* | **Partial overlap** — closest match in this lineage for "reconstruct every caller's individual result from a merged contended operation," narrows C1 to *automatic* discovery of exactly this, which TransactionMerger does not attempt and does not formally certify. |
| **Parallel Combining**, Aksenov/Kuznetsov/Shalyto, arXiv 1710.07588 (OPODIS 2018), [full text read](https://arxiv.org/pdf/1710.07588) | Takes an already-written "parallel batched algorithm" as input and wraps it with an explicit combiner so concurrent callers' requests become one batch call. | Contains no mention of commutativity, associativity, or prefix scans at all (grepped the full text: zero hits); assumes the batch algorithm is supplied, never discovers or synthesizes one. | **Partial overlap**, narrow — adjacent on "batch instead of serialize," does not frame the problem in C1's terms. |
| **IC3**, Wang/Mu/Cui/Yi/Chen/Li, SIGMOD 2016, [TR version read in full](https://cs.nyu.edu/media/publications/TR2016-981.pdf) | Builds a static conflict graph over *pieces* of a fixed, known transaction workload, then tracks and enforces runtime dependencies between pieces, merging pieces to break deadlock-prone cycles. 434K TPS, single-warehouse TPC-C, 64 cores. | No synthesis of a compact associative summary; pieces execute one at a time with ordering enforced, not combined via a scan; no certificate — correctness rests on the SC-graph proof, not a checked equivalence. | **Partial overlap** — static+runtime hybrid reordering of a fixed workload, not a summary-composition system. |
| **Callas / MCC**, Xie et al., SOSP 2015, [full text read](https://www.cs.utexas.edu/~lorenzo/papers/Chao15Callas.pdf) | Statically and heuristically partitions the transaction workload into groups, each free to use a more aggressive concurrency-control mechanism as long as a declared isolation property composes across groups. 8.2× over MySQL Cluster on TPC-C. | Groups are a partitioning of the workload, not a synthesized summary of effects; no parallel-prefix evaluation; the in-group mechanism is a static+runtime hybrid, not synthesis. | **Partial overlap**, same shape as IC3. |

### 2.3 Lineage B — CRDT / automatic merge synthesis

| Work | What it actually does (verified from primary text) | What it does not do, relative to C1 | Classification |
|---|---|---|---|
| **Homeostasis**, Roy et al., SIGMOD 2015, arXiv 1403.2307, [full text re-read, Theorem 3.8](https://arxiv.org/pdf/1403.2307) | Fully automated program analysis derives per-site **treaties** — predicates bounding permissible inconsistency between sites — and resolves treaty-violating concurrent writers by a **voting algorithm that picks one serial winner**. Its own Theorem 3.8 establishes that treaty-governed execution produces the **same per-transaction logs and the same final database state as some serial execution**; §3.2 defines a log as the transaction's externally visible, i.e. per-caller, output. | Per-caller responses *are* preserved — the first draft's exclusion ground (weaker-than-serializability response loss) was wrong, and is corrected here. What it actually does not do: it synthesizes treaties and resolves conflicts by a serial vote, not by evaluating an associative combine/parallel-prefix scan over a batch — no bounded-size summary structure is composed at all, and the setting is geo-replicated, not single-node contended-batch. | **Partial overlap** — automatic derivation, narrowed by mechanism (vote-resolved treaties, not scan-evaluated summaries) and setting, not by response loss. |
| **MRDT / Quark**, Kaki/Priya/Sivaramakrishnan/Jagannathan, OOPSLA 2019, DOI 10.1145/3360580, [full text read](https://www.cs.purdue.edu/homes/suresh/papers/oopsla19-mrdt.pdf) | Automatically derives a **three-way merge function** for an ordinary sequential OCaml data type from a manually-supplied invertible relational specification, for operations that explicitly **do not commute** (its running example is a queue: "pop do not commute," line 482). The resulting semantics is explicitly *"not linearizable or serializable... but nonetheless well-defined (i.e., convergent)"* (line 190). | This is automatic synthesis of a three-way *state* merge over versioned data, against a version's lowest common ancestor — not synthesis of a bounded-size summary with a binary associative combine operator evaluated over a batch by a parallel-prefix scan, and not aimed at reconstructing a per-caller response. *(Restated from the first draft, which described this as "summary synthesis" — the mechanism is version-control-style three-way merge, not summary composition.)* | **Partial overlap** — the strongest automatic, non-commutativity-tolerant synthesis mechanism found, for a different target (convergent final state, not serializable per-caller responses) and a different composition shape (pairwise three-way merge, not scan-evaluated associative combine). |
| **The Scalable Commutativity Rule / COMMUTER**, Clements/Kaashoek/Zeldovich/Morris/Kohler, SOSP 2013, [bibliographic and abstract-level, not deep-read — §6](https://www.read.seas.harvard.edu/~kohler/pubs/clements13scalable.pdf) | Automatically derives, from a POSIX-style interface specification, the precise conditions under which a pair of operations commute (SAT-based search), to license lock-free kernel design. | Discovers *when* operations already commute; never synthesizes a summary for operations that do not. | **Adjacent** — the clearest instance of "automatic discovery of a concurrency condition from code" outside the database literature; should not be mistaken for a C1 collision. |
| **Koskinen & Bansal**, arXiv 2004.08450 (2020), [abstract-level only] | Automatically *proves* commutativity conditions for existing abstract-data-type implementations by reducing the problem to reachability. | Same relationship to C1 as COMMUTER: verifies commutativity of given code, does not synthesize summaries for non-commuting code. | **Adjacent**. |

### 2.4 Lineage C — loop-parallelization synthesis (opened this pass)

This lineage was entirely absent from the first draft. Its mechanism is exactly the
"automatic + checked + associative + bounded-cost + parallel-prefix-evaluated"
clause of C1 — supplied for general sequential loops and aggregations, not for
database transactions, contended shared state, or batches of concurrent callers at
all. Each primary text below was grepped in full for "database" and "transaction":
Farzan & Nicolet (2017) and Fedyukovich & Bodik have zero substantive hits (one
incidental citation-list match each); Mytkowicz et al. have zero; Symple has one
incidental related-work mention of SQL pattern-matching UDAs, not transactional
semantics.

| Work | What it actually does (verified from primary text) | What it does not do, relative to C1 | Classification |
|---|---|---|---|
| **Data-Parallel Finite-State Machines**, Mytkowicz/Musuvathi/Schulte, ASPLOS 2014, [full text read](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/asplos302-mytkowicz.pdf) | Parallelizes FSM computation with an associative "gather" operator in three phases (§3.4): a parallel first phase computes, per processor, the final state reached from every possible start state over its input chunk; a sequential second phase "computes the start states for each processor" from those arrays; a parallel third phase re-runs the base algorithm per chunk from its correct start state and calls the output function — "the φ function is only called in the third phase" (§3.4) — reconstructing per-position outputs there, not in the second phase. Up to 21× on 8 cores. | No synthesis (the FSM is given, fixed, and finite); no checked certificate of any kind; no database or transaction framing. | **Partial overlap** — supplies the associative-composition and parallel-prefix-evaluation half of C1's mechanism, mechanically, for a different (finite-state, non-synthesized) object. |
| **Symple**, Raychev/Musuvathi/Mytkowicz, SOSP 2015, [full text read](https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/143-raychev.pdf) | Automatically generates, via symbolic execution, a **symbolic summary** of a user-defined aggregation's effect as a function of unresolved loop-carried dependences; summaries compose associatively across input chunks to match the sequential semantics. §5.2 names two separate explosion-handling mechanisms, not one: if the path count on a *single* input exceeds a maximum, Symple "halts with a warning" that the UDA may not admit symbolic parallelism; separately, once the total symbolic-path count **across processed inputs** exceeds a bound ("currently set to 8"), a mapper "stores the current summary and restarts the symbolic execution from a fresh 'unknown' symbolic variable," so it "produces multiple summaries that have to [be] composed at the reducer" — trading parallelism for sequential efficiency, with "a fallback to no parallelization in the worst case" when the UDA has none. | No checked equivalence certificate (soundness/precision is a design property Symple assumes, not something it proves and checks per instance); output is one aggregate value per group, not a set of individual per-caller responses; no database/transaction framing. | **Partial overlap** — automatic symbolic-summary synthesis with associative composition, exactly C1's shape for UDAs; its explosion-handling trade-off (more, smaller summaries composed at the reducer, not an outright failure) is directly relevant to whether kiln's own summaries would stay bounded (§2.6). |
| **GRASSP**, Fedyukovich & Bodik, SYNT 2016 ("Approaching Symbolic Parallelization by Synthesis of Recurrence Decompositions"), [full text read](https://arxiv.org/pdf/1611.07629) | Synthesizes a `prefix`/`merge` decomposition for a sequential loop using SMT-based bounded model checking and CEGIS (built on Rosette): `prefix` identifies how segment boundaries must move, `merge` combines partial outputs; a candidate decomposition is **formally verified equivalent** to the original sequential program, bounded by segment count and length. | Verification is bounded (segment count/length), not an unbounded proof; no database/transaction framing anywhere in the text (zero hits for either term). | **Partial overlap** — automatic synthesis with a genuinely checked (if bounded) equivalence certificate, for general recurrence loops. |
| **Automated Synthesis of Divide and Conquer Parallelism**, Farzan & Nicolet, PLDI 2017, arXiv 1701.08345, [full text read](https://arxiv.org/pdf/1701.08345) | Synthesizes a `join` operator for a sequential loop via syntax-guided synthesis, restricted to loops with a **constant-size homomorphic extension** (the associative, constant-state accumulator C1 needs); §7, "Correctness of the Synthesized Programs," generates and checks the join's correctness as **Dafny proofs**, automatically, per synthesized instance. Worked examples span both order-sensitive, non-commuting computations (`mts`, maximum tail sum; `is-sorted`) and order-insensitive, commutative reductions (`sum`; second-smallest) — the synthesis mechanism and its Dafny-checked certificate apply to both. Corollary 6.5 proves that **not every** single-pass function has a constant-size homomorphic extension — boundedness is a property the algorithm detects, not one it guarantees for arbitrary input code. | No database, transaction, or per-caller-response framing anywhere in the text; targets one sequential program's loop body, not a batch of heterogeneous procedure calls; no notion of contended shared state between concurrent callers. | **Partial overlap** — the closest single match, across every lineage, to C1's "automatic + checked-by-generated-proof + constant-size + associative" clause, entirely outside the database literature. |
| **Descendants**: Farzan & Nicolet, PLDI 2019 ("Modular Divide-and-Conquer Parallelization of Nested Loops," [full text read](https://www.cs.toronto.edu/~azadeh/resources/papers/pldi19.pdf)) and PLDI 2021 ("Phased Synthesis of Divide and Conquer Programs," [full text read](https://www.cs.toronto.edu/~azadeh/resources/papers/pldi21.pdf) — program chairs Freund and Yahav, confirmed via DBLP and the paper's own byline to be the PLDI'21 chairs, not co-authors; corrected from the independent verification's attribution) | Generalize the 2017 mechanism to nested loops (2019) and to a three-phase split/solve/combine synthesis pipeline implemented in the tool Parsynt (2021), still for general sequential/recursive programs. The two descendants check correctness differently: PLDI 2019 keeps Dafny — "we use the same scheme as [11] to produce correctness proofs verified through Dafny" (§9) — while PLDI 2021 drops it for **bounded verification**: "the synthesis step ... is effectively reduced to a bounded verification of the already discovered ⊙" (§7.2), using Z3 and Rosette, not Dafny. | Same gap as the 2017 paper: no database, transaction, or contention framing. | **Adjacent/partial** — extend the lineage's generality, not its applicability to C1's database setting. |

### 2.5 Other adjacent systems

- **Cozy**, Loncaric/Ernst/Torlak, ICSE 2018, [full text read](https://homes.cs.washington.edu/~mernst/pubs/generalized-synthesis-icse2018.pdf). Synthesizes single-process, in-memory data structures from a query+update specification. No transactions, no concurrency, no contended shared state. **Adjacent.**
- **Beta**, arXiv 1807.09887, [full text re-read, §7](https://arxiv.org/pdf/1807.09887). The first draft read Beta's own line *"we cannot — and will not claim to — offer a design to address"* as excluding concurrency outright; re-reading in context, the quoted sentence refers to the whole space of concurrency-control *design directions* it declines to survey exhaustively, not to excluding the topic. §7 implements and benchmarks Beta-compiled TPC-C-style programs under record-level optimistic MVCC ("we chose optimistic MVCC [39] as the concurrency control algorithm... Figure 15 shows the throughput," §7). §7.4 separately *proposes*, without reporting an implementation or a benchmark of its own, moving to attribute-level OMVCC validation by precomputing each program's accessed/modified columns at compile time ("a better approach is to precompute these columns during compilation," §7.4). For C1 this remains **adjacent** — no summary synthesis of any kind is attempted, and the corrected description does not change that. For C2 it becomes relevant: see §3.2.
- **DBSP / Feldera**, arXiv 2203.16684, [full text read](https://arxiv.org/pdf/2203.16684). Z-sets form an abelian group under addition, making incremental view updates composable; treats transactions as an already-committed input stream and incrementalizes read-side query views from it. Genuine structural cousin of "associative composition," applied to a different object (query-result deltas) for a different purpose (read-side incrementalization, downstream of commit). **Adjacent.**
- **Noria**, Gjengset et al., OSDI 2018, [full text read](https://www.usenix.org/system/files/osdi18-gjengset.pdf). For canonical write-write contention (vote counts, "hotness" scores) moves aggregation into a dataflow so no transaction is needed, trading away serializability for eventual consistency rather than solving C1's trade. **Adjacent.**
- **CoqSQL**, Suriyakarn, MIT MEng thesis, 2017, [full text read, §1.3, §2.3–2.4](https://adam.chlipala.net/theses/sorawit.pdf), promoted this pass from "unverified" (the first draft could not obtain the primary text). "A Framework for Synthesizing Transactional Database Implementations in a Proof Assistant": §2.3–2.4 build a **predicated structure** (`PStruct`) from an invertible relational specification of a SQL-subset program, using a technique the thesis calls *transactional predication*, and mechanically **prove linearizability in Coq** for the synthesized implementation — but only from the **query plan** onward, not from the user's original SQL: "CoqSQL compiles the high-level SQL methods into naive query plans. This translation process is unverified" (§1.3). For the common workflow of writing SQL rather than a query plan directly, the checked certificate therefore starts after an unverified translation step. For the query-plan-to-implementation step itself, this is a genuine automatic-synthesis-plus-checked-certificate mechanism — the certificate (a machine-checked Coq proof) is at least as strong as C1's "checked equivalence certificate" requirement. It synthesizes a whole concurrent implementation from a specification, though, not a composable batch-summary for an already-written procedure, and has no parallel-prefix/scan evaluation over a batch of callers. **Classification: partial overlap for C1** — automatic synthesis plus a mechanically checked certificate for the query-plan-to-implementation step, for a different synthesis target (whole-implementation generation, not summary composition over a batch), with an unverified translation step ahead of it when the input is SQL.

### 2.6 Conclusion for C1

Two separate confidence questions apply, not one: whether the *mechanism* C1 names is
novel, and whether kiln's *specific combination* of that mechanism's clauses with a
database transaction batch is unoccupied. The first draft's single "medium" confidence
conflated them; both are restated here with their own evidence.

**C1 as a mechanism — confidence: low.** Automatic, checked synthesis of an
associative, bounded-size/cost summary, evaluated by a parallel-prefix scan, is
established prior art for general sequential loops and aggregations: Farzan & Nicolet
synthesize such join/combine operators and check them with automatically generated
Dafny proofs in PLDI 2017 and PLDI 2019 (§2.4); PLDI 2021 drops Dafny for bounded
verification via Rosette/Z3 instead (§2.4) — a weaker, bounded guarantee, not a third
instance of the same proof mechanism. Fedyukovich & Bodik (SYNT
2016) do the equivalent with bounded SMT verification; Symple (SOSP 2015) and
Mytkowicz et al. (ASPLOS 2014) supply the associative-composition and parallel-prefix-
evaluation machinery for symbolic and finite-state summaries respectively. None of
this is database-specific, and "can a checked, bounded, associative summary be
synthesized automatically" has a yes, outside the literature the first draft searched.

**The transaction-batch combination — confidence it remains unoccupied: medium.** No
source in either pass reaches the specific combination kiln needs: a **batch of
heterogeneous stored-procedure calls** (not one fixed loop body) on **contended keyed
database state** (not an array or a UDA's input), where **each caller's individual
response is reconstructed** under the kernel's serializable semantics (not a
convergent CRDT-style final state), admitted into an OLTP kernel by a **checked
certificate against the transaction's own semantics** (not a generic loop-equivalence
proof). Lineage A has real OLTP batches and per-caller responses but hand-written
merges and no certificate; lineage B is split on this point (§2.3) — Homeostasis
preserves per-caller responses but resolves non-commuting writes by a serial vote, not
a scan-evaluated summary, while MRDT/Quark's three-way merge targets a convergent
final state only, with no per-caller responses; lineage C has the
automatic+checked+associative+parallel-prefix mechanism itself but
no databases, no heterogeneous call batches, and no per-caller responses at all.
Medium rather than high because three active lineages are now visibly converging on
adjacent pieces of exactly this (lineage C's synthesis mechanism, 2014–2021; lineage
B's non-commutativity tolerance, 2015–2019), and a full 2023–2026 proceedings crawl
across all three lineages' venues was not completed (§6).

**Whether synthesized summaries stay bounded on real transaction code — an open
empirical question; no confidence is claimed either way.** The two systems closest to
"automatic synthesis of a bounded, associative summary" both carry an explicit
boundedness warning on exactly this dimension: Symple's cross-input bound of 8 trades
parallelism for sequential efficiency rather than guaranteeing a bounded summary for
every UDA — exceeding it makes a mapper restart from a fresh "unknown" variable and
accumulate multiple summaries to compose at the reducer, falling back to no
parallelization only in the worst case (§2.4); and Farzan & Nicolet's own Corollary 6.5
proves that not every single-pass function has a constant-size homomorphic extension —
boundedness is the exception their algorithm detects, not a property it guarantees.
Whether kiln's stored-procedure summaries stay bounded on real application code is
therefore untested by any existing synthesis result in either direction; it is a
property for E2/E3 to demonstrate empirically, not one literature search can settle.

**Narrowest honest restatement:** *Automatic, checked synthesis of a compact,
size/cost-bounded associative summary for a batch of heterogeneous stored-procedure
calls on contended keyed database state, admitted into an OLTP kernel by a certificate
checked against the batch's own serializable semantics, that reconstructs every
individual caller's response.* Every clause up to "database state" is independently
solved, for non-database loop/aggregation code, by lineage C; non-commutativity-
tolerant automatic synthesis exists for CRDT-style convergence (lineage B); batches of
real OLTP calls with per-caller response reconstruction exist for hand-written,
uncertified merges (lineage A). No source combines all of it, and whether the result
stays size/cost-bounded on real transaction code is untested by any of them.

## 3. Claim C2 — parametric per-call footprint derivation at build time

### 3.1 Known collisions, re-checked against the primary source

| Work | Direct quote / confirmed mechanism | Classification |
|---|---|---|
| **Calvin**, Thomson et al., SIGMOD 2012, [full text read](https://www.cs.princeton.edu/courses/archive/fall19/cos418/papers/calvin.pdf) | §3.2.1: for transactions whose read/write set cannot be known in advance, Calvin runs "an inexpensive, low-isolation, unreplicated, read-only reconnaissance query" against live data, then executes "using the reconnaissance query's results for its read/write set" — a runtime step against live data. | **Partial overlap** |
| **Bohm**, Faleiro & Abadi, PVLDB 2015, [full text read](https://arxiv.org/pdf/1412.2324) | "it is not necessary for transactions to pre-declare these... sets... Calvin proposes a speculative technique which predicts each transaction's read-/write-sets on the fly... Bohm can make use of this technique." | **Partial overlap** |
| **Aria**, Lu/Yu/Cao/Madden, PVLDB 2020, [full text read](https://www.cs.arizona.edu/~caolei/papers/Aria.pdf) | "earlier deterministic databases require that transaction read/write sets are known before execution"; Aria's fix is speculative execution against one snapshot, resolved deterministically afterward — discovery via one execution pass, not static derivation. | **Partial overlap** |
| **Caracal**, Qin/Demke Brown/Goel, SOSP 2021 (DBLP-confirmed), [full text read](https://www.eecg.utoronto.ca/~ashvin/publications/caracal.pdf) | Batches transactions into epochs and analyzes the batch *at runtime* to determine contention — per-epoch runtime analysis, not build-time static derivation. | **Partial overlap** |
| **Amato, Giannotti & Mainetto**, VLDB 1993, [full text re-read, §6](https://www.vldb.org/conf/1993/P405.PDF) | A correctness proof with two hand-worked examples; its own closing section proposes "the development of a prototype of the analyser" as next work, and no later publication by these authors or citing this one, found in either pass, reports that prototype built. The actual discriminator from C2 is mechanism, not implementation status: the read/write-set abstraction is tuned by an **abstract-domain granularity parameter approximating a snapshot of the whole schema's state space**, not derived as a pure function of one call's declared arguments — the paper analyzes relative to an approximated store, where C2 derives a footprint from argument values alone (plus at most one dependent read). *(Corrected from the first draft, which treated the 1993 future-work sentence as proof the mechanism was never implemented, and which added "typed reservations" and "deployed" as exclusion grounds not present in C2's own definition.)* | **Partial overlap**, the strongest shape-level match on "derive from code, feed a scheduler," excluded by its abstract-domain-over-a-store mechanism rather than by implementation status. |
| **Transaction chopping**, Shasha/Llirbat/Simon/Valduriez, ACM TODS 20(3), 1995, DOI 10.1145/211414.211427 — full text not independently obtainable (paywalled, 403 on two mirrors; §6); characterized via a short author-written encyclopedia summary and two citing papers' own descriptions (both read in full) | Builds a static conflict graph over *pieces* of a fixed, already-known set of transaction programs — a transformation of known code, not a build-time derivation of footprints as a function of a call's arguments for a general admission protocol. | **Partial overlap**, access-limited. |
| **Block-STM**, arXiv 2203.06871 (carried over, primary-source quote unchanged) | "every transaction records its own read-set and write-set during (speculative) execution." | **Partial overlap**, carried over. |
| **PWV**, Faleiro/Abadi/Hellerstein, PVLDB 2017, DOI 10.14778/3055540.3055546 — re-fetch attempted and failed again (NSF Public Access mirror timeout, §6); carried over from the first draft's primary-source quote | Restricts *when* a transaction may abort to make writes visible early; not a footprint-derivation mechanism. | **Adjacent**, carried over. |

Every deterministic database re-read this pass resolves an unknown footprint at
*runtime* — a reconnaissance query, a snapshot-and-reconcile pass, or a per-epoch
batch analysis — never by static derivation from source code; this remains true after
re-checking each primary text. What the first draft overstated was turning that
observation into a strict "declared or discovered, nothing else" dichotomy: §3.2
below re-opens it with mechanisms found this pass that are static and code-derived,
located outside the OLTP lineage entirely — Bartoletti et al.'s abstract-model key
approximation is per-call-instance, while Conthereum's implemented, running
mechanism works on pairs of functions instead, not per-call-instance.

### 3.2 New partial overlaps found by the fresh search

- **QURO**, Yan & Cheung, VLDB 2016, [full VLDB PDF obtained and read this pass](https://www.vldb.org/pvldb/vol9/p444-yan.pdf) — corrected from the first draft's project-page characterization, and from an over-read "deployed" that the paper's text does not support. A build-time compiler that statically analyzes real OLTP application transaction code (C/C++ with embedded ODBC calls) and reorders statements to reduce lock-hold time, up to 6.53× throughput and 85% latency reduction. The paper states plainly: *"we implemented a prototype of QURO"* (§1) and *"our current prototype is built on Clang"* (§3); the one "deploys" in the text ("QURO deploys the instrumented version using the same settings as the original application," §3) describes deploying its own profiling build for benchmarking, not a production rollout. **Classification: partial overlap** for "build-time static analysis of real transaction code" — implemented and benchmarked, not deployed in production; the output is a within-transaction statement reordering, not a cross-transaction footprint export, so it does not touch C2's admission-protocol half at all.
- **Beta**, arXiv 1807.09887, §7.4 (re-read this pass, §2.5) — *proposes*, without reporting an implementation or benchmark, precomputing the accessed/modified columns of each compiled OLTP program at compile time to drive attribute-level OMVCC validation ("a better approach is to precompute these columns during compilation," §7.4; its §7 experiments evaluate record-level OMVCC instead, §2.5). Even taken as implemented it would be a build-time static footprint at the wrong granularity for C2: per-*program*, not per-call-argument, and feeding a single concurrency-control mechanism rather than C2's one logical-resource admission protocol. **Classification: adjacent** for C2.
- **Bartoletti, Galletta & Murgia**, arXiv 1905.04366, [full text read, §2, Definition 7, and p.10 Example 6](https://arxiv.org/pdf/1905.04366) — the most materially relevant new finding for C2, inside an abstraction, not on deployed Ethereum, and conditional rather than demonstrated. Their model "abstract[s] from the actual smart contracts language" (§2): a contract is a set of functions with an unspecified body semantics, not EVM bytecode. Strong swappability itself "abstracts from the actual static analysis used to obtain the safe approximations: it is sufficient that such an analysis exists" (§3, before Definition 7) — Definition 7 parameterizes the result over *any* W and R satisfying a safe-approximation property; the paper assumes such an analysis exists rather than constructing or proving one. Example 6 hand-derives concrete key sets such as `{A.balance, C.balance, C.k1}` for one specific call with a concrete sender `A` — one worked instantiation of what an assumed analysis's output looks like, per transaction instance, not a per-function or per-contract set, and not a demonstrated general extractor. Applying it to real Ethereum is explicitly unfinished work: "Applying our approach to Ethereum would require a static analysis of read/written keys at the level of EVM bytecode[; the only tool implementing such an analysis] ... make[s] ES-ETH not directly usable to the purposes of our work" (§6). It targets a pairwise swappability test for a scheduler, not a general OLTP admission protocol, with no one-hop dependent-read carve-out or application-code setting. **Classification: partial overlap** — a conditional framework plus one hand-worked instantiation, not a demonstrated per-call extractor; not yet for EVM bytecode or OLTP code.
- **Conthereum**, Zareh Chahoki/Herlihy/Roveri, arXiv 2504.07280, [full text read, §III-B](https://arxiv.org/pdf/2504.07280) — "statically analyzes Solidity code to construct a repository of potential conflicts between pairs of public and external functions callable by Ethereum transactions" (§III-B): **function-pair** granularity, not per-call-instance — coarser than Bartoletti's approximation, and a different mechanism, not an operationalization of it. Genuinely implemented and running in a real multi-core scheduler, not only a correctness test. Still blockchain-specific. **Classification: partial overlap.**
- **"Static Analysis for Detecting Transaction Conflicts in Ethereum Smart Contracts,"** Zareh Chahoki & Roveri, arXiv 2507.04357 — the conflict-detection method Conthereum cites and operationalizes as its own static-analysis step (Conthereum's reference [16], same first author); extracts conflicts between **function pairs**, the same granularity as Conthereum's repository and coarser than Bartoletti's per-call-argument approximation. **Classification: adjacent** (per-function-pair, not per-instance).
- **FAFO / ParaLyze**, arXiv 2507.10757 (2025), blockchain/EVM setting. Determines an approximate read/write set from node-supplied metadata or from simulation — no static-analysis-of-code path. **Classification: adjacent.**
- **Afek, Korland & Zilberstein**, LCPC 2010/2011, "Lowering STM Overhead with Static Analysis." Compile-time data-flow analysis that removes redundant STM instrumentation — a genuine build-time static analysis, but of STM bookkeeping overhead in a general in-process program, not of a database transaction's footprint. **Classification: adjacent.**
- **CoqSQL**, Suriyakarn thesis, [full text read this pass, §1.2, §4.3](https://adam.chlipala.net/theses/sorawit.pdf) — classified for C1 in §2.5; for C2 the first draft's exclusion ground does not hold. "A predicate is parameterized by the schema on which the test is performed" (§4.3) is a fact about the Coq **type** of a predicate (which schema it type-checks against), not about whether its runtime value depends on a call's arguments — it can: the bookstore example's `placeOrder id isbn` method builds the predicate `VAR("b.Isbn") == VAL(isbn)` directly from the call argument `isbn` (§1.2). CoqSQL's predicates are therefore call-argument-parametric, the same axis C2 needs; what keeps this **adjacent** rather than a stronger overlap is scope, not parameterization — CoqSQL synthesizes a whole implementation from a specification rather than deriving a footprint for already-written procedure code, and drives none of C2's specific arrangement (an argument-derived footprint feeding a separate admission decision). That narrow absence is not an absence of conflict handling: transactional predication itself, the technique CoqSQL's `PStruct` builds on, resolves conflicts directly — under optimistic concurrency control "an operation may get aborted when a conflict arises" (§2.4) — it is simply not C2's footprint-to-admission shape. **Classification: adjacent** for C2.

### 3.3 Conclusion for C2

**No collision**, and the margin is narrower than the first draft's "medium-high"
claimed, for two reasons surfaced this pass. First, Amato's (1993) exclusion rests on
the wrong ground in the first draft — a 1993 future-work sentence cannot prove an
analyzer was never built, and "typed reservations" and "deployed" are requirements C2
itself does not state (C2 names only "one logical-resource protocol," opening
paragraph); the honest basis for exclusion is that Amato's approximation is tuned
relative to an abstract snapshot of the whole schema's state space, not derived purely
from a call's own arguments. Second, and more materially, Bartoletti, Galletta & Murgia
(2019) assume, and illustrate with one hand-worked instance, exactly the shape C2's
build-time, per-call, code-derived half requires — a conditional framework, not a
demonstrated extractor, and inside their own abstraction, which "abstract[s] from the
actual smart contracts language" (§2), not on deployed Ethereum, where the paper itself says the analysis
would still need to be done (§6, §3.2). Conthereum (2025) operationalizes a
*different*, coarser mechanism into a real scheduler — conflicts "between pairs of
public and external functions" (§III-B), function-pair granularity, not Bartoletti's
per-call-instance approximation. The "declared or discovered, nothing else" dichotomy
the first draft used to exclude every deterministic-database system no longer holds as
a blanket claim once the blockchain lineage is in scope; it holds within the
deterministic-OLTP lineage, and even the blockchain lineage's one implemented, running
mechanism (Conthereum) is function-pair, not per-call-argument.

**Confidence: medium** (down from the first draft's medium-high), reflecting
Bartoletti et al.'s per-instance static key approximation — the strongest single data
point for "can a per-call, code-derived footprint be statically approximated at all" —
but, read precisely, the paper *assumes* rather than demonstrates that a safe
key-extraction analysis exists ("it is sufficient that such an analysis exists,"
§3/Definition 7) and illustrates that assumption with one hand-worked instance
(Example 6), not a general extractor it builds or proves; neither form is on deployed
EVM bytecode or OLTP source. What C2 needs that is not yet found anywhere: the
per-call, static, code-derived footprint mechanism *transplanted* into ordinary OLTP
application source code, driving admission under one logical-resource protocol, with
a one-hop carve-out for dependent reads resolved through a statically-known foreign
key. Medium, not low, because the one implemented blockchain-setting mechanism
(Conthereum) is coarser (function-pair) than C2 needs, and Bartoletti's framework is
unimplemented outside its own abstract model and conditional on an assumed analysis —
the transplant is not mechanical either way. Medium, not medium-high, because "can a
per-call, code-derived footprint be statically approximated at all" — most of what
made the first draft's dichotomy feel airtight — is no longer open in the abstract as
a blanket "no," but the honest form of that "yes" is an assumed existence illustrated
once, not a confirmed, demonstrated mechanism; implemented or not, it is not yet at
kiln's setting or granularity. **This correction does not move C2 off medium**: the
"medium, not low" reasoning already rested on Bartoletti being unimplemented outside
its abstract model, and sharpening "unimplemented" to "assumed and illustrated once,
not demonstrated" narrows the same gap further without crossing into low.

**Narrowest honest restatement:** unchanged in substance from `docs/concept.md` §15 —
*static, build-time derivation of a transaction's read/write footprint as a pure
function of its declared call arguments (plus at most one dependent read resolved
through a statically-known foreign key), from compiled OLTP application code, feeding
one logical-resource admission protocol.* This review found the per-call-instance
static-derivation half of that sentence assumed to exist and illustrated by one
hand-worked instance, inside an abstract model that itself abstracts from the
contract language (Bartoletti et al., 2019) — not demonstrated by a general
extractor, and not yet carried to EVM bytecode or OLTP code; the one blockchain-setting
system that is implemented and running (Conthereum, 2025) works at a coarser,
function-pair granularity instead.
No source occupies the whole sentence, including the OLTP/application-code setting and
the per-call-argument granularity together.

## 4. Claim C3 — measuring how much real code admits C2

### 4.1 The one directly relevant measurement, read in full

**Nguyen, Niechen, DeCarolis & Abadi, "Are Database System Researchers Making Correct
Assumptions About Transaction Workloads?", SIGMOD 2025 (Proc. ACM Manag. Data 3(3),
Article 131), DOI 10.1145/3725268 — [full text re-read this pass](http://www.cs.umd.edu/~abadi/papers/database-workloads.pdf).**
This is the "111-application study" `docs/experiments.md` already cites. Re-reading
the methodology and Fig. 5 directly (not the figures as already quoted) corrects two
numbers the first draft got wrong:

- **Corpus and star thresholds, corrected:** 78 Django applications (≥1000 GitHub
  stars) **and 33 TypeORM applications (≥300 stars, not ≥500** as the first draft
  stated — confirmed from the paper's own text, §3.2: "Django repositories were
  required to have at least 1000 stars, and TypeORM repositories were required to have
  at least 300 stars"), over 30,000+ transactions.
- **The 0.9% figure, corrected:** "Attribute dependencies only exist in (33/111 ≈ 30%)
  of the applications, averaging 0.9% of transactions per application **among the 33
  applications**" (confirmed verbatim) — the first draft's phrasing left it ambiguous
  whether 0.9% averaged over all 111 applications or over the 33 that exhibit the
  dependency at all; it is the latter, a materially higher per-affected-application
  rate than the ambiguous reading implied.
- The 58%/90% CDA-Based figure and the 26.9% CDA-Mismatch figure, both already quoted
  by `docs/experiments.md`, are confirmed accurate against the primary text and
  unchanged by this correction.
- **The Predicate and CDA-Based access-set definitions (Def. 4.2–4.4) are themselves a
  static, value-parametric footprint notion**, not merely a measurement category with
  no mechanism behind it: each is a set of triples (relation, predicate, read/write)
  evaluated against concrete attribute values extracted statically from the query
  code. This is a genuine, if coarser, partial overlap for C2's "derive a footprint
  statically" half (cross-referenced at §3.2) — the first draft's blanket "cannot even
  be a partial overlap for C2" was too strong. Their fallback scenarios (Complex
  Predicates, Attribute Dependency, CDA Mismatch, Non-equality Predicates) require
  *some* fallback when the access set cannot be read off the code, but the paper
  leaves the mechanism to the implementer rather than forcing a dynamic one: "[o]ne
  fallback approach is to use the Relation definition for the problematic
  transactions," itself static — a conservative, whole-relation approximation — "[a]n
  alternative approach is to issue a reconnaissance query" that partially executes the
  transaction (§4.1). The paper proposes no admission protocol and does not drive
  scheduling decisions from these footprints — it only measures whether today's
  systems' existing inferability assumptions hold.
- The paper's **Attribute Dependency** category remains, as the first draft noted, a
  strict uniform fallback that draws no distinction between kiln's statically-known
  "one-hop" dependent read and an unresolvable one — confirming `docs/experiments.md`'s
  own statement that this study's binary split cannot answer what E1's one-hop
  category measures.

**Classification: partial overlap for C3** on the measurement question, and a **minor
partial overlap for C2's static-derivation half** at the relation/predicate
granularity (coarser than C2's per-call-argument granularity). It does not and cannot
settle either claim.

### 4.2 Other measurements sought and not found

No other empirical study was found, in either pass, that measures static read/write-
set or footprint inferability on a corpus of real application transaction code, under
any category scheme, for any date range. Amato et al.'s two hand-worked examples and
IC3's single TPC-C throughput number are not corpus measurements. No TPC-C-focused,
non-ORM, or enterprise-codebase replication of the Abadi et al. study was found.

### 4.3 Conclusion for C3

C3 names a measurement to run, not a question literature search settles directly —
but it still earns a confidence, not "not applicable": the honest question is how
confident this review is that no existing measurement already answers it, and how
confident it is that the one close measurement found is a usable proxy.

**Confidence: medium** that no existing measurement already settles C3 — not high,
given this search's own stated gaps (§6): no independent ACM Digital Library, DBLP
crawler/API, or Google Scholar access in either pass, no full 2023–2026
proceedings-by-year crawl across the named venues, and Semantic Scholar never queried
through its own interface. Those gaps mean an existing replication cannot be ruled out
with "high" confidence. The Abadi et al. (2025) study is the only close measurement
found across both passes, and it answers a related but coarser, binary (inferable/
not), narrower-corpus (popular open-source Django/TypeORM web applications only)
question that cannot distinguish kiln's statically-known one-hop case from an
unresolvable dependent read; no replication on TPC-C-style, enterprise, or non-ORM
codebases was found in either pass.

**Confidence: medium** that the Abadi et al. figures are a usable order-of-magnitude
proxy for what E1 will find. This is a hypothesis about transfer between populations,
not a validated one — no study was found, in either pass, that measures the gap
between an open-source ORM corpus and an operator's own enterprise transaction mix
directly. Medium, not high, because the corpus and the category scheme both differ
from kiln's target population in directions this review cannot resolve from the paper
alone — a narrower, ORM-driven, open-source corpus could read either more or less
statically admissible than kiln's own target workloads, and the paper's coarser
categories (no one-hop carve-out) could over- or under-state the one-hop share
specifically.

## 5. Summary of new collisions / partial overlaps not in the issue's original list

- **The loop-parallelization synthesis lineage** (Mytkowicz et al., ASPLOS 2014;
  Symple, SOSP 2015; GRASSP, SYNT 2016; Farzan & Nicolet, PLDI 2017/2019/2021) — the
  most important correction in this revision: a third, previously unsearched lineage
  that supplies C1's automatic+checked+associative+parallel-prefix mechanism in full,
  for general sequential loops rather than database transactions (§2.4, §2.6).
- **Mergeable Replicated Data Types / Quark** (OOPSLA 2019) — automatic synthesis of a
  three-way merge for explicitly non-commuting operations, restated this pass as
  state-merge synthesis rather than summary synthesis (§2.3).
- **Bartoletti, Galletta & Murgia** (arXiv 1905.04366, 2019) assumes a per-call-instance
  key-extraction analysis exists and hand-illustrates it with one example, inside an
  abstraction that "abstract[s] from the actual smart contracts language," neither
  demonstrated as a general extractor nor yet applied to EVM bytecode; **Conthereum** (arXiv
  2504.07280, 2025) and the 2025 Ethereum static-conflict-analysis paper it cites and
  operationalizes (arXiv 2507.04357, same first author) instead derive conflicts
  between **function pairs** — a coarser, implemented, blockchain-specific lineage,
  materially narrowing C2's confidence (§3.2, §3.3).
- **The Scalable Commutativity Rule / COMMUTER** (SOSP 2013) and **Koskinen & Bansal**
  (arXiv 2004.08450, 2020) — automatic discovery of commutativity conditions from code,
  adjacent to C1's "automatic" angle but solving the opposite problem.
- **QURO** (VLDB 2016) — the clearest real-world instance of build-time static
  analysis of actual OLTP application code found for C2, at the wrong granularity;
  corrected this pass from "deployed" to "implemented and benchmarked" (§3.2).
  **Beta** (arXiv 1807.09887) — corrected this pass from "excludes concurrency
  entirely" to "implements record-level OMVCC and separately *proposes*, without
  reporting an implementation, precomputing columns at compile time," newly relevant
  to C2 at the wrong (per-program) granularity.
- **Strife** (SIGMOD 2020, Prasaad/Cheung/Suciu) — runtime, sampling-based clustering
  of transactions into conflict-free batches; adjacent to both C1 and C2 but neither
  static/build-time nor a synthesized summary.
- **FAFO/ParaLyze** (arXiv 2507.10757, 2025) and **Afek/Korland/Zilberstein** (LCPC
  2010/2011) — adjacent, as in the first draft.
- **CoqSQL** (MIT thesis, Suriyakarn, 2017) — promoted this pass from "unverified" to
  classified using primary text: partial overlap for C1 (synthesis plus a mechanically
  Coq-checked certificate, but only from the query plan onward — the SQL-to-query-plan
  translation is itself unverified, §1.3); adjacent for C2 (its predicates are
  call-argument-parametric, e.g. `placeOrder`'s `isbn`, but it synthesizes a whole
  implementation and drives none of C2's specific footprint-to-admission arrangement —
  transactional predication itself already resolves conflicts, by aborting) (§2.5, §3.2).
- Corrected bibliographic facts carried forward: **Caracal is SOSP 2021** (unchanged
  from the first draft); the Abadi et al. **TypeORM threshold is ≥300 stars** and the
  **0.9% Attribute Dependency figure averages over the 33 affected applications**, not
  the whole 111-application corpus or ≥500 stars as the first draft had both (§4.1);
  the PLDI 2021 divide-and-conquer paper's program chairs (Freund, Yahav) were
  corrected to not be co-authors.

## 6. Limits of this search

- **No independent ACM Digital Library, DBLP crawler/API, or Google Scholar access, in
  either pass.** The single largest structural limit on how systematic this review can
  claim to be.
- **The original pass's general web-search queries were not logged with per-hit
  screening counts** — §1.6's register states this plainly for rows 1–3 rather than
  retrofitting invented numbers onto a session this review did not run. Every work
  this pass opened and read is classified individually in §§2–4, not pooled into a
  query-level tally. Screened-but-not-opened hits are counted, not listed one by one,
  in §1.6: exact for a direct fetch (rows 4–17, one document each) and approximate —
  marked "~", not claimed as exact — for the two bibliographic web searches that
  confirmed a fact without opening the paper (rows 18–19).
- **Two primary sources could not be obtained despite repeated attempts, in either
  pass:** Shasha/Llirbat/Simon/Valduriez's full ACM TODS 1995 text (two mirrors, both
  403) and Faleiro/Abadi/Hellerstein's PWV, PVLDB 2017 (NSF Public Access mirror
  timeout, twice).
- **Two candidates surfaced by bibliographic search this pass were not opened and are
  carried as unresolved, not classified:** Fisher & Ghuloum, "Parallelizing Complex
  Scans and Reductions," PLDI 1994, pp. 135–146 — a plausible, earlier ancestor of the
  loop-parallelization lineage (§2.4), confirmed to exist and its title/venue/pages
  confirmed, but its primary text was not located within this session's reach; and
  Morita/Morihata/Matsuzaki/Hu/Takeichi, "Automatic Inversion Generates Divide-and-
  Conquer Parallel Programs," PLDI 2007, pp. 146–155, DOI 10.1145/1250734.1250752 — a
  plausible intermediate step in the same lineage, same status. Neither is scored
  above; a future pass that opens them may narrow lineage C's history further, but
  neither changes §2.6's confidences, which already treat lineage C's mechanism as
  established.
- **Semantic Scholar was not queried through its own interface or API**, in either
  pass — a targeted citation-graph pass might surface further descendants of lineage C
  or the blockchain static-key lineage that general web search missed.
- **No full SIGMOD/VLDB/CIDR/OSDI/SOSP/EuroSys/NSDI/PLDI/POPL/OOPSLA/ASPLOS/PPoPP/SPAA
  proceedings-by-year crawl was run for 2023–2026**, beyond the targeted queries in
  §1.3, in either pass — the natural next increment if the operator wants higher
  confidence than this revision's "medium" on C1's transaction-batch combination or C2.
- **Not re-verified this pass, carried over from the existing notes unchanged** (not
  flagged by the independent review, not independently re-surfaced by any fresh
  query): Katara, Hamsaz, Hampa, IPA, ECROs, CISE/CISE3, Bailis et al.'s I-confluence
  paper, and the chopping-lineage follow-ons (Dias/Lourenço/Preguiça StarTM). Their
  characterization in
  `docs/research/notes/invariant_driven_coordination.md` is accepted as-is.
