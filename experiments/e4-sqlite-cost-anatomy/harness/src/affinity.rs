//! CPU affinity (protocol.md §9): the harness process pins itself to
//! [`crate::machine::PINNED_CPU`]; the interference monitor pins itself to
//! the complement of `{PINNED_CPU, SIBLING_CPU}` — "never merely
//! unpinned, since an unpinned process can still land there by scheduler
//! choice." Linux-only (`sched_setaffinity`), like every other
//! `/proc`-reading piece of this harness (protocol §9 is itself
//! Linux-specific).

use std::io;

use crate::machine;

/// Every logical CPU id on this host (`0..total_cpus`) except those in
/// `excluded` — the interference monitor's pin set (protocol §9: "the
/// complement of {2, 8}").
pub fn complement(total_cpus: usize, excluded: &[usize]) -> Vec<usize> {
    (0..total_cpus)
        .filter(|cpu| !excluded.contains(cpu))
        .collect()
}

/// Pins the calling process to exactly `cpus` via `sched_setaffinity`
/// (pid 0 means the calling thread; this harness is single-threaded by
/// protocol §4, so that is the whole process). Fails loud: an unpinnable
/// process must not silently run unpinned, since §9's isolation between
/// the harness and its monitor depends on the pin actually taking
/// effect.
pub fn pin_current_process(cpus: &[usize]) -> io::Result<()> {
    // SAFETY: `set` is a local, correctly sized `cpu_set_t`; `CPU_ZERO`/
    // `CPU_SET` only ever write within it, and `sched_setaffinity` is
    // called with that same size.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }
        let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Pins the calling process to the complement of `{PINNED_CPU,
/// SIBLING_CPU}` (protocol §9: "never merely unpinned"): the place for
/// everything that must not disturb the measured core or its sibling —
/// the interference monitor and the pilot's own orchestration.
pub fn pin_outside_measurement_cpus() -> io::Result<()> {
    let total_cpus = std::thread::available_parallelism()?.get();
    let cpus = complement(total_cpus, &[machine::PINNED_CPU, machine::SIBLING_CPU]);
    if cpus.is_empty() {
        return Err(io::Error::other(format!(
            "no CPUs left to pin to once {} and {} are excluded (host reports only {total_cpus} logical CPUs)",
            machine::PINNED_CPU,
            machine::SIBLING_CPU
        )));
    }
    pin_current_process(&cpus)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod kiln_e4_07 {
        use super::*;

        #[test]
        fn complement_excludes_the_named_cpus() {
            assert_eq!(
                complement(12, &[2, 8]),
                vec![0, 1, 3, 4, 5, 6, 7, 9, 10, 11]
            );
        }

        #[test]
        fn complement_of_every_cpu_is_empty() {
            assert_eq!(complement(2, &[0, 1]), Vec::<usize>::new());
        }

        #[test]
        fn complement_ignores_an_excluded_id_outside_the_range() {
            assert_eq!(complement(3, &[0, 99]), vec![1, 2]);
        }

        #[test]
        fn pinning_to_every_available_cpu_succeeds() {
            // Pinning to the host's whole available set is always legal,
            // proving the real syscall path works without depending on
            // which specific CPU ids this test process happens to run
            // under (a CI container may restrict the host to fewer than
            // the 12 machine.rs assumes for the frozen pin/sibling ids).
            let total = std::thread::available_parallelism()
                .expect("available_parallelism")
                .get();
            pin_current_process(&(0..total).collect::<Vec<_>>()).expect("pin to every CPU");
        }

        #[test]
        fn pinning_outside_the_measurement_cpus_succeeds_on_this_host() {
            // Pins only the calling test thread, away from CPUs 2 and 8.
            pin_outside_measurement_cpus().expect("pin outside the measurement CPUs");
        }
    }
}
