//! The schema, the four workloads, and the connection settings frozen by
//! protocol.md §2 and §4. Every operation is one transaction; `BEGIN`,
//! `COMMIT`, and each statement are prepared once per connection, at
//! startup, and held for the run — [`TransactionStatements`] for the
//! shared `BEGIN`/`COMMIT` statements, a per-workload `*Statements`
//! struct for the rest — never re-prepared or looked up per operation,
//! and never `Connection::transaction()` (which issues `BEGIN`/`COMMIT`
//! through a one-off `execute_batch`, not a cached statement — protocol
//! §2).

use std::ffi::{CStr, c_int};
use std::ptr;

use libsqlite3_sys as ffi;
use rusqlite::{Connection, Error as SqlError, ErrorCode, Statement};

use crate::generators::{self, InsertParams, ReadParams, TransferParams, UpdateParams};

/// Protocol §2's schema, verbatim.
pub const SCHEMA_SQL: &str = "
CREATE TABLE accounts (
  id      INTEGER PRIMARY KEY,
  email   TEXT NOT NULL,
  balance INTEGER NOT NULL
);
CREATE UNIQUE INDEX accounts_email ON accounts(email);
";

/// The four workloads (protocol §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workload {
    PointRead,
    PointUpdate,
    Insert,
    Transfer,
}

/// The two sizes (protocol §3), both frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
}

impl Size {
    pub fn row_count(self) -> u64 {
        match self {
            Size::Small => 10_000,
            Size::Medium => 10_000_000,
        }
    }
}

/// The two `synchronous` settings (protocol §4), judged separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synchronous {
    Full,
    Normal,
}

impl Synchronous {
    fn pragma_value(self) -> &'static str {
        match self {
            Synchronous::Full => "FULL",
            Synchronous::Normal => "NORMAL",
        }
    }
}

/// One (workload, size, `synchronous`) cell (protocol §8). Enumeration
/// order is not pinned by the protocol; the simplest correct option —
/// workload, then size, then `synchronous`, each fixed-width — gives
/// every cell a stable, distinct index without needing a lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub workload: Workload,
    pub size: Size,
    pub synchronous: Synchronous,
}

impl Cell {
    pub fn index(&self) -> u64 {
        let workload = match self.workload {
            Workload::PointRead => 0,
            Workload::PointUpdate => 1,
            Workload::Insert => 2,
            Workload::Transfer => 3,
        };
        let size = match self.size {
            Size::Small => 0,
            Size::Medium => 1,
        };
        let synchronous = match self.synchronous {
            Synchronous::Full => 0,
            Synchronous::Normal => 1,
        };
        workload * 4 + size * 2 + synchronous
    }
}

/// Transfer's fixed amount (protocol §2: "fixed at 1, §2.1's `balance`
/// unit").
pub const TRANSFER_AMOUNT: i64 = 1;

/// Applies protocol §4's per-connection settings. `page_size` is set
/// first, before any table exists, since SQLite only honours it before
/// the first page is written.
pub fn configure_connection(conn: &Connection, synchronous: Synchronous) -> rusqlite::Result<()> {
    conn.pragma_update(None, "page_size", 4096)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", synchronous.pragma_value())?;
    conn.pragma_update(None, "cache_size", -65536)?;
    conn.pragma_update(None, "wal_autocheckpoint", 1000)?;
    Ok(())
}

/// Creates protocol §2's schema. Must run after [`configure_connection`]
/// so `page_size` is already in force.
pub fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SCHEMA_SQL)
}

/// The initial load (protocol §2.1): `id = 1..=row_count`,
/// `balance = 1000000`, `email` derived from `id`. Not part of any timed
/// window, so it uses one ordinary transaction rather than the cached
/// per-operation statements the workloads below reuse.
pub fn initial_load(conn: &Connection, row_count: u64) -> rusqlite::Result<()> {
    conn.execute_batch("BEGIN;")?;
    {
        let mut insert =
            conn.prepare("INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 1000000)")?;
        for id in 1..=row_count {
            insert.execute(rusqlite::params![
                id as i64,
                generators::initial_load_email(id)
            ])?;
        }
    }
    conn.execute_batch("COMMIT;")?;
    Ok(())
}

fn is_constraint_violation(err: &SqlError) -> bool {
    matches!(
        err,
        SqlError::SqliteFailure(e, _) if e.code == ErrorCode::ConstraintViolation
    )
}

/// Builds a [`rusqlite::Error::SqliteFailure`] from a raw result code and
/// the connection's current error message — the same information
/// `rusqlite`'s own statement methods attach, assembled by hand because
/// [`RawCommitStatement`] bypasses them.
fn sqlite_error(db: *mut ffi::sqlite3, rc: c_int) -> rusqlite::Error {
    // SAFETY: `db` is a live connection handle for the whole call; a null
    // `sqlite3_errmsg` result (no error recorded) is handled below rather
    // than assumed non-null.
    let message = unsafe {
        let msg = ffi::sqlite3_errmsg(db);
        if msg.is_null() {
            None
        } else {
            Some(CStr::from_ptr(msg).to_string_lossy().into_owned())
        }
    };
    rusqlite::Error::SqliteFailure(ffi::Error::new(rc), message)
}

/// `COMMIT`, prepared and stepped through direct FFI calls rather than
/// [`rusqlite::Statement::execute`]/`raw_execute`: both funnel into
/// `rusqlite`'s shared `execute_with_bound_parameters` (0.40.2), which
/// always steps, resets, and reads `sqlite3_changes()` in one call —
/// `rusqlite` exposes no public way to split them. Protocol §5's timed
/// window must end the instant `COMMIT`'s step returns, so [`step`] does
/// only that; [`reset`] runs separately, after the window has closed,
/// via [`TransactionStatements::end_operation`].
///
/// [`step`]: RawCommitStatement::step
/// [`reset`]: RawCommitStatement::reset
struct RawCommitStatement {
    stmt: *mut ffi::sqlite3_stmt,
    // Counts `reset` calls for the window-boundary test below.
    // `sqlite3_stmt_busy` cannot stand in for this: it already reports
    // "not busy" the instant `step` returns `SQLITE_DONE`, reset or not
    // (https://www.sqlite.org/c3ref/stmt_busy.html), so only this
    // self-instrumented counter can show reset has not run yet.
    #[cfg(test)]
    reset_calls: std::cell::Cell<u32>,
}

impl RawCommitStatement {
    /// Prepares `COMMIT` against `conn`'s raw handle. The returned value
    /// must not outlive `conn` — enforced by [`TransactionStatements`]
    /// never constructing one outside its own `'conn`-bound fields.
    fn prepare(conn: &Connection) -> rusqlite::Result<Self> {
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        // SAFETY: `conn.handle()` is valid for this call; `c"COMMIT"` is
        // nul-terminated; `stmt`/`ptr::null_mut()` (no tail pointer
        // needed, one statement per string) are valid out-parameters.
        let rc = unsafe {
            ffi::sqlite3_prepare_v2(
                conn.handle(),
                c"COMMIT".as_ptr(),
                -1,
                &mut stmt,
                ptr::null_mut(),
            )
        };
        if rc != ffi::SQLITE_OK {
            // SAFETY: `conn.handle()` is valid for this call.
            return Err(sqlite_error(unsafe { conn.handle() }, rc));
        }
        Ok(Self {
            stmt,
            #[cfg(test)]
            reset_calls: std::cell::Cell::new(0),
        })
    }

    /// Steps `COMMIT` to completion and returns immediately — the
    /// caller's measured window must end here (protocol §5): nothing
    /// this call's return triggers, including [`reset`](Self::reset),
    /// may run before the caller has already stopped timing.
    fn step(&mut self) -> rusqlite::Result<()> {
        // SAFETY: `self.stmt` is a live statement from `prepare`, not yet
        // finalized (finalization only happens in `Drop`).
        match unsafe { ffi::sqlite3_step(self.stmt) } {
            ffi::SQLITE_DONE => Ok(()),
            rc => Err(self.error(rc)),
        }
    }

    /// Resets `COMMIT` for its next run. Must only be called after the
    /// timed window [`step`](Self::step) ended has already closed.
    fn reset(&mut self) -> rusqlite::Result<()> {
        #[cfg(test)]
        self.reset_calls.set(self.reset_calls.get() + 1);
        // SAFETY: see `step`.
        match unsafe { ffi::sqlite3_reset(self.stmt) } {
            ffi::SQLITE_OK => Ok(()),
            rc => Err(self.error(rc)),
        }
    }

    fn error(&self, rc: c_int) -> rusqlite::Error {
        // SAFETY: `self.stmt` is live; `sqlite3_db_handle` never fails
        // for a non-null statement.
        let db = unsafe { ffi::sqlite3_db_handle(self.stmt) };
        sqlite_error(db, rc)
    }

    /// How many times [`reset`](Self::reset) has actually run — the
    /// instrumented boundary a test pins the window's end against: still
    /// 0 right after [`step`](Self::step) returns, only becoming nonzero
    /// once `reset` has actually been called.
    #[cfg(test)]
    fn reset_calls(&self) -> u32 {
        self.reset_calls.get()
    }
}

impl Drop for RawCommitStatement {
    fn drop(&mut self) {
        // SAFETY: `self.stmt` is live and owned exclusively by `self`.
        unsafe {
            ffi::sqlite3_finalize(self.stmt);
        }
    }
}

/// The `BEGIN`/`COMMIT` statements every workload shares (protocol §2:
/// "it prepares the three BEGIN variants and COMMIT itself"), prepared
/// once per connection and held for the run — one `COMMIT` statement
/// object reused by every workload's operations, not one copy each.
pub struct TransactionStatements<'conn> {
    begin_deferred: Statement<'conn>,
    begin_immediate: Statement<'conn>,
    commit: RawCommitStatement,
}

impl<'conn> TransactionStatements<'conn> {
    pub fn prepare(conn: &'conn Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            begin_deferred: conn.prepare("BEGIN DEFERRED")?,
            begin_immediate: conn.prepare("BEGIN IMMEDIATE")?,
            commit: RawCommitStatement::prepare(conn)?,
        })
    }

    /// Resets `COMMIT` so the next operation's `BEGIN` can run. Must be
    /// called exactly once per completed operation, after the caller has
    /// already obtained its [`crate::vfs::time_operation`] result —
    /// protocol §5's timed window has closed by then, so the reset's own
    /// (negligible but nonzero) cost is correctly excluded from it.
    pub fn end_operation(&mut self) -> rusqlite::Result<()> {
        self.commit.reset()
    }
}

/// Workload A's own statement (protocol §2), prepared once and held.
pub struct PointReadStatements<'conn> {
    select: Statement<'conn>,
}

impl<'conn> PointReadStatements<'conn> {
    pub fn prepare(conn: &'conn Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            select: conn.prepare("SELECT id, email, balance FROM accounts WHERE id = ?1")?,
        })
    }
}

/// Workload A (protocol §2): `BEGIN DEFERRED; SELECT id, email, balance
/// FROM accounts WHERE id = ?1; COMMIT;`. No statement is prepared or
/// looked up here — only bind, step (`SELECT`'s row materializes inside
/// this call, protocol §5), and reset, on statements [`TransactionStatements`]
/// and [`PointReadStatements`] already hold.
pub fn execute_point_read(
    txn: &mut TransactionStatements<'_>,
    stmts: &mut PointReadStatements<'_>,
    params: &ReadParams,
) -> rusqlite::Result<(i64, String, i64)> {
    txn.begin_deferred.execute(())?;
    let row = stmts.select.query_row([params.id as i64], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?;
    txn.commit.step()?;
    Ok(row)
}

/// Workload B's own statement (protocol §2), prepared once and held.
pub struct PointUpdateStatements<'conn> {
    update: Statement<'conn>,
}

impl<'conn> PointUpdateStatements<'conn> {
    pub fn prepare(conn: &'conn Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            update: conn.prepare("UPDATE accounts SET balance = balance + 1 WHERE id = ?1")?,
        })
    }
}

/// Workload B (protocol §2): `BEGIN IMMEDIATE; UPDATE accounts SET
/// balance = balance + 1 WHERE id = ?1; COMMIT;`.
pub fn execute_point_update(
    txn: &mut TransactionStatements<'_>,
    stmts: &mut PointUpdateStatements<'_>,
    params: &UpdateParams,
) -> rusqlite::Result<()> {
    txn.begin_immediate.execute(())?;
    stmts.update.execute([params.id as i64])?;
    txn.commit.step()?;
    Ok(())
}

/// Workload C's own statement (protocol §2), prepared once and held.
pub struct InsertStatements<'conn> {
    insert: Statement<'conn>,
}

impl<'conn> InsertStatements<'conn> {
    pub fn prepare(conn: &'conn Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            insert: conn.prepare("INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 0)")?,
        })
    }
}

/// Whether workload C's insert committed on the primary email or needed
/// the precomputed collision fallback (protocol §2) — the signal the
/// caller uses to decide whether to adopt
/// [`generators::InsertFallback`]'s advanced RNG state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Committed,
    CommittedWithFallback,
}

/// Workload C (protocol §2): `BEGIN IMMEDIATE; INSERT INTO accounts (id,
/// email, balance) VALUES (?1, ?2, 0); COMMIT;`. On `SQLITE_CONSTRAINT`
/// (an email collision), retries once with `fallback_email`; a second
/// collision propagates the constraint error unchanged — "a second
/// collision aborts the run as a protocol violation." The collision is
/// only known once the first `INSERT` has already run, inside the timed
/// transaction, but protocol §5 forbids any PRNG work inside that
/// window, so `fallback_email` must already be drawn —
/// [`generators::prepare_insert_fallback`], before this function (and
/// before `crate::vfs::time_operation`'s window around it) is even
/// called.
pub fn execute_insert(
    txn: &mut TransactionStatements<'_>,
    stmts: &mut InsertStatements<'_>,
    params: &InsertParams,
    fallback_email: &str,
) -> rusqlite::Result<InsertOutcome> {
    txn.begin_immediate.execute(())?;
    let primary = stmts
        .insert
        .execute(rusqlite::params![params.id as i64, &params.email]);
    match primary {
        Ok(_) => {
            txn.commit.step()?;
            Ok(InsertOutcome::Committed)
        }
        Err(err) if is_constraint_violation(&err) => {
            stmts
                .insert
                .execute(rusqlite::params![params.id as i64, fallback_email])?;
            txn.commit.step()?;
            Ok(InsertOutcome::CommittedWithFallback)
        }
        Err(err) => Err(err),
    }
}

/// Workload D's own statements (protocol §2), prepared once and held.
pub struct TransferStatements<'conn> {
    select_balance: Statement<'conn>,
    debit: Statement<'conn>,
    credit: Statement<'conn>,
}

impl<'conn> TransferStatements<'conn> {
    pub fn prepare(conn: &'conn Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            select_balance: conn.prepare("SELECT balance FROM accounts WHERE id = ?1")?,
            debit: conn.prepare("UPDATE accounts SET balance = balance - 1 WHERE id = ?2")?,
            credit: conn.prepare("UPDATE accounts SET balance = balance + 1 WHERE id = ?3")?,
        })
    }
}

/// Workload D (protocol §2): one transaction reading the sender's
/// balance, then — if it covers [`TRANSFER_AMOUNT`] — debiting the
/// sender and crediting the receiver. Every repetition commits
/// regardless of the guard outcome; the return value says whether the
/// guard blocked the transfer, so a caller can count how often it did.
/// `?2`/`?3` are explicit numbered placeholders (protocol §2's literal
/// SQL), bound with `raw_bind_parameter` since position 1 is unused in
/// those two statements.
pub fn execute_transfer(
    txn: &mut TransactionStatements<'_>,
    stmts: &mut TransferStatements<'_>,
    params: &TransferParams,
) -> rusqlite::Result<bool> {
    txn.begin_immediate.execute(())?;
    let balance: i64 = stmts
        .select_balance
        .query_row([params.sender as i64], |r| r.get(0))?;
    let blocked = balance < TRANSFER_AMOUNT;
    if !blocked {
        stmts.debit.raw_bind_parameter(2, params.sender as i64)?;
        stmts.debit.raw_execute()?;

        stmts.credit.raw_bind_parameter(3, params.receiver as i64)?;
        stmts.credit.raw_execute()?;
    }
    txn.commit.step()?;
    Ok(blocked)
}

/// Per-repetition correctness checks (protocol §2.1).
pub mod checks {
    use rusqlite::Connection;

    /// After B/D: no `balance` negative.
    pub fn no_negative_balances(conn: &Connection) -> rusqlite::Result<bool> {
        let count: i64 =
            conn.query_row("SELECT count(*) FROM accounts WHERE balance < 0", [], |r| {
                r.get(0)
            })?;
        Ok(count == 0)
    }

    /// After D: `SELECT sum(balance)` unchanged (zero-sum transfers).
    pub fn sum_of_balances(conn: &Connection) -> rusqlite::Result<i64> {
        conn.query_row("SELECT sum(balance) FROM accounts", [], |r| r.get(0))
    }

    /// After A/B/C/D: the current row count, to check C's growth bound
    /// or a restore's row count against the manifest (protocol §2.1/§3).
    pub fn row_count(conn: &Connection) -> rusqlite::Result<u64> {
        let count: i64 = conn.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::{Rng, SeedableRng};
    use rusqlite::StatementStatus;

    fn loaded_connection(row_count: u64) -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory connection");
        configure_connection(&conn, Synchronous::Full).expect("configure connection");
        create_schema(&conn).expect("create schema");
        initial_load(&conn, row_count).expect("initial load");
        conn
    }

    fn balance_of(conn: &Connection, id: u64) -> i64 {
        conn.query_row(
            "SELECT balance FROM accounts WHERE id = ?1",
            [id as i64],
            |r| r.get(0),
        )
        .expect("row must exist")
    }

    #[test]
    fn the_same_prepared_statement_runs_every_operation_never_reprepared() {
        let conn = loaded_connection(10);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = PointReadStatements::prepare(&conn).unwrap();

        // A freshly prepared statement's run counter starts at zero; a
        // production path that re-prepared (or looked up a cached
        // statement) per call would hand back a different statement
        // object each time, whose own counter would never accumulate
        // past 1 — only reusing this exact held statement lets its run
        // count climb with every operation.
        assert_eq!(stmts.select.get_status(StatementStatus::Run), 0);

        for id in 1..=5u64 {
            execute_point_read(&mut txn, &mut stmts, &ReadParams { id }).unwrap();
            txn.end_operation().unwrap();
        }

        assert_eq!(
            stmts.select.get_status(StatementStatus::Run),
            5,
            "the SELECT statement must be prepared once and run once per \
             operation, never re-prepared inside an operation"
        );
    }

    #[test]
    fn point_update_increments_the_targeted_balance_by_exactly_one() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = PointUpdateStatements::prepare(&conn).unwrap();

        let before = balance_of(&conn, 3);
        execute_point_update(&mut txn, &mut stmts, &UpdateParams { id: 3 }).unwrap();
        let after = balance_of(&conn, 3);

        assert_eq!(after - before, 1);
    }

    #[test]
    fn transfer_moves_exactly_one_unit_from_sender_to_receiver_when_unblocked() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = TransferStatements::prepare(&conn).unwrap();

        let total_before = checks::sum_of_balances(&conn).unwrap();
        let sender_before = balance_of(&conn, 1);
        let receiver_before = balance_of(&conn, 2);

        let blocked = execute_transfer(
            &mut txn,
            &mut stmts,
            &TransferParams {
                sender: 1,
                receiver: 2,
            },
        )
        .unwrap();

        assert!(
            !blocked,
            "the loaded balance must cover the transfer amount"
        );
        assert_eq!(sender_before - balance_of(&conn, 1), TRANSFER_AMOUNT);
        assert_eq!(balance_of(&conn, 2) - receiver_before, TRANSFER_AMOUNT);
        assert_eq!(
            checks::sum_of_balances(&conn).unwrap(),
            total_before,
            "a transfer must be zero-sum across every account"
        );
    }

    #[test]
    fn transfer_leaves_every_balance_unchanged_when_the_guard_blocks_it() {
        let conn = loaded_connection(5);
        // Drain the sender below the transfer amount first.
        conn.execute_batch("UPDATE accounts SET balance = 0 WHERE id = 1;")
            .unwrap();
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = TransferStatements::prepare(&conn).unwrap();

        let total_before = checks::sum_of_balances(&conn).unwrap();
        let blocked = execute_transfer(
            &mut txn,
            &mut stmts,
            &TransferParams {
                sender: 1,
                receiver: 2,
            },
        )
        .unwrap();

        assert!(blocked);
        assert_eq!(balance_of(&conn, 1), 0);
        assert_eq!(checks::sum_of_balances(&conn).unwrap(), total_before);
    }

    #[test]
    fn insert_falls_back_on_a_collision_and_commits_with_the_precomputed_email() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();

        // Force the primary attempt's email to collide with row 1's.
        let colliding_email = generators::initial_load_email(1);
        let params = InsertParams {
            id: 6,
            email: colliding_email.clone(),
        };
        let rng = generators::rng_for_attempt(0, 0);
        let fallback = generators::prepare_insert_fallback(&rng, 1);

        let outcome = execute_insert(&mut txn, &mut stmts, &params, &fallback.email).unwrap();
        txn.end_operation().unwrap();

        assert_eq!(outcome, InsertOutcome::CommittedWithFallback);
        assert_eq!(checks::row_count(&conn).unwrap(), 6);
        let landed_email: String = conn
            .query_row("SELECT email FROM accounts WHERE id = 6", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            landed_email, fallback.email,
            "the row must have landed with the precomputed fallback email, not the colliding one"
        );
        assert_ne!(landed_email, colliding_email);
    }

    #[test]
    fn insert_errors_as_a_protocol_violation_on_a_second_collision() {
        let conn = loaded_connection(2);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();

        let colliding_email = generators::initial_load_email(1);
        let rng = generators::rng_for_attempt(0, 0);
        let repetition_counter = 1;
        let fallback = generators::prepare_insert_fallback(&rng, repetition_counter);

        // Pre-insert a row already holding the precomputed fallback
        // email, so the retry collides too.
        conn.execute(
            "INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 0)",
            rusqlite::params![999_i64, &fallback.email],
        )
        .unwrap();
        let row_count_before = checks::row_count(&conn).unwrap();

        let params = InsertParams {
            id: 3,
            email: colliding_email,
        };
        let result = execute_insert(&mut txn, &mut stmts, &params, &fallback.email);

        assert!(
            result.is_err(),
            "a second collision must propagate as an error, never succeed silently"
        );
        assert_eq!(
            checks::row_count(&conn).unwrap(),
            row_count_before,
            "neither the primary nor the fallback attempt may have committed a row"
        );
    }

    #[test]
    fn the_main_rng_advances_only_when_the_fallback_is_actually_used() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();

        let mut rng = generators::rng_for_attempt(0, 0);
        let before_draw = rng.clone().next_u64();
        let fallback = generators::prepare_insert_fallback(&rng, 1);

        // A genuinely unique email: no collision, so the fallback must
        // never be adopted.
        let params = InsertParams {
            id: 6,
            email: "0000000000000000-unique-1".to_string(),
        };
        let outcome = execute_insert(&mut txn, &mut stmts, &params, &fallback.email).unwrap();
        txn.end_operation().unwrap();

        assert_eq!(outcome, InsertOutcome::Committed);
        // `fallback` is dropped here without ever calling `commit`, so
        // `rng`'s stream must be exactly as if the fallback had never
        // been drawn.
        assert_eq!(
            rng.next_u64(),
            before_draw,
            "a non-colliding insert must leave the main RNG stream untouched"
        );
    }

    #[test]
    fn the_main_rng_adopts_the_fallback_s_advanced_state_once_committed() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();

        let mut rng = generators::rng_for_attempt(0, 0);
        let colliding_email = generators::initial_load_email(1);
        let params = InsertParams {
            id: 6,
            email: colliding_email,
        };
        let fallback = generators::prepare_insert_fallback(&rng, 1);
        let expected_next = fallback_rng_lookahead(&rng, 1);

        let outcome = execute_insert(&mut txn, &mut stmts, &params, &fallback.email).unwrap();
        txn.end_operation().unwrap();
        assert_eq!(outcome, InsertOutcome::CommittedWithFallback);

        fallback.commit(&mut rng);

        assert_eq!(
            rng.next_u64(),
            expected_next,
            "committing the fallback must leave `rng` exactly where the clone that drew it ended up"
        );
    }

    /// What `rng` would draw next *after* drawing one fallback email —
    /// computed independently of [`generators::prepare_insert_fallback`]
    /// itself, so the test above pins its contract rather than mirroring
    /// its implementation.
    fn fallback_rng_lookahead(rng: &ChaCha8Rng, repetition_counter: u64) -> u64 {
        let mut clone = rng.clone();
        generators::inserted_email(&mut clone, repetition_counter);
        clone.next_u64()
    }

    #[test]
    fn no_prng_call_happens_inside_insert_s_timed_window() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();

        let colliding_email = generators::initial_load_email(1);
        let params = InsertParams {
            id: 6,
            email: colliding_email,
        };
        let rng = CountingRng::new(1);
        let fallback = generators::prepare_insert_fallback(&rng, 1);
        let calls_before_window = rng.calls();
        assert!(
            calls_before_window > 0,
            "preparing the fallback must itself draw from the PRNG, before any window opens"
        );

        let (outcome, _timing) = crate::vfs::time_operation(|| {
            execute_insert(&mut txn, &mut stmts, &params, &fallback.email)
        })
        .unwrap();
        txn.end_operation().unwrap();

        assert_eq!(outcome, InsertOutcome::CommittedWithFallback);
        assert_eq!(
            rng.calls(),
            calls_before_window,
            "no PRNG draw may happen inside the timed window"
        );
    }

    #[test]
    fn the_timed_window_ends_at_commits_step_leaving_it_unreset() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = PointReadStatements::prepare(&conn).unwrap();

        let (_, _timing) = crate::vfs::time_operation(|| {
            execute_point_read(&mut txn, &mut stmts, &ReadParams { id: 1 })
        })
        .unwrap();

        assert_eq!(
            txn.commit.reset_calls(),
            0,
            "the op must return immediately after COMMIT's step, before any reset runs — \
             nothing after the step may fall inside the measured interval"
        );

        txn.end_operation().unwrap();
        assert_eq!(
            txn.commit.reset_calls(),
            1,
            "end_operation must reset COMMIT exactly once the window has closed"
        );
    }

    /// A `Rng` that counts every draw, via a shared counter so the test
    /// can read it after the counted `Rng` has been moved into a
    /// closure. Delegates to a real `ChaCha8Rng` so its draws are still
    /// genuine PRNG output, not a stub value.
    #[derive(Clone)]
    struct CountingRng {
        inner: rand_chacha::ChaCha8Rng,
        calls: std::rc::Rc<std::cell::Cell<u64>>,
    }

    impl CountingRng {
        fn new(seed: u64) -> Self {
            CountingRng {
                inner: rand_chacha::ChaCha8Rng::seed_from_u64(seed),
                calls: std::rc::Rc::new(std::cell::Cell::new(0)),
            }
        }

        fn calls(&self) -> u64 {
            self.calls.get()
        }

        fn record(&self) {
            self.calls.set(self.calls.get() + 1);
        }
    }

    impl rand_chacha::rand_core::TryRng for CountingRng {
        type Error = std::convert::Infallible;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            self.record();
            Ok(self.inner.next_u32())
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            self.record();
            Ok(self.inner.next_u64())
        }

        fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Self::Error> {
            self.record();
            self.inner.fill_bytes(dst);
            Ok(())
        }
    }
}
