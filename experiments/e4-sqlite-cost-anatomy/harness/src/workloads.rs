//! The schema, the four workloads, and the connection settings frozen by
//! protocol.md §2 and §4. Every operation is one transaction; `BEGIN`,
//! `COMMIT`, and each statement are prepared once per connection, at
//! startup, and held for the run — [`TransactionStatements`] for the
//! shared `BEGIN`/`COMMIT` statements, a per-workload `*Statements`
//! struct for the rest — never re-prepared or looked up per operation,
//! and never `Connection::transaction()` (which issues `BEGIN`/`COMMIT`
//! through a one-off `execute_batch`, not a cached statement — protocol
//! §2).

use rand_chacha::rand_core::Rng;
use rusqlite::{Connection, Error as SqlError, ErrorCode, Statement};

use crate::generators::{self, InsertParams, ReadParams, TransferParams, UpdateParams};
use crate::vfs::Excluded;

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

/// The `BEGIN`/`COMMIT` statements every workload shares (protocol §2:
/// "it prepares the three BEGIN variants and COMMIT itself"), prepared
/// once per connection and held for the run — one `COMMIT` statement
/// object reused by every workload's operations, not one copy each.
pub struct TransactionStatements<'conn> {
    begin_deferred: Statement<'conn>,
    begin_immediate: Statement<'conn>,
    commit: Statement<'conn>,
}

impl<'conn> TransactionStatements<'conn> {
    pub fn prepare(conn: &'conn Connection) -> rusqlite::Result<Self> {
        Ok(Self {
            begin_deferred: conn.prepare("BEGIN DEFERRED")?,
            begin_immediate: conn.prepare("BEGIN IMMEDIATE")?,
            commit: conn.prepare("COMMIT")?,
        })
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
    txn.commit.execute(())?;
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
    txn.commit.execute(())?;
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

/// Workload C (protocol §2): `BEGIN IMMEDIATE; INSERT INTO accounts (id,
/// email, balance) VALUES (?1, ?2, 0); COMMIT;`. On `SQLITE_CONSTRAINT`
/// (an email collision), redraws the random component once from `rng`;
/// a second collision propagates the constraint error — "a second
/// collision aborts the run as a protocol violation." The collision is
/// only known once the first `INSERT` has already run inside the timed
/// transaction, so the redraw cannot happen before the window opens
/// (protocol §5); `excluded` instead marks its PRNG draw and formatting
/// as excluded from `wall` once it is known to be needed — the RNG still
/// advances only on that conditional path, never on every operation.
pub fn execute_insert(
    txn: &mut TransactionStatements<'_>,
    stmts: &mut InsertStatements<'_>,
    params: &InsertParams,
    rng: &mut impl Rng,
    repetition_counter: u64,
    excluded: &Excluded,
) -> rusqlite::Result<()> {
    txn.begin_immediate.execute(())?;
    let mut email = params.email.clone();
    for attempt in 0..2 {
        let outcome = stmts
            .insert
            .execute(rusqlite::params![params.id as i64, email]);
        match outcome {
            Ok(_) => {
                txn.commit.execute(())?;
                return Ok(());
            }
            Err(err) if attempt == 0 && is_constraint_violation(&err) => {
                email = excluded.exclude(|| generators::inserted_email(rng, repetition_counter));
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("the loop above returns on its first or second attempt")
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
    txn.commit.execute(())?;
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
    fn insert_redraws_once_on_a_collision_and_commits_with_the_fallback_email() {
        let conn = loaded_connection(5);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();
        let excluded = Excluded::new();

        // Force the first attempt's email to collide with row 1's.
        let colliding_email = generators::initial_load_email(1);
        let params = InsertParams {
            id: 6,
            email: colliding_email.clone(),
        };
        let mut rng = generators::rng_for_attempt(0, 0);

        execute_insert(&mut txn, &mut stmts, &params, &mut rng, 1, &excluded).unwrap();

        assert_eq!(checks::row_count(&conn).unwrap(), 6);
        let landed_email: String = conn
            .query_row("SELECT email FROM accounts WHERE id = 6", [], |r| r.get(0))
            .unwrap();
        assert_ne!(
            landed_email, colliding_email,
            "the row must have landed with the redrawn fallback email, not the colliding one"
        );
    }

    #[test]
    fn insert_errors_as_a_protocol_violation_on_a_second_collision() {
        let conn = loaded_connection(2);
        let mut txn = TransactionStatements::prepare(&conn).unwrap();
        let mut stmts = InsertStatements::prepare(&conn).unwrap();
        let excluded = Excluded::new();

        let colliding_email = generators::initial_load_email(1);
        let mut rng = generators::rng_for_attempt(0, 0);
        let repetition_counter = 1;

        // Predict the exact fallback email the real call will redraw,
        // from a cloned RNG at the same point in the stream, and
        // pre-insert a row holding it — so the retry collides too.
        let predicted_fallback = generators::inserted_email(&mut rng.clone(), repetition_counter);
        conn.execute(
            "INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 0)",
            rusqlite::params![999_i64, predicted_fallback],
        )
        .unwrap();
        let row_count_before = checks::row_count(&conn).unwrap();

        let params = InsertParams {
            id: 3,
            email: colliding_email,
        };
        let result = execute_insert(
            &mut txn,
            &mut stmts,
            &params,
            &mut rng,
            repetition_counter,
            &excluded,
        );

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
}
