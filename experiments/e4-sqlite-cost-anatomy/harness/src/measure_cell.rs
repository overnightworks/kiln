//! One measured half of a pilot pair, as the child process that runs it
//! (protocol.md §5, §8, §9, §13): restore the checkpointed pristine
//! snapshot of the cell's size (§2.1), open it through the shim VFS, run
//! the discarded warm-up and the timed operations of one workload, and
//! report only the mean wall time per operation — no `xsync`/`xwrite`/
//! `remainder` figure exists in this module's types, so none can leak
//! (§13).
//!
//! The child announces `ready` once the snapshot is restored and every
//! statement prepared, then waits for `go`: the parent starts its monitor
//! in between, so the monitored window is the warm-up and the timed
//! operations and nothing else (§9). [`measure_in_process`] and
//! [`serve_measure_cell`] are tested in-process; the `measure-cell`
//! subcommand in `main.rs` only wires them to stdin, stdout and the pin.

use std::fmt;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::Duration;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use rusqlite::{Connection, OpenFlags};

use crate::driver::OperationPlan;
use crate::generators::{self, InsertParams};
use crate::restore;
use crate::vfs::{self, TimingError};
use crate::workloads::{
    self, InsertOutcome, InsertStatements, PointReadStatements, PointUpdateStatements, Synchronous,
    TransactionStatements, TransferStatements, Workload,
};

/// One in-process measurement: total wall time summed over every timed
/// operation, divided by the operation count. Carries no `xsync`/`xwrite`/
/// `remainder` field — there is nowhere in this type to put one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredCell {
    pub operations_measured: usize,
    pub mean_wall_ns: f64,
}

/// Protocol §8.1/§8.2: one attempt's total operation count is warm-up
/// plus the timed count — named and exactly tested so a wrong
/// combination (the two swapped, multiplied, or otherwise miscombined)
/// is caught directly, rather than only showing up as a plausible-looking
/// wrong magnitude several steps later.
pub fn total_operation_count(warmup: usize, operations: usize) -> usize {
    warmup + operations
}

/// Protocol §8.2: whether operation `index` (0-based, issue order) falls
/// at or after the warm-up boundary, i.e. is one of the timed operations
/// whose wall time counts toward the mean — pure and exactly tested so
/// `>=` cannot silently become `<` or land one operation off.
pub fn is_past_warmup(index: usize, warmup: usize) -> bool {
    index >= warmup
}

/// Protocol §8.5's `Σwall/N`: `total_wall`'s mean per operation, in
/// nanoseconds — pure and exactly tested so `/` cannot silently become
/// `*` or `%`.
pub fn mean_wall_nanos(total_wall: Duration, operations: usize) -> f64 {
    total_wall.as_nanos() as f64 / operations as f64
}

/// Everything one measurement child needs, and nothing it must derive:
/// the workload and setting, the row count of the snapshot it restores
/// (its size, protocol §3), the warm-up and timed counts, the seed of the
/// attempt's input trace (§2.1), the pristine snapshot to restore, and a
/// directory to keep its working copy in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasureCellRequest {
    pub workload: Workload,
    pub synchronous: Synchronous,
    pub row_count: u64,
    pub plan: OperationPlan,
    pub seed: u64,
    pub pristine_dir: PathBuf,
    pub scratch_dir: PathBuf,
}

impl MeasureCellRequest {
    /// The `measure-cell` subcommand's arguments.
    pub fn to_args(&self) -> Vec<String> {
        vec![
            self.workload.label().to_string(),
            self.synchronous.label().to_string(),
            self.row_count.to_string(),
            self.plan.warmup.to_string(),
            self.plan.timed.to_string(),
            self.seed.to_string(),
            self.pristine_dir.display().to_string(),
            self.scratch_dir.display().to_string(),
        ]
    }

    /// Reads [`to_args`](Self::to_args)'s arguments back; the error names
    /// what was wrong.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let [
            workload,
            synchronous,
            row_count,
            warmup,
            timed,
            seed,
            pristine_dir,
            scratch_dir,
        ] = args
        else {
            return Err(format!("expected 8 arguments, got {}", args.len()));
        };
        let number = |name: &str, value: &str| {
            value
                .parse::<u64>()
                .map_err(|_| format!("{name} must be a non-negative integer, got {value:?}"))
        };
        Ok(MeasureCellRequest {
            workload: Workload::parse(workload)
                .ok_or_else(|| format!("unknown workload {workload:?}"))?,
            synchronous: Synchronous::parse(synchronous)
                .ok_or_else(|| format!("unknown synchronous setting {synchronous:?}"))?,
            row_count: number("row count", row_count)?,
            plan: OperationPlan {
                warmup: number("warm-up", warmup)? as usize,
                timed: number("operations", timed)? as usize,
            },
            seed: number("seed", seed)?,
            pristine_dir: PathBuf::from(pristine_dir),
            scratch_dir: PathBuf::from(scratch_dir),
        })
    }
}

/// Why a measurement child could not measure.
#[derive(Debug)]
pub enum MeasureError {
    Sqlite(rusqlite::Error),
    Io(io::Error),
    ShimInstall(String),
    /// The restored snapshot does not hold the rows the cell's size
    /// promises (protocol §3).
    SnapshotRows {
        expected: u64,
        found: Option<u64>,
    },
    /// One sanitized message for every inconsistent timing (protocol §13:
    /// the pilot's output "never contains `xsync_time`, `xwrite_time`,
    /// `remainder`, or any ratio from them" — not even on a failure path).
    InconsistentTiming,
}

/// What an inconsistent measurement reports instead of its numbers.
const INCONSISTENT_TIMING_MESSAGE: &str = "timing measurement was internally inconsistent (protocol §5/§13); rerun to collect a fresh sample";

impl fmt::Display for MeasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeasureError::Sqlite(err) => write!(f, "{err}"),
            MeasureError::Io(err) => write!(f, "{err}"),
            MeasureError::ShimInstall(reason) => {
                write!(f, "installing the shim VFS failed: {reason}")
            }
            MeasureError::SnapshotRows { expected, found } => write!(
                f,
                "the restored snapshot should hold {expected} rows (highest id), found {found:?}"
            ),
            MeasureError::InconsistentTiming => f.write_str(INCONSISTENT_TIMING_MESSAGE),
        }
    }
}

impl std::error::Error for MeasureError {}

impl From<rusqlite::Error> for MeasureError {
    fn from(err: rusqlite::Error) -> Self {
        MeasureError::Sqlite(err)
    }
}

impl From<io::Error> for MeasureError {
    fn from(err: io::Error) -> Self {
        MeasureError::Io(err)
    }
}

impl From<TimingError<rusqlite::Error>> for MeasureError {
    fn from(err: TimingError<rusqlite::Error>) -> Self {
        match err {
            // An ordinary SQLite error from the operation itself — never a
            // numeric timing split, so it is safe to report verbatim.
            TimingError::Operation(inner) => MeasureError::Sqlite(inner),
            TimingError::Measurement(_) => MeasureError::InconsistentTiming,
        }
    }
}

/// Times one operation and returns the wall time that counts toward the
/// mean: zero while the operation is still in the warm-up (§8.2).
fn time_counted_operation<T>(
    txn: &mut TransactionStatements<'_>,
    index: usize,
    warmup: usize,
    operation: impl FnOnce(&mut TransactionStatements<'_>) -> rusqlite::Result<T>,
) -> Result<(T, Duration), MeasureError> {
    let (value, timing) = vfs::time_operation(|| operation(&mut *txn))?;
    txn.end_operation()?;
    let counted = if is_past_warmup(index, warmup) {
        timing.wall
    } else {
        Duration::ZERO
    };
    Ok((value, counted))
}

/// Runs a pre-drawn trace, one transaction per entry, and sums the timed
/// operations' wall time. `start` runs once everything is prepared, just
/// before the first operation.
fn run_trace<P>(
    txn: &mut TransactionStatements<'_>,
    plan: OperationPlan,
    trace: &[P],
    start: impl FnOnce() -> io::Result<()>,
    mut execute: impl FnMut(&mut TransactionStatements<'_>, &P) -> rusqlite::Result<()>,
) -> Result<Duration, MeasureError> {
    start()?;
    let mut total_wall = Duration::ZERO;
    for (index, params) in trace.iter().enumerate() {
        let ((), counted) =
            time_counted_operation(txn, index, plan.warmup, |txn| execute(txn, params))?;
        total_wall += counted;
    }
    Ok(total_wall)
}

/// Workload C draws each collision fallback from the attempt's own RNG
/// stream, outside the timed window (protocol §2, §5), so its trace is
/// generated as it runs.
fn run_inserts(
    conn: &Connection,
    txn: &mut TransactionStatements<'_>,
    request: &MeasureCellRequest,
    start: impl FnOnce() -> io::Result<()>,
) -> Result<Duration, MeasureError> {
    let mut stmts = InsertStatements::prepare(conn)?;
    let mut rng = ChaCha8Rng::seed_from_u64(request.seed);
    start()?;
    let total = total_operation_count(request.plan.warmup, request.plan.timed);
    let mut total_wall = Duration::ZERO;
    for index in 0..total {
        let ordinal = index as u64 + 1;
        let params = InsertParams {
            id: request.row_count + ordinal,
            email: generators::inserted_email(&mut rng, ordinal),
        };
        let fallback = generators::prepare_insert_fallback(&rng, ordinal);
        let (outcome, counted) = time_counted_operation(txn, index, request.plan.warmup, |txn| {
            workloads::execute_insert(txn, &mut stmts, &params, &fallback.email)
        })?;
        if outcome == InsertOutcome::CommittedWithFallback {
            fallback.commit(&mut rng);
        }
        total_wall += counted;
    }
    Ok(total_wall)
}

/// Dispatches the request's workload to its trace and statements.
fn run_workload(
    conn: &Connection,
    txn: &mut TransactionStatements<'_>,
    request: &MeasureCellRequest,
    start: impl FnOnce() -> io::Result<()>,
) -> Result<Duration, MeasureError> {
    let plan = request.plan;
    let total = total_operation_count(plan.warmup, plan.timed);
    let (seed, rows) = (request.seed, request.row_count);
    match request.workload {
        Workload::PointRead => {
            let mut stmts = PointReadStatements::prepare(conn)?;
            let trace = generators::point_read_trace(seed, rows, total);
            run_trace(txn, plan, &trace, start, |txn, params| {
                workloads::execute_point_read(txn, &mut stmts, params).map(|_| ())
            })
        }
        Workload::PointUpdate => {
            let mut stmts = PointUpdateStatements::prepare(conn)?;
            let trace = generators::point_update_trace(seed, rows, total);
            run_trace(txn, plan, &trace, start, |txn, params| {
                workloads::execute_point_update(txn, &mut stmts, params)
            })
        }
        Workload::Insert => run_inserts(conn, txn, request, start),
        Workload::Transfer => {
            let mut stmts = TransferStatements::prepare(conn)?;
            let trace = generators::transfer_trace(seed, rows, total);
            run_trace(txn, plan, &trace, start, |txn, params| {
                workloads::execute_transfer(txn, &mut stmts, params).map(|_| ())
            })
        }
    }
}

/// The snapshot must hold exactly the ids `1..=row_count` the trace draws
/// from: its highest id is a constant-time check, unlike counting rows,
/// which would also pull a large table through the page cache before the
/// warm-up.
fn check_snapshot_rows(conn: &Connection, row_count: u64) -> Result<(), MeasureError> {
    let highest: Option<i64> =
        conn.query_row("SELECT max(id) FROM accounts", [], |row| row.get(0))?;
    let found = highest.map(|id| id as u64);
    if found == Some(row_count) {
        Ok(())
    } else {
        Err(MeasureError::SnapshotRows {
            expected: row_count,
            found,
        })
    }
}

/// Restores the pristine snapshot into a working copy (protocol §2.1:
/// before every attempt, outside the timed window — never a freshly
/// loaded database), runs `warmup` discarded then `timed` timed
/// operations of the request's workload through the shim VFS, and reduces
/// the timed operations' wall times to their mean. `start` runs once the
/// snapshot is restored and everything prepared, immediately before the
/// first operation. The working copy is removed on return.
pub fn measure_in_process(
    request: &MeasureCellRequest,
    start: impl FnOnce() -> io::Result<()>,
) -> Result<MeasuredCell, MeasureError> {
    vfs::install().map_err(MeasureError::ShimInstall)?;

    let working_dir = tempfile::tempdir_in(&request.scratch_dir)?;
    let db_path = working_dir.path().join("accounts.db");
    let wal_path = working_dir.path().join("accounts.db-wal");
    restore::restore_pristine(&request.pristine_dir, &db_path, &wal_path)?;

    let conn = Connection::open_with_flags_and_vfs(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE,
        vfs::VFS_NAME,
    )?;
    workloads::configure_connection(&conn, request.synchronous)?;
    check_snapshot_rows(&conn, request.row_count)?;

    let mut txn = TransactionStatements::prepare(&conn)?;
    let total_wall = run_workload(&conn, &mut txn, request, start)?;
    Ok(MeasuredCell {
        operations_measured: request.plan.timed,
        mean_wall_ns: mean_wall_nanos(total_wall, request.plan.timed),
    })
}

/// The `measure-cell` child's whole conversation (the parent's side is
/// [`crate::host`]): writes `ready` once prepared, waits for a `go` line,
/// measures, and writes `mean_wall_ns=<value>` — the one number safe for
/// another process to read back (protocol §13). Anything but `go`, or no
/// line at all, ends the child without measuring.
pub fn serve_measure_cell(
    request: &MeasureCellRequest,
    mut input: impl BufRead,
    mut output: impl Write,
) -> Result<(), MeasureError> {
    let measured = measure_in_process(request, || {
        writeln!(output, "{READY_LINE}")?;
        output.flush()?;
        let mut line = String::new();
        input.read_line(&mut line)?;
        if line.trim() == GO_LINE {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "expected {GO_LINE:?} from the parent, got {line:?}"
            )))
        }
    })?;
    writeln!(output, "{MEAN_WALL_PREFIX}{}", measured.mean_wall_ns)?;
    output.flush()?;
    Ok(())
}

/// What the child writes once its snapshot is restored and its
/// statements prepared.
pub const READY_LINE: &str = "ready";
/// What the parent writes to let the child start measuring.
pub const GO_LINE: &str = "go";
/// Prefix of the child's one result line.
pub const MEAN_WALL_PREFIX: &str = "mean_wall_ns=";

/// Parses the child's result line out of its stdout — pure, so a
/// malformed or missing line is tested without spawning anything.
pub fn parse_mean_wall_ns(stdout: &str) -> Option<f64> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(MEAN_WALL_PREFIX))
        .and_then(|value| value.trim().parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const SNAPSHOT_ROWS: u64 = 50;
    const TINY_PLAN: OperationPlan = OperationPlan {
        warmup: 5,
        timed: 10,
    };

    struct Snapshot {
        _root: tempfile::TempDir,
        pristine_dir: PathBuf,
        scratch_dir: PathBuf,
    }

    fn snapshot() -> Snapshot {
        let root = tempfile::tempdir().expect("tempdir");
        let pristine_dir = root.path().join("pristine");
        restore::prepare_pristine(&pristine_dir, SNAPSHOT_ROWS).expect("prepare snapshot");
        let scratch_dir = root.path().join("scratch");
        fs::create_dir(&scratch_dir).unwrap();
        Snapshot {
            _root: root,
            pristine_dir,
            scratch_dir,
        }
    }

    impl Snapshot {
        fn request(&self, workload: Workload) -> MeasureCellRequest {
            MeasureCellRequest {
                workload,
                synchronous: Synchronous::Full,
                row_count: SNAPSHOT_ROWS,
                plan: TINY_PLAN,
                seed: 7,
                pristine_dir: self.pristine_dir.clone(),
                scratch_dir: self.scratch_dir.clone(),
            }
        }

        fn pristine_bytes(&self) -> Vec<u8> {
            fs::read(self.pristine_dir.join("main.db")).unwrap()
        }
    }

    fn nothing_to_wait_for() -> io::Result<()> {
        Ok(())
    }

    mod kiln_e4_05 {
        use super::*;

        #[test]
        fn total_operation_count_is_warmup_plus_operations() {
            assert_eq!(total_operation_count(5, 10), 15);
            assert_eq!(total_operation_count(0, 0), 0);
        }

        #[test]
        fn is_past_warmup_is_true_at_and_after_the_boundary_only() {
            assert!(!is_past_warmup(4, 5));
            assert!(is_past_warmup(5, 5));
            assert!(is_past_warmup(6, 5));
        }

        #[test]
        fn mean_wall_nanos_divides_total_by_operation_count() {
            assert_eq!(mean_wall_nanos(Duration::from_nanos(100), 10), 10.0);
            // 105 / 10 = 10.5, distinct from 105 % 10 = 5 and 105 * 10 = 1050
            // — pins the division against both other arithmetic mutations.
            assert_eq!(mean_wall_nanos(Duration::from_nanos(105), 10), 10.5);
        }

        #[test]
        fn every_workload_reports_a_positive_mean_wall_time_over_the_timed_operations() {
            // The shim's counters are process-wide globals (protocol §5), so
            // this test — which drives real commits through them — must not
            // run concurrently with any other test that resets or reads them
            // (see crate::test_support and vfs.rs's own tests).
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            for workload in Workload::ALL {
                let measured = measure_in_process(&snapshot.request(workload), nothing_to_wait_for)
                    .expect("measuring a small restored cell must succeed");
                assert_eq!(measured.operations_measured, TINY_PLAN.timed);
                assert!(
                    measured.mean_wall_ns > 0.0,
                    "{workload:?}: a real SQLite operation's wall time must be positive, got {}",
                    measured.mean_wall_ns
                );
            }
        }

        #[test]
        fn inserts_commit_exactly_the_frozen_trace_when_no_email_collides() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let request = snapshot.request(Workload::Insert);
            let working_dir = tempfile::tempdir_in(&snapshot.scratch_dir).unwrap();
            let db_path = working_dir.path().join("accounts.db");
            restore::restore_pristine(
                &snapshot.pristine_dir,
                &db_path,
                &working_dir.path().join("accounts.db-wal"),
            )
            .unwrap();
            let conn = Connection::open(&db_path).unwrap();
            workloads::configure_connection(&conn, Synchronous::Full).unwrap();
            let mut txn = TransactionStatements::prepare(&conn).unwrap();

            run_inserts(&conn, &mut txn, &request, nothing_to_wait_for).unwrap();

            let committed: Vec<String> = conn
                .prepare("SELECT email FROM accounts WHERE id > ?1 ORDER BY id")
                .unwrap()
                .query_map([SNAPSHOT_ROWS as i64], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            let total = total_operation_count(TINY_PLAN.warmup, TINY_PLAN.timed);
            let frozen: Vec<String> = generators::insert_trace(request.seed, SNAPSHOT_ROWS, total)
                .into_iter()
                .map(|params| params.email)
                .collect();
            assert_eq!(committed, frozen);
        }

        #[test]
        fn the_start_signal_comes_after_the_snapshot_is_restored_and_before_any_operation() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let scratch_entries_at_start = std::cell::Cell::new(0);

            measure_in_process(&snapshot.request(Workload::PointUpdate), || {
                scratch_entries_at_start.set(fs::read_dir(&snapshot.scratch_dir)?.count());
                Ok(())
            })
            .expect("measure");

            assert_eq!(
                scratch_entries_at_start.get(),
                1,
                "the working copy exists when the child announces it is ready"
            );
        }

        #[test]
        fn a_refused_start_signal_ends_the_measurement_without_a_mean() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();

            let result = measure_in_process(&snapshot.request(Workload::PointRead), || {
                Err(io::Error::other("parent hung up"))
            });

            assert!(matches!(result, Err(MeasureError::Io(_))));
        }

        fn serve(snapshot: &Snapshot, input: &str) -> (Result<(), MeasureError>, String) {
            let mut output = Vec::new();
            let result = serve_measure_cell(
                &snapshot.request(Workload::PointRead),
                input.as_bytes(),
                &mut output,
            );
            (result, String::from_utf8(output).unwrap())
        }

        #[test]
        fn the_child_says_ready_then_reports_its_mean_once_told_to_go() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();

            let (result, output) = serve(&snapshot, "go\n");

            result.expect("a child told to go measures");
            let mut lines = output.lines();
            assert_eq!(lines.next(), Some("ready"));
            assert!(parse_mean_wall_ns(lines.next().unwrap()).unwrap() > 0.0);
            assert_eq!(lines.next(), None);
        }

        #[test]
        fn the_child_measures_nothing_until_told_to_go() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();

            for input in ["", "stop\n"] {
                let (result, output) = serve(&snapshot, input);

                assert!(matches!(result, Err(MeasureError::Io(_))), "{input:?}");
                assert_eq!(output, "ready\n", "no mean without a go: {input:?}");
            }
        }
    }

    mod kiln_e4_23 {
        use super::*;

        #[test]
        fn a_measurement_starts_from_the_pristine_snapshot_and_leaves_it_untouched() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let before = snapshot.pristine_bytes();

            for workload in [Workload::PointUpdate, Workload::Insert, Workload::Transfer] {
                measure_in_process(&snapshot.request(workload), nothing_to_wait_for)
                    .expect("measure");
                assert_eq!(
                    snapshot.pristine_bytes(),
                    before,
                    "{workload:?} must mutate its working copy, never the snapshot"
                );
            }
        }

        #[test]
        fn a_measurement_removes_its_working_copy() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();

            measure_in_process(&snapshot.request(Workload::Insert), nothing_to_wait_for)
                .expect("measure");

            assert_eq!(fs::read_dir(&snapshot.scratch_dir).unwrap().count(), 0);
        }

        #[test]
        fn a_missing_snapshot_is_an_error_never_a_freshly_loaded_database() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let mut request = snapshot.request(Workload::PointUpdate);
            request.pristine_dir = snapshot.scratch_dir.join("no-such-snapshot");

            let result = measure_in_process(&request, nothing_to_wait_for);

            assert!(matches!(result, Err(MeasureError::Io(_))));
        }

        #[test]
        fn a_snapshot_with_other_rows_than_the_size_promises_is_refused() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let mut request = snapshot.request(Workload::PointUpdate);
            request.row_count = SNAPSHOT_ROWS + 1;

            let result = measure_in_process(&request, nothing_to_wait_for);

            assert!(matches!(
                result,
                Err(MeasureError::SnapshotRows {
                    expected: 51,
                    found: Some(50)
                })
            ));
        }
    }

    mod kiln_e4_08 {
        use super::*;

        fn request() -> MeasureCellRequest {
            MeasureCellRequest {
                workload: Workload::Transfer,
                synchronous: Synchronous::Normal,
                row_count: 10_000_000,
                plan: OperationPlan {
                    warmup: 200,
                    timed: 2000,
                },
                seed: 12_345,
                pristine_dir: PathBuf::from("/data/medium/pristine"),
                scratch_dir: PathBuf::from("/data/scratch"),
            }
        }

        #[test]
        fn a_request_round_trips_through_its_arguments() {
            assert_eq!(
                MeasureCellRequest::parse(&request().to_args()),
                Ok(request())
            );
        }

        #[test]
        fn a_request_with_a_missing_or_malformed_argument_is_rejected() {
            let mut missing = request().to_args();
            missing.pop();
            assert!(MeasureCellRequest::parse(&missing).is_err());
            for (position, bad) in [(0, "range-scan"), (1, "off"), (2, "many"), (5, "-1")] {
                let mut args = request().to_args();
                args[position] = bad.to_string();
                assert!(MeasureCellRequest::parse(&args).is_err(), "{bad}");
            }
        }

        #[test]
        fn parse_mean_wall_ns_reads_the_measure_cell_contract() {
            assert_eq!(parse_mean_wall_ns("mean_wall_ns=1234.5\n"), Some(1234.5));
            assert_eq!(parse_mean_wall_ns("ready\nmean_wall_ns=7\n"), Some(7.0));
        }

        #[test]
        fn parse_mean_wall_ns_is_none_for_output_missing_the_line() {
            assert_eq!(parse_mean_wall_ns("something else entirely\n"), None);
            assert_eq!(parse_mean_wall_ns("mean_wall_ns=fast\n"), None);
        }
    }

    mod kiln_e4_15 {
        use super::*;

        #[test]
        fn an_inconsistent_measurement_never_renders_its_numbers() {
            let inconsistent = vfs::InconsistentTiming {
                wall: Duration::from_nanos(10),
                xsync: Duration::from_nanos(8),
                xwrite: Duration::from_nanos(5),
            };
            let err: MeasureError =
                TimingError::<rusqlite::Error>::Measurement(inconsistent).into();
            let rendered = err.to_string();
            assert_eq!(
                rendered, INCONSISTENT_TIMING_MESSAGE,
                "a measurement error must render as the one sanitized message, never \
                 InconsistentTiming's own Display (protocol §13)"
            );
            for forbidden in ["xsync", "xwrite", "10ns", "8ns", "5ns"] {
                assert!(
                    !rendered.contains(forbidden),
                    "a sanitized measurement error must never leak {forbidden:?} (protocol §13); got {rendered:?}"
                );
            }
        }

        #[test]
        fn an_operation_error_is_reported_as_the_sqlite_error_it_is() {
            let err: MeasureError =
                TimingError::Operation(rusqlite::Error::QueryReturnedNoRows).into();
            assert!(matches!(
                err,
                MeasureError::Sqlite(rusqlite::Error::QueryReturnedNoRows)
            ));
        }
    }
}
