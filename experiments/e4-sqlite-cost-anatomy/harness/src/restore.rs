//! Pristine-state restore (protocol.md §2.1): each size loads once, is
//! checkpointed and truncated to an empty WAL, and that main-file/WAL
//! pair is the state restored, outside the timed window, before every
//! attempt — including A, B, D, which mutate values but not row count:
//! a fresh process alone does not restore on-disk files, so a
//! repetition would otherwise inherit whatever the previous attempt
//! left.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::workloads::{self, Synchronous};

/// Why [`checkpoint_truncate`] refused to proceed: either SQLite itself
/// reported the checkpoint blocked, or it reported success but left a
/// nonempty WAL behind — either way, the pristine snapshot [`save_pristine`]
/// is about to copy would not be the fully-checkpointed state protocol
/// §2.1 requires.
#[derive(Debug)]
pub enum CheckpointError {
    /// `PRAGMA wal_checkpoint(TRUNCATE)`'s first returned column
    /// reported the checkpoint blocked
    /// (https://www.sqlite.org/pragma.html#pragma_wal_checkpoint): some
    /// other reader or writer still needed WAL content.
    Blocked,
    /// The checkpoint reported success, but the WAL file was not empty
    /// or absent afterward.
    WalNotEmpty,
    Sqlite(rusqlite::Error),
    Io(io::Error),
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckpointError::Blocked => write!(
                f,
                "wal_checkpoint(TRUNCATE) reported itself blocked by another reader or writer"
            ),
            CheckpointError::WalNotEmpty => write!(
                f,
                "wal_checkpoint(TRUNCATE) reported success but left a nonempty WAL"
            ),
            CheckpointError::Sqlite(err) => write!(f, "{err}"),
            CheckpointError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for CheckpointError {}

impl From<rusqlite::Error> for CheckpointError {
    fn from(err: rusqlite::Error) -> Self {
        CheckpointError::Sqlite(err)
    }
}

impl From<io::Error> for CheckpointError {
    fn from(err: io::Error) -> Self {
        CheckpointError::Io(err)
    }
}

/// Why [`save_pristine`] refused to proceed: the WAL beside `db_path` was
/// not empty. The pristine snapshot is only ever meant to be the
/// fully-checkpointed state [`checkpoint_truncate`] establishes
/// (protocol §2.1); copying a nonempty WAL would freeze a snapshot with
/// outstanding, uncheckpointed frames instead, defeating the restore
/// boundary every repetition depends on.
#[derive(Debug)]
pub enum PristineSaveError {
    NonemptyWal,
    Io(io::Error),
}

impl fmt::Display for PristineSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PristineSaveError::NonemptyWal => write!(
                f,
                "refusing to save a pristine snapshot: the WAL file is not empty"
            ),
            PristineSaveError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PristineSaveError {}

impl From<io::Error> for PristineSaveError {
    fn from(err: io::Error) -> Self {
        PristineSaveError::Io(err)
    }
}

/// Whether `PRAGMA wal_checkpoint(TRUNCATE)`'s first returned column
/// reports the checkpoint blocked
/// (https://www.sqlite.org/pragma.html#pragma_wal_checkpoint). A pure
/// function so the decision is tested without a real SQLite connection.
pub fn checkpoint_was_blocked(busy_column: i64) -> bool {
    busy_column != 0
}

/// A missing WAL file counts as already empty — a checkpoint that fully
/// truncates it may remove the file rather than leave a zero-byte one,
/// depending on the filesystem.
pub fn wal_is_empty_or_absent(wal_path: &Path) -> io::Result<bool> {
    match fs::metadata(wal_path) {
        Ok(metadata) => Ok(metadata.len() == 0),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(err) => Err(err),
    }
}

/// Checkpoints `db_path`'s WAL to empty and truncates it (protocol
/// §2.1), on a connection distinct from whichever loaded the data. Must
/// run after the loading connection has closed. Reads the checkpoint's
/// own result row rather than discarding it, so a blocked checkpoint —
/// which SQLite still reports as `execute_batch`-level success — fails
/// closed instead of leaving [`save_pristine`] to copy a WAL that was
/// never actually emptied.
pub fn checkpoint_truncate(db_path: &Path, wal_path: &Path) -> Result<(), CheckpointError> {
    let conn = Connection::open(db_path)?;
    // Protocol §4's single-writer, single-connection assumption means
    // nothing should ever contend with this checkpoint; `rusqlite`'s
    // default 5-second busy timeout exists for ordinary multi-writer
    // use, and would otherwise turn unexpected contention into a silent
    // multi-second stall between repetitions instead of the immediate,
    // reportable `Blocked` this function already has to return.
    conn.busy_timeout(std::time::Duration::from_millis(0))?;
    let busy_column: i64 =
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE);", [], |row| row.get(0))?;
    if checkpoint_was_blocked(busy_column) {
        return Err(CheckpointError::Blocked);
    }
    if !wal_is_empty_or_absent(wal_path)? {
        return Err(CheckpointError::WalNotEmpty);
    }
    Ok(())
}

fn pristine_main(pristine_dir: &Path) -> PathBuf {
    pristine_dir.join("main.db")
}

fn pristine_wal(pristine_dir: &Path) -> PathBuf {
    pristine_dir.join("main.db-wal")
}

/// Copies the main database file and its WAL (empty after
/// [`checkpoint_truncate`]) into `pristine_dir`, creating it if needed.
/// A missing WAL file is treated as already empty, not an error — a
/// checkpoint that fully truncates it may remove the file rather than
/// leave a zero-byte one, depending on the filesystem. Refuses — before
/// creating `pristine_dir` or copying anything — to save a nonempty WAL
/// (see [`PristineSaveError::NonemptyWal`]): the pristine snapshot's own
/// save boundary must hold even if a caller skips
/// [`checkpoint_truncate`] or it silently left frames behind.
pub fn save_pristine(
    db_path: &Path,
    wal_path: &Path,
    pristine_dir: &Path,
) -> Result<(), PristineSaveError> {
    if !wal_is_empty_or_absent(wal_path)? {
        return Err(PristineSaveError::NonemptyWal);
    }
    fs::create_dir_all(pristine_dir)?;
    fs::copy(db_path, pristine_main(pristine_dir))?;
    if wal_path.exists() {
        fs::copy(wal_path, pristine_wal(pristine_dir))?;
    } else {
        fs::write(pristine_wal(pristine_dir), [])?;
    }
    Ok(())
}

/// Loads `row_count` rows once (protocol §2.1), asserts the row count,
/// closes the loading connection, checkpoints and truncates the WAL on a
/// second one, and saves that main-file/WAL pair as the pristine state in
/// `pristine_dir`. The loading database is a scratch directory beside
/// `pristine_dir`, removed before returning.
pub fn prepare_pristine(pristine_dir: &Path, row_count: u64) -> io::Result<()> {
    let parent = pristine_dir.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let loading = tempfile::tempdir_in(parent)?;
    let db_path = loading.path().join("accounts.db");
    let wal_path = loading.path().join("accounts.db-wal");

    let conn = Connection::open(&db_path).map_err(io::Error::other)?;
    workloads::configure_connection(&conn, Synchronous::Full).map_err(io::Error::other)?;
    workloads::create_schema(&conn).map_err(io::Error::other)?;
    workloads::initial_load(&conn, row_count).map_err(io::Error::other)?;
    let loaded = workloads::checks::row_count(&conn).map_err(io::Error::other)?;
    if loaded != row_count {
        return Err(io::Error::other(format!(
            "the initial load wrote {loaded} rows, not the {row_count} protocol §2.1 asks for"
        )));
    }
    drop(conn);

    checkpoint_truncate(&db_path, &wal_path).map_err(io::Error::other)?;
    save_pristine(&db_path, &wal_path, pristine_dir).map_err(io::Error::other)
}

/// Restores `db_path`/`wal_path` from `pristine_dir`, outside any timed
/// window (protocol §2.1).
pub fn restore_pristine(pristine_dir: &Path, db_path: &Path, wal_path: &Path) -> io::Result<()> {
    fs::copy(pristine_main(pristine_dir), db_path)?;
    fs::copy(pristine_wal(pristine_dir), wal_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_yields_byte_identical_starting_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("main.db");
        let wal_path = dir.path().join("main.db-wal");
        let pristine_dir = dir.path().join("pristine");

        fs::write(&db_path, b"pristine database bytes").unwrap();
        // A real pristine snapshot's WAL is always empty by the time
        // save_pristine runs (it only follows a successful TRUNCATE
        // checkpoint, protocol §2.1).
        fs::write(&wal_path, b"").unwrap();
        save_pristine(&db_path, &wal_path, &pristine_dir).unwrap();

        // An attempt mutates both files...
        fs::write(&db_path, b"mutated by a repetition").unwrap();
        fs::write(&wal_path, b"mutated wal").unwrap();

        // ...and restoring brings them back byte-for-byte.
        restore_pristine(&pristine_dir, &db_path, &wal_path).unwrap();
        assert_eq!(fs::read(&db_path).unwrap(), b"pristine database bytes");
        assert_eq!(fs::read(&wal_path).unwrap(), b"");
    }

    #[test]
    fn save_pristine_refuses_a_nonempty_wal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("main.db");
        let wal_path = dir.path().join("main.db-wal");
        let pristine_dir = dir.path().join("pristine");

        fs::write(&db_path, b"not actually checkpointed").unwrap();
        fs::write(&wal_path, b"still has outstanding frames").unwrap();

        let result = save_pristine(&db_path, &wal_path, &pristine_dir);

        assert!(matches!(result, Err(PristineSaveError::NonemptyWal)));
        assert!(
            !pristine_dir.exists(),
            "a refused save must not leave a partial pristine snapshot behind"
        );
    }

    #[test]
    fn save_pristine_treats_a_missing_wal_as_already_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("main.db");
        let wal_path = dir.path().join("main.db-wal"); // never created
        let pristine_dir = dir.path().join("pristine");

        fs::write(&db_path, b"checkpointed, truncated to no wal").unwrap();
        save_pristine(&db_path, &wal_path, &pristine_dir).unwrap();

        assert_eq!(
            fs::read(pristine_wal(&pristine_dir)).unwrap(),
            Vec::<u8>::new()
        );
    }

    #[test]
    fn repeated_restores_stay_identical_to_the_pristine_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("main.db");
        let wal_path = dir.path().join("main.db-wal");
        let pristine_dir = dir.path().join("pristine");

        fs::write(&db_path, b"row count 10000").unwrap();
        fs::write(&wal_path, b"").unwrap();
        save_pristine(&db_path, &wal_path, &pristine_dir).unwrap();

        for _ in 0..3 {
            fs::write(&db_path, b"a different mutation each time").unwrap();
            restore_pristine(&pristine_dir, &db_path, &wal_path).unwrap();
            assert_eq!(fs::read(&db_path).unwrap(), b"row count 10000");
        }
    }

    #[test]
    fn checkpoint_was_blocked_reads_the_pragmas_first_column() {
        assert!(!checkpoint_was_blocked(0));
        assert!(checkpoint_was_blocked(1));
    }

    #[test]
    fn wal_is_empty_or_absent_accepts_a_missing_file_and_a_zero_byte_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("never-created.db-wal");
        assert!(wal_is_empty_or_absent(&missing).unwrap());

        let empty = dir.path().join("empty.db-wal");
        fs::write(&empty, []).unwrap();
        assert!(wal_is_empty_or_absent(&empty).unwrap());
    }

    #[test]
    fn wal_is_empty_or_absent_rejects_a_nonempty_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nonempty = dir.path().join("nonempty.db-wal");
        fs::write(&nonempty, b"still has frames").unwrap();
        assert!(!wal_is_empty_or_absent(&nonempty).unwrap());
    }

    #[test]
    fn checkpoint_truncate_succeeds_and_leaves_an_empty_wal_when_nothing_else_holds_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("main.db");
        let wal_path = dir.path().join("main.db-wal");

        let loader = Connection::open(&db_path).unwrap();
        loader
            .execute_batch(
                "PRAGMA journal_mode = WAL; CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);",
            )
            .unwrap();
        drop(loader);

        checkpoint_truncate(&db_path, &wal_path).unwrap();
        assert!(wal_is_empty_or_absent(&wal_path).unwrap());
    }

    #[test]
    fn checkpoint_truncate_fails_when_a_concurrent_reader_blocks_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("main.db");
        let wal_path = dir.path().join("main.db-wal");

        let loader = Connection::open(&db_path).unwrap();
        loader
            .execute_batch(
                "PRAGMA journal_mode = WAL; CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);",
            )
            .unwrap();

        // The reader's snapshot must start while the WAL still holds the
        // insert above — SQLite auto-checkpoints (and can truncate) a
        // WAL the moment its last connection closes, so `loader` must
        // not be the only connection when it closes, or there would be
        // nothing left for a reader to block.
        let reader = Connection::open(&db_path).unwrap();
        reader
            .execute_batch("BEGIN; SELECT count(*) FROM t;")
            .unwrap();
        drop(loader);

        // A second connection holding an open read transaction forces
        // SQLite to keep serving WAL frames a TRUNCATE checkpoint would
        // otherwise reclaim, so the checkpoint reports itself blocked
        // (protocol reference: pragma_wal_checkpoint).
        let result = checkpoint_truncate(&db_path, &wal_path);

        assert!(matches!(result, Err(CheckpointError::Blocked)));
        reader.execute_batch("COMMIT;").unwrap();
    }

    mod kiln_e4_23 {
        use super::*;

        #[test]
        fn a_prepared_snapshot_restores_to_the_loaded_rows_with_an_empty_wal() {
            let dir = tempfile::tempdir().expect("tempdir");
            let pristine_dir = dir.path().join("pristine");
            prepare_pristine(&pristine_dir, 50).expect("prepare");

            let db_path = dir.path().join("work.db");
            let wal_path = dir.path().join("work.db-wal");
            restore_pristine(&pristine_dir, &db_path, &wal_path).expect("restore");

            assert!(wal_is_empty_or_absent(&wal_path).unwrap());
            let conn = Connection::open(&db_path).unwrap();
            assert_eq!(workloads::checks::row_count(&conn).unwrap(), 50);
            let first_balance: i64 = conn
                .query_row("SELECT balance FROM accounts WHERE id = 1", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(first_balance, 1_000_000);
        }

        #[test]
        fn preparing_a_snapshot_leaves_no_loading_database_behind() {
            let dir = tempfile::tempdir().expect("tempdir");
            let pristine_dir = dir.path().join("pristine");
            prepare_pristine(&pristine_dir, 5).expect("prepare");

            let entries: Vec<_> = fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert_eq!(entries, vec![std::ffi::OsString::from("pristine")]);
        }
    }
}
