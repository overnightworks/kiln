//! Machine conditions frozen by protocol.md §9 for the one host E4 was
//! measured on (AMD Ryzen 5 5600X, read 2026-10-15). A different machine
//! re-measures `lscpu`/`thread_siblings_list` and freezes its own values
//! here; these are not a portable assumption.

use std::time::Duration;

/// The CPU core the harness runs pinned to (`taskset -c 2`, protocol §9).
pub const PINNED_CPU: usize = 2;

/// `PINNED_CPU`'s SMT sibling, confirmed via
/// `/sys/devices/system/cpu/cpu2/topology/thread_siblings_list` (`2,8`).
/// The interference monitor watches this core's idle time instead of its
/// scheduler wait time, since a sibling core's `/proc/stat` idle field
/// already carries the signal `/proc/schedstat` lacks for it.
pub const SIBLING_CPU: usize = 8;

/// Interference sampling interval: "200 ms catches a burst within a
/// multi-second run; 5 seconds could miss one entirely" (protocol §9).
pub const SAMPLE_INTERVAL: Duration = Duration::from_millis(200);

/// `/proc/schedstat` reports its wait-time field in USER_HZ-independent
/// nanoseconds already, but its *idle* counterpart in `/proc/stat` is in
/// USER_HZ ticks (`sysconf(_SC_CLK_TCK)`), which is 100 on every glibc
/// Linux target regardless of the kernel's timer frequency.
pub const USER_HZ: u64 = 100;

/// `/proc/schedstat` field 8 (CPU 2's accumulated wait time, ns) may grow
/// by at most this much over one sampling window before the sample fails
/// (protocol §9: "1,000,000 ns (1 ms)... implies real contention").
pub const MAX_SCHEDSTAT_WAIT_NS: u64 = 1_000_000;

/// `/proc/loadavg`'s 1-minute average fails a sample at or above this
/// value (protocol §9: "1.5x 12 logical CPUs").
pub const MAX_LOAD_AVERAGE: f64 = 18.0;

/// Repetitions per cell (protocol §8.4): the min/max of 9 i.i.d.
/// repetition summaries bound the population median at `(1 - 2*0.5^9)`,
/// about 99.609% confidence.
pub const REPETITIONS_PER_CELL: usize = 9;

/// Retries after the first attempt: "up to 3 retries (4 attempts)"
/// (protocol §9).
pub const MAX_RETRIES: usize = 3;

/// Operation counts per workload, warm-up excluded (protocol §8.1).
pub const OPERATION_COUNT_AB_D: usize = 2_000;
pub const OPERATION_COUNT_C: usize = 500;

/// Warm-up operations, discarded from timing (protocol §8.2).
pub const WARMUP_OPERATIONS: usize = 200;

/// `sqlite3_libversion()` the bundled build must report (protocol §4).
pub const PINNED_SQLITE_VERSION: &str = "3.53.2";

/// Environment variables that can silently change what the bundled
/// SQLite build measures; preflight asserts all four are unset
/// (protocol §12).
pub const UNSET_BUILD_ENV_VARS: [&str; 4] = [
    "CC",
    "CFLAGS",
    "LIBSQLITE3_FLAGS",
    "LIBSQLITE3_SYS_USE_PKG_CONFIG",
];
