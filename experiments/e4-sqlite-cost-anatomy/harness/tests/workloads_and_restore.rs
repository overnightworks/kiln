//! End-to-end mechanics across generators, workloads, and restore,
//! against real (tiny) SQLite files — never a benchmark or pilot run,
//! just proof that the pieces compose correctly.

use kiln_e4_harness::workloads::Synchronous;
use kiln_e4_harness::{generators, restore, workloads};
use rusqlite::Connection;
use tempfile::TempDir;

fn setup(
    row_count: u64,
    synchronous: Synchronous,
) -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("accounts.db");
    let wal_path = dir.path().join("accounts.db-wal");
    let conn = Connection::open(&db_path).expect("open");
    workloads::configure_connection(&conn, synchronous).expect("configure");
    workloads::create_schema(&conn).expect("create schema");
    workloads::initial_load(&conn, row_count).expect("initial load");
    drop(conn);
    (dir, db_path, wal_path)
}

fn reopen(db_path: &std::path::Path, synchronous: Synchronous) -> Connection {
    let conn = Connection::open(db_path).expect("reopen");
    workloads::configure_connection(&conn, synchronous).expect("configure");
    conn
}

#[test]
fn point_read_returns_the_row_it_asked_for() {
    let (_dir, db_path, _wal) = setup(50, Synchronous::Full);
    let conn = reopen(&db_path, Synchronous::Full);

    let trace = generators::point_read_trace(generators::seed_for(0, 0), 50, 5);
    for params in &trace {
        let (id, _email, _balance) = workloads::execute_point_read(&conn, params).unwrap();
        assert_eq!(id as u64, params.id);
    }
}

#[test]
fn point_update_never_drives_a_balance_negative() {
    let (_dir, db_path, _wal) = setup(50, Synchronous::Full);
    let conn = reopen(&db_path, Synchronous::Full);

    let trace = generators::point_update_trace(generators::seed_for(1, 0), 50, 20);
    for params in &trace {
        workloads::execute_point_update(&conn, params).unwrap();
    }
    assert!(workloads::checks::no_negative_balances(&conn).unwrap());
}

#[test]
fn insert_grows_the_row_count_by_exactly_the_operation_count() {
    let (_dir, db_path, _wal) = setup(50, Synchronous::Full);
    let conn = reopen(&db_path, Synchronous::Full);
    let before = workloads::checks::row_count(&conn).unwrap();

    // A real driver threads one rng across both generation and
    // execution's redraw-on-collision, rather than pre-generating a
    // closed trace: a redraw consumes extra draws from the same stream.
    let mut rng = generators::rng_for_attempt(2, 0);
    let row_count = 50;
    for ordinal in 1..=10u64 {
        let params = generators::InsertParams {
            id: row_count + ordinal,
            email: generators::inserted_email(&mut rng, ordinal),
        };
        workloads::execute_insert(&conn, &params, &mut rng, ordinal).unwrap();
    }

    let after = workloads::checks::row_count(&conn).unwrap();
    assert_eq!(after, before + 10);
}

#[test]
fn transfer_keeps_the_sum_of_balances_invariant_and_never_goes_negative() {
    let (_dir, db_path, _wal) = setup(50, Synchronous::Full);
    let conn = reopen(&db_path, Synchronous::Full);
    let before_sum = workloads::checks::sum_of_balances(&conn).unwrap();

    let trace = generators::transfer_trace(generators::seed_for(3, 0), 50, 30);
    for params in &trace {
        workloads::execute_transfer(&conn, params).unwrap();
    }

    assert_eq!(
        workloads::checks::sum_of_balances(&conn).unwrap(),
        before_sum
    );
    assert!(workloads::checks::no_negative_balances(&conn).unwrap());
}

#[test]
fn restoring_pristine_state_undoes_a_real_write() {
    let (dir, db_path, wal_path) = setup(50, Synchronous::Full);
    restore::checkpoint_truncate(&db_path).unwrap();
    let pristine_dir = dir.path().join("pristine");
    restore::save_pristine(&db_path, &wal_path, &pristine_dir).unwrap();

    let conn = reopen(&db_path, Synchronous::Full);
    let before_row_count = workloads::checks::row_count(&conn).unwrap();
    workloads::execute_point_update(&conn, &generators::UpdateParams { id: 1 }).unwrap();
    drop(conn);

    restore::restore_pristine(&pristine_dir, &db_path, &wal_path).unwrap();

    let conn = reopen(&db_path, Synchronous::Full);
    assert_eq!(
        workloads::checks::row_count(&conn).unwrap(),
        before_row_count
    );
    let balance: i64 = conn
        .query_row("SELECT balance FROM accounts WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        balance, 1_000_000,
        "the update's +1 must be undone by restore"
    );
}
