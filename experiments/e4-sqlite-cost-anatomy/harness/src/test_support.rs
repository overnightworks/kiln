//! Test-only support shared across the crate's test modules.
//!
//! [`vfs::counters`](crate::vfs::counters) is a set of plain process-wide
//! globals by protocol design (protocol §5: the real harness is
//! single-threaded, so plain globals are correct there). `cargo test`
//! instead runs every test in one process across several threads, so any
//! two tests that reset or read those counters — directly, or through
//! [`crate::vfs::time_operation`] — must not run concurrently, or one can
//! erase or consume the other's measurement. [`lock_vfs_counters`] is the
//! one lock every such test takes, so none of them has to invent its own.

use std::sync::{Mutex, MutexGuard};

use crate::interference::TimedSample;
use crate::machine;
use crate::monitor::Sample;

static VFS_COUNTERS_LOCK: Mutex<()> = Mutex::new(());

/// Acquires the lock serializing every test that touches the VFS shim's
/// counters. Held guards block until released, so holding one is correct
/// synchronization, not best-effort; a previous holder's panic poisons the
/// `Mutex`, which is lock contention for the test's own assertions, never
/// for this counter protection, so a poisoned lock is still taken.
pub(crate) fn lock_vfs_counters() -> MutexGuard<'static, ()> {
    VFS_COUNTERS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `count` monitor samples of a machine that stays quiet: one per
/// sampling interval, CPU 8 idle throughout, no scheduler wait, low load.
pub(crate) fn quiet_samples(count: u64) -> Vec<TimedSample> {
    let interval_ns = machine::SAMPLE_INTERVAL.as_nanos() as u64;
    let idle_ticks_per_interval =
        machine::SAMPLE_INTERVAL.as_millis() as u64 * machine::USER_HZ / 1000;
    (0..count)
        .map(|index| TimedSample {
            at_ns: index * interval_ns,
            sample: Sample {
                sibling_idle_ticks: index * idle_ticks_per_interval,
                pinned_wait_ns: 0,
                load_average_1m: 1.0,
            },
        })
        .collect()
}
