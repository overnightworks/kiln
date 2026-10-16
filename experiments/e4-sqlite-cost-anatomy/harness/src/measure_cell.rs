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
//! in between. Right after the last operation it writes a `done` line
//! stamped with the monotonic clock, before its correctness checks and
//! before the working copy is dropped and removed: the parent closes the
//! monitored window at that stamp, so the window is the warm-up and the
//! timed operations and nothing else (§9). [`measure_in_process`] and
//! [`serve_measure_cell`] are tested in-process; the `measure-cell`
//! subcommand in `main.rs` only wires them to stdin, stdout and the pin.

use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::time::Duration;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;
use rusqlite::{Connection, OpenFlags};

use crate::driver::OperationPlan;
use crate::generators::{self, InsertParams};
use crate::interference::monotonic_ns;
use crate::measure_error::MeasureError;
use crate::measure_protocol::{
    DONE_PREFIX, GO_LINE, MEAN_WALL_PREFIX, MeasureCellRequest, READY_LINE,
};
use crate::repetition_checks::RepetitionChecks;
use crate::restore;
use crate::vfs;
use crate::workloads::{
    self, InsertOutcome, InsertStatements, PointReadStatements, PointUpdateStatements,
    TransactionStatements, TransferStatements, Workload,
};

/// One in-process measurement: total wall time summed over every timed
/// operation, divided by the operation count. Carries no `xsync`/`xwrite`/
/// `remainder` field — there is nowhere in this type to put one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredCell {
    pub operations_measured: usize,
    pub mean_wall_ns: f64,
    /// Workload D's guard outcomes (protocol §2: "the harness counts how
    /// often it blocked a transfer"), warm-up included; zero elsewhere.
    pub blocked_transfers: u64,
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

/// What running every operation of a workload produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkloadRun {
    timed_wall: Duration,
    blocked_transfers: u64,
}

impl WorkloadRun {
    fn without_blocked_transfers(timed_wall: Duration) -> Self {
        WorkloadRun {
            timed_wall,
            blocked_transfers: 0,
        }
    }
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
) -> Result<WorkloadRun, MeasureError> {
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
            .map(WorkloadRun::without_blocked_transfers)
        }
        Workload::PointUpdate => {
            let mut stmts = PointUpdateStatements::prepare(conn)?;
            let trace = generators::point_update_trace(seed, rows, total);
            run_trace(txn, plan, &trace, start, |txn, params| {
                workloads::execute_point_update(txn, &mut stmts, params)
            })
            .map(WorkloadRun::without_blocked_transfers)
        }
        Workload::Insert => {
            run_inserts(conn, txn, request, start).map(WorkloadRun::without_blocked_transfers)
        }
        Workload::Transfer => {
            let mut stmts = TransferStatements::prepare(conn)?;
            let trace = generators::transfer_trace(seed, rows, total);
            let mut blocked_transfers = 0;
            let timed_wall = run_trace(txn, plan, &trace, start, |txn, params| {
                let blocked = workloads::execute_transfer(txn, &mut stmts, params)?;
                blocked_transfers += u64::from(blocked);
                Ok(())
            })?;
            Ok(WorkloadRun {
                timed_wall,
                blocked_transfers,
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
/// first operation. `finished` runs with the monotonic instant right
/// after the last operation, before the repetition's correctness checks
/// (§2.1) and before the connection is dropped and the working copy
/// removed, which happens on return — none of that is part of the
/// monitored window.
pub fn measure_in_process(
    request: &MeasureCellRequest,
    start: impl FnOnce() -> io::Result<()>,
    finished: impl FnOnce(u64) -> io::Result<()>,
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
    let checks = RepetitionChecks::before(
        &conn,
        request.workload,
        request.row_count,
        total_operation_count(request.plan.warmup, request.plan.timed),
    )?;

    let mut txn = TransactionStatements::prepare(&conn)?;
    let run = run_workload(&conn, &mut txn, request, start)?;
    finished(monotonic_ns())?;
    checks.verify(&conn)?;
    Ok(MeasuredCell {
        operations_measured: request.plan.timed,
        mean_wall_ns: mean_wall_nanos(run.timed_wall, request.plan.timed),
        blocked_transfers: run.blocked_transfers,
    })
}

/// The `measure-cell` child's whole conversation (the parent's side is
/// [`crate::host`]): writes `ready` once prepared, waits for a `go` line,
/// measures, writes `done monotonic_ns=<instant>` right after the last
/// operation, and finally `mean_wall_ns=<value>` — the one number safe
/// for another process to read back (protocol §13). Anything but `go`, or
/// no line at all, ends the child without measuring.
pub fn serve_measure_cell(
    request: &MeasureCellRequest,
    mut input: impl BufRead,
    output: impl Write,
) -> Result<(), MeasureError> {
    let output = RefCell::new(output);
    let measured = measure_in_process(
        request,
        || {
            let mut output = output.borrow_mut();
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
        },
        |finished_ns| {
            let mut output = output.borrow_mut();
            writeln!(output, "{DONE_PREFIX}{finished_ns}")?;
            output.flush()
        },
    )?;
    let mut output = output.into_inner();
    writeln!(output, "{MEAN_WALL_PREFIX}{}", measured.mean_wall_ns)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure_protocol::{parse_done_ns, parse_mean_wall_ns};
    use crate::repetition_checks::CheckViolation;
    use crate::workloads::Synchronous;
    use std::fs;
    use std::path::PathBuf;

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

    fn nobody_waiting_for_the_end(_finished_ns: u64) -> io::Result<()> {
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
                let measured = measure_in_process(
                    &snapshot.request(workload),
                    nothing_to_wait_for,
                    nobody_waiting_for_the_end,
                )
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

            measure_in_process(
                &snapshot.request(Workload::PointUpdate),
                || {
                    scratch_entries_at_start.set(fs::read_dir(&snapshot.scratch_dir)?.count());
                    Ok(())
                },
                nobody_waiting_for_the_end,
            )
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

            let result = measure_in_process(
                &snapshot.request(Workload::PointRead),
                || Err(io::Error::other("parent hung up")),
                nobody_waiting_for_the_end,
            );

            assert!(matches!(result, Err(MeasureError::Io(_))));
        }

        #[test]
        fn the_end_signal_comes_after_the_last_operation_and_before_any_cleanup() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let working_copies_at_end = std::cell::Cell::new(0);
            let signalled_ns = std::cell::Cell::new(0);
            let before_ns = monotonic_ns();

            measure_in_process(
                &snapshot.request(Workload::Insert),
                nothing_to_wait_for,
                |finished_ns| {
                    working_copies_at_end.set(fs::read_dir(&snapshot.scratch_dir)?.count());
                    signalled_ns.set(finished_ns);
                    Ok(())
                },
            )
            .expect("measure");

            assert_eq!(
                working_copies_at_end.get(),
                1,
                "the working copy still exists when the window closes"
            );
            assert!((before_ns..=monotonic_ns()).contains(&signalled_ns.get()));
        }

        #[test]
        fn the_child_says_ready_then_done_then_its_mean() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();

            let (result, output) = serve(&snapshot, "go\n");

            result.expect("a child told to go measures");
            let lines: Vec<&str> = output.lines().collect();
            assert_eq!(lines.len(), 3, "{output:?}");
            assert_eq!(lines[0], READY_LINE);
            assert!(parse_done_ns(lines[1]).is_some(), "{output:?}");
            assert!(parse_mean_wall_ns(lines[2]).is_some(), "{output:?}");
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
            assert!(parse_done_ns(lines.next().unwrap()).is_some());
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

    mod kiln_e4_26 {
        use super::*;

        /// Changes the working copy through a second connection while the
        /// first is idle, the moment the window closes.
        fn corrupt_working_copy_at_the_end(snapshot: &Snapshot, statement: &str) -> io::Result<()> {
            let working_dir = fs::read_dir(&snapshot.scratch_dir)?.next().expect("copy")?;
            let conn = Connection::open(working_dir.path().join("accounts.db"))
                .map_err(io::Error::other)?;
            conn.execute(statement, []).map_err(io::Error::other)?;
            Ok(())
        }

        #[test]
        fn a_repetition_that_breaks_a_correctness_check_is_an_execution_failure() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let extra_row = "INSERT INTO accounts (id, email, balance) VALUES (9999, 'extra', 0)";
            let negative = "UPDATE accounts SET balance = -1 WHERE id = 1";
            let moved_money = "UPDATE accounts SET balance = balance + 1 WHERE id = 1";
            let total = (TINY_PLAN.warmup + TINY_PLAN.timed) as u64;

            for (workload, corruption, expected) in [
                (
                    Workload::PointUpdate,
                    negative,
                    CheckViolation::NegativeBalance,
                ),
                (
                    Workload::Transfer,
                    negative,
                    CheckViolation::NegativeBalance,
                ),
                (
                    Workload::Transfer,
                    moved_money,
                    CheckViolation::BalanceSumChanged {
                        before: SNAPSHOT_ROWS as i64 * 1_000_000,
                        after: SNAPSHOT_ROWS as i64 * 1_000_000 + 1,
                    },
                ),
                (
                    Workload::Insert,
                    extra_row,
                    CheckViolation::RowCountGrowth {
                        expected: SNAPSHOT_ROWS + total,
                        found: SNAPSHOT_ROWS + total + 1,
                    },
                ),
            ] {
                let result =
                    measure_in_process(&snapshot.request(workload), nothing_to_wait_for, |_| {
                        corrupt_working_copy_at_the_end(&snapshot, corruption)
                    });

                assert!(
                    matches!(&result, Err(MeasureError::CheckViolation(found)) if *found == expected),
                    "{workload:?}: {result:?}"
                );
            }
        }

        #[test]
        fn a_clean_repetition_blocks_no_transfer_and_a_drained_one_counts_every_block() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let total = (TINY_PLAN.warmup + TINY_PLAN.timed) as u64;

            let clean = measure_in_process(
                &snapshot.request(Workload::Transfer),
                nothing_to_wait_for,
                nobody_waiting_for_the_end,
            )
            .expect("measure");
            let pristine = Connection::open(snapshot.pristine_dir.join("main.db")).unwrap();
            pristine
                .execute_batch("UPDATE accounts SET balance = 0; PRAGMA wal_checkpoint(TRUNCATE);")
                .unwrap();
            let drained = measure_in_process(
                &snapshot.request(Workload::Transfer),
                nothing_to_wait_for,
                nobody_waiting_for_the_end,
            )
            .expect("measure");

            assert_eq!(clean.blocked_transfers, 0);
            assert_eq!(drained.blocked_transfers, total);
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
                measure_in_process(
                    &snapshot.request(workload),
                    nothing_to_wait_for,
                    nobody_waiting_for_the_end,
                )
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

            measure_in_process(
                &snapshot.request(Workload::Insert),
                nothing_to_wait_for,
                nobody_waiting_for_the_end,
            )
            .expect("measure");

            assert_eq!(fs::read_dir(&snapshot.scratch_dir).unwrap().count(), 0);
        }

        #[test]
        fn a_missing_snapshot_is_an_error_never_a_freshly_loaded_database() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let mut request = snapshot.request(Workload::PointUpdate);
            request.pristine_dir = snapshot.scratch_dir.join("no-such-snapshot");

            let result =
                measure_in_process(&request, nothing_to_wait_for, nobody_waiting_for_the_end);

            assert!(matches!(result, Err(MeasureError::Io(_))));
        }

        #[test]
        fn a_snapshot_with_other_rows_than_the_size_promises_is_refused() {
            let _guard = crate::test_support::lock_vfs_counters();
            let snapshot = snapshot();
            let mut request = snapshot.request(Workload::PointUpdate);
            request.row_count = SNAPSHOT_ROWS + 1;

            let result =
                measure_in_process(&request, nothing_to_wait_for, nobody_waiting_for_the_end);

            assert!(matches!(
                result,
                Err(MeasureError::SnapshotRows {
                    expected: 51,
                    found: Some(50)
                })
            ));
        }
    }
}
