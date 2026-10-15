# `kiln-e4`

`experiments/e4-sqlite-cost-anatomy/harness` is E4's measurement harness:
the run driver, the passthrough build, the feasibility pilot and the night
script that runs it. This file owns the observable behavior of those parts.
The frozen protocol (`experiments/e4-sqlite-cost-anatomy/protocol.md`, as
amended in #30) owns the measurements, thresholds and rules; each criterion
cites the section it derives from instead of restating it. A test claims a
criterion by living in the module named for it: `mod kiln_e4_05 { #[test]
fn ... }` claims `KILN-E4-05`; the night script's test claims `KILN-E4-19`
in its check names. One criterion per module; several tests may share it.

## Run driver

- [KILN-E4-01] Repetition 1 of every cell runs before repetition 2 of any cell, and so on (§8.6); a run with no cells or no repetitions runs nothing.
- [KILN-E4-02] A repetition rejected for interference is retried from the start, up to 3 retries (4 attempts) (§9); a repetition that uses all four attempts is reported `FAILED-INTERFERENCE` and the run goes on with the remaining cells.
- [KILN-E4-03] A cell is reported complete only when all its repetitions succeeded; with one exhausted repetition it is reported `inconclusive — insufficient repetitions`, never completed with fewer, and the repetitions that did succeed are kept with their indices (§9).
- [KILN-E4-04] Every rejected attempt is appended to the rejected-attempt log immediately, with timestamp, cell, attempt and reason, and every repetition that exhausted its budget gets a `FAILED-INTERFERENCE` line; an existing log is never overwritten (§9, §11).
- [KILN-E4-05] Each attempt runs the cell's fixed operation count after a discarded warm-up, 500 operations for the insert workload and 2,000 for the others (§8.1-§8.2), and reports the mean wall time over the timed operations only (§8.5).
- [KILN-E4-06] An attempt is accepted when its monitor window shows no CPU 8 activity, at most 1 ms of CPU 2 scheduler wait and a 1-minute load below 18, and otherwise rejected with a reason naming the violated limit (§9).
- [KILN-E4-07] Every repetition and every comparison attempt runs as its own child process pinned to CPU 2, sampled by a separate monitor process pinned to the CPUs outside {2, 8}; a child that cannot be started or exits unsuccessfully is an error that stops the run: it is never rejected as interference, never retried and never logged as a rejected attempt (§9).
- [KILN-E4-08] A repetition's measurement and a monitor sample cross the process boundary as one line each and read back exactly; a line missing a field is rejected rather than defaulted.
- [KILN-E4-18] The measurement child selects its workload by the documented spelling (`point-read`, `point-update`), and an unknown spelling is rejected.

## Preflight and passthrough build

- [KILN-E4-09] The preflight manifest records the source commit, the SHA-256 of both builds (the instrumented one preflight ran from and the passthrough one handed to it), the `rustc` version and the SQLite version, and preflight itself fails for a binary built outside a git checkout or from uncommitted changes; a later run compares its binaries with the manifest and fails closed on any mismatch or an unreadable or incomplete manifest, and the pilot is such a run: it starts only when both measured binaries have the manifest's hashes and were compiled from its commit, so a replaced or stale instrumented or passthrough binary is refused (§12, §13).
- [KILN-E4-10] The passthrough build commits through the same shim path as the instrumented build but records no `xWrite` or `xSync` time, while the instrumented build records both (§5).
- [KILN-E4-20] Each binary reports which build it is and which source commit it was compiled from through `build-role`; preflight records a manifest only for an instrumented and a passthrough build of one commit, and the pilot starts only when the instrumented binary reports the instrumented build and the passthrough binary the passthrough build, so the same build given twice, the two swapped, a build of another commit or a binary that cannot report its role is refused (§5, §12).

## Feasibility pilot

- [KILN-E4-11] The overhead bound is judged only for cells whose timed transaction calls `xWrite` or `xSync` (here the point-update workload); a read-only cell runs no comparison and is reported as not gated, never as pass or fail (§5).
- [KILN-E4-12] Pair `k` of a gated cell runs the instrumented build first when `k` is odd and the passthrough build first when `k` is even, so nine pairs start five with the instrumented build; every reported pair carries its 1-based pair index and the build that ran first (§5, §11).
- [KILN-E4-13] A gated cell runs the protocol's nine pairs; each pair's overhead ratio is instrumented mean wall time over passthrough mean wall time minus one; the cell reports the median and min-max range and is `PASS` when the maximum is below 2 %, `FAIL` when the minimum is above 2 %, and `INCONCLUSIVE` otherwise, a ratio of exactly 2 % included (§5).
- [KILN-E4-14] A pair with a rejected half is discarded and retried as a pair under the driver's retry budget; a pair that exhausts it makes the cell `inconclusive — insufficient repetitions`, shown with its count and never as a pass, and every pair that did complete stays in the report with its pair index and the build that ran first (§5, §9, §11).
- [KILN-E4-15] The pilot report carries the two binary hashes, run duration, rejected-attempt count, monitor coverage and why `perf` was skipped, and never any `xsync`, `xwrite` or `remainder` figure, including in an error message (§13).
- [KILN-E4-16] The pilot succeeds only if every gated cell passed; a failing, inconclusive or insufficient gated cell makes it fail, and a not-gated cell does not vote (§5, §13).
- [KILN-E4-17] The pilot reports how many 200 ms monitor samples were written against how many the window should hold, rounded down, and a window shorter than one interval expects none (§13).
- [KILN-E4-21] The pilot runs the protocol's frozen warm-up and operation counts only; it has no option to change them and refuses `--warmup` and `--operations` (§13).
- [KILN-E4-22] The pilot creates its report and its rejected-attempt log exclusively: a directory that already holds either is refused and its evidence stays untouched (§9, §11).

## Night script

- [KILN-E4-19] `run-pilot-night.sh` refuses to start without both binaries, takes the shared probe-stack lock, runs preflight before the pilot and starts no pilot when preflight fails, passes the pilot the manifest preflight wrote, and writes each run to a newly created UTC-timestamped directory that no earlier run's directory can be (§12, §13).
