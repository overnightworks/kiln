# E4: where SQLite spends its time on small transactions

Status: preregistered protocol, not yet reviewed or frozen. No benchmark,
build, or profiler has run against this document. Every number below is a
setting, a threshold, or a measured machine fact, decided in advance. Once
frozen, nothing below changes after a measurement run is seen (the freeze
rule in `docs/experiments.md`). A different agent must be able to build and
run E4 from this file alone.

## 1. Question and verdict quantity

Where does SQLite spend time on a small, single-row-or-few-row transaction
— flushing to disk, writing pages, or something else — and how does that
change between a dataset that fits SQLite's page cache and one that does
not? `docs/concept.md` §12 names "parity, not a win" as the ceiling on a
single-writer path bound by flush time, and names a skippable B-tree walk
and per-writer lock bookkeeping as removable costs — not parse/compile at
request time, a third cost it names but out of scope, since §2's cached
prepared statements already remove it from every operation. E4 puts a
number on the ceiling for the two costs it does measure, before kiln
exists.

**Verdict quantity.** Per (workload, size, `synchronous` setting), the share
of per-operation wall time spent outside `xSync` and `xWrite` calls (§5)
— the directly measured, flush-independent, write-independent remainder,
the only part a closed-world compiler could plausibly shrink. §10 fixes
the thresholds and decision rule.

## 2. Schema and workloads

```sql
CREATE TABLE accounts (
  id      INTEGER PRIMARY KEY,
  email   TEXT NOT NULL,
  balance INTEGER NOT NULL
);
CREATE UNIQUE INDEX accounts_email ON accounts(email);
```

`id` aliases the rowid, so a point lookup/update by `id` touches one B-tree.
`accounts_email` is the one secondary index; only workload C uses it.

Every workload is one transaction per call: `BEGIN DEFERRED` for reads (no
write lock needed), `BEGIN IMMEDIATE` for writes (the write lock starts at
`BEGIN`, not the first write statement). `BEGIN`, `COMMIT`, and each
statement are prepared once per connection at startup and reused by
`sqlite3_reset()` + rebind, never re-prepared per call
(https://www.sqlite.org/cintro.html, fetched 2026-10-15 — "the time
needed to run `sqlite3_prepare()` equals or exceeds `sqlite3_step()`").
The harness never calls `rusqlite`'s `Connection::transaction()` helper,
since it issues `BEGIN`/`COMMIT` through `execute_batch`, a one-off call,
not a cached statement
(https://docs.rs/rusqlite/0.40.2/src/rusqlite/transaction.rs.html, fetched
2026-10-15); it prepares the three `BEGIN` variants and `COMMIT` itself.

- **A. Point read.** `BEGIN DEFERRED; SELECT id, email, balance FROM
  accounts WHERE id = ?1; COMMIT;` `?1` uniform random in
  `[1, row_count]`.
- **B. Point update.** `BEGIN IMMEDIATE; UPDATE accounts SET balance =
  balance + 1 WHERE id = ?1; COMMIT;` `?1` uniform random in
  `[1, row_count]`.
- **C. Insert under a unique index.** `BEGIN IMMEDIATE; INSERT INTO
  accounts (id, email, balance) VALUES (?1, ?2, 0); COMMIT;` `?1` is the
  next id from a counter starting at `row_count + 1`. `?2` is 16 random
  lowercase hex characters (leading, so inserts land in random B-tree
  regions, not append-only ones) followed by `-` and the repetition
  counter (uniqueness). On `SQLITE_CONSTRAINT` (collision), redraw the
  random component once; a second collision aborts the run as a protocol
  violation.
- **D. Transfer.** One transaction: `SELECT balance FROM accounts WHERE id
  = ?1;`; if that balance is ≥ the transfer amount (fixed at **1**, §2.1's
  `balance` unit), `UPDATE accounts SET balance = balance - 1 WHERE id =
  ?2;` then `UPDATE accounts SET balance = balance + 1 WHERE id = ?3;`.
  `?1`/`?2` are bound to the same value, the sender's id, so the guard
  reads exactly the row the first `UPDATE` debits; `?3` is an independent
  uniform-random draw in `[1, row_count]`, redrawn on a collision with
  `?1`/`?2`. Every repetition commits one transaction regardless of the
  guard outcome; the harness counts how often it blocked a transfer.

### 2.1 Inputs, frozen

- **PRNG.** `ChaCha8Rng` from `rand_chacha` `0.10.0`
  (https://lib.rs/crates/rand_chacha, fetched 2026-10-15), re-seeded at
  the start of every repetition **attempt** (original or retried, §9)
  from `1 + cell_index × 1000 + repetition_index` — never one seed running
  across a whole cell, so a retry draws the same trace as the attempt it
  replaces. `1` is the simplest base value not itself a tuned parameter;
  the offsets keep every (cell, repetition) seed distinct.
- **Initial load.** Each size (§3) loads once with `id = 1..row_count`,
  `balance = 1000000` (large enough D's guard almost never fires), and
  `email` = the first 16 lowercase hex characters of `SHA-256` over the
  decimal ASCII encoding of `id` (not the repetition PRNG, a collision a
  protocol violation) plus `-init-` and `id` — C's inserted-emails'
  random-leading shape (§2), so initial and inserted keys interleave through
  the B-tree instead of concentrating. The harness then asserts the row
  count, closes the loading connection, opens a second connection, and runs
  `PRAGMA wal_checkpoint(TRUNCATE);` on it ("checkpoints all frames... and
  syncs the database file," truncating the WAL "to zero bytes upon successful
  completion," https://www.sqlite.org/pragma.html#pragma_wal_checkpoint,
  fetched 2026-10-15), and copies that main file plus the empty WAL to
  `data/<cell>/pristine/` — the state restored before every attempt.
- **Per-repetition correctness checks.** After B/D: no `balance` negative.
  After D: `SELECT sum(balance)` unchanged (zero-sum). After C: row count
  grew by exactly the operation count.
- **Baseline restore.** The pristine backup is copied back over the
  working database/WAL files before every attempt, outside the timed
  window — including A, B, D, which mutate values but not row count: a
  fresh process alone does not restore on-disk files, so a repetition
  would otherwise inherit whatever the previous attempt left.

## 3. Sizes

Two sizes, both frozen:

- **Small: 10,000 rows.**
- **Medium: 10,000,000 rows.**

§4 fixes a 64 MiB page-cache budget. 10,000 rows sit far below it;
10,000,000 sit far above it under any budget this protocol freezes —
three orders of magnitude, so the two sizes land unambiguously on
opposite sides of the cache boundary. After loading, the harness reads
the on-disk byte size (main + WAL) and active `cache_size` into the
manifest, and reports a violation if the fits/does-not-fit relation
fails.

### 3.1 Key distribution

Uniform random over the live key range for every workload reading an
existing key (A, B, D) — not insertion order, not skewed — because the
question is SQLite's unconditional per-operation cost, not hot-key caching.
Workload C's keys are described in §2.

## 4. Connection settings, frozen

- **SQLite version.** Linked via `rusqlite` `0.40.2`'s `bundled` feature,
  vendoring SQLite `3.53.2` via `libsqlite3-sys` `0.38.2` (both latest as
  of 2026-10-15: https://lib.rs/crates/rusqlite,
  https://lib.rs/crates/libsqlite3-sys, fetched 2026-10-15), pinned
  exactly in `Cargo.lock`. The harness calls `sqlite3_libversion()` at
  startup and writes it into the manifest; a mismatch is a logged
  deviation.
- **WAL and synchronous.** `PRAGMA journal_mode = WAL;` once per
  connection; both `PRAGMA synchronous = FULL;` and `= NORMAL;` are
  measured, every cell under both. In WAL mode, writers sync on commit
  under FULL but skip it under NORMAL, which "does lose durability" but
  stays consistent (https://www.sqlite.org/pragma.html#pragma_synchronous,
  fetched 2026-10-15). FULL is concept.md §12's realistic ceiling; NORMAL
  approximates what is left once the flush is gone.
- **Page cache.** `PRAGMA cache_size = -65536;` once per connection — an
  explicit 64 MiB budget, not the ~2 MiB default
  (https://www.sqlite.org/pragma.html#pragma_cache_size, fetched
  2026-10-15), the budget §3's two sizes straddle.
- **Page size and checkpointing.** `PRAGMA page_size = 4096;` (this
  machine's block size), set before table creation. `PRAGMA
  wal_autocheckpoint = 1000;`, SQLite's documented default
  (https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint, fetched
  2026-10-15), stays in force, so a checkpoint fires naturally and its
  cost is measured, not hidden — "an occasional COMMIT" becomes "much
  slower" (https://www.sqlite.org/wal.html, fetched 2026-10-15), and
  §5/§8 weight that cost by wall time rather than averaging it away.
- **One connection, one thread, single writer.** Measures a transaction's
  own cost, not contention (E2's analogue once kiln exists).
- **In-memory control.** Every workload also runs once, small size only,
  against `:memory:` with the same `cache_size` and statement discipline,
  no `journal_mode`, no `xSync`/`xWrite` to measure. §6 uses it as a
  cross-check, not a primary number.
- **Effective-settings check.** The harness queries `page_size`,
  `cache_size`, `wal_autocheckpoint`, `synchronous`, `journal_mode` after
  opening and writes them into the manifest; any mismatch is logged.

## 5. Primary measurement: the VFS shim

A sampling profiler cannot see this experiment's main signal: a thread
blocked inside `fsync` accrues no CPU cycles, so a cycle-sampling profiler
attributes it near-zero time no matter how long the wait actually was
(https://perfwiki.github.io/main/tutorial/#profiling-sleep-times, fetched
2026-10-15 — cycle sampling "does not count when the processor is idle").
E4's gating measurement is therefore a direct timer at the real boundary,
not a statistical sample of an indirect proxy.

**The shim.** A custom `sqlite3_vfs`, registered with
`sqlite3_vfs_register(&shimVfs, 0)` (`makeDflt = 0`), wraps the default
VFS found via `sqlite3_vfs_find(0)` — the documented shim pattern
(https://www.sqlite.org/vfs.html, fetched 2026-10-15: "a particular VFS
might just be a thin wrapper around another VFS that does the real
work... We call a wrapper VFS a 'shim'"). Its `io_methods` delegate every
call unchanged (https://www.sqlite.org/c3ref/io_methods.html, fetched
2026-10-15) except `xWrite`/`xSync`, timed with
`clock_gettime(CLOCK_MONOTONIC, ...)`. `xOpen`'s flags carry
`SQLITE_OPEN_MAIN_DB`/`SQLITE_OPEN_WAL`
(https://www.sqlite.org/c3ref/c_open_autoproxy.html, fetched 2026-10-15),
so counts/durations are kept per file kind: an ordinary commit never
calls `xSync` on the main-file handle, only the WAL file, so the
main-file `xSync` count is exactly the checkpoint count.

**Per-operation accounting.** The harness is single-threaded (§4), so the
shim's accumulators are plain globals, reset by the harness immediately
before issuing an operation's first statement and read immediately after
its `COMMIT` step returns. For each operation the harness records:

- **wall** — `CLOCK_MONOTONIC` delta from just before the first statement
  to just after `COMMIT` returns. PRNG draws/key formatting (§2) happen
  before this window opens (the call's inputs, not SQLite's work);
  result-column materialization (`sqlite3_column_*` after `sqlite3_step`
  on A's/D's `SELECT`) happens inside it — SQLite's own work on the path
  §1 measures.
- **xsync_time** — sum of the shim's timed `xSync` durations in that
  window.
- **xwrite_time** — sum of the shim's timed `xWrite` durations, its own
  bucket, never folded into "outside flush."
- **remainder** = `wall - xsync_time - xwrite_time`, exact per operation
  — the three always sum to `wall`. §8 aggregates by summing each over a
  repetition before dividing, never medianing a share first, so the same
  exactness holds at the level §10's verdict rule consumes — and so a
  checkpoint firing inside a commit (§4), whose `xSync` cost lands inside
  that one operation's `xsync_time`, is weighted by the wall time it
  actually occupied rather than lost to a per-operation median.
- **cpu_time** — `clock_gettime(CLOCK_THREAD_CPUTIME_ID, ...)` delta,
  measured by the harness, not the shim — "measures CPU time consumed by
  this thread," "not settable"
  (https://man7.org/linux/man-pages/man2/clock_gettime.2.html, fetched
  2026-10-15). A cross-check, not a fourth bucket: it should sit near
  zero during `xsync_time` and track `remainder + xwrite_time` otherwise;
  a material, repeated disagreement is a limitation (§14), not resolved
  silently.

**Overhead acceptance bound, frozen.** A second, identical build with a
passthrough shim (no timer calls) measures what the two `clock_gettime`
calls per `xWrite`/`xSync` cost. Bound: the instrumented build's
per-repetition `Σwall/N` (§8) must not exceed the uninstrumented's by
more than **2%** — a VDSO read costs tens of nanoseconds, so 2% covers
that plus jitter while still catching an unexpectedly costly shim. A
failed bound is repaired by cheaper instrumentation, never loosened;
§13's pilot checks it first. The gating run never runs under `perf`; §7
is a separate invocation of the same binary afterward.

**Which cells the bound gates.** The bound gates exactly the cells whose
timed transaction (§8.3's window) calls `xWrite` or `xSync` at least
once over the repetition: workloads B, C, D, at both sizes and under
both `synchronous` settings — every commit of a `BEGIN IMMEDIATE` write
appends to the WAL ("Writers merely append new content to the end of
the WAL file," https://www.sqlite.org/wal.html, fetched 2026-10-15),
and even under NORMAL a checkpoint still syncs ("with `PRAGMA
synchronous` set to NORMAL, the checkpoint is the only operation to
issue an I/O barrier or sync operation," same page). Workload A's
transaction (`BEGIN DEFERRED; SELECT...; COMMIT;`) starts a read
transaction, not a write one — "If the first statement after BEGIN
DEFERRED is a SELECT, then a read transaction is started"
(https://www.sqlite.org/lang_transaction.html, fetched 2026-10-15) —
and a reader never appends a commit record to the WAL the way a
writer does; the same holds for the `:memory:` control (§4: "no
`xSync`/`xWrite` to measure"). For these read-only cells the
instrumented and passthrough builds execute the identical path inside
the timed window — there is nothing for the shim's timer calls to
wrap — so the 2% bound is satisfied **by construction**, not by
measurement, and no instrumented-versus-passthrough comparison runs
for them. A measured difference on a read-only cell is nuisance
variation, not timer overhead, and is never grounds to waive, widen,
or otherwise reinterpret the bound for a gated cell.

**How the bound is judged.** For each gated cell, the pilot (§13) runs
**9** interleaved instrumented/passthrough pairs — alternating
instrumented, passthrough, instrumented, ... the same way §8.6
interleaves cells, so a load spike biases both sides of the comparison
equally — each pair running that cell's full warm-up and operation
count (§8.1–§8.3) once per build, from the same restored pristine
state and input trace (§2.1). 9 is not a new number invented for this
check: it is §8.4's own frozen repetition count, and its confidence
argument carries over unchanged — the min and max of 9 i.i.d. per-pair
overhead ratios bound the true population median with
`(1 − 2×0.5⁹) ≈ 99.609%` confidence, exactly as §8.4 states for the
gating repetitions. One convention governs every min/max bound in this
protocol, not a second one for this check. Each pair's **overhead
ratio** is `instrumented Σwall/N ÷ passthrough Σwall/N − 1`. The judged
statistic is the **median** of the 9 pairs' overhead ratios together
with their **min–max range** — median and min/max, not a mean or
standard deviation, because §8.5 already prefers them for this
protocol's skewed timings and reusing that convention here avoids a
second rule.

Three outcomes, mutually exclusive, mirroring §10's own bound structure:

- **Pass** if the range's **max** `< 2%` — every one of the 9 pairs
  clears the bound, so the 2% ceiling holds even at its noisiest pair.
- **Fail** if the range's **min** `> 2%` — every one of the 9 pairs
  exceeds the bound, so the excess is overhead, not noise.
- **Inconclusive** if the range straddles 2% (`min ≤ 2% ≤ max`) — the
  pairs disagree among themselves by more than the bound itself, so
  this check cannot separate real timer overhead from run-to-run noise
  for that cell.

A **fail** or an **inconclusive** result keeps that cell out of the
result-bearing run: it is not measured for §10's verdict until the
bound passes. The bound itself is never loosened, widened, or waived
for a failing or straddling cell — the only repair is cheaper
instrumentation (fewer or lighter timer calls in the shim), re-checked
by the same nine-pair procedure before that cell may run. Cells whose
bound already passed, and cells the bound does not gate, are
unaffected and proceed on schedule.

The shim's own code is reviewed, not benchmarked; it adds two timer calls
around two already-existing method calls, nothing else.

## 6. Controls

- **Flush-only control.** A SQLite-free mode opens a scratch file beside
  the database files and repeats, for §8's operation count: append a fixed
  512-byte buffer, then `fdatasync` — append, since every workload here
  only appends to the WAL between checkpoints (§4); `fdatasync`, not
  `fsync`, since it "does not flush modified metadata unless that metadata
  is needed," and here each append grows the file, so `fdatasync` may also
  flush size metadata, the same cost a growing WAL append incurs
  (https://man7.org/linux/man-pages/man2/fdatasync.2.html, fetched 2026-10-15).
  Timed the same way (`CLOCK_MONOTONIC`), giving this machine's device-level
  flush cost to sanity-check `xsync_time` against (§14: a cross-check, not a decomposition).
- **In-memory cross-check.** Small-size, `synchronous=FULL`, on-disk
  median wall time minus the `:memory:` median (§4) is an **upper-bound**
  cross-check, not a second estimate: `:memory:` removes every
  `xWrite`/`xSync` call entirely, so the difference also includes
  file-only work (page allocation, WAL bookkeeping) no closed-world
  proposal has committed to removing. Reported alongside `xsync_time +
  xwrite_time` as a ceiling check, not averaged in; a material
  disagreement is reported, not resolved either way.

## 7. Secondary, non-gating breakdown (perf)

Run only if the operator has granted `perf` access for this session. If not
granted, this section is skipped and the output says so explicitly — never
silently retried with reduced permissions.

If run: `perf record --call-graph dwarf -F 999 --delay=-1 --control=fifo:ctl,ack
-- <harness invocation>` (`-1` starts disabled; the harness sends `enable`
before an operation's first statement and `disable` after `COMMIT`
returns, so sampling covers exactly the timed window —
https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-record.txt,
fetched 2026-10-15). `-F 999` avoids lockstep with round-frequency system
activity (perf's default is 1000 Hz,
https://perfwiki.github.io/main/tutorial/, fetched 2026-10-15).

This reports **period-weighted CPU samples only** ("the sum of all the
'self' overhead values should be 100%,"
https://perfwiki.github.io/main/tutorial/#overhead-calculation, fetched
2026-10-15) — a CPU-sampled profile of the whole enable/disable window,
flush included, since the FIFO brackets one full operation, not the
`xWrite`/`xSync` sub-intervals §5 times separately; nothing here
subtracts a sample already counted as `xsync_time`/`xwrite_time` by §5 —
the two sections measure the same window by overlapping methods. Buckets
are named for what `sqlite3.c`'s own file boundaries contain, by line
range: the amalgamation marks source files with `/* Begin file <name>
*/`/`/* End file <name> */`, and before the frozen run the harness greps
these into a `{source file → [start_line, end_line)}` table, disjoint and
exhaustive by construction. Each sample's line is resolved from debug
info (`addr2line -i`, to its **innermost** frame when inlined), looked up
in that table, and bucketed per `sqlite3`'s architecture page
(https://www.sqlite.org/arch.html, fetched 2026-10-15): VDBE
(`vdbe.c`/`vdbeaux.c`/`vdbeapi.c`/`vdbemem.c`); B-tree/page
(`btree.c`/`pager.c`/`wal.c`/`pcache.c`/`pcache1.c`); **"unix VFS (reads,
locking, other)"** for `os_unix.c` — not "locking" alone, since it also
implements `xRead`; everything else is "rest." A sample outside every
range is **unclassified**, never folded into "rest." The primary build
(§12) keeps normal optimization and inlining — disabling it to chase
attribution precision risks changing the measured cost (finding 5).

The line-range table is written once to `data/bucket-map.tsv` before the
frozen run, not extended afterward; a cell whose unclassified share
exceeds 2% is provisional. No `Begin file`/`End file` markers means this
section reports itself unable to run rather than guessing boundaries.

## 8. Run procedure

For each cell (one workload × one size × one `synchronous` setting, plus
the in-memory and flush-only controls):

1. **Operation count, fixed, not duration-driven.** 2,000 for A, B, D;
   500 for C — same for every `synchronous` setting and repetition, no
   adaptation to run speed. C's count is smaller because it grows the
   table and growth must stay small relative to `row_count` (§3): 500
   plus the warm-up below is ≤7% of the small size's 10,000 rows.
2. **Warm-up.** 200 operations run first, discarded from timing and the
   shim's counters, letting the OS and page cache reach steady state. For
   C, warm-up insertions count toward the growth bound above; the
   pristine restore (§2.1) happens before warm-up, once per attempt.
3. **Timed window.** The cell's fixed operation count runs back-to-back,
   one transaction per operation, shim counters and `cpu_time` recorded
   individually (§5).
4. **Repetitions.** 9 per cell, each its own process, from the same
   restored pristine state and input trace (§2.1) — i.i.d. under that
   restoration, the assumption the bound below relies on. The min/max of
   `n` i.i.d. repetition summaries (`Σremainder/Σwall`, below) bound the
   true population median with `(1 − 2×0.5ⁿ)` confidence. `n = 8` already
   clears 99% (`≈99.219%`); 9, the next count up, is chosen odd over the
   sufficient 8 because an odd `n` has one well-defined middle value,
   where even `n` needs an averaging convention; 9 gives `≈99.609%`.
5. **Reported statistics.** Per repetition: `Σxsync_time`,
   `Σxwrite_time`, `Σremainder`, `Σwall`, summed over every operation
   (checkpoints included, §5); the first three divided by `Σwall` give
   that repetition's share — exact, summing to 100%, and the gating
   quantity §10 consumes (not a per-operation median's share). Also:
   `Σwall/N` (mean per-operation wall time) and the latency tail
   (99th-percentile/maximum per-operation `wall`). Across the 9
   repetitions: the median of each statistic and its min–max range (not
   a standard deviation — flush-bound latencies are expected skewed).
6. **Cell order.** Interleaved — repetition 1 of every cell, then
   repetition 2, and so on — so a load spike biases every cell equally.

## 9. Machine conditions

Hardware, read 2026-10-15: AMD Ryzen 5 5600X, 6 physical cores, SMT2 (12
logical CPUs, `/proc/cpuinfo`/`lscpu`); storage `/dev/nvme0n1`,
non-rotational per `lsblk`'s `ROTA=0`; filesystem `ext4` at `/`. A
different machine re-collects these fresh; not a portable assumption.

- **Pinning.** The harness runs under `taskset -c 2` (`-c`: "a numerical
  list of processors," https://man7.org/linux/man-pages/man1/taskset.1.html,
  fetched 2026-10-15). Its SMT sibling is CPU 8, confirmed via
  `/sys/devices/system/cpu/cpu2/topology/thread_siblings_list` (`2,8`). The
  monitor and, if §7 runs, `perf record` both run `taskset -c` pinned to
  the complement of `{2, 8}`, never merely "unpinned," since an unpinned
  process can still land there by scheduler choice.
- **Interference monitoring.** The pinned monitor samples every 200 ms:
  `/proc/stat`'s idle field for CPU 8; `/proc/schedstat`'s field 8 for
  CPU 2 ("sum of all time spent waiting to run by tasks on this processor
  (in nanoseconds)," https://docs.kernel.org/scheduler/sched-stats.html,
  fetched 2026-10-15 — `/proc/stat` lacks this signal, so CPU 2 needs
  schedstat where CPU 8's idle check does not); and `/proc/loadavg`.
  200 ms catches a burst within a multi-second run; 5 seconds could miss
  one entirely.
- **Interference limits, frozen.** A sample fails the repetition if: CPU
  8 shows any non-idle time; CPU 2's schedstat field 8 increases by more
  than **1,000,000 ns (1 ms)** over the window (decision: hundreds of
  sub-millisecond operations fill a window, so 1 ms of wait implies real
  contention, not noise); or `/proc/loadavg`'s 1-minute average is at or
  above **18** (1.5× 12 logical CPUs, the bound this environment already
  uses to gate a job).
- **Retry budget.** A repetition failing a sample above is discarded and
  retried from warm-up (§8.2) with the same input trace (§2.1) — up to
  **3 retries (4 attempts)**: one clears a single spike, two more guard a
  second, and a 4th shows sustained contention for that repetition, marked
  `FAILED-INTERFERENCE` and logged (below). A cell's verdict (§10) requires
  all 9 repetitions to succeed; one exhausted repetition makes that cell
  `inconclusive — insufficient repetitions` (distinct from §10's
  inconclusive) instead of stopping the run — the remaining cells still run
  to completion. Warm-up count and cell order (§8.2, §8.6) stay fixed per attempt.
- **Rejected-attempt log.** Every discarded attempt (timestamp, cell,
  attempt, reason) is appended to `data/rejected.log`, never overwritten.

## 10. Verdict rule

For a (workload, size, `synchronous`) cell, the gating quantity is each
repetition's `Σremainder/Σwall` (§8.5), summarized by its **min** and
**max** across the 9 repetitions — not the median, since this is a
boundary check and the min/max already carry §8.4's confidence bound.
FULL and NORMAL are judged separately, never combined — NORMAL mostly
skips the per-commit sync (§4), so it answers what is left once the
flush is gone, versus FULL's what is left while paying the ceiling.

A genuinely removable wall-time fraction `f` bounds the best-case speedup
at `1 / (1 - f)` — e.g. `0.10 → 1.11×`, `0.25 → 1.33×`, `0.50 → 2×` —
**conditional** on removing `remainder` entirely while `xsync_time`/
`xwrite_time` stay as measured: kiln need not match SQLite's own I/O
cost, so this is what a closed-world compiler could win on the
flush/write path unchanged, not an unconditional prediction. This fixes
the thresholds below, naming the smallest/largest conditional bounds
worth distinguishing, not round percentages:

- **A small-data win is plausible** if **min** `≥ 20%` (speedup ≥ 1.25×,
  the smallest bound worth a closed-world compiler's engineering cost).
- **A low conditional ceiling** holds if **max** `< 5%` (speedup
  `1/0.95 ≈ 1.0526×` — stated "< 1.0526×", not "< 1.05×").
- **Inconclusive** otherwise — neither bound holds, including a range
  straddling both figures. Not rounded to either side.

These three outcomes are mutually exclusive, applied to every cell
independently. The medium size is expected to show a **larger**
`Σremainder/Σwall` than the small size: only `xWrite`/`xSync` are timed
out of `remainder` (§5), so a larger-than-cache set's extra cost —
reading pages outside the 64 MiB budget (§3, §4) — lands inside
`remainder`; the verdict table (§11) reports what is measured.

E4's overall statement is the full table of per-cell verdicts, never one
collapsed number — the four workloads need not land on the same side.

## 11. Outputs

**Tables.** One table per `synchronous` setting, one row per (workload ×
size), columns: the median and min–max range, across repetitions, of
`Σwall/N`, the 99th-percentile and maximum per-operation `wall`, and the
`xsync_time`/`xwrite_time`/`remainder`/`cpu_time` shares (§8.5), plus the
§10 verdict. A separate table holds the in-memory and flush-only controls.
If §7 ran, its module breakdown is a separate, clearly labeled secondary
table.

**Figures.** One stacked-bar chart per `synchronous` setting, one bar per
(workload × size), three segments (xsync, xwrite, remainder); one chart
compares on-disk `FULL` median wall time against the in-memory control,
small size, per workload (§6).

**Raw data.** Every repetition's per-operation `wall`/`xsync_time`/
`xwrite_time`/`remainder`/`cpu_time`; the rejected-attempt log (§9); each
cell's manifest (settings, PRNG seed, byte size, `cache_size` check,
`sqlite3_libversion()`, hardware facts) under `data/`, one subdirectory
per cell. If §7 ran, `bucket-map.tsv` lives alongside this protocol.

## 12. Prerequisites and operator grants

**Needed from the operator.** A Rust toolchain (not installed yet):
install via rustup (https://rustup.rs), then `rustup toolchain install
stable` matching the pin below. `perf` access, only if §7 runs.

**Toolchain pin.** A `rust-toolchain.toml` pins the latest stable Rust
release when the harness is first built, confirmed by `rustc --version
--verbose` and written into the manifest like `sqlite3_libversion()`
(§4). The C compiler is whichever of `$CC`, `cc`, `gcc`, `clang` the `cc`
crate finds first on `$PATH` (recorded; none found fails closed).
`libsqlite3-sys`'s build script sets no C optimization/debug flags of
its own (pinned-tag source,
https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/build.rs,
fetched 2026-10-15) — it inherits Cargo's profile unless overridden via
`CC`/`CFLAGS` (the `cc` crate reads these,
https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables,
fetched 2026-10-15) or the script's own `LIBSQLITE3_FLAGS` (extra
`-D`/`-U`/`SQLITE_*` defines) and `LIBSQLITE3_SYS_USE_PKG_CONFIG`
(non-`"0"` skips the bundled build). Any of these four, set in the
environment, can silently change what this protocol measures, so
preflight asserts all are unset.

**Build.** `cargo build --release`, `[profile.release]` =
`opt-level = 3`, `debug = true` (symbols for §7), `lto = false` — the one
build measurements run against; §7 uses the same build.

**Preflight, non-result-bearing, fails closed.** Before any frozen run:
`rustc --version` matches the pin; `sqlite3_libversion()` returns
`3.53.2`; `CC`, `CFLAGS`, `LIBSQLITE3_FLAGS`,
`LIBSQLITE3_SYS_USE_PKG_CONFIG` are unset; CPU 2/8 confirmed SMT siblings;
the shim VFS registers and `sqlite3_vfs_find` returns non-null;
`addr2line -i` resolves a known `sqlite3_step` address into the
`vdbeapi.c` range §7's bucket-map assigns it, proving symbolization works
before the frozen run. Preflight hashes the harness binary (SHA-256) into
the manifest; the result-bearing run re-hashes and fails closed on a
mismatch — preflight's build must be the exact bytes producing the
result, not a rebuild in between. If §7 is requested, `perf stat -e
cycles:u -- /bin/true` and the symbolization check must both pass; either
failing skips §7 alone (logged), the rest proceeds.

## 13. Feasibility pilot

After freezing, before the result-bearing run, a pilot checks mechanics
only — never the quantities §10 judges.

**What it measures.** Per-operation `wall` for instrumented and
uninstrumented builds (§5's overhead bound); run duration; monitor
sample coverage (200 ms samples §9 expects versus written); and, if §7
is granted, `perf` sample count per operation, proving the FIFO
bracketing gates sampling. **Its output never contains `xsync_time`,
`xwrite_time`, `remainder`, or any ratio from them** — only mechanics —
so it cannot leak the verdict quantity into how the protocol is judged.

**What it may change.** A finding may justify amending, each logged with
its reason and the binary's SHA-256 before/after, to
`data/amendments.log`: monitor CPU mask, sampling method/interval,
bracketing, perf FIFO ack handling, retry mechanics, logging.
**Everything else is untouchable** — workloads (§2), inputs (§2.1),
verdict quantity (§1, §10), thresholds (§10), repetition count (§8.4),
operation count (§8.1), overhead bound (§5). The operation count is
untouchable for the same reason the repetition count is: it fixes what
every repetition's `Σwall`, shares, and tail latencies measure, and
workload C's ≤7% growth bound (§8.1) is derived from it, so changing it
changes the measured quantity itself, not merely how the protocol
measures or judges it. A finding against one of these means
re-reviewing and re-freezing, not amending.

**Before the result-bearing run**, a delta check reads
`amendments.log`: every entry names an amendable knob; the hash chain is
continuous; the current binary's hash matches the last logged "after"
(preflight's if empty). Any break fails the run closed.

## 14. Threats to validity

- **E4 says nothing about kiln's own speed** — it bounds SQLite's side only.
- **One machine, device, filesystem.** Flush numbers are specific to this
  NVMe SSD and `ext4`; WAL "does not work over a network filesystem"
  (https://www.sqlite.org/wal.html, fetched 2026-10-15).
- **Shared, loaded machine.** Pinning and the monitors (§9) reduce, not
  eliminate, interference from memory, cache, or thermal sharing.
- **No concurrency.** One writer, one connection (§4); locking cost is
  single-writer bookkeeping, not contention.
- **Workload C's table grows during its window** — bounded to ≤7% (§8).
- **§7's module mapping comes from one pilot pass**, not exhaustive
  static analysis — hence the unclassified-bucket and 2% rules.
- **The C compiler is recorded, not version-pinned** (§12) — accepted
  rather than inventing false precision.

## 15. Amendment log

- **2026-10-15 — §5 scope and judging procedure; §13 operation-count
  classification.** The feasibility pilot (#28) and its independent
  review found two gaps in the then-frozen protocol: §5's 2% overhead
  bound did not say which cells it gates or how it is judged against
  measured noise, and §13 classified neither the operation count (§8.1)
  as amendable nor as untouchable. This amendment: (1) restricts the
  bound to cells whose timed transaction calls `xWrite` or `xSync` —
  workloads B, C, D — and states that a read-only cell (workload A, the
  `:memory:` control) satisfies the bound by construction, since the
  instrumented and passthrough builds execute identical code inside its
  window; (2) replaces a single instrumented-versus-passthrough
  comparison with 9 frozen interleaved pairs per gated cell, judged by
  the median and min–max range of their overhead ratios under a
  three-way pass/fail/inconclusive rule, with a failing or inconclusive
  cell held out of the result-bearing run until cheaper instrumentation
  repairs it — the bound itself is never loosened; (3) classifies the
  operation count (§8.1) as untouchable, for the same reason the
  repetition count already is. These rules were decided from the
  protocol's own existing conventions (§8.4's confidence argument,
  §8.5's median/min–max preference, §10's three-way bound structure)
  and SQLite's documented transaction/WAL semantics, before any
  result-bearing run, and without using the pilot's measured numbers —
  including its reported 3.66% point-read overhead — to set, tune, or
  waive any threshold.

## References

- SQLite VFS `io_methods`, VFS/shims, `vfs_find`/`vfs_register`, `xOpen`
  flags: https://www.sqlite.org/c3ref/io_methods.html,
  https://www.sqlite.org/vfs.html, https://www.sqlite.org/c3ref/vfs_find.html,
  https://www.sqlite.org/c3ref/c_open_autoproxy.html
- SQLite C API intro (prepared statements): https://www.sqlite.org/cintro.html
- SQLite WAL mode/checkpointing: https://www.sqlite.org/wal.html
- SQLite `PRAGMA synchronous`/`cache_size`/`wal_autocheckpoint`/
  `wal_checkpoint`: https://www.sqlite.org/pragma.html (anchors
  `#pragma_synchronous`, `#pragma_cache_size`,
  `#pragma_wal_autocheckpoint`, `#pragma_wal_checkpoint`)
- SQLite transaction types: https://www.sqlite.org/lang_transaction.html
- SQLite architecture: https://www.sqlite.org/arch.html
- Linux man pages: `clock_gettime(2)`
  https://man7.org/linux/man-pages/man2/clock_gettime.2.html,
  `fdatasync(2)` https://man7.org/linux/man-pages/man2/fdatasync.2.html,
  `taskset(1)` https://man7.org/linux/man-pages/man1/taskset.1.html
- `/proc/schedstat` per-CPU fields: https://docs.kernel.org/scheduler/sched-stats.html
- `perf` tutorial and `perf record` manual:
  https://perfwiki.github.io/main/tutorial/,
  https://perfwiki.github.io/main/tutorial/#profiling-sleep-times,
  https://perfwiki.github.io/main/tutorial/#overhead-calculation,
  https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-record.txt
- `rusqlite`/`libsqlite3-sys`/`rand_chacha` crates and the
  `libsqlite3-sys` build script: https://lib.rs/crates/rusqlite,
  https://lib.rs/crates/libsqlite3-sys, https://lib.rs/crates/rand_chacha,
  https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/build.rs
- `cc` crate environment variables:
  https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables
- `rusqlite`'s `Transaction` helper (`execute_batch`, not cached):
  https://docs.rs/rusqlite/0.40.2/src/rusqlite/transaction.rs.html

All URLs above were fetched 2026-10-15, the day this protocol was rewritten.
