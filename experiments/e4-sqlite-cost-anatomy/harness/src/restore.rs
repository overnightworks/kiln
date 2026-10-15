//! Pristine-state restore (protocol.md §2.1): each size loads once, is
//! checkpointed and truncated to an empty WAL, and that main-file/WAL
//! pair is the state restored, outside the timed window, before every
//! attempt — including A, B, D, which mutate values but not row count:
//! a fresh process alone does not restore on-disk files, so a
//! repetition would otherwise inherit whatever the previous attempt
//! left.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Checkpoints `db_path`'s WAL to empty and truncates it (protocol
/// §2.1), on a connection distinct from whichever loaded the data. Must
/// run after the loading connection has closed.
pub fn checkpoint_truncate(db_path: &Path) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
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
/// leave a zero-byte one, depending on the filesystem.
pub fn save_pristine(db_path: &Path, wal_path: &Path, pristine_dir: &Path) -> io::Result<()> {
    fs::create_dir_all(pristine_dir)?;
    fs::copy(db_path, pristine_main(pristine_dir))?;
    if wal_path.exists() {
        fs::copy(wal_path, pristine_wal(pristine_dir))?;
    } else {
        fs::write(pristine_wal(pristine_dir), [])?;
    }
    Ok(())
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
        fs::write(&wal_path, b"pristine wal bytes").unwrap();
        save_pristine(&db_path, &wal_path, &pristine_dir).unwrap();

        // An attempt mutates both files...
        fs::write(&db_path, b"mutated by a repetition").unwrap();
        fs::write(&wal_path, b"mutated wal").unwrap();

        // ...and restoring brings them back byte-for-byte.
        restore_pristine(&pristine_dir, &db_path, &wal_path).unwrap();
        assert_eq!(fs::read(&db_path).unwrap(), b"pristine database bytes");
        assert_eq!(fs::read(&wal_path).unwrap(), b"pristine wal bytes");
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
}
