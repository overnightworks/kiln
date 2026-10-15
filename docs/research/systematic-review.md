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
limitation on how systematic "systematic" can be here, named in §4, not papered over.

Sources used, in the order tried for each candidate: arXiv (abstract and PDF),
author/university-hosted PDF mirrors, conference open-access proceedings (USENIX,
VLDB.org, EuroSys, OPODIS/Dagstuhl), DBLP bibliographic pages (via search, for
venue/year ground truth), Microsoft Learn (for the one production-API citation), and
general web search as the discovery layer standing in for ACM DL and Google Scholar.
Semantic Scholar was not queried through its own API; its indexed content overlaps
heavily with what general web search already surfaces, and no query in this pass was
run against it directly — named as a gap in §4.

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

### 1.4 Inclusion / exclusion rules

A work is **read in full** (primary text obtained and read, not just an abstract or a
secondary summary) if it is (a) one of the collisions the issue names as already known
and due for re-checking, or (b) surfaced by a targeted query as a plausible collision
or partial overlap for one of the three claims specifically. A work that surfaces only
as a tangential citation inside another paper's related-work section, and does not
itself independently satisfy (a) or (b), is named and classified at citation level
(bibliographic facts plus the citing paper's characterization of it), not deep-read —
flagged per entry below. This follows the instruction to prioritize depth on
collisions and partial overlaps over breadth of irrelevant hits; §4 states what this
cost in coverage.

### 1.5 Date of search

2026-10-15 (same day as this item's pull and dispatch).

## 2. Claim C1 — checked synthesis of compact non-commuting summaries

### 2.1 Known collisions, re-checked against the primary source

| Work | What it actually does (verified from primary text) | What it does not do, relative to C1 | Classification |
|---|---|---|---|
| **TransactionMerger**, Ren/Li/Wang, arXiv 2601.10596 (Jan 2026), [full text read](https://arxiv.org/html/2601.10596v1) | A middleware plus a static-analysis *detector* that finds merge opportunities; the merged procedure is then **hand-written** by the application developer. The paper's own increment-by-one example is explicit: merging "increment by one" into "increment by n" and reconstructing each caller's allocated ID (`old_id+1`, `old_id+2`, …) is written by the developer, and the paper says outright: *"determining how to do this automatically probably would require complex analysis"* (line 424). Reports 2.65× (TPC-C) and 3.52× (Spree) throughput. | Not automatic synthesis (explicitly named as future, harder work); no associative/parallel-prefix evaluation (one hand-coded aggregate rewrite, not a composable summary structure); no checked equivalence certificate (correctness is argued by the developer and spot-checked by the isolation-violation detector, not proved against a reference semantics). | **Partial overlap** — closest match for "reconstruct every caller's individual result from a merged contended operation," narrows C1 to *automatic* discovery of exactly this, which TransactionMerger does not attempt. |
| **Parallel Combining**, Aksenov/Kuznetsov/Shalyto, arXiv 1710.07588 (OPODIS 2018), [full text read](https://arxiv.org/pdf/1710.07588) | Takes an already-written "parallel batched algorithm" as input and wraps it with an explicit combiner so concurrent callers' requests become one batch call; evaluated on a read-dominated graph structure and a batched priority queue. | The paper contains **no mention of commutativity, associativity, or prefix scans at all** (grepped the full text; zero hits) — it is a request-batching/combiner technique, not a summary-composition algebra, and it explicitly assumes the batch algorithm is supplied, never discovers or synthesizes one. | **Partial overlap**, narrower than previously characterized — it is adjacent on the "batch instead of serialize" idea but does not even frame the problem in C1's terms. |
| **Cozy**, Loncaric/Ernst/Torlak, ICSE 2018, [full text read](https://homes.cs.washington.edu/~mernst/pubs/generalized-synthesis-icse2018.pdf) | Synthesizes single-process, in-memory data structures (supporting subsets/aggregations over multiple related collections) from a query+update specification, via enumerative synthesis plus incrementalization. | No transactions, no concurrency, no contended shared state, no ACID — it synthesizes a sequential data structure for one program's internal state. | **Adjacent**, confirmed — not a collision at any granularity relevant to C1. |
| **Beta**, arXiv 1807.09887, [full text read](https://arxiv.org/pdf/1807.09887) | Compiles and specializes whole database application programs (TPC-C et al.) via partial evaluation and record-structure specialization, in the main-memory, single-partition, single-core, sequential case. | The paper says this itself: *"we first focus on... single-core case where programs are... run in sequence... we later discuss the concurrency aspects... We cannot – and will not claim to – offer a design to address"* concurrency control (lines 287–315). No summary synthesis of any kind is attempted. | **Adjacent** — confirms, from the primary text, the existing note's characterization that concurrency is explicitly out of scope. |
| **IC3**, Wang/Mu/Cui/Yi/Chen/Li, SIGMOD 2016, [TR version read in full](https://cs.nyu.edu/media/publications/TR2016-981.pdf) | Builds a static conflict graph (SC-graph) over *pieces* of a fixed, known transaction workload, then tracks and enforces only direct runtime dependencies between pieces, merging pieces to break deadlock-prone cycles. Confirmed 434K TPS, single-warehouse TPC-C, 64 cores. | No synthesis of a compact associative summary for a batch; pieces still execute one at a time with ordering enforced, not combined/applied via a scan; no certificate — correctness rests on the SC-graph proof, not a checked equivalence against a reference. | **Partial overlap** (mechanism: static+runtime hybrid reordering of a fixed workload) — not a summary-composition system. |
| **Callas / MCC**, Xie et al., SOSP 2015 (not VLDB — corrected in the earlier pass already; re-confirmed here), [full text read](https://www.cs.utexas.edu/~lorenzo/papers/Chao15Callas.pdf) | Statically and heuristically partitions the transaction workload into groups, each free to use a specialized, more aggressive concurrency-control mechanism as long as a declared isolation property composes across groups. 8.2× over a MySQL Cluster baseline on TPC-C. | Groups are a partitioning of the *workload*, not a synthesized summary of *effects*; no parallel-prefix evaluation; the in-group mechanism "refines the static analysis approach" with runtime techniques, confirming it is static+runtime hybrid grouping, not synthesis. | **Partial overlap**, same shape as IC3. |
| **Homeostasis**, Roy et al., SIGMOD 2015, arXiv 1403.2307, [full text read](https://arxiv.org/pdf/1403.2307) | Fully automated program analysis extracts "symbolic tables" and derives **global treaties** — predicates bounding permissible inconsistency between sites — splitting them into local, per-site-checkable treaties. Treaty-violating concurrent writes are resolved by a **voting algorithm that picks one serial winner**, not a parallel-prefix evaluation of the batch. The paper's own definition states its guarantee is explicitly *"a relaxation of view-serializability"* (line 743). | Multi-site/geo-replicated setting, not single-node many-core; guarantee is weaker than serializability by the paper's own statement, where C1 requires preserving every caller's response under (implicitly) the kernel's serializable semantics; no notion of summary size/cost bounds; conflict resolution is a serial vote, not an associative composition evaluated by a scan. | **Partial overlap** on "automatic derivation," narrowed sharply by setting and by guarantee strength. |
| **O'Neil escrow**, 1986, [full text read](https://www.cs.umb.edu/~poneil/EscrowTM.pdf) | A manually-specified, per-aggregate-type mechanism; the paper states the database must be made an "expert" about the specific update type (grepped for "automat*"/"static analysis"/"compile" in the full text: one incidental hit, unrelated). | No automatic derivation of anything — the escrow method is hand-declared per aggregate, predates and is orthogonal to any synthesis question. This is kiln's own class-2 (typed reservation) mechanism, not class-3 (C1); re-checking confirms it was never a strong C1 collision to begin with, only a C1-adjacent historical root. | **Adjacent**, confirmed. |
| **Doppel**, Narula/Cutler/Kohler/Morris, OSDI 2014, [full text read](https://pdos.csail.mit.edu/papers/doppel:osdi14.pdf) | Phase reconciliation: splits a contended value per core, reconciling split per-core states back into the global store during a short reconciliation phase. The paper states this explicitly and restrictively: *"all modifications to a contended item must commute"* (line 53) — the technique is defined only for commutative updates. | C1 is specifically about operations that **do not commute** — Doppel is, by its own stated restriction, outside C1's scope entirely, not merely a narrower partial overlap. | **Adjacent**, re-classified down from "partial overlap" — the earlier note already flagged the commutativity restriction, but under this task's stricter four-way taxonomy a mechanism that explicitly excludes the hard case C1 is about is adjacent, not a partial overlap of it. |
| **ESE `JetEscrowUpdate`**, Microsoft, [docs page read in full](https://learn.microsoft.com/en-us/windows/win32/extensible-storage-engine/jetescrowupdate-function) | A single explicit API a developer calls per column, after declaring that column `JET_bitColumnEscrowUpdate`; confirmed shipping, production, single-column atomic-add mechanism with versioned rollback. | Entirely manual and declared, one operation type (add), no synthesis, no batching, no parallel evaluation. | **Adjacent**, confirmed — the one real production deployment in the escrow lineage, but of the class-2 mechanism, not C1. |
| **DBSP / Feldera**, arXiv 2203.16684, [full text read](https://arxiv.org/pdf/2203.16684) | Z-sets — functions with finite support into ℤ — form an **abelian group** under addition, which is what makes incremental view updates composable and deletion "free." Transactions are treated as an already-committed, linearizable input *stream*; DBSP incrementally recomputes **query views** from that stream. | This is downstream of commit, not a mechanism for executing or ordering writes: it assumes transactions already happened in a total order and incrementalizes read-side views from the resulting diffs. It says nothing about synthesizing a write-transaction summary, nothing about non-commuting operations on contended state, nothing about per-caller responses to concurrent writers, and no certificate against a reference semantics for writes. | **Adjacent** — the abelian-group framing is a genuine structural cousin of "associative composition," but applied to a different object (query-result deltas, not transaction effects) for a different purpose (read-side incrementalization, not write admission). |
| **Noria**, Gjengset et al., OSDI 2018, [full text read](https://www.usenix.org/system/files/osdi18-gjengset.pdf) | Partial-state dataflow caches/materializes query results incrementally; for a canonical write-write contention case (vote counts, "hotness" scores) it moves the aggregation into the dataflow so **no transaction is needed at all**, at the cost of consistency: the paper states "Noria... avoids write-write conflicts without a transaction, albeit at the cost of eventual consistency for reads" (line 469) and "Noria operators and the contents of its external views are eventually-consistent" (line 451). Migrations are "near-instantaneous, dynamic transitions for dataflow graphs," reusing existing state, not described as a diff of two compiled programs. | Sidesteps the problem C1 is about rather than solving it: it gives up serializability for contended writes instead of synthesizing a serializability-preserving summary. No per-caller individual-response reconstruction (vote counts are aggregated, not attributed back to callers). | **Adjacent**, confirmed — a different trade entirely, not a narrower version of C1's trade. |

### 2.2 New partial overlaps found by the fresh search, not in the issue's list

- **Mergeable Replicated Data Types (MRDT) / Quark**, Kaki/Priya/Sivaramakrishnan/
  Jagannathan, OOPSLA 2019, DOI 10.1145/3360580, [full text read](https://www.cs.purdue.edu/homes/suresh/papers/oopsla19-mrdt.pdf).
  This is the strongest new partial overlap found for C1. Quark **automatically
  derives a three-way merge function** for an ordinary sequential OCaml data type
  from a manually-supplied "invertible relational specification" of its inductive
  structure — and, unlike every escrow/CRDT-lineage system already in the issue's
  list, it explicitly targets operations that **do not commute**: its running example
  is a queue, and the paper states directly, "pop do not commute. Hence, its semantics
  under (operation-centric) asynchronous replication is..." (line 482), going on to
  define a context-aware three-way merge for exactly this case. This is a genuine,
  automatic, non-commutativity-tolerant synthesis mechanism — the closest any single
  system came, in this search, to C1's hardest requirement. It fails C1 on every other
  axis, though, and says so of itself: the resulting semantics is explicitly **"not
  linearizable or serializable... but nonetheless well-defined (i.e., convergent)"**
  (line 190) — the opposite trade from C1, which requires preserving every caller's
  response under the kernel's serializable semantics, not merely convergence. It has
  no notion of reconstructing an individual per-caller response (the object is a
  branch's final *state* — a counter value, a queue's contents — not a set of
  responses owed to distinct concurrent callers); no parallel-prefix-scan batch
  evaluation (merges are pairwise, against a version's lowest common ancestor, in a
  version-control/branching model, not a scan over a batch of concurrent calls); and
  the setting is geo-replicated/asynchronous, not single-node many-core. **Classification: partial overlap** — it narrows C1 further than the issue's list did: automatic
  synthesis of a composable summary *for non-commuting operations* is no longer
  unattempted anywhere in the literature, but automatic synthesis of one that also
  preserves individual caller responses under ACID/serializable semantics, evaluated
  by a parallel scan, remains unattempted everywhere this search reached.
- **The Scalable Commutativity Rule / COMMUTER**, Clements/Kaashoek/Zeldovich/Morris/
  Kohler, SOSP 2013, [bibliographic and abstract-level, not deep-read this session — see §4](https://www.read.seas.harvard.edu/~kohler/pubs/clements13scalable.pdf).
  Automatically derives, from a POSIX-style interface specification (not database
  transaction code), the precise conditions under which a pair of operations commute,
  using a SAT-based search (COMMUTER), to guide scalable kernel design (sv6). This is
  adjacent, not a partial overlap: it discovers *when* operations already commute
  (to license lock-free concurrency), never synthesizes a summary for operations that
  do not. Named here because it is the clearest instance of "automatic discovery of a
  concurrency condition from an interface's code" outside the database literature, and
  a fresh builder searching nearby terms will find it quickly; it should not be mistaken
  for a C1 collision. **Classification: adjacent.**
- **Koskinen & Bansal**, arXiv 2004.08450 (2020), [abstract-level only]. Automatically
  *proves* commutativity conditions for existing abstract-data-type implementations by
  reducing the problem to reachability. Same relationship to C1 as COMMUTER: verifies
  commutativity of given code, does not synthesize summaries for non-commuting code.
  **Classification: adjacent.**

### 2.3 Conclusion for C1

**No collision.** Nothing found makes C1, read conjunctively (automatic + checked
certificate + associative + bounded size/cost + non-commuting + parallel-prefix-
evaluated + per-caller-response-preserving), already-occupied territory. The field
splits cleanly into two lineages that each supply roughly half the claim: an
application-side/OLTP lineage (TransactionMerger, Parallel Combining, Doppel, IC3,
Callas) that reconstructs individual caller responses and targets real contended OLTP
state, but only for hand-written or commutative merges, with no certificate and no
compositional-algebra framing; and a replicated-data-type lineage (O'Neil → Homeostasis
→ MRDT/Quark) that automatically synthesizes composable merges, now including explicitly
non-commuting operations (MRDT/Quark, 2019), but for convergence rather than
serializability, over final data-type state rather than per-caller responses, in a
geo-replicated rather than single-node-batch setting.

**Confidence: medium.** This is a slight correction, not an escalation, of the earlier
pass's framing: MRDT/Quark is a materially closer mechanism match than anything
previously on record (it is the first found system that both automates synthesis *and*
tolerates non-commutativity), so the gap is narrower than "nobody has tried this," but
it is still real and specific — no source found merges the serializability-preserving,
per-caller-response, parallel-prefix-scan requirements with automatic non-commutative
summary synthesis. Medium rather than high because two active research lines (merge
synthesis for replicated types; application-side transaction merging) are visibly
converging on adjacent pieces of this as of 2019–2026, and a full ACM DL/DBLP crawl
(§4) was not completed.

**Narrowest honest restatement:** *Automatic, checked synthesis of a compact,
size/cost-bounded associative summary for database transactions whose effects do not
commute, evaluated by a parallel-prefix scan over a batch, that preserves every
individual caller's response under the kernel's serializable semantics* — every
clause of this sentence has independent prior art; no source combines automatic
non-commutative synthesis with serializability-preserving per-caller response
reconstruction and scan-based batch evaluation.

## 3. Claim C2 — parametric per-call footprint derivation at build time

### 3.1 Known collisions, re-checked against the primary source

Re-reading the primary sources for the deterministic-database lineage directly (not
through the secondary blog retrospective the earlier pass leaned on) sharpens the
existing "declare vs. discover" framing into a direct quote from each system itself:

| Work | Direct quote / confirmed mechanism | Classification |
|---|---|---|
| **Calvin**, Thomson et al., SIGMOD 2012, [full text read](https://www.cs.princeton.edu/courses/archive/fall19/cos418/papers/calvin.pdf) | §3.2.1, "Dependent transactions": for transactions whose read/write set cannot be known in advance, Calvin's own fix is "an inexpensive, low-isolation, unreplicated, read-only reconnaissance query" executed against live data, after which "the actual transaction is... executed, using the reconnaissance query's results for its read/write set" (lines 437–450) — a genuine runtime step against live data, confirmed from Calvin's own text, not a secondary characterization. | **Partial overlap** |
| **Bohm**, Faleiro & Abadi, PVLDB 2015, [full text read](https://arxiv.org/pdf/1412.2324) | The paper's own words: the concurrency-control phase "needs advance knowledge of each transaction's write-set... it is not necessary for transactions to pre-declare these... sets. For example, Calvin proposes a speculative technique which predicts each transaction's read-/write-sets on the fly... Bohm can make use of this technique if transactions' write-sets are not... derivable in advance" (lines 222–230). | **Partial overlap** |
| **Aria**, Lu/Yu/Cao/Madden, PVLDB 2020, [full text read](https://www.cs.arizona.edu/~caolei/papers/Aria.pdf) | Direct quote: "earlier deterministic databases require that transaction read/write sets are known before execution" (line 9); Aria's own fix is to execute the batch speculatively against one snapshot and resolve conflicts deterministically after the fact — discovery via one execution pass, not static derivation from code. | **Partial overlap** |
| **Caracal**, Qin/Demke Brown/Goel, **SOSP 2021** (corrected: the earlier pass flagged the venue as "unverified, possibly SIGMOD 2021"; DBLP confirms SOSP 2021, pp. 180–194), [full text read](https://www.eecg.utoronto.ca/~ashvin/publications/caracal.pdf) | Confirmed: batches transactions into epochs and *analyzes the batch at runtime* ("allows analyzing the transactions in the epoch to determine... contention," line 31) via its batch-append/split-on-demand optimizations — per-epoch runtime analysis, not build-time static derivation from source. | **Partial overlap** |
| **Amato, Giannotti & Mainetto**, VLDB 1993, [full text read in full — recovered from a poorly-OCR'd scan; every substantive claim below was legible despite character-level noise](https://www.vldb.org/conf/1993/P405.PDF) | Confirmed, from the paper's own closing section ("Discussion and further work"): this is a pure correctness proof with two hand-worked examples; no analyzer or scheduler was ever implemented ("The first next step is the development of a prototype of the analyser"); the paper's own open questions are exactly what E1 measures 33 years later: *"what is the cost of the analysis in terms of space and time... is the information gathered by the analysis significant, i.e., is it really useful for realistic optimizations?"* The read/write-set abstraction is tuned by an abstract-domain granularity parameter, not derived as an explicit function of a call's declared arguments in kiln's sense, and the scheduler is plain conservative 2PL with no typed-reservation distinction (no claims/pins/escrows, no "one logical-resource protocol" spanning several reservation kinds). | **Partial overlap**, confirmed unimplemented and unbenchmarked by its own text — the strongest shape-level match remains the weakest evidentially. |
| **Transaction chopping**, Shasha/Llirbat/Simon/Valduriez, ACM TODS 20(3), 1995, DOI 10.1145/211414.211427 — **full TODS text not independently obtainable this session** (paywalled; NYU's own mirror and an academia.edu copy both returned HTTP 403 even with a browser user-agent — see §4); characterized here via a short author-written encyclopedia summary, the paper's own citing descendants (IC3, TransactionMerger both cite and summarize it), and DBLP's bibliographic record | Builds a static conflict graph (the "SC-graph") over *pieces* of a **fixed, already-known** set of transaction programs to find the finest safe chopping; TransactionMerger's own related-work section (which was read in full) explicitly lists chopping as one of several *application-side rewriting* techniques, i.e., a transformation of known code, not a build-time derivation of footprints as a function of a call's arguments for a general admission protocol. | **Partial overlap**, access-limited (see §4) — characterization is consistent with, not independently stronger than, the earlier pass's note. |
| **Block-STM**, arXiv 2203.06871 — not re-fetched this session; the earlier note's direct quote ("every transaction records its own read-set and write-set during (speculative) execution") was itself sourced from the primary text and is carried over unchanged | Runtime discovery via speculative execution, re-executed on conflict. | **Partial overlap**, carried over. |
| **PWV**, Faleiro/Abadi/Hellerstein, PVLDB 2017, DOI 10.14778/3055540.3055546 — **re-fetch attempted and failed this session** (the NSF Public Access mirror timed out twice; see §4); carried over from the earlier pass's direct quote | Restricts *when* a transaction may abort to make writes visible early; not a footprint-derivation mechanism at all. | **Adjacent**, carried over. |

### 3.2 New partial overlaps found by the fresh search

- **QURO**, Yan & Cheung, VLDB 2016, [characterized via author project page and
  secondary summaries — not independently full-text read this session, see §4](https://db.cs.washington.edu/projects/quro/).
  A genuine, deployed, build-time compiler that statically analyzes **real** OLTP
  application transaction code (not a research toy language) and automatically
  reorders statements within a transaction to move contended queries later, reducing
  lock-hold time; reported up to 6.53× throughput and 85% latency reduction, without
  changing the concurrency-control mechanism. This is the clearest real-world instance
  found of "a compiler performs static analysis of actual application transaction code
  at build time" — but the output is a *reordering* of one transaction's own statements,
  not a read/write footprint exported as a function of the call's arguments to a
  cross-transaction admission protocol. **Classification: partial overlap** for the
  "build-time static analysis of real transaction code" half of C2; it does not touch
  the "drives admission under one logical-resource protocol" half at all.
- **FAFO / ParaLyze**, arXiv 2507.10757 (2025), blockchain/EVM setting. Determines an
  approximate read/write set either from node-supplied metadata (an access list) or
  from simulation — confirming the declare-or-discover dichotomy holds even in the
  newest (2025) blockchain-adjacent lineage, with no static-analysis-of-code third
  path found there either. **Classification: adjacent.**
- **Afek, Korland & Zilberstein**, LCPC 2010/2011, "Lowering STM Overhead with Static
  Analysis." Compile-time data-flow analysis that **removes redundant instrumentation**
  in already-compiled software-transactional-memory code (eliding read/write barrier
  calls the analysis proves unnecessary) — a genuine build-time static analysis, but of
  STM bookkeeping overhead in a general in-process program, not of a database
  transaction's footprint as a function of declared call arguments, and it optimizes
  existing instrumentation rather than deriving information that drives an admission
  decision. **Classification: adjacent.**
- **CoqSQL**, Suriyakarn (MIT MEng thesis, 2017), building on Kaseorg's and Lesani's
  prior framework. Per secondary description (the thesis itself was not independently
  obtained this session — see §4), automatically synthesizes concurrent, multi-processor
  transaction implementations from a SQL-subset specification, with accompanying
  correctness proofs. This is a plausible partial overlap for both C1's certificate
  requirement and C2's build-time-derivation requirement, but it was not possible to
  confirm from primary text within this session's reach whether footprints are derived
  parametrically from call arguments or the specification is closer to Amato-style
  whole-schema analysis; flagged honestly as **unverified** rather than classified with
  confidence — a gap for a future pass, not a claim either way.

### 3.3 Conclusion for C2

**No collision.** The declared-or-discovered dichotomy holds across every deterministic
database this search reached, confirmed this time directly from each system's own
primary text rather than through a secondary retrospective: Calvin's reconnaissance
query, Bohm's "derivable in advance... Calvin's speculative technique," Aria's
snapshot-and-reconcile, Caracal's per-epoch runtime analysis, Block-STM's and
ParaLyze's speculative/metadata discovery. Amato et al. (1993) remains the one source
that automatically derives an approximate footprint *from transaction source text*
feeding a 2PL scheduler — the closest match in shape — but it is an unimplemented,
unbenchmarked correctness proof for a research language, with no per-call parametric
sense, no one-hop/dependent-read category, and no unified protocol spanning typed
reservations; it was never picked up by the deterministic-database lineage that
followed it by two decades. QURO (2016) is the one real system found that performs
genuine build-time static analysis of real OLTP application code, but at the wrong
granularity (within-transaction reordering, not cross-transaction footprint export).

**Confidence: medium-high** that the exact combination in C2 — parametric, build-time,
per-call derivation from compiled application code, with a one-hop carve-out for
dependent reads resolved through a known foreign key, driving admission under one
logical-resource protocol spanning typed reservations and conservative locking alike —
remains open. This is unchanged from, and now better evidenced than, the earlier
pass's conclusion: every primary source re-read this session confirmed rather than
complicated the "nobody derives it statically from code" finding, and the dedicated
2023–2026 search (§1.3) surfaced no counter-example, only QURO (2016, wrong
granularity) and FAFO/ParaLyze (2025, confirms the dichotomy persists into the newest
lineage). Not "high" because a full ACM DL/DBLP proceedings crawl for this specific
combination was not completed (§4).

**Narrowest honest restatement:** unchanged from `docs/concept.md` §15 — *static,
build-time derivation of a transaction's read/write footprint as a pure function of
its declared call arguments (plus at most one dependent read resolved through a
statically-known foreign key), from compiled application code, feeding one admission
protocol that spans typed reservations and conservative locking* — this review found
no system that does this, and the two nearest neighbors on either side of the
combination (Amato 1993 for "derive from code, feed a scheduler"; QURO 2016 for
"real build-time static analysis of real OLTP code") were each already named or
findable, and each misses the combination for a different, now-confirmed reason.

## 4. Claim C3 — measuring how much real code admits C2

### 4.1 The one directly relevant measurement, read in full

**Nguyen, Niechen, DeCarolis & Abadi, "Are Database System Researchers Making Correct
Assumptions About Transaction Workloads?", SIGMOD 2025 (Proc. ACM Manag. Data 3(3),
Article 131), DOI 10.1145/3725268 — [full text obtained and read](http://www.cs.umd.edu/~abadi/papers/database-workloads.pdf).**
This is the "111-application study" `docs/experiments.md` already cites and explicitly
declines to treat as a substitute for E1. Reading the full methodology (not only the
figures already quoted) confirms and sharpens that framing:

- The study defines four increasingly precise **access-set definitions** — Relation,
  Predicate, CDA-Based (conflict-detection attribute), CDAeq-Based — each with named
  **fallback scenarios** that force a runtime mechanism: Complex Predicates, Attribute
  Dependency (a predicate that depends on a value read earlier in the same
  transaction — the paper's closest analogue to kiln's "dependent read"), CDA Mismatch
  (looking a row up by a non-primary attribute), and Non-equality Predicates.
- The oft-quoted **58%/90% figure** is specifically the CDA-Based definition's result:
  "for 90% of applications, at least 58% of transactions have read/write sets that are
  possible to infer in advance" (line 126), confirmed against the paper's own CDF
  figure description (line 1310: "over 90% of applications can run the majority of
  their transactions without fallbacks" under CDA-Based). The **27%-by-secondary-attribute
  figure** `docs/experiments.md` cites corresponds exactly to the paper's own number:
  "26.9% transactions per application would result in a CDA Mismatch" (line 1200) —
  confirmed as an accurate prior citation, not an approximation.
- Crucially, the paper's **Attribute Dependency** category — its only analogue to a
  "dependent read" — is a strict, uniform fallback: *any* predicate depending on a
  value read earlier always needs a runtime mechanism in this paper's scheme (occurring
  in only 30% of applications, averaging 0.9% of transactions). The paper draws **no
  distinction** between a dependent read that resolves through a statically-known
  foreign key (kiln's "one-hop," pinned and therefore still staticaly admissible) and
  one that does not (kiln's "unknown") — confirming, from the primary text itself, that
  `docs/experiments.md`'s own statement ("the study's binary inferable/not-inferable
  split does not distinguish static from one-hop") is accurate and that E1's one-hop
  category measures something this paper's own categories cannot answer.
- The paper is **purely a measurement**; it proposes no compiler, no admission
  protocol, and does not discuss driving scheduling decisions from parametric,
  argument-indexed footprints — it only asks whether today's systems' *existing*
  assumptions about inferability hold on real code. It therefore cannot be stretched
  into even a partial overlap for C1 or C2, only for C3.
- Corpus: 78 Django + 33 TypeORM applications, selected by GitHub-star popularity
  (≥1000/≥500 stars respectively), over 30,000+ transactions, via a semi-automated
  annotation tool with manual review specifically for interactive-transaction and
  attribute-dependency cases (confirmed from the methodology section, lines 292–637).

**Classification: partial overlap for C3** — a close, careful, primary-source-verified
measurement of a *related but not identical* question, on a narrower and non-random
corpus (popular open-source ORM-driven web apps only, explicitly not TPC-C-style
enterprise or ad hoc analyst workloads), using a category scheme that is strictly
coarser than kiln's own (no one-hop carve-out). It does not and cannot settle C3.

### 4.2 Other measurements sought and not found

No other empirical study was found — in this pass or recoverable from the existing
notes — that measures static read/write-set or footprint inferability on a corpus of
real application transaction code, under any category scheme, for any date range.
The only other quantitative data point in the lineage is Amato et al.'s own two
hand-worked examples (not a corpus measurement) and IC3's single TPC-C throughput
number (a performance result, not a coverage measurement). No TPC-C-focused,
non-ORM, or enterprise-codebase replication of the Abadi et al. study was found.

### 4.3 Conclusion for C3

C3 is, by its own nature, not a claim literature search can resolve — it names a
measurement to run, not a question with a citable answer. This review's job for C3 is
narrower: confirm that no existing measurement already settles it (so E1 is not
redundant) and confirm that the one measurement close enough to cite as a cross-check
(Abadi et al. 2025) is being relied on at exactly the strength it supports — as an
order-of-magnitude proxy on a related but coarser and narrower-corpus question, not as
a substitute result. Both hold, confirmed by a full reading of the paper rather than
its abstract. **Confidence: not applicable in the usual sense** — there is nothing to
have confidence in or against yet; the honest statement is that the literature search
found no reason to believe E1 is already answered, and found one useful, honestly-
scoped calibration point for roughly what order of magnitude to expect.

## 5. Summary of new collisions / partial overlaps not in the issue's original list

- **Mergeable Replicated Data Types / Quark** (OOPSLA 2019) — the single most
  important new finding: automatic synthesis of a composable summary for explicitly
  **non-commuting** operations, the first found anywhere in this or the prior search
  pass. Narrows C1's novelty (§2.2) without eliminating it.
- **The Scalable Commutativity Rule / COMMUTER** (SOSP 2013) and **Koskinen & Bansal**
  (arXiv 2004.08450, 2020) — automatic discovery of commutativity conditions from code,
  adjacent to C1's "automatic" angle but solving the opposite problem (verifying
  existing commutativity, not synthesizing summaries for its absence).
- **QURO** (VLDB 2016) — the clearest real-world instance of build-time static
  analysis of actual OLTP application code found for C2, at the wrong granularity
  (within-transaction reordering).
- **Strife** (SIGMOD 2020, Prasaad/Cheung/Suciu) — runtime, sampling-based clustering
  of transactions into conflict-free batches; adjacent to both C1 and C2 but neither
  static/build-time nor a synthesized summary.
- **FAFO/ParaLyze** (arXiv 2507.10757, 2025) — confirms the declare-or-discover
  dichotomy persists into the newest (2025) blockchain-adjacent lineage.
- **Afek/Korland/Zilberstein** (LCPC 2010/2011) — build-time static analysis, but of
  STM instrumentation overhead, not transaction footprints.
- **CoqSQL** (MIT thesis, 2017) — a plausible partial overlap for C1's certificate and
  C2's build-time derivation that this session could not verify from primary text;
  named honestly as unverified, not claimed.
- A corrected bibliographic fact carried forward: **Caracal is SOSP 2021**, not an
  "unverified, possibly SIGMOD 2021" venue as the earlier pass's note had it (confirmed
  via DBLP, page range 180–194).

## 6. Limits of this search

- **No independent ACM Digital Library, DBLP crawler/API, or Google Scholar access.**
  Every "DBLP" or "ACM DL" lookup in this protocol was actually a general web search
  for the bibliographic record plus a hunt for a non-paywalled mirror of the paper
  itself. This is the single largest structural limit on how systematic this review
  can claim to be; a reviewer with institutional ACM DL access or a DBLP API key could
  run the proceedings-by-venue-by-year crawl this session could not.
- **Two primary sources could not be obtained despite repeated attempts:**
  Shasha/Llirbat/Simon/Valduriez's full ACM TODS 1995 text (the NYU and academia.edu
  mirrors both returned HTTP 403 even with a browser user-agent; characterized instead
  via a short author-written encyclopedia summary and two citing papers' own
  descriptions, read in full) and Faleiro/Abadi/Hellerstein's PWV, PVLDB 2017 (the NSF
  Public Access mirror timed out on two separate attempts; characterized via the
  earlier pass's already-primary-source-sourced direct quote and an independently
  confirmed DOI, not re-verified beyond that).
- **Semantic Scholar was not queried through its own interface or API** — a targeted
  pass specifically against its citation-graph tooling (rather than general web search)
  might surface forward citations this session's method could miss, particularly for
  the youngest items (TransactionMerger, MRDT/Quark, FAFO).
- **No full SIGMOD/VLDB/CIDR/OSDI/SOSP/EuroSys/NSDI/PLDI/POPL/OOPSLA/ASPLOS/PPoPP/SPAA
  proceedings-by-year crawl was run for 2023–2026** beyond the targeted queries in
  §1.3; per the dispatch brief's own instruction, depth on named collisions and partial
  overlaps was prioritized over such breadth. A dedicated pass reading every 2024–2026
  CIDR/SIGMOD/VLDB proceedings table of contents for titles plausibly touching
  "transaction," "commute," "footprint," or "synthesis" would be the natural next
  increment if the operator wants higher confidence than "medium" on C1 or "medium-high"
  on C2.
- **Not re-verified this session, carried over from the existing notes unchanged**
  (not flagged in the issue as needing re-check, and not independently re-surfaced by
  the fresh queries): Katara, Hamsaz, Hampa, IPA, ECROs, CISE/CISE3, Bailis et al.'s
  I-confluence paper itself, and the chopping-lineage follow-ons (Dias/Lourenço/Preguiça
  StarTM). Their characterization in `docs/research/notes/invariant_driven_coordination.md`
  is accepted as-is for this review.
- **CoqSQL's thesis text itself was not obtained** — characterized only via secondary
  description of its abstract and catalog record; flagged as unverified in §3.2/§5
  rather than scored.
