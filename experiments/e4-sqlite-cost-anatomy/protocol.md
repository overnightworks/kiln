# E4: where SQLite spends its time on small transactions

Status: preregistered protocol, not yet reviewed or frozen. Nothing in this
document is a measured result. No benchmark and no profiler run backs any
number below; every number here is a setting, a threshold, or a fact about
this machine, decided in advance.

This is the first deliverable named by issue #7, under the freeze rule in
`docs/experiments.md` ("an experiment's concrete inputs are written down ...
and independently reviewed" before any result-bearing run). A different
agent must be able to build and run E4's harness from this file alone. Once
reviewed and frozen, no setting, workload, threshold, or bucket rule below
changes after a measurement run is seen — a change would void every run
taken under the old version, exactly as the freeze rule requires for E1–E3.

## 1. Question and hypothesis

**Question.** For a handful of small, single-row-or-few-row transactions,
where does SQLite actually spend its time — flushing to disk, running
compiled bytecode (VDBE), walking the B-tree and handling pages, taking
locks, or something else — and how does that split change between a dataset
that fits in SQLite's own page cache and one that does not?

**Why this matters.** `docs/concept.md` §12 (the performance goal, operator
ruling 15.10.2026) states that kiln must not lose to SQLite on small data,
and names exactly the costs a closed-world compiler can remove: "no SQL text
to parse or compile to bytecode at request time," no B-tree walk where a
maintained view answers the lookup instead, and shared `fsync` across
concurrent committers. The same section also names the honest ceiling:
"parity, not a win, is the realistic ceiling" on a single-writer path where
every transaction waits for its own `fsync`, because "no amount of
compilation changes how long a flush takes." E4 measures SQLite's own time
split before kiln exists, so that ceiling is stated as a number, not an
assumption, and the later kiln-vs-SQLite harness (an engine-phase item, not
this one) starts from a baseline this document already made fair.

A critique of an earlier draft of the performance argument (finding 17,
critique pass 2) found the assumed SQLite baseline "unnecessarily weak":
SQLite does not need to parse and compile SQL on every call, because
prepared statements are reusable
(https://www.sqlite.org/cintro.html, fetched 2026-10-15). Every setting in
§3 below exists to make the measured baseline the strongest fair one, not
the weakest convenient one.

**Hypothesis, stated so it can be checked against a number.** SQLite's
non-flush cost (VDBE execution, B-tree and page handling, locking, and the
catch-all "rest" bucket of §4) is a non-trivial share of total per-operation
time for small, cache-resident data — because that cost does not depend on
waiting for a physical flush, closed-world compilation is a candidate to
remove some of it, and a measurable share therefore means a small-data win
is at least plausible. If, instead, that non-flush share is close to zero
once the flush itself is paid, there is nothing left below the device's
flush floor for any compiler to remove, and parity is the ceiling — exactly
the case concept.md §12 already names as "the realistic ceiling."

The exact split points between "plausible win" and "parity is the ceiling"
are fixed in §6 (**Verdict rule**), before any run, so that no threshold is
chosen after seeing where the needle lands.

## 2. Workloads, frozen

### 2.1 Schema

```sql
CREATE TABLE accounts (
  id      INTEGER PRIMARY KEY,
  email   TEXT NOT NULL,
  balance INTEGER NOT NULL
);
CREATE UNIQUE INDEX accounts_email ON accounts(email);
```

`id` is an `INTEGER PRIMARY KEY`, which SQLite treats as an alias for the
table's own rowid, so a point lookup or update by `id` walks exactly one
B-tree (the table's own) with no secondary index involved.
`accounts_email` is the experiment's one secondary unique index; only the
insert workload (2.2.C) touches it on its hot path.

### 2.2 The four workloads

Every workload runs as exactly one transaction per call —
`BEGIN IMMEDIATE` before its statement(s), `COMMIT` after — even the
read-only one. This is deliberate: it keeps the transaction boundary
identical across all four workloads, so a difference in measured buckets
reflects what each workload actually does, not an accident of one workload
skipping the commit path another one pays.

Each workload's SQL is prepared exactly once per connection, at harness
startup, and reused for every repetition by rebinding parameters and
calling reset — never re-prepared per call
(https://www.sqlite.org/cintro.html, fetched 2026-10-15; see §3.3 for the
exact API calls this requires).

- **A. Point read by primary key.**
  `SELECT id, email, balance FROM accounts WHERE id = ?1;`
  `?1` is drawn uniformly at random from `[1, row_count]` for the active
  size (2.3). One statement, one transaction.

- **B. Point update.**
  `UPDATE accounts SET balance = balance + ?1 WHERE id = ?2;`
  `?1` is a fixed delta (`+1`), `?2` drawn uniformly at random from
  `[1, row_count]`. One statement, one transaction.

- **C. Insert under a unique secondary index.**
  `INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 0);`
  `?1` is the next unused id, taken from a counter the harness keeps
  starting at `row_count + 1` (never reused across repetitions). `?2` is a
  freshly generated, globally unique random string (the harness's
  repetition counter plus a fixed-length random suffix), so every insert
  lands in a random, previously untouched region of the unique index's
  B-tree rather than repeatedly extending the same leaf — the harder case
  for a unique secondary index, not the easier append-only one.
  This workload grows the table during its timed window. The growth is
  bounded by the operation count the timing rule in §4.4 fixes per cell
  (in the low thousands in practice) against a starting size of
  `row_count` (2.3) in the millions for the medium size and the low
  thousands for the small size — named here as a controlled, documented
  perturbation, not an unnoticed one. It is why insert gets its own
  freshly loaded table instance per run rather than sharing a table with
  workloads A/B/D.

- **D. Small multi-row transaction (a transfer).**
  One transaction containing, in order:
  `SELECT balance FROM accounts WHERE id = ?1;`
  then, only if that balance is at least the fixed transfer amount,
  `UPDATE accounts SET balance = balance - ?1 WHERE id = ?2;` and
  `UPDATE accounts SET balance = balance + ?1 WHERE id = ?3;`.
  `?2` (`from_id`) and `?3` (`to_id`) are drawn uniformly at random and
  independently from `[1, row_count]`, redrawing `to_id` on a collision
  with `from_id`. The fixed transfer amount is small relative to the
  frozen initial balance every row is loaded with, so the guard almost
  never blocks the transfer; every repetition still commits exactly one
  transaction regardless of outcome, and the harness counts and reports
  how often the guard actually blocked a transfer, as a cheap sanity
  check rather than a tuning knob.

### 2.3 Sizes

Two sizes, both frozen now:

- **Small: 10,000 rows.**
- **Medium: 10,000,000 rows.**

**Justification.** §3.4 freezes an explicit page-cache budget
(`PRAGMA cache_size`) instead of relying on SQLite's default; the point of
having two sizes at all is to contrast a dataset that fits entirely inside
that frozen budget against one that does not, because that is exactly the
axis along which the B-tree/page-handling bucket is expected to grow: a
cache-resident lookup touches only already-resident pages, a dataset far
larger than the cache forces at least some pages to be re-fetched from
outside SQLite's own cache. Ten thousand rows of this schema (a handful of
integers plus a short string per row, one table plus one unique index) sit
far below any reasonable cache budget; ten million rows — three orders of
magnitude more — sit far above it under any cache budget this experiment
is going to freeze. Three orders of magnitude is chosen to put the two
sizes unambiguously on opposite sides of the cache boundary, rather than
risking a size difference too small to tell which side either one lands
on.

The harness does not assume this contrast holds — it checks it. After
loading each size, the harness reads the database file's actual byte size
from the filesystem and the active `cache_size` setting (§3.4), and writes
both numbers into that size's manifest entry (§6). A size whose on-disk
footprint and declared cache budget do not show the intended fits/does
not-fit relationship is reported as a violation of this section before any
result for that size is trusted.

### 2.4 Key distribution

Uniform random, over the live key range, for every workload that reads an
existing key (A, B, D) — not the dataset's insertion order, and not a
skewed (e.g. Zipfian) distribution, because the question this experiment
asks is about SQLite's unconditional per-operation cost, not about
hot-key caching effects a skewed distribution would introduce as a
confound. The insert workload (C) is described in 2.2.C.

A single fixed pseudo-random seed is used for every cell (one size × one
workload × one `synchronous` setting, §3.2) that needs randomness, chosen
once by the harness and written verbatim into that cell's manifest entry,
so a rerun against the same frozen protocol reproduces the same sequence
of keys byte for byte.

## 3. Settings, frozen

### 3.1 SQLite version: bundled via a named Rust crate

**Decision.** The harness links SQLite through the `rusqlite` crate,
version `0.40.2`, with its `bundled` Cargo feature enabled. That feature
"uses a bundled version of SQLite" — it compiles and links a vendored copy
instead of the system's installed library — and `rusqlite` 0.40.2 depends
on `libsqlite3-sys` `0.38.2` for that vendored copy (both the latest
published versions as of 2026-10-15, per
https://lib.rs/crates/rusqlite and https://lib.rs/crates/libsqlite3-sys,
fetched 2026-10-15). `libsqlite3-sys` `0.38.2`'s `bundled` feature vendors
"currently SQLite 3.53.2" per the same page.

**Justification.** Bundling pins the exact SQLite build whose bytecode and
module boundaries this experiment's bucket mapping (§4.3) is built
against, independent of whatever `libsqlite3` happens to be installed on
whichever machine runs the harness — the system library's version varies
by distribution and by when it was last updated, which would make the
bucket mapping and the cache/page-size arithmetic in §2.3 silently wrong
on a different machine. A reproducible, explicitly versioned build is worth
the (small, one-time) compile cost of vendoring.

**Verification, not just intent.** Crate versions and their vendored
SQLite versions can move between when this protocol was written and when
the harness is actually built. The harness pins exact versions in
`Cargo.lock` (checked into the repository alongside the harness) and, at
startup, calls `sqlite3_libversion()` and writes the returned string into
every run's manifest. A mismatch between the version named above and the
version the running binary reports is logged as a protocol deviation, not
silently accepted — the freeze is on the intended version, the manifest
records the actual one.

### 3.2 WAL and synchronous

- `PRAGMA journal_mode = WAL;` is set once per connection, right after
  opening. In WAL mode, "a COMMIT occurs when a special record indicating
  a commit is appended to the WAL" rather than to the main database file,
  and WAL "uses many fewer fsync() operations" than the default rollback
  journal (https://www.sqlite.org/wal.html, fetched 2026-10-15).

- Both `PRAGMA synchronous = FULL;` and `PRAGMA synchronous = NORMAL;` are
  measured — every cell in §2 runs once under each setting, never only
  one. In WAL mode, "writers sync the WAL on every transaction commit if
  `PRAGMA synchronous` is set to FULL but omit this sync if `PRAGMA
  synchronous` is set to NORMAL"
  (https://www.sqlite.org/wal.html, fetched 2026-10-15). FULL "is atomic,
  consistent, isolated, and durable (ACID) in WAL mode"; NORMAL in WAL mode
  "does lose durability" — "a transaction committed in WAL mode with
  synchronous=NORMAL might roll back following a power loss or system
  crash," though it stays consistent
  (https://www.sqlite.org/pragma.html#pragma_synchronous, fetched
  2026-10-15).

  Measuring both is the point, not a hedge: FULL is the setting that pays
  a real flush on (almost) every commit — the case concept.md §12 calls
  the realistic ceiling — while NORMAL approximates what is left once that
  per-commit flush is removed, without yet building any engine to remove
  it. Comparing the two is a second, independent way (alongside the
  in-memory control, §3.6, and the raw-flush control, §4.2) to see how much
  of SQLite's own time the flush itself accounts for.

### 3.3 Cached prepared statements

Every workload's statement(s) (§2.2) are prepared once, at harness
startup, with `sqlite3_prepare_v2()` (exposed by `rusqlite` as
`Connection::prepare`). "This API does not actually evaluate the SQL
statement. It merely prepares the SQL statement for evaluation"
(https://www.sqlite.org/cintro.html, fetched 2026-10-15) — the expensive
parse-and-compile-to-bytecode step happens exactly once per statement, not
once per call.

For every repetition after the first, the harness calls `sqlite3_reset()`
on the already-prepared statement and rebinds its parameters with
`sqlite3_bind_*()`, then steps it again. The documentation is explicit that
this is the intended reuse pattern: "Using `sqlite3_reset()` on an existing
prepared statement rather than creating a new prepared statement avoids
unnecessary calls to `sqlite3_prepare()`," and "for many SQL statements, the
time needed to run `sqlite3_prepare()` equals or exceeds the time needed by
`sqlite3_step()`. So avoiding calls to `sqlite3_prepare()` can give a
significant performance improvement" (https://www.sqlite.org/cintro.html,
fetched 2026-10-15). No workload in §2 ever calls prepare more than once
per statement shape during a timed run.

### 3.4 Page cache size

`PRAGMA cache_size = -65536;` is set once per connection, right after
opening — an explicit 64 MiB budget, not SQLite's own default. A negative
argument sets the budget in kibibytes: "if the argument N is negative,
then the number of cache pages is adjusted to be a number of pages that
would use approximately `abs(N*1024)` bytes of memory"
(https://www.sqlite.org/pragma.html#pragma_cache_size, fetched
2026-10-15); the default, left unset, is "-2000, which means the cache size
is limited to 2048000 bytes" — about 2 MiB, which this experiment
deliberately overrides to a known, round, explicitly stated number instead
of inheriting a compile-time default. This is the budget §2.3's two sizes
are chosen to sit on opposite sides of.

### 3.5 One connection, one thread, single writer

The harness opens exactly one SQLite connection, on one thread, and runs
every operation serially through it — no second connection, no concurrent
reader, no concurrent writer. This is a deliberate simplification, not an
oversight: E4 asks where a single transaction's own time goes, not how
SQLite behaves under contention between multiple writers (that question
belongs to a different experiment; see §7). The locking bucket measured
in §4.3 is therefore SQLite's own single-writer locking machinery — the
advisory file locks and pager lock-state transitions it still performs
even with nothing else contending for the database — not contention cost.

### 3.6 In-memory control

Yes — in addition to the two on-disk sizes, every workload in §2.2 is also
run once, at the small size only, against a SQLite connection opened on
`:memory:` instead of a file, with the same `cache_size` (§3.4) and the
same cached-prepared-statement discipline (§3.3), but no `journal_mode`
pragma (WAL is meaningless for an in-memory database) and no measurable
sync cost (there is no file to flush). Comparing this control's per-bucket
breakdown against the on-disk, small-size, `synchronous=FULL` breakdown
gives a second, independent estimate of the flush bucket's true cost —
one that does not depend on perf's call-graph attribution (§4.3) being
right — by simple subtraction. The in-memory control is run only at the
small size because its purpose is isolating the flush cost, which §2.3
already shows is a cache-residency question only at the on-disk sizes; an
in-memory medium-size run would not test anything the small one does not.

## 4. Measurement method

### 4.1 Tool and buckets

Each timed run (§4.4) is profiled with `perf record`, using call-graph
sampling: `perf record --call-graph dwarf -F 999 -- <harness invocation for
that cell>`. `perf`'s own default sampling rate is 1000 Hz
("the average rate ... is set to 1000Hz, or 1000 samples/sec"); this
experiment uses 999 Hz instead — a non-round number chosen so sampling does
not fall into lockstep with any periodic system activity scheduled at a
round frequency — and "to specify a custom rate, it is necessary to use
the `-F` option"
(https://perfwiki.github.io/main/tutorial/, fetched 2026-10-15, serving
the content formerly at perf.wiki.kernel.org/index.php/Tutorial).
`perf report` then attributes each sample to the function it was taken in
("self" overhead) and, with callchains collected, to that function's
callers too ("children" overhead) — this experiment uses self overhead
only, because the question is which function the time was actually spent
in, not which callers eventually led there
(https://perfwiki.github.io/main/tutorial/, fetched 2026-10-15).

Every sample's self-overhead function name is assigned to exactly one of
five buckets:

1. **Flush** — the cost of making a commit durable on disk.
2. **Bytecode execution (VDBE)** — running the already-compiled program
   for a statement.
3. **B-tree and page handling** — walking a B-tree and fetching, caching,
   or writing back the pages underneath it, apart from the act of
   flushing them.
4. **Locking** — acquiring or releasing the file and in-process locks
   SQLite takes even as the sole connection (§3.5).
5. **The rest** — everything else: the SQL C-API glue outside the VDBE
   module, memory allocation, and (expected to be near-zero, because
   statements are cached per §3.3) the parser and code generator.

### 4.2 Flush-only control

Independent of perf's attribution, a second, SQLite-free control measures
the device's own flush cost directly: a small standalone benchmark mode
that opens a scratch file in the same directory as the database files used
by the rest of this experiment, writes a small fixed-size buffer, and
calls `fsync`, repeated the same number of times §4.4 fixes for a cell,
profiled the same way. This gives a SQLite-independent ceiling for what
"flush" should cost on this machine's storage and filesystem (§5), to
sanity-check the perf-attributed flush bucket of §4.1 and the in-memory
control of §3.6 against each other.

### 4.3 The function-to-bucket mapping

SQLite's own architecture page names the module each source file owns:
the tokenizer and parser, a code generator, and then, in the harness's
hot path, three modules that matter here — "the virtual machine itself ...
entirely contained in [`vdbe.c`]," with `vdbeaux.c` ("utilities used to
build and run VM programs"), `vdbeapi.c` (the external interfaces such as
`sqlite3_bind_int()` and `sqlite3_step()`), and `vdbemem.c`; "an SQLite
database is maintained on disk using a B-tree implementation found in the
`btree.c` source file"; and the page cache, "responsible for reading,
writing, and caching... pages" and providing "the rollback and atomic
commit abstraction" and "locking of the database file," implemented across
`pager.c` (with WAL-mode logic in `wal.c`) and the in-memory cache
structures in `pcache.c`/`pcache1.c`; finally the OS interface (VFS), which
"provides methods for opening, reading, writing, and closing files on
disk," implemented for this machine's platform in `os_unix.c`
(https://www.sqlite.org/arch.html, fetched 2026-10-15).

The mapping from a perf-reported function name to one of §4.1's five
buckets is built mechanically, once, before the frozen run, and written
into the repository as a plain data file the harness loads — not guessed
function-by-function from memory:

1. Build the bundled SQLite source (§3.1) with debug symbols retained and
   inlining disabled or minimized (a non-release or a release build with
   `panic=unwind`-equivalent and `-g`, no LTO), so every function keeps its
   own symbol and its defining source file is recoverable from debug
   info.
2. Run one short pilot invocation of each workload (§2.2) under `perf
   record --call-graph dwarf`, not a frozen run — its numbers are never
   reported, only its list of sampled function names is used.
3. For every distinct function name that appears as a self-overhead entry
   in the pilot's `perf report`, resolve its defining source file (via the
   build's debug info, e.g. `addr2line` or `perf report`'s own
   `--sort=srcfile,symbol` view), and assign it to a bucket by this fixed
   rule:
   - defined in `vdbe.c`, `vdbeaux.c`, `vdbeapi.c`, or `vdbemem.c` →
     bytecode execution (VDBE);
   - defined in `btree.c`, `pcache.c`, or `pcache1.c`, or defined in
     `pager.c`/`wal.c` but not matching the flush rule below → B-tree and
     page handling;
   - defined in `pager.c`, `wal.c`, or `os_unix.c`, and its own body
     calls the VFS's `xSync` method, or a `fsync`/`fdatasync`/
     `F_FULLFSYNC`-family OS call, directly or through one further wrapper
     → flush;
   - defined in `os_unix.c`, and its own body calls `fcntl` for a file
     lock, or is one of `pager.c`'s lock-state-transition functions
     (identified by inspecting `pager.c` for a function that changes the
     pager's lock state field) → locking;
   - anything else (parser, code generator, memory allocator, API glue
     outside `vdbeapi.c`) → the rest.
4. Write the resulting `{function name → bucket}` table to
   `experiments/e4-sqlite-cost-anatomy/data/bucket-map.tsv` before the
   frozen run starts, and freeze it alongside this document.

A symbol encountered during the frozen run that the pilot never saw, and
that therefore has no entry in the written table, is logged to an
**unclassified** bucket and reported as its own number, never silently
folded into "the rest." If unclassified samples exceed 2% of a cell's
total samples, that cell's bucket breakdown is reported as provisional and
flagged, not treated as a finished result — the mapping is extended
(following the same rule above) and that cell is rerun before its number
is trusted.

### 4.4 Runs, warm-up, and variance

For each cell (one workload × one size × one `synchronous` setting, plus
the in-memory and flush-only controls of §3.6/§4.2):

1. **Sizing pilot.** Starting from 2,000 operations, double the operation
   count until a pilot `perf record` session for that count runs for at
   least 2 seconds of wall-clock time (checked from the recorded session's
   own duration). The resulting operation count is written into that
   cell's manifest and reused, unchanged, for every repetition of that
   cell — this adapts to how fast or slow a cell's operations are without
   letting anyone pick the count after seeing the real run's numbers,
   because it is fixed by a pilot that is itself discarded.
2. **Warm-up.** Before timing starts, 200 operations of that cell's
   workload run and are discarded from both the timing and the profiling
   — this lets the OS and SQLite's own page cache reach a steady state
   before any sample counts.
3. **Timed window.** The cell's frozen operation count (step 1) runs
   back-to-back, one transaction per operation, under `perf record`.
4. **Repetitions.** Each cell is run 5 times, each its own process and its
   own `perf record` session (so no state leaks between repetitions).
5. **Reported statistics.** Per repetition: the median per-operation
   wall-clock latency, and each bucket's share of total self-overhead
   samples. Across the 5 repetitions: the median of those per-repetition
   medians, and the min–max range across the 5 repetition-level numbers —
   a range, not a standard deviation, because flush-bound latencies are
   expected to be skewed, and a range makes no assumption about the shape
   of that skew.

## 5. Machine conditions

This machine is shared with other lanes and agents and is frequently
loaded; a run that looks fast only because nothing else was competing for
the CPU or the disk is not evidence.

**Hardware, recorded now by reading this machine, not assumed.**
CPU: AMD Ryzen 5 5600X, 6 physical cores, SMT2 (12 logical CPUs), per
`/proc/cpuinfo` and `lscpu` read on this machine on 2026-10-15. Storage:
`/dev/nvme0n1`, a non-rotational (NVMe SSD) device per `lsblk`'s `ROTA=0`.
Filesystem: `ext4`, mounted at `/`, which is also where this repository
and its benchmark scratch files live. If E4 ultimately runs on a different
machine, its harness re-collects and records these same facts fresh for
that machine — the numbers here describe this machine, they are not a
portable assumption about whatever machine actually runs the benchmark.

**CPU pinning.** The harness and its `perf record` session are pinned with
`taskset -c 2` to logical CPU 2, deliberately avoiding its SMT sibling
(logical CPU 8 on this machine, confirmed by reading
`/sys/devices/system/cpu/cpu2/topology/thread_siblings_list`, which reports
`2,8`) — pinning to a single logical CPU without also pinning something
else onto its SMT sibling avoids another process's instructions
competing for the same physical core's execution units underneath the
pinned thread. On a different machine, the sibling pair is re-derived by
reading that same sysfs file for whichever logical CPU is chosen, not by
reusing "2,8" as a fact about a different machine.

**Load bound.** A run counts only if the 1-minute load average
(`/proc/loadavg`, first field) stays below **18** (1.5× this machine's 12
logical CPUs) for the run's entire duration — the same 1.5×-cores bound
this environment already uses to decide whether to start a job at all, so
one number governs both "don't start" and "this run counts," rather than
two different ad hoc thresholds. The harness samples `/proc/loadavg` every
5 seconds while a timed window (§4.4) is running; if any sample is at or
above 18, the run in progress is discarded in full — not partially kept —
and that cell's repetition is retried from its warm-up step once the load
average drops back under the bound.

## 6. Outputs

**Tables.** One table per `synchronous` setting (FULL, NORMAL), with one
row per (workload × size) cell, columns: median per-operation latency, the
min–max range across repetitions, and the five bucket shares (flush, VDBE,
B-tree/page, locking, rest) each as a median percentage with its min–max
range. A separate, smaller table holds the in-memory control (§3.6, small
size only, all four workloads) and the flush-only control (§4.2, one row).

**Figures.** Four stacked bar charts (one per size × `synchronous` pair),
each with one bar per workload and five stacked segments per bar for the
bucket shares. One additional comparison chart shows, per workload at the
small size, the on-disk `synchronous=FULL` median latency next to the
in-memory control's median latency, with the gap between them labeled as
the estimated flush cost — the subtraction-based cross-check against
§4.1's perf-attributed flush bucket.

**Verdict rule, fixed now.** For a given (workload, size) pair under
`synchronous=FULL`, call the combined share of the non-flush buckets
(VDBE + B-tree/page handling + locking + the rest) `nonflush%`.

- **A small-data win is plausible** for that pair if `nonflush% ≥ 25%`
  under `synchronous=FULL`, or `nonflush% ≥ 50%` under
  `synchronous=NORMAL`, at the small size. Below the per-commit flush,
  there is enough of SQLite's own, flush-independent cost left for a
  closed-world compiler to plausibly remove — exactly the costs
  concept.md §12 names (parse/compile at request time, a B-tree walk a
  maintained view could skip, lock bookkeeping a single writer still
  pays).
- **Parity is the ceiling** for that pair if `nonflush% < 10%` under
  `synchronous=FULL` at the small size — the operation is, for practical
  purposes, almost entirely the device's own flush latency, matching
  concept.md §12's own statement of the floor ("durable single-writer
  latency is bounded by the storage device's flush time") with a number
  attached.
- A `nonflush%` between 10% and 25% at the small size is reported as
  **inconclusive** for that pair, not rounded into either verdict —
  mirroring how E1's own gate (`docs/experiments.md`) treats a coverage
  number strictly between its two thresholds.

The overall small-data ceiling this experiment reports is the set of
per-(workload, size) verdicts above, read together with the medium-size
numbers (expected, but not assumed, to show a smaller `nonflush%` as the
B-tree/page bucket grows with data that no longer fits the frozen cache,
per §2.3) — not a single collapsed number, because the four workloads are
not expected to land on the same side of the line.

**Raw data.** Every repetition's raw `perf.data` (or its exported text
report, if raw files are too large to keep in version control) and the
per-cell manifest (frozen settings actually used, the PRNG seed, the
on-disk byte size and `cache_size` check from §2.3, the detected
`sqlite3_libversion()` string from §3.1, and the hardware facts from §5 as
collected on the machine that produced that run) are stored under
`experiments/e4-sqlite-cost-anatomy/data/`, one subdirectory per cell. The
function-to-bucket mapping of §4.3 lives at
`experiments/e4-sqlite-cost-anatomy/data/bucket-map.tsv`, versioned
alongside this protocol.

## 7. Threats to validity, and what E4 does not answer

- **E4 says nothing about kiln's own speed.** Kiln has no implementation
  yet. This experiment bounds one side of the comparison — what SQLite's
  own time is made of — not the other side. A verdict of "a small-data win
  is plausible" is a statement about room existing to win in, not a
  measurement of kiln winning anything.
- **Sampling profilers can mis-attribute inlined code.** An optimizing
  build can merge functions from different modules into one apparent
  frame, which would blur the boundary §4.3's bucket rule assumes is
  clean. The harness cross-checks this by also running each cell once
  under a build with reduced inlining, reporting that run's bucket shares
  as a secondary number; a material disagreement between the two builds
  is reported as a limitation on the primary numbers, not quietly
  resolved in the primary numbers' favor.
- **One machine, one storage device, one filesystem.** The flush cost
  this experiment measures is specific to this machine's NVMe SSD and
  `ext4` filesystem (§5). A spinning disk, a different SSD controller, or
  a network or cloud block device would very plausibly show a different
  flush share, and WAL itself "does not work over a network filesystem"
  at all (https://www.sqlite.org/wal.html, fetched 2026-10-15). The
  verdict in §6 is a statement about this machine, not a portable claim
  until re-run elsewhere.
- **A shared, loaded development machine, not a dedicated benchmark
  host.** CPU pinning (§5) and the load-average admission bound (§5)
  reduce, but do not eliminate, interference from other processes sharing
  memory bandwidth, cache, or thermal headroom with the pinned core.
- **No concurrency is tested.** One connection, one thread, single writer
  (§3.5). The locking bucket measured here is SQLite's own single-writer
  bookkeeping, not contention between multiple writers; nothing in E4
  speaks to how SQLite or kiln behaves under concurrent writers — that
  question belongs to a different experiment (`docs/experiments.md`'s E2
  is the closest existing analogue, for kiln's own side, once kiln
  exists).
- **The insert workload's table grows during its timed window** (2.2.C),
  a bounded and documented departure from the "frozen row count" framing
  the other three workloads keep exactly.
- **The bucket mapping is mechanical but built from one pilot pass,**
  not an exhaustive static analysis of SQLite's source; §4.3's
  unclassified-bucket rule and its 2%-of-samples flag exist precisely
  because a first pilot pass is not guaranteed to see every function the
  frozen run will.

## References

- SQLite C API introduction (prepared statements, `sqlite3_prepare_v2`,
  `sqlite3_reset`, `sqlite3_bind`): https://www.sqlite.org/cintro.html
- SQLite WAL mode: https://www.sqlite.org/wal.html
- SQLite `PRAGMA synchronous`:
  https://www.sqlite.org/pragma.html#pragma_synchronous
- SQLite `PRAGMA cache_size`:
  https://www.sqlite.org/pragma.html#pragma_cache_size
- SQLite architecture (modules and their source files):
  https://www.sqlite.org/arch.html
- `rusqlite` crate, latest version and its `bundled` feature:
  https://lib.rs/crates/rusqlite
- `libsqlite3-sys` crate, latest version and its vendored SQLite version:
  https://lib.rs/crates/libsqlite3-sys
- `perf` tutorial (call graphs, sampling frequency, `perf report`
  attribution): https://perfwiki.github.io/main/tutorial/

All URLs above were fetched 2026-10-15, the day this protocol was written.
