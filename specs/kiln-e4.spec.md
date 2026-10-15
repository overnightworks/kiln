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
- [KILN-E4-05] Each attempt runs the cell's fixed operation count after a discarded warm-up, 500 operations for the insert workload and 2,000 for the others (§8.1-§8.2), and reports the mean wall time over the timed operations only (§8.5); the child announces `ready` once it is restored and prepared and starts only when told `go`, so the monitored window is its warm-up and timed operations alone (§9).
- [KILN-E4-06] A measured window is accepted when none of its consecutive monitor samples shows CPU 8 activity (its idle time advancing less than the time that passed), more than 1 ms of CPU 2 scheduler wait in its interval, or a 1-minute load of 18 or more (the first sample's included); a window with fewer than two samples is rejected, and a rejection names the violated limit and the sample (§9).
- [KILN-E4-07] Every measured half runs as its own child process pinned to CPU 2, sampled by a separate monitor process pinned to the CPUs outside {2, 8}, and the pilot's own process stays outside {2, 8} too; a child that cannot be started or exits unsuccessfully is an error that stops the run: it is never rejected as interference, never retried and never logged as a rejected attempt (§9).
- [KILN-E4-08] A measurement child's arguments and result, and a monitor sample with the monotonic time it was taken at, cross the process boundary as plain text and read back exactly; a missing, malformed or unknown field is rejected rather than defaulted.
- [KILN-E4-23] Every measurement child restores the checkpointed pristine snapshot of its cell's size before it measures, never a freshly loaded database: it measures a working copy, leaves the snapshot untouched, removes the copy, and refuses a snapshot that is missing or does not hold the rows the size promises (§2.1, §3, §8.2).
- [KILN-E4-18] The measurement child selects its workload by the documented spelling (`point-read`, `point-update`, `insert`, `transfer`) and its setting by `full` or `normal`, and an unknown spelling is rejected; every cell is named `workload/size/setting`.

## Preflight and passthrough build

- [KILN-E4-09] The preflight manifest records the source commit, the SHA-256 of both builds (the instrumented one preflight ran from and the passthrough one handed to it), the `rustc` version and the SQLite version, and preflight itself fails for a binary built outside a git checkout or from uncommitted changes; a later run compares its binaries with the manifest and fails closed on any mismatch or an unreadable or incomplete manifest, and the pilot is such a run: it starts only when both measured binaries have the manifest's hashes and were compiled from its commit, so a replaced or stale instrumented or passthrough binary is refused (§12, §13).
- [KILN-E4-10] The passthrough build commits through the same shim path as the instrumented build but records no `xWrite` or `xSync` time, while the instrumented build records both (§5).
- [KILN-E4-20] Each binary reports which build it is and which source commit it was compiled from through `build-role`; preflight records a manifest only for an instrumented and a passthrough build of one commit, and the pilot starts only when the instrumented binary reports the instrumented build and the passthrough binary the passthrough build, so the same build given twice, the two swapped, a build of another commit or a binary that cannot report its role is refused (§5, §12).

## Feasibility pilot

- [KILN-E4-11] The overhead bound is judged for the twelve cells whose timed transaction calls `xWrite` or `xSync` (point-update, insert and transfer, each at both sizes under both `synchronous` settings); the four point-read cells run no comparison and are reported as not gated, never as pass or fail; the report holds exactly one row for each of the sixteen cells, and a report with a missing, repeated or misclassified row never passes (§5).
- [KILN-E4-12] Pair `k` of a gated cell runs the instrumented build first when `k` is odd and the passthrough build first when `k` is even, so nine pairs start five with the instrumented build; every reported pair carries its 1-based pair index and the build that ran first (§5, §11).
- [KILN-E4-13] A gated cell runs the protocol's nine pairs; each pair's overhead ratio is instrumented mean wall time over passthrough mean wall time minus one; the cell reports the median and min-max range and is `PASS` when the maximum is below 2 %, `FAIL` when the minimum is above 2 %, and `INCONCLUSIVE` otherwise, a ratio of exactly 2 % included (§5).
- [KILN-E4-14] A pair with a rejected half is discarded and retried as a pair under the driver's retry budget, without running its second half; a pair that exhausts it makes the cell `inconclusive — insufficient repetitions`, shown with its count and never as a pass, the remaining cells still run, and every pair that did complete stays in the report with its pair index and the build that ran first (§5, §9, §11).
- [KILN-E4-15] The pilot report carries the source commit, the two binary hashes, run duration, rejected-attempt count, monitor coverage and why `perf` was skipped, and never any `xsync`, `xwrite` or `remainder` figure, including in an error message (§13).
- [KILN-E4-16] The pilot succeeds only if every one of the sixteen cells is reported once and every gated cell passed; a failing, inconclusive or insufficient gated cell makes it fail, and a not-gated cell does not vote (§5, §13); the process exits 0 for a passing report and 1 for any other, and the report ends with that outcome.
- [KILN-E4-17] The pilot reports how many 200 ms monitor intervals it covered against how many its measured windows should hold, rounded down per window, and a window shorter than one interval expects none (§13).
- [KILN-E4-24] The pinned monitor samples at the configured interval (200 ms) from before a half's measured window opens until after it closes, and every sample is persisted with its cell, pair, attempt and phase, rejected halves' included (§9, §13).
- [KILN-E4-25] Before a pair starts, the otherwise idle machine is watched for one interval and judged by the same limits; a failing check rejects the attempt under the retry budget and neither half starts (§9).
- [KILN-E4-21] The pilot runs the protocol's frozen warm-up and operation counts only; it has no option to change them and refuses `--warmup` and `--operations` (§13).
- [KILN-E4-22] The pilot creates its report, its rejected-attempt log and its monitor-sample log exclusively: a directory that already holds any of them is refused and its evidence stays untouched (§9, §11).

## Night script

- [KILN-E4-19] `run-pilot-night.sh` refuses to start without both binaries, takes the shared probe-stack lock, runs preflight before the pilot and starts no pilot when preflight fails, passes the pilot the manifest preflight wrote, and writes each run to a newly created UTC-timestamped directory that no earlier run's directory can be (§12, §13).
