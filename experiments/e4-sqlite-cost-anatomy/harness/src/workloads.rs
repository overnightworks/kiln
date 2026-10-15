//! The schema, the four workloads, and the connection settings frozen by
//! protocol.md §2 and §4. Every operation is one transaction; `BEGIN`,
//! `COMMIT`, and each statement are prepared once per connection and
//! reused via `rusqlite`'s statement cache (`prepare_cached`), never
//! `Connection::transaction()` (which issues `BEGIN`/`COMMIT` through a
//! one-off `execute_batch`, not a cached statement — protocol §2).

use rand_chacha::rand_core::Rng;
use rusqlite::{Connection, Error as SqlError, ErrorCode};

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

/// Workload A (protocol §2): `BEGIN DEFERRED; SELECT id, email, balance
/// FROM accounts WHERE id = ?1; COMMIT;`.
pub fn execute_point_read(
    conn: &Connection,
    params: &ReadParams,
) -> rusqlite::Result<(i64, String, i64)> {
    conn.prepare_cached("BEGIN DEFERRED")?.execute(())?;
    let row = conn
        .prepare_cached("SELECT id, email, balance FROM accounts WHERE id = ?1")?
        .query_row([params.id as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
    conn.prepare_cached("COMMIT")?.execute(())?;
    Ok(row)
}

/// Workload B (protocol §2): `BEGIN IMMEDIATE; UPDATE accounts SET
/// balance = balance + 1 WHERE id = ?1; COMMIT;`.
pub fn execute_point_update(conn: &Connection, params: &UpdateParams) -> rusqlite::Result<()> {
    conn.prepare_cached("BEGIN IMMEDIATE")?.execute(())?;
    conn.prepare_cached("UPDATE accounts SET balance = balance + 1 WHERE id = ?1")?
        .execute([params.id as i64])?;
    conn.prepare_cached("COMMIT")?.execute(())?;
    Ok(())
}

/// Workload C (protocol §2): `BEGIN IMMEDIATE; INSERT INTO accounts (id,
/// email, balance) VALUES (?1, ?2, 0); COMMIT;`. On `SQLITE_CONSTRAINT`
/// (an email collision), redraws the random component once from `rng`;
/// a second collision propagates the constraint error — "a second
/// collision aborts the run as a protocol violation."
pub fn execute_insert(
    conn: &Connection,
    params: &InsertParams,
    rng: &mut impl Rng,
    repetition_counter: u64,
) -> rusqlite::Result<()> {
    conn.prepare_cached("BEGIN IMMEDIATE")?.execute(())?;
    let mut email = params.email.clone();
    for attempt in 0..2 {
        let outcome = conn
            .prepare_cached("INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 0)")?
            .execute(rusqlite::params![params.id as i64, email]);
        match outcome {
            Ok(_) => {
                conn.prepare_cached("COMMIT")?.execute(())?;
                return Ok(());
            }
            Err(err) if attempt == 0 && is_constraint_violation(&err) => {
                email = generators::inserted_email(rng, repetition_counter);
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("the loop above returns on its first or second attempt")
}

/// Workload D (protocol §2): one transaction reading the sender's
/// balance, then — if it covers [`TRANSFER_AMOUNT`] — debiting the
/// sender and crediting the receiver. Every repetition commits
/// regardless of the guard outcome; the return value says whether the
/// guard blocked the transfer, so a caller can count how often it did.
/// `?2`/`?3` are explicit numbered placeholders (protocol §2's literal
/// SQL), bound with `raw_bind_parameter` since position 1 is unused in
/// those two statements.
pub fn execute_transfer(conn: &Connection, params: &TransferParams) -> rusqlite::Result<bool> {
    conn.prepare_cached("BEGIN IMMEDIATE")?.execute(())?;
    let balance: i64 = conn
        .prepare_cached("SELECT balance FROM accounts WHERE id = ?1")?
        .query_row([params.sender as i64], |r| r.get(0))?;
    let blocked = balance < TRANSFER_AMOUNT;
    if !blocked {
        let mut debit =
            conn.prepare_cached("UPDATE accounts SET balance = balance - 1 WHERE id = ?2")?;
        debit.raw_bind_parameter(2, params.sender as i64)?;
        debit.raw_execute()?;

        let mut credit =
            conn.prepare_cached("UPDATE accounts SET balance = balance + 1 WHERE id = ?3")?;
        credit.raw_bind_parameter(3, params.receiver as i64)?;
        credit.raw_execute()?;
    }
    conn.prepare_cached("COMMIT")?.execute(())?;
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
