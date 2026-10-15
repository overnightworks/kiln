//! The timing VFS shim (protocol.md §5): a thin wrapper VFS, registered
//! alongside (not replacing) SQLite's default unix VFS, whose
//! `io_methods` delegate every call unchanged except `xWrite`/`xSync`,
//! which it times with `CLOCK_MONOTONIC`.
//!
//! SQLite always invokes a file's methods with the pointer it handed
//! back from `xOpen`. Our shim's `xOpen` hands back a [`ShimFileHeader`]
//! sitting in front of the real VFS's own file struct, so every method —
//! not only the two timed ones — must be a trampoline that recomputes the
//! real file pointer and calls through; copying the real vtable's raw
//! function pointers into our own table would hand them our header
//! pointer instead of the private struct they expect, corrupting memory
//! the moment one dereferenced it.

use std::cell::Cell;
use std::ffi::{CStr, c_int, c_void};
use std::mem::size_of;
use std::ptr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use libsqlite3_sys as ffi;

/// The name the shim registers under (`makeDflt = 0`, protocol §5): every
/// connection that wants it opens a URI with `?vfs=kiln_shim_vfs`.
pub const VFS_NAME: &CStr = c"kiln_shim_vfs";

static REAL_VFS_PTR: AtomicUsize = AtomicUsize::new(0);
static INSTALLED: OnceLock<()> = OnceLock::new();

/// Registers the shim VFS once per process. Idempotent: a later call is a
/// no-op. Returns an error if no default VFS is registered (should never
/// happen once `sqlite3` is linked in) or if registration itself fails.
pub fn install() -> Result<(), String> {
    let mut result = Ok(());
    INSTALLED.get_or_init(|| {
        result = install_once();
    });
    result
}

fn install_once() -> Result<(), String> {
    unsafe {
        let real_vfs = ffi::sqlite3_vfs_find(ptr::null());
        if real_vfs.is_null() {
            return Err("sqlite3_vfs_find(0) returned no default VFS".to_string());
        }
        REAL_VFS_PTR.store(real_vfs as usize, Ordering::SeqCst);

        // Bitwise-duplicate the real VFS struct: every field it doesn't
        // own (mxPathname, pAppData, every method but xOpen) must stay
        // exactly as the real unix VFS set it. These are VFS-level
        // slots — copied callbacks, called with the real VFS's own
        // pAppData — not trampolines; only the per-file io_methods built
        // in shim_open (below) recompute a pointer and forward.
        let mut shim_vfs: ffi::sqlite3_vfs = ptr::read(real_vfs);
        shim_vfs.szOsFile = HEADER_SIZE as c_int + (*real_vfs).szOsFile;
        shim_vfs.zName = VFS_NAME.as_ptr();
        shim_vfs.xOpen = Some(shim_open);
        // Must not alias the real VFS's position in SQLite's internal
        // linked list; sqlite3_vfs_register sets this itself on success.
        shim_vfs.pNext = ptr::null_mut();

        let leaked: &'static mut ffi::sqlite3_vfs = Box::leak(Box::new(shim_vfs));
        let rc = ffi::sqlite3_vfs_register(leaked as *mut ffi::sqlite3_vfs, 0);
        if rc != ffi::SQLITE_OK {
            return Err(format!("sqlite3_vfs_register failed with code {rc}"));
        }
    }
    Ok(())
}

/// `true` once [`install`] has registered the shim and `sqlite3_vfs_find`
/// can see it under [`VFS_NAME`] — the preflight check (protocol §12).
pub fn is_registered() -> bool {
    unsafe { !ffi::sqlite3_vfs_find(VFS_NAME.as_ptr()).is_null() }
}

unsafe fn real_vfs() -> *mut ffi::sqlite3_vfs {
    REAL_VFS_PTR.load(Ordering::SeqCst) as *mut ffi::sqlite3_vfs
}

/// Which file the timed calls landed on (protocol §5: "`xOpen`'s flags
/// carry `SQLITE_OPEN_MAIN_DB`/`SQLITE_OPEN_WAL`, so counts/durations are
/// kept per file kind: an ordinary commit never calls `xSync` on the
/// main-file handle, only the WAL file, so the main-file `xSync` count is
/// exactly the checkpoint count").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum FileKind {
    Main = 0,
    Wal = 1,
    Other = 2,
}

const FILE_KINDS: usize = 3;

impl FileKind {
    fn from_open_flags(flags: c_int) -> Self {
        if flags & ffi::SQLITE_OPEN_MAIN_DB != 0 {
            FileKind::Main
        } else if flags & ffi::SQLITE_OPEN_WAL != 0 {
            FileKind::Wal
        } else {
            FileKind::Other
        }
    }
}

/// The shim's own file record: SQLite allocates exactly `szOsFile` bytes
/// and treats the address as a `sqlite3_file*`, so this struct's layout
/// *is* the memory SQLite hands every method — `base` must be the first
/// field, and the real VFS's own file struct must immediately follow (see
/// [`real_file_ptr`]).
#[repr(C)]
struct ShimFileHeader {
    base: ffi::sqlite3_file,
    kind: FileKind,
    methods: ffi::sqlite3_io_methods,
}

const HEADER_SIZE: usize = size_of::<ShimFileHeader>();

unsafe fn real_file_ptr(file: *mut ffi::sqlite3_file) -> *mut ffi::sqlite3_file {
    unsafe { (file as *mut u8).add(HEADER_SIZE) as *mut ffi::sqlite3_file }
}

unsafe extern "C" fn shim_open(
    _vfs: *mut ffi::sqlite3_vfs,
    name: ffi::sqlite3_filename,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out_flags: *mut c_int,
) -> c_int {
    unsafe {
        let header = file as *mut ShimFileHeader;
        (*header).kind = FileKind::from_open_flags(flags);
        // SQLite's xOpen contract (https://www.sqlite.org/c3ref/vfs.html)
        // requires pMethods to stay null on a failed open, and SQLite
        // does not promise the memory it hands us is zeroed first. Set
        // it explicitly before delegating, so a non-zero-filled
        // allocation still reports null on failure — never rely on
        // caller-zeroed memory.
        (*header).base.pMethods = ptr::null();

        let real = real_vfs();
        let real_file = real_file_ptr(file);
        let xopen = (*real).xOpen.expect("real VFS has no xOpen");
        let rc = xopen(real, name, real_file, flags, out_flags);
        if rc != ffi::SQLITE_OK {
            return rc;
        }

        let real_methods_ptr = (*real_file).pMethods;
        if !real_methods_ptr.is_null() {
            (*header).methods = build_shim_methods(&*real_methods_ptr);
            (*header).base.pMethods = &(*header).methods;
        }
        ffi::SQLITE_OK
    }
}

/// Builds the shim's per-file method table: every slot the real file
/// populated becomes a trampoline to the same slot on the real file
/// (recomputed from the real pointer, never the header pointer); a slot
/// the real file left `None` stays `None`, so SQLite's own "is this
/// method supported" checks keep seeing the real VFS's answer.
fn build_shim_methods(real: &ffi::sqlite3_io_methods) -> ffi::sqlite3_io_methods {
    ffi::sqlite3_io_methods {
        iVersion: real.iVersion,
        xClose: Some(shim_close),
        xRead: real.xRead.map(|_| shim_read as _),
        xWrite: real.xWrite.map(|_| shim_write as _),
        xTruncate: real.xTruncate.map(|_| shim_truncate as _),
        xSync: real.xSync.map(|_| shim_sync as _),
        xFileSize: real.xFileSize.map(|_| shim_file_size as _),
        xLock: real.xLock.map(|_| shim_lock as _),
        xUnlock: real.xUnlock.map(|_| shim_unlock as _),
        xCheckReservedLock: real
            .xCheckReservedLock
            .map(|_| shim_check_reserved_lock as _),
        xFileControl: real.xFileControl.map(|_| shim_file_control as _),
        xSectorSize: real.xSectorSize.map(|_| shim_sector_size as _),
        xDeviceCharacteristics: real
            .xDeviceCharacteristics
            .map(|_| shim_device_characteristics as _),
        xShmMap: real.xShmMap.map(|_| shim_shm_map as _),
        xShmLock: real.xShmLock.map(|_| shim_shm_lock as _),
        xShmBarrier: real.xShmBarrier.map(|_| shim_shm_barrier as _),
        xShmUnmap: real.xShmUnmap.map(|_| shim_shm_unmap as _),
        xFetch: real.xFetch.map(|_| shim_fetch as _),
        xUnfetch: real.xUnfetch.map(|_| shim_unfetch as _),
    }
}

unsafe extern "C" fn shim_close(file: *mut ffi::sqlite3_file) -> c_int {
    unsafe {
        let real = real_file_ptr(file);
        let methods = (*real).pMethods;
        if methods.is_null() {
            return ffi::SQLITE_OK;
        }
        let close = (*methods).xClose.expect("real file has no xClose");
        close(real)
    }
}

/// Generates a passthrough trampoline for a method whose only job is to
/// recompute the real file pointer and call straight through — every
/// method except `xOpen`/`xClose` (lifecycle) and `xWrite`/`xSync`
/// (timed, protocol §5).
macro_rules! passthrough {
    ($name:ident, $field:ident, ($($arg:ident : $ty:ty),*) -> $ret:ty) => {
        unsafe extern "C" fn $name(file: *mut ffi::sqlite3_file, $($arg: $ty),*) -> $ret {
            unsafe {
                let real = real_file_ptr(file);
                let method = (*(*real).pMethods)
                    .$field
                    .unwrap_or_else(|| panic!("real file has no {}", stringify!($field)));
                method(real, $($arg),*)
            }
        }
    };
}

passthrough!(shim_read, xRead, (buf: *mut c_void, amt: c_int, offset: ffi::sqlite3_int64) -> c_int);
passthrough!(shim_truncate, xTruncate, (size: ffi::sqlite3_int64) -> c_int);
passthrough!(shim_file_size, xFileSize, (size: *mut ffi::sqlite3_int64) -> c_int);
passthrough!(shim_lock, xLock, (level: c_int) -> c_int);
passthrough!(shim_unlock, xUnlock, (level: c_int) -> c_int);
passthrough!(shim_check_reserved_lock, xCheckReservedLock, (out: *mut c_int) -> c_int);
passthrough!(shim_file_control, xFileControl, (op: c_int, arg: *mut c_void) -> c_int);
passthrough!(shim_sector_size, xSectorSize, () -> c_int);
passthrough!(shim_device_characteristics, xDeviceCharacteristics, () -> c_int);
passthrough!(shim_shm_map, xShmMap, (page: c_int, page_size: c_int, extend: c_int, out: *mut *mut c_void) -> c_int);
passthrough!(shim_shm_lock, xShmLock, (offset: c_int, n: c_int, flags: c_int) -> c_int);
passthrough!(shim_shm_barrier, xShmBarrier, () -> ());
passthrough!(shim_shm_unmap, xShmUnmap, (delete_flag: c_int) -> c_int);
passthrough!(shim_fetch, xFetch, (offset: ffi::sqlite3_int64, amt: c_int, out: *mut *mut c_void) -> c_int);
passthrough!(shim_unfetch, xUnfetch, (offset: ffi::sqlite3_int64, buf: *mut c_void) -> c_int);

unsafe extern "C" fn shim_write(
    file: *mut ffi::sqlite3_file,
    buf: *const c_void,
    amt: c_int,
    offset: ffi::sqlite3_int64,
) -> c_int {
    unsafe {
        let header = file as *mut ShimFileHeader;
        let real = real_file_ptr(file);
        let write = (*(*real).pMethods).xWrite.expect("real file has no xWrite");
        let start = Instant::now();
        let rc = write(real, buf, amt, offset);
        counters::record((*header).kind, Bucket::Write, start.elapsed());
        rc
    }
}

unsafe extern "C" fn shim_sync(file: *mut ffi::sqlite3_file, flags: c_int) -> c_int {
    unsafe {
        let header = file as *mut ShimFileHeader;
        let real = real_file_ptr(file);
        let sync = (*(*real).pMethods).xSync.expect("real file has no xSync");
        let start = Instant::now();
        let rc = sync(real, flags);
        counters::record((*header).kind, Bucket::Sync, start.elapsed());
        rc
    }
}

#[derive(Clone, Copy)]
enum Bucket {
    Write,
    Sync,
}

/// Plain global accumulators (protocol §5: "the harness is single-threaded,
/// so the shim's accumulators are plain globals, reset by the harness
/// immediately before issuing an operation's first statement and read
/// immediately after its `COMMIT` step returns").
pub mod counters {
    use super::*;

    struct KindCounters {
        write_ns: AtomicU64,
        write_count: AtomicU64,
        sync_ns: AtomicU64,
        sync_count: AtomicU64,
    }

    impl KindCounters {
        const fn new() -> Self {
            KindCounters {
                write_ns: AtomicU64::new(0),
                write_count: AtomicU64::new(0),
                sync_ns: AtomicU64::new(0),
                sync_count: AtomicU64::new(0),
            }
        }
    }

    static COUNTERS: [KindCounters; FILE_KINDS] = [
        KindCounters::new(),
        KindCounters::new(),
        KindCounters::new(),
    ];

    pub(super) fn record(kind: FileKind, bucket: Bucket, elapsed: Duration) {
        let c = &COUNTERS[kind as usize];
        let ns = elapsed.as_nanos().min(u64::MAX as u128) as u64;
        match bucket {
            Bucket::Write => {
                c.write_ns.fetch_add(ns, Ordering::Relaxed);
                c.write_count.fetch_add(1, Ordering::Relaxed);
            }
            Bucket::Sync => {
                c.sync_ns.fetch_add(ns, Ordering::Relaxed);
                c.sync_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Zeroes every counter. Call immediately before an operation's first
    /// statement (protocol §5).
    pub fn reset() {
        for c in &COUNTERS {
            c.write_ns.store(0, Ordering::Relaxed);
            c.write_count.store(0, Ordering::Relaxed);
            c.sync_ns.store(0, Ordering::Relaxed);
            c.sync_count.store(0, Ordering::Relaxed);
        }
    }

    /// Per-file-kind totals since the last [`reset`]. Call immediately
    /// after `COMMIT` returns (protocol §5).
    #[derive(Debug, Clone, Copy, Default)]
    pub struct BucketTotals {
        pub xwrite_count: u64,
        pub xwrite_ns: u64,
        pub xsync_count: u64,
        pub xsync_ns: u64,
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub struct Snapshot {
        pub main: BucketTotals,
        pub wal: BucketTotals,
        pub other: BucketTotals,
    }

    impl Snapshot {
        /// `xwrite_time` (protocol §5): the write bucket's own time,
        /// summed across every file kind.
        pub fn xwrite_duration(&self) -> Duration {
            Duration::from_nanos(self.main.xwrite_ns + self.wal.xwrite_ns + self.other.xwrite_ns)
        }

        /// `xsync_time` (protocol §5): the sync bucket's own time, summed
        /// across every file kind — a checkpoint's main-file `xSync`
        /// included, since it lands inside the operation whose `COMMIT`
        /// triggered it.
        pub fn xsync_duration(&self) -> Duration {
            Duration::from_nanos(self.main.xsync_ns + self.wal.xsync_ns + self.other.xsync_ns)
        }
    }

    fn totals_of(c: &KindCounters) -> BucketTotals {
        BucketTotals {
            xwrite_count: c.write_count.load(Ordering::Relaxed),
            xwrite_ns: c.write_ns.load(Ordering::Relaxed),
            xsync_count: c.sync_count.load(Ordering::Relaxed),
            xsync_ns: c.sync_ns.load(Ordering::Relaxed),
        }
    }

    pub fn snapshot() -> Snapshot {
        Snapshot {
            main: totals_of(&COUNTERS[FileKind::Main as usize]),
            wal: totals_of(&COUNTERS[FileKind::Wal as usize]),
            other: totals_of(&COUNTERS[FileKind::Other as usize]),
        }
    }
}

/// `CLOCK_THREAD_CPUTIME_ID` right now, measured by the harness itself,
/// never the shim (protocol §5: `cpu_time` is "a cross-check, not a
/// fourth bucket").
fn thread_cpu_time() -> Duration {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, correctly sized out-parameter for this
    // call; CLOCK_THREAD_CPUTIME_ID is supported on every Linux kernel
    // this harness targets (protocol §9).
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    assert_eq!(rc, 0, "clock_gettime(CLOCK_THREAD_CPUTIME_ID) failed");
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Per-operation timing (protocol §5): `wall` is the `CLOCK_MONOTONIC`
/// delta the caller measured around the operation; `xsync`/`xwrite` come
/// from the shim; `remainder` is computed, never measured, so the three
/// always sum to `wall` exactly. `cpu` is a cross-check only: "it should
/// sit near zero during `xsync_time` and track `remainder + xwrite_time`
/// otherwise."
#[derive(Debug, Clone, Copy)]
pub struct OperationTiming {
    pub wall: Duration,
    pub xsync: Duration,
    pub xwrite: Duration,
    pub remainder: Duration,
    pub cpu: Duration,
}

/// `xsync + xwrite` exceeded `wall`: the three can never sum to `wall` as
/// §5 requires, so this is an inconsistent measurement — a pathological
/// clock reading or a shim/harness bug — never silently saturated to a
/// remainder of zero, which would hide the very inconsistency it masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InconsistentTiming {
    pub wall: Duration,
    pub xsync: Duration,
    pub xwrite: Duration,
}

impl std::fmt::Display for InconsistentTiming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "inconsistent measurement: xsync ({:?}) + xwrite ({:?}) exceeds wall ({:?})",
            self.xsync, self.xwrite, self.wall
        )
    }
}

impl std::error::Error for InconsistentTiming {}

impl OperationTiming {
    pub fn from_measurements(
        wall: Duration,
        cpu: Duration,
        snapshot: &counters::Snapshot,
    ) -> Result<Self, InconsistentTiming> {
        let xsync = snapshot.xsync_duration();
        let xwrite = snapshot.xwrite_duration();
        let remainder = wall
            .checked_sub(xsync)
            .and_then(|r| r.checked_sub(xwrite))
            .ok_or(InconsistentTiming {
                wall,
                xsync,
                xwrite,
            })?;
        Ok(OperationTiming {
            wall,
            xsync,
            xwrite,
            remainder,
            cpu,
        })
    }
}

/// Either the timed operation itself failed (`Operation`), or its
/// measurement came back inconsistent (`Measurement`, [`InconsistentTiming`]).
#[derive(Debug)]
pub enum TimingError<E> {
    Operation(E),
    Measurement(InconsistentTiming),
}

/// Accumulates durations an operation explicitly excludes from `wall` —
/// work that is the call's own input (PRNG draws, key formatting, §5),
/// not SQLite's, but that cannot be done before the window opens because
/// it is only known to be needed partway through the transaction (e.g.
/// workload C's collision redraw, protocol §2). [`time_operation`]
/// subtracts the total from the measured elapsed time.
pub struct Excluded(Cell<Duration>);

impl Excluded {
    pub fn new() -> Self {
        Excluded(Cell::new(Duration::ZERO))
    }

    /// Runs `f`, timing it, and adds that duration to the excluded total.
    pub fn exclude<T>(&self, f: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let value = f();
        self.0.set(self.0.get() + start.elapsed());
        value
    }

    fn total(&self) -> Duration {
        self.0.get()
    }
}

impl Default for Excluded {
    fn default() -> Self {
        Self::new()
    }
}

/// Times one operation end to end: resets the shim counters, runs `op`,
/// and reports the wall/xsync/xwrite/remainder/cpu split (protocol §5).
/// `op` receives an [`Excluded`] handle for any sub-interval that must
/// not count toward `wall` even though it cannot run before this
/// function is called; everything else — bind, step, reset, materializing
/// a `SELECT`'s rows — is SQLite's own work and stays timed.
pub fn time_operation<T, E>(
    op: impl FnOnce(&Excluded) -> Result<T, E>,
) -> Result<(T, OperationTiming), TimingError<E>> {
    counters::reset();
    let excluded = Excluded::new();
    let cpu_start = thread_cpu_time();
    let start = Instant::now();
    let value = op(&excluded).map_err(TimingError::Operation)?;
    let elapsed = start.elapsed();
    let wall = elapsed.saturating_sub(excluded.total());
    let cpu = thread_cpu_time().saturating_sub(cpu_start);
    let snapshot = counters::snapshot();
    let timing = OperationTiming::from_measurements(wall, cpu, &snapshot)
        .map_err(TimingError::Measurement)?;
    Ok((value, timing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, OpenFlags};

    fn open_through_shim(db_path: &std::path::Path) -> Connection {
        install().expect("install");
        Connection::open_with_flags_and_vfs(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
            VFS_NAME,
        )
        .expect("open through the shim VFS")
    }

    #[test]
    fn install_is_idempotent() {
        install().expect("first install");
        install().expect("second install is a no-op");
        assert!(is_registered());
    }

    // The shim's counters are process-wide globals by protocol design
    // (§5: the real harness is single-threaded, so plain globals are
    // correct there) — but `cargo test` runs tests in parallel threads
    // within one process, so any two tests that both drive real commits
    // through the shim must not run concurrently. One combined test
    // keeps that true without a lock every future shim test would have
    // to remember to take.
    #[test]
    fn a_real_commit_times_through_the_shim_and_reset_zeroes_it_for_the_next_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("shim-test.db");
        let conn = open_through_shim(&db_path);
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;")
            .unwrap();
        conn.execute_batch("CREATE TABLE t (x INTEGER);").unwrap();

        let (_, timing) = time_operation(|_excluded| -> rusqlite::Result<()> {
            conn.execute_batch("BEGIN IMMEDIATE; INSERT INTO t (x) VALUES (1); COMMIT;")
        })
        .unwrap();

        assert_eq!(timing.wall, timing.xsync + timing.xwrite + timing.remainder);

        let snapshot = counters::snapshot();
        let total_xwrite_count =
            snapshot.main.xwrite_count + snapshot.wal.xwrite_count + snapshot.other.xwrite_count;
        assert!(
            total_xwrite_count > 0,
            "a real commit must drive at least one xWrite through the shim"
        );

        counters::reset();
        let after_reset = counters::snapshot();
        assert_eq!(after_reset.main.xwrite_count, 0);
        assert_eq!(after_reset.wal.xwrite_count, 0);
        assert_eq!(after_reset.main.xsync_count, 0);
        assert_eq!(after_reset.wal.xsync_count, 0);

        // Per-file-kind accounting (protocol §5: "an ordinary commit
        // never calls xSync on the main-file handle, only the WAL
        // file, so the main-file xSync count is exactly the checkpoint
        // count"). Reusing this test, not a fresh #[test], because the
        // shim's counters are process-wide globals (see above).
        // `time_operation` resets the counters itself, so no manual
        // reset is needed between scenarios.
        let (_, ordinary_commit) = time_operation(|_excluded| -> rusqlite::Result<()> {
            conn.execute_batch("BEGIN IMMEDIATE; INSERT INTO t (x) VALUES (2); COMMIT;")
        })
        .unwrap();
        let after_ordinary_commit = counters::snapshot();
        assert!(
            after_ordinary_commit.wal.xwrite_count > 0,
            "an ordinary commit must write the WAL file"
        );
        assert!(
            after_ordinary_commit.wal.xsync_count > 0,
            "synchronous=FULL must sync the WAL file on commit"
        );
        assert_eq!(
            after_ordinary_commit.main.xsync_count, 0,
            "an ordinary commit must not sync the main file at all"
        );
        assert_eq!(
            ordinary_commit.xsync,
            after_ordinary_commit.xsync_duration()
        );

        let (_, _checkpoint_timing) = time_operation(|_excluded| -> rusqlite::Result<()> {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        })
        .unwrap();
        let after_checkpoint = counters::snapshot();
        assert_eq!(
            after_checkpoint.main.xsync_count, 1,
            "one forced checkpoint must sync the main file exactly once: \
             the main-file xSync count equals the checkpoint count"
        );
    }

    #[test]
    fn main_file_kind_is_classified_from_sqlite_open_main_db() {
        assert_eq!(
            FileKind::from_open_flags(ffi::SQLITE_OPEN_MAIN_DB),
            FileKind::Main
        );
        assert_eq!(
            FileKind::from_open_flags(ffi::SQLITE_OPEN_WAL),
            FileKind::Wal
        );
        assert_eq!(FileKind::from_open_flags(0), FileKind::Other);
    }

    #[test]
    fn xopen_leaves_pmethods_null_in_nonzero_memory_on_a_failing_open() {
        install().expect("install");

        // A real header-sized-plus-real-file-sized buffer, filled with a
        // nonzero byte pattern, 8-byte aligned (a `Vec<u64>` guarantees
        // that; sqlite3_file/our header hold pointers, which a plain
        // `Vec<u8>` is not guaranteed to align for) — standing in for
        // memory SQLite did *not* zero before calling xOpen, so the test
        // fails if shim_open ever relied on that zeroing instead of
        // setting pMethods itself.
        let real_size = unsafe { (*real_vfs()).szOsFile } as usize;
        let total_bytes = HEADER_SIZE + real_size;
        let words = total_bytes.div_ceil(size_of::<u64>());
        let mut buf = vec![u64::MAX; words];
        let file_ptr = buf.as_mut_ptr() as *mut ffi::sqlite3_file;

        // A path in a directory that does not exist, with no CREATE flag:
        // the real unix VFS's xOpen fails (SQLITE_CANTOPEN), never
        // reaching the success path that builds real io_methods.
        let missing_dir_path: &CStr = c"/nonexistent-kiln-e4-harness-dir/db.sqlite";
        let mut out_flags: c_int = 0;
        let rc = unsafe {
            shim_open(
                ptr::null_mut(),
                missing_dir_path.as_ptr(),
                file_ptr,
                ffi::SQLITE_OPEN_MAIN_DB | ffi::SQLITE_OPEN_READWRITE,
                &mut out_flags,
            )
        };

        assert_ne!(
            rc,
            ffi::SQLITE_OK,
            "opening a path inside a missing directory, without SQLITE_OPEN_CREATE, must fail"
        );
        let header = file_ptr as *mut ShimFileHeader;
        assert!(
            unsafe { (*header).base.pMethods }.is_null(),
            "a failed xOpen must leave pMethods null even when the memory SQLite \
             handed us was not zero-filled"
        );
    }

    #[test]
    fn an_inconsistent_measurement_where_xsync_plus_xwrite_exceeds_wall_is_an_explicit_error() {
        let snapshot = counters::Snapshot {
            main: counters::BucketTotals {
                xwrite_count: 1,
                xwrite_ns: 5,
                xsync_count: 1,
                xsync_ns: 8,
            },
            wal: counters::BucketTotals::default(),
            other: counters::BucketTotals::default(),
        };

        let result =
            OperationTiming::from_measurements(Duration::from_nanos(10), Duration::ZERO, &snapshot);

        assert_eq!(
            result.unwrap_err(),
            InconsistentTiming {
                wall: Duration::from_nanos(10),
                xsync: Duration::from_nanos(8),
                xwrite: Duration::from_nanos(5),
            }
        );
    }
}
