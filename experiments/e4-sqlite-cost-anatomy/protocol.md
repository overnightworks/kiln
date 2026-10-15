# E4: where SQLite spends its time on small transactions

Status: preregistered protocol, not yet reviewed or frozen. No benchmark,
build, or profiler has run against this document. Every number below is a
setting, a threshold, or a measured machine fact, decided in advance. Once
frozen, nothing below changes after a measurement run is seen (the freeze
rule in `docs/experiments.md`). A different agent must be able to build and
run E4 from this file alone.

## 1. Question and verdict quantity

Where does SQLite spend time on a small, single-row-or-few-row transaction —
flushing to disk, writing pages, or something else — and how does that
change between a dataset that fits SQLite's page cache and one that does
not? `docs/concept.md` §12 names "parity, not a win" as the ceiling on a
single-writer path bound by flush time, and names the costs a closed-world
compiler could remove (parse/compile at request time, a skippable B-tree
walk, per-writer lock bookkeeping). E4 puts a number on that ceiling before
kiln exists.

**Verdict quantity.** Per (workload, size, `synchronous` setting), the share
of per-operation wall time spent outside `xSync` and `xWrite` calls
(§5). This is the directly measured, flush-independent, write-independent
remainder — the only part a closed-world compiler could plausibly shrink.
§10 fixes the thresholds and the decision rule.

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

Every workload is one transaction per call. Read workloads use
`BEGIN DEFERRED` (a read needs no write lock); write workloads use
`BEGIN IMMEDIATE` (a write starts its write lock at `BEGIN`, not on its
first write statement).
`BEGIN`, `COMMIT`, and each workload's statement are each prepared once per
connection at startup and reused by `sqlite3_reset()` + rebind, never
re-prepared per call (https://www.sqlite.org/cintro.html, fetched
2026-10-15 — "the time needed to run `sqlite3_prepare()` equals or exceeds
the time needed by `sqlite3_step()`"). The harness never calls
`rusqlite`'s `Connection::transaction()`/`Transaction` helper for this
reason: that type issues `BEGIN ...`/`COMMIT` through `execute_batch`, a
one-off call, not a cached prepared statement
(https://docs.rs/rusqlite/0.40.2/src/rusqlite/transaction.rs.html, fetched
2026-10-15). The harness prepares the three `BEGIN` variants and `COMMIT`
itself, the same way as every other statement in this section.

- **A. Point read.** `BEGIN DEFERRED; SELECT id, email, balance FROM
  accounts WHERE id = ?1; COMMIT;` `?1` uniform random in
  `[1, row_count]`.
- **B. Point update.** `BEGIN IMMEDIATE; UPDATE accounts SET balance =
  balance + 1 WHERE id = ?1; COMMIT;` `?1` uniform random in
  `[1, row_count]`.
- **C. Insert under a unique index.** `BEGIN IMMEDIATE; INSERT INTO
  accounts (id, email, balance) VALUES (?1, ?2, 0); COMMIT;` `?1` is the
  next id from a counter starting at `row_count + 1`. `?2` is 16 random
  lowercase hex characters (the random-leading component) followed by `-`
  and the harness's repetition counter (the uniqueness suffix) — the
  random component leads so inserts land in random regions of the index's
  B-tree, not append-only ones. On `SQLITE_CONSTRAINT` (a collision), redraw
  the random component once; a second collision aborts the run as a
  protocol violation.
- **D. Transfer.** One transaction: `SELECT balance FROM accounts WHERE id
  = ?1;`; if that balance is ≥ the transfer amount (fixed at **1**, the
  same unit §2.1 loads `balance` in), `UPDATE accounts SET balance =
  balance - 1 WHERE id = ?2;` then `UPDATE
  accounts SET balance = balance + 1 WHERE id = ?3;`. `?2`/`?3` are
  independent uniform-random draws in `[1, row_count]`, redrawing `?3` on a
  collision with `?2`. Every repetition commits exactly one transaction
  regardless of the guard outcome; the harness counts and reports how often
  the guard blocked a transfer.

### 2.1 Inputs, frozen

- **PRNG.** `ChaCha8Rng` from the `rand_chacha` crate, version `0.10.0`
  (https://lib.rs/crates/rand_chacha, fetched 2026-10-15), seeded once per
  cell from a single fixed integer seed: **`1`**. Reason: any fixed seed
  reproduces the same key sequence byte for byte; `1` is the simplest
  value that is not itself a parameter being tuned.
- **Initial load.** Each size (§3) is loaded once with `id = 1..row_count`,
  `email = 'user' || id || '@example.test'` (deterministic; only the C
  workload's hot-path email needs the random-leading property, §2), and
  `balance = 1000000` (an arbitrary round unit, large enough relative to
  the transfer amount that D's guard almost never fires). After loading,
  the harness asserts `SELECT count(*) FROM accounts` equals `row_count`,
  then copies the database and WAL files to a pristine backup under
  `data/<cell>/pristine/`.
- **Per-repetition correctness checks.** After B or D, the harness
  periodically (every repetition) checks no `balance` is negative. After D
  specifically, it checks `SELECT sum(balance) FROM accounts` is unchanged
  from before the repetition (a transfer is zero-sum). After C, it checks
  the row count grew by exactly the operation count run (§8).
- **Baseline restore.** Workloads A, B, D mutate values but never row
  count or schema; a fresh process per repetition (§8) is restore enough.
  Workload C grows the table; its pristine backup is copied back over the
  working files before every repetition, outside the timed window.

## 3. Sizes

Two sizes, both frozen:

- **Small: 10,000 rows.**
- **Medium: 10,000,000 rows.**

§4 fixes a 64 MiB page-cache budget. 10,000 rows of this schema sit far
below it; 10,000,000 rows sit far above it under any cache budget this
protocol freezes — three orders of magnitude, chosen so the two sizes land
unambiguously on opposite sides of the cache boundary. After loading each
size, the harness reads the database file's on-disk byte size (main file +
WAL file) and the active `cache_size`, writes both into the size's manifest
entry, and reports a violation if the intended fits/does-not-fit relation
does not hold.

### 3.1 Key distribution

Uniform random over the live key range for every workload reading an
existing key (A, B, D) — not insertion order, not skewed — because the
question is SQLite's unconditional per-operation cost, not hot-key caching.
Workload C's keys are described in §2.

## 4. Connection settings, frozen

- **SQLite version.** Linked via the `rusqlite` crate `0.40.2` with its
  `bundled` feature, which vendors SQLite `3.53.2` via `libsqlite3-sys`
  `0.38.2` (both latest published as of 2026-10-15:
  https://lib.rs/crates/rusqlite, https://lib.rs/crates/libsqlite3-sys,
  fetched 2026-10-15). `Cargo.lock` pins both exactly. At startup the
  harness calls `sqlite3_libversion()` and writes the returned string into
  every run's manifest; a mismatch against `3.53.2` is logged as a protocol
  deviation, not silently accepted.
- **WAL and synchronous.** `PRAGMA journal_mode = WAL;` once per
  connection. Both `PRAGMA synchronous = FULL;` and `= NORMAL;` are
  measured — every cell runs under both, never only one. In WAL mode,
  "writers sync the WAL on every transaction commit if `PRAGMA synchronous`
  is set to FULL but omit this sync if `PRAGMA synchronous` is set to
  NORMAL"; NORMAL "does lose durability" but stays consistent
  (https://www.sqlite.org/pragma.html#pragma_synchronous, fetched
  2026-10-15). FULL is the case concept.md §12 calls the realistic ceiling;
  NORMAL approximates what is left once the per-commit flush is gone,
  without building anything to remove it.
- **Page cache.** `PRAGMA cache_size = -65536;` once per connection — an
  explicit 64 MiB budget (not the compiled-in default of about 2 MiB)
  (https://www.sqlite.org/pragma.html#pragma_cache_size, fetched
  2026-10-15). This is the budget §3's two sizes straddle.
- **Page size and checkpointing.** `PRAGMA page_size = 4096;` (this
  machine's filesystem block size) set before the table is created.
  `PRAGMA wal_autocheckpoint = 1000;` — SQLite's own documented default
  (https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint, fetched
  2026-10-15: "Autocheckpointing is enabled by default with an interval of
  1000") is left in force, not disabled, so a checkpoint fires naturally
  within a cell and its cost is measured, not hidden (§5, §8). A checkpoint
  "does require sync operations," making "an occasional COMMIT... much
  slower" (https://www.sqlite.org/wal.html, fetched 2026-10-15) — this is
  expected, and the per-operation shares (§8, "reported statistics") are
  reported with a range across repetitions precisely so one expensive,
  checkpoint-triggered operation does not get averaged away.
- **One connection, one thread, single writer.** No second connection, no
  concurrent reader or writer. This measures a transaction's own cost, not
  contention (a different experiment, `docs/experiments.md` E2's analogue
  once kiln exists).
- **In-memory control.** Every workload also runs once, at the small size
  only, against `:memory:` instead of a file, with the same `cache_size`
  and prepared-statement discipline, no `journal_mode` pragma, and no
  `xSync`/`xWrite` calls to measure (there is no file). §6 uses it as a
  cross-check, not an independent primary number.
- **Effective-settings check.** After opening, the harness queries
  `page_size`, `cache_size`, `wal_autocheckpoint`, `synchronous`, and
  `journal_mode` and writes all five into the manifest; any value that does
  not match this section is a logged protocol deviation.

## 5. Primary measurement: the VFS shim

A sampling profiler cannot see this experiment's main signal: a thread
blocked inside `fsync` accrues no CPU cycles, so a cycle-sampling profiler
attributes it near-zero time no matter how long the wait actually was
(https://perfwiki.github.io/main/tutorial/#profiling-sleep-times, fetched
2026-10-15 — cycle sampling "does not count when the processor is idle" and
"is not equivalent to wall clock time"). E4's gating measurement is
therefore a direct timer at the real boundary, not a statistical sample of
an indirect proxy.

**The shim.** A custom `sqlite3_vfs`, registered with
`sqlite3_vfs_register(&shimVfs, 0)` (`makeDflt = 0`; the harness opens
connections by name), wraps the platform's default VFS found via
`sqlite3_vfs_find(0)` — the documented shim pattern: "a particular VFS might
just be a thin wrapper around another VFS that does the real work... We
call a wrapper VFS a 'shim'" (https://www.sqlite.org/vfs.html, fetched
2026-10-15). Its `io_methods` delegate every call unchanged to the wrapped
file's own methods (https://www.sqlite.org/c3ref/io_methods.html, fetched
2026-10-15, for the full method table) except `xWrite` and `xSync`, which
the shim times around the delegate call with `clock_gettime(CLOCK_MONOTONIC,
...)`. `xOpen`'s flags argument carries `SQLITE_OPEN_MAIN_DB` or
`SQLITE_OPEN_WAL` (https://www.sqlite.org/c3ref/c_open_autoproxy.html,
fetched 2026-10-15), so the shim keeps separate call counts and total
durations per file kind — this is how checkpoint syncs (against the main
file) are told apart from per-commit WAL syncs: an ordinary WAL commit
never calls `xSync` on the `SQLITE_OPEN_MAIN_DB` file, only on the WAL
file, so the count of `xSync` calls against the main-file handle is
exactly the checkpoint count for that cell, no separate instrumentation
needed.

**Per-operation accounting.** The harness is single-threaded (§4), so the
shim's accumulators are plain globals, reset by the harness immediately
before issuing an operation's first statement and read immediately after
its `COMMIT` step returns. For each operation the harness records:

- **wall** — `CLOCK_MONOTONIC` delta from just before the first statement
  to just after `COMMIT` returns.
- **xsync_time** — sum of the shim's timed `xSync` durations inside that
  window.
- **xwrite_time** — sum of the shim's timed `xWrite` durations inside that
  window, its own bucket, never folded into "outside flush."
- **remainder** = `wall - xsync_time - xwrite_time`. Exact by construction
  — the three always sum to `wall`, so bucket shares always sum to 100%.
- **cpu_time** — `clock_gettime(CLOCK_THREAD_CPUTIME_ID, ...)` delta over
  the same window, measured by the harness, not the shim. This clock
  "measures CPU time consumed by this thread" and "is not settable"
  (https://man7.org/linux/man-pages/man2/clock_gettime.2.html, fetched
  2026-10-15). It is a cross-check, not a fourth bucket: `cpu_time` should
  sit near zero during `xsync_time` (a blocked thread burns no CPU) and
  track `remainder + xwrite_time` otherwise; a material, repeated
  disagreement is reported as a limitation (§13), not resolved silently.

A checkpoint that fires inside a commit (§4) runs on the same thread, so its
`xSync` cost lands inside that one operation's `xsync_time` — amortized
cost, not a separate event, and visible in that cell's reported range
(§8, "reported statistics") rather than averaged away.

The shim's own code is reviewed, not benchmarked; it adds two timer calls
around two already-existing method calls, nothing else.

## 6. Controls

- **Flush-only control.** A SQLite-free benchmark mode opens a scratch
  file beside the experiment's database files, writes a small fixed
  512-byte buffer, and calls `fsync`, repeated for the same operation count
  §8 fixes, timed the same way (`CLOCK_MONOTONIC` around each call). This
  gives this machine's own device-level flush cost, independent of SQLite,
  to sanity-check the shim's `xsync_time` against (§13: not a decomposition
  of SQLite's cost, a cross-check on one number).
- **In-memory cross-check.** The small-size, `synchronous=FULL`,
  on-disk median wall time minus the `:memory:` median wall time (§4) is a
  second, shim-independent estimate of flush-plus-write cost, reported
  alongside the shim's own `xsync_time + xwrite_time` for the same cell. A
  material disagreement is reported, not resolved in either number's favor.

## 7. Secondary, non-gating breakdown (perf)

Run only if the operator has granted `perf` access for this session. If not
granted, this section is skipped and the output says so explicitly — never
silently retried with reduced permissions.

If run: `perf record --call-graph dwarf -F 999 --delay=-1 --control=fifo:ctl,ack
-- <harness invocation>` (`-1` starts with events disabled; the harness
sends `enable` to the control FIFO immediately before an operation's first
statement and `disable` immediately after `COMMIT` returns, so sampling
covers exactly the timed window, not process startup or warm-up —
https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-record.txt,
fetched 2026-10-15). `-F 999` avoids lockstep with round-frequency system
activity (perf's own default is 1000 Hz,
https://perfwiki.github.io/main/tutorial/, fetched 2026-10-15).

This breaks down only `remainder` (the already-isolated non-flush,
non-write wall time) into SQLite's VDBE, B-tree/page, locking, and "rest"
modules, by line range, not by function name — the bundled amalgamation
`sqlite3.c` concatenates SQLite's source files with `/* Begin file <name>
*/` / `/* End file <name> */` comment markers. Before the frozen run, the
harness greps these markers out of the exact `sqlite3.c` the build actually
vendored and writes a `{source file → [start_line, end_line)}` table —
disjoint and exhaustive by construction, because the markers partition the
whole file. For every self-overhead sample
(https://perfwiki.github.io/main/tutorial/#overhead-calculation, fetched
2026-10-15 — self overhead sums period values, and "the sum of all the
'self' overhead values should be 100%"), the harness resolves the sample's
line inside `sqlite3.c` from the build's debug info (`addr2line` against
the `-g` symbols §12 keeps), looks up which file-range contains that line,
and assigns a bucket by that file, per `sqlite3`'s architecture page: VDBE
for `vdbe.c`, `vdbeaux.c`, `vdbeapi.c`, `vdbemem.c`; B-tree/page for
`btree.c`, `pager.c`, `wal.c`, `pcache.c`, `pcache1.c`; locking for
`os_unix.c` (https://www.sqlite.org/arch.html, fetched 2026-10-15) —
`os_unix.c` means locking here specifically because the shim (§5) has
already pulled every `xWrite`/`xSync` call out of `remainder`, so any
`os_unix.c` function left in a `remainder` sample is lock/file-state work,
not I/O; everything else (parser, code generator, allocator, API glue) is
"rest." A sample whose line falls outside every known range is logged
**unclassified**, never folded into "rest." The primary build (§12) keeps
normal optimization and inlining — this section is informational and
approximate, so disabling inlining to chase attribution precision is not
worth the risk of changing the measured cost (review finding 5).

The line-range table is written once to
`experiments/e4-sqlite-cost-anatomy/data/bucket-map.tsv` before the frozen
run and is not extended afterward; a cell whose unclassified share exceeds
2% is reported as provisional, not as a finished secondary number. If the
vendored `sqlite3.c` carries no `Begin file`/`End file` markers, this
section reports itself unable to run rather than guessing file boundaries.

## 8. Run procedure

For each cell (one workload × one size × one `synchronous` setting, plus
the in-memory and flush-only controls):

1. **Operation count, fixed now, not duration-driven.** 2,000 operations
   for A, B, D; 500 for C. Same count for every `synchronous` setting and
   every repetition of a cell — no doubling, no adaptation to how fast a
   cell runs. Reason for C's smaller count: it grows the table, and growth
   must stay small relative to `row_count` (§3) in both directions; 500
   plus the 200-operation warm-up below is ≤ 7% of the small size's 10,000
   rows.
2. **Warm-up.** 200 operations of the cell's workload run first and are
   discarded from timing and from the shim's counters — letting the OS and
   SQLite's page cache reach steady state. For workload C, warm-up
   insertions count toward the growth bound above; the pristine-backup
   restore (§2.1) happens before warm-up, once per repetition.
3. **Timed window.** The cell's fixed operation count runs back-to-back,
   one transaction per operation, each one's shim counters and
   `cpu_time` recorded individually (§5).
4. **Repetitions.** 9 per cell, each its own process (no state leaks
   between repetitions). Reason for 9: with the per-operation median as the
   repetition's summary statistic, the min and max across 9 independent
   repetitions bound the true per-cell median with (1 − 2×0.5⁹) ≈ 99.6%
   confidence — each repetition has an even chance of landing above or
   below the true median, so the two-sided failure probability is
   2×(1/2)⁹. This is a property of order statistics, not a tuned number;
   9 was chosen as the smallest count clearing 99% under this rule.
5. **Reported statistics.** Per repetition: the median `wall`, `xsync_time`,
   `xwrite_time`, `remainder`, `cpu_time`, and each as a share of `wall`.
   Across the 9 repetitions: the median of those per-repetition medians,
   and the min–max range (a range, not a standard deviation — flush-bound
   latencies are expected to be skewed, and a range assumes nothing about
   the shape of that skew).
6. **Cell order.** Cells are interleaved — repetition 1 of every cell, then
   repetition 2 of every cell, and so on — so a transient load spike biases
   every cell's data equally instead of concentrating in whichever cells
   happen to run consecutively.

## 9. Machine conditions

Hardware, read on this machine 2026-10-15: AMD Ryzen 5 5600X, 6 physical
cores, SMT2 (12 logical CPUs, per `/proc/cpuinfo`/`lscpu`); storage
`/dev/nvme0n1`, non-rotational per `lsblk`'s `ROTA=0`; filesystem `ext4` at
`/`. A different machine re-collects these facts fresh; they are not a
portable assumption.

- **Pinning.** The harness runs under `taskset -c 2`. Its SMT sibling is
  logical CPU 8, confirmed by reading
  `/sys/devices/system/cpu/cpu2/topology/thread_siblings_list` (`2,8`) — a
  different machine re-derives its own sibling pair the same way, never
  reusing "2,8." Nothing else is deliberately scheduled onto 2 or 8.
- **Profiler placement (§7 only).** `perf record` attaches to the already
  running, already-pinned harness process (`perf record -p <pid> ...`)
  from an unpinned shell, or one pinned away from CPUs 2 and 8 — it never
  shares a CPU with the measured harness.
- **Interference monitoring.** During every timed window, a separate,
  unpinned monitor process samples `/proc/stat`'s per-CPU fields for CPUs 2
  and 8, and `/proc/loadavg`, every 200 ms — frequent enough to catch an
  interference burst within a single cell's run, which can last a few
  seconds. Reason for 200 ms over the old 5-second interval: a 5-second
  sample can miss an entire short run; 200 ms cannot, at negligible
  overhead since the monitor itself never runs on 2 or 8.
- **Validity.** A repetition is discarded in full (not partially kept) and
  retried from its warm-up step if, at any sample during its timed window:
  CPU 8 shows any utilization from another process (the sibling must stay
  idle — that is the point of pinning away from it), or `/proc/loadavg`'s
  1-minute average is at or above **18** (1.5× this machine's 12 logical
  CPUs — the same bound this environment already uses to decide whether to
  start a job, so one number governs both decisions).
- **Rejected-attempt log.** Every discarded attempt (timestamp, cell,
  reason) is appended to
  `experiments/e4-sqlite-cost-anatomy/data/rejected.log`, kept, never
  overwritten.

## 10. Verdict rule

For a (workload, size, `synchronous`) cell, `nonflush% = remainder / wall`
(§5), as a median over repetitions with its min–max range (§8, "reported
statistics"). FULL and
NORMAL are judged separately and never combined into one number — NORMAL
mostly skips the per-commit sync by design (§4), so its `nonflush%` answers
a different question (what is left once the flush is already gone) than
FULL's (what is left while still paying the realistic ceiling).

A genuinely removable wall-time fraction `f` bounds the best-case speedup at
`1 / (1 - f)` — e.g. `f = 0.10 → 1.11×`, `f = 0.25 → 1.33×`, `f = 0.50 → 2×`.
This fixes the two thresholds below by naming the smallest and largest
speedup bounds worth distinguishing, not by picking round percentages after
the fact:

- **A small-data win is plausible** if the median `nonflush% ≥ 20%` (bounds
  a best-case speedup ≥ 1.25×, chosen as the smallest bound worth the
  engineering cost of a closed-world compiler) **and** the cell's min–max
  range does not cross below 20% (precedence: a range straddling a
  threshold outranks the point estimate — see "inconclusive" below).
- **Parity is the ceiling** if the median `nonflush% < 5%` (bounds a
  best-case speedup < 1.05×, below which the measurement's own noise could
  explain the whole gap) **and** the range does not cross above 5%.
- **Inconclusive** otherwise — the median falls in `[5%, 20%)`, or a
  threshold falls inside the min–max range. Reported as inconclusive, not
  rounded to either side.

These three outcomes are mutually exclusive and apply to every (workload,
size, synchronous) cell independently — both sizes, not only the small one.
The medium size is expected, not assumed, to show a smaller `nonflush%` as
the page-handling share of `remainder` grows with data that no longer fits
the frozen cache (§3); the verdict table (§11) reports what is measured.

E4's overall statement is the full table of per-cell verdicts, never a
single collapsed number — the four workloads are not expected to land on
the same side of any threshold.

## 11. Outputs

**Tables.** One table per `synchronous` setting, one row per (workload ×
size), columns: median `wall`, its range, and `xsync_time`, `xwrite_time`,
`remainder`, `cpu_time` each as a median share of `wall` with its range,
plus the §10 verdict. A separate table holds the in-memory and flush-only
controls. If §7 ran, its module breakdown of `remainder` is a separate,
clearly labeled secondary table.

**Figures.** One stacked-bar chart per `synchronous` setting, one bar per
(workload × size), three segments (xsync, xwrite, remainder). One chart
compares on-disk `synchronous=FULL` median wall time against the in-memory
control, small size, per workload (§6).

**Raw data.** Every repetition's per-operation `wall`, `xsync_time`,
`xwrite_time`, `remainder`, `cpu_time`; the rejected-attempt log (§9); and
each cell's manifest (settings actually in force, PRNG seed, on-disk byte
size and `cache_size` check, `sqlite3_libversion()`, hardware facts) under
`experiments/e4-sqlite-cost-anatomy/data/`, one subdirectory per cell. If
§7 ran, its `bucket-map.tsv` lives alongside this protocol.

## 12. Prerequisites and operator grants

**Needed from the operator.** A Rust toolchain (not installed in this
environment yet): install via rustup (https://rustup.rs), then `rustup
toolchain install stable` matching the pin below. `perf` access, only if
§7 is run.

**Toolchain pin.** A `rust-toolchain.toml` pins the latest stable Rust
release available at the time the harness is first built — confirmed by
`rustc --version --verbose` and written verbatim into the manifest, exactly
as `sqlite3_libversion()` already is (§4). This rule is deterministic
(there is exactly one "latest stable release" at any moment); it needs no
number decided in this document. The C compiler is whichever of `$CC`,
`cc`, `gcc`, `clang` the `cc` crate finds first on `$PATH`; its `--version`
output is recorded in the manifest, and a run with no C compiler found
fails closed. `libsqlite3-sys`'s build script sets no C optimization or
debug flags of its own (confirmed by reading its source at the pinned tag,
https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/build.rs,
fetched 2026-10-15); it inherits them from Cargo's active profile.

**Build.** `cargo build --release`, with `[profile.release]` set to
`opt-level = 3`, `debug = true` (symbols kept for §7's optional mapping,
without disabling optimization), `lto = false`. This is the one build this
protocol runs measurements against; §7's build is the same build, not a
separate diagnostic one.

**Preflight, non-result-bearing, fails closed.** Before any frozen run:
`rustc --version` matches the pinned toolchain; `sqlite3_libversion()`
returns `3.53.2`; CPU 2 and CPU 8 are confirmed SMT siblings via sysfs; the
shim VFS registers and `sqlite3_vfs_find("<shim name>")` returns non-null.
If §7 is requested, `perf stat -e cycles:u -- /bin/true` must succeed; if it
fails, §7 alone is skipped (logged), the rest of the run proceeds.

## 13. Threats to validity

- **E4 says nothing about kiln's own speed.** It bounds SQLite's side only.
- **One machine, one device, one filesystem.** The flush numbers are
  specific to this NVMe SSD and `ext4`. WAL itself "does not work over a
  network filesystem" at all (https://www.sqlite.org/wal.html, fetched
  2026-10-15).
- **Shared, loaded machine.** Pinning and the load/sibling monitors (§9)
  reduce, not eliminate, interference from memory bandwidth, cache, or
  thermal sharing with the pinned core.
- **No concurrency.** One writer, one connection (§4); the locking cost
  measured is single-writer bookkeeping, not contention.
- **Workload C's table grows during its timed window** — bounded to ≤7% of
  the starting size (§8) and documented, not hidden.
- **§7's module mapping comes from one pilot pass**, not exhaustive static
  analysis; its unclassified-bucket and 2% rules exist because of that, and
  it is informational, not gating.
- **The exact C compiler is not version-pinned**, only recorded (§12) —
  a limitation accepted rather than invented a false precision around.

## References

- SQLite VFS `io_methods`: https://www.sqlite.org/c3ref/io_methods.html
- SQLite VFS and shims: https://www.sqlite.org/vfs.html
- SQLite `sqlite3_vfs_find`/`sqlite3_vfs_register`:
  https://www.sqlite.org/c3ref/vfs_find.html
- SQLite `xOpen` flags (`SQLITE_OPEN_MAIN_DB`, `SQLITE_OPEN_WAL`):
  https://www.sqlite.org/c3ref/c_open_autoproxy.html
- SQLite C API introduction (prepared statements, reset/reuse):
  https://www.sqlite.org/cintro.html
- SQLite WAL mode and checkpointing: https://www.sqlite.org/wal.html
- SQLite `PRAGMA synchronous`:
  https://www.sqlite.org/pragma.html#pragma_synchronous
- SQLite `PRAGMA cache_size`:
  https://www.sqlite.org/pragma.html#pragma_cache_size
- SQLite `PRAGMA wal_autocheckpoint`:
  https://www.sqlite.org/pragma.html#pragma_wal_autocheckpoint
- SQLite transaction types: https://www.sqlite.org/lang_transaction.html
- SQLite architecture (module/source-file ownership):
  https://www.sqlite.org/arch.html
- `clock_gettime(2)`, `CLOCK_THREAD_CPUTIME_ID`/`CLOCK_MONOTONIC`:
  https://man7.org/linux/man-pages/man2/clock_gettime.2.html
- `perf` tutorial (sampling rate, self-overhead, sleep-time limits):
  https://perfwiki.github.io/main/tutorial/,
  https://perfwiki.github.io/main/tutorial/#profiling-sleep-times,
  https://perfwiki.github.io/main/tutorial/#overhead-calculation
- `perf record` manual (`--call-graph`, `-F`, `--delay`, `--control`):
  https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/Documentation/perf-record.txt
- `rusqlite` crate: https://lib.rs/crates/rusqlite
- `libsqlite3-sys` crate: https://lib.rs/crates/libsqlite3-sys
- `libsqlite3-sys` build script (v0.40.2 tag):
  https://raw.githubusercontent.com/rusqlite/rusqlite/v0.40.2/libsqlite3-sys/build.rs
- `rand_chacha` crate: https://lib.rs/crates/rand_chacha
- `rusqlite`'s `Transaction` helper (uses `execute_batch`, not a cached
  prepared statement):
  https://docs.rs/rusqlite/0.40.2/src/rusqlite/transaction.rs.html

All URLs above were fetched 2026-10-15, the day this protocol was rewritten.
