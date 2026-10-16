//! Why a measurement child could not measure (protocol.md §2.1, §13):
//! one error type for the restore, the SQLite work, the repetition's
//! correctness checks and the sanitized timing failure.

use std::fmt;
use std::io;

use crate::repetition_checks::{CheckViolation, VerifyError};
use crate::vfs::TimingError;

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
    /// The repetition left the database in a state protocol §2.1 forbids.
    CheckViolation(CheckViolation),
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
            MeasureError::CheckViolation(violation) => write!(f, "{violation}"),
        }
    }
}

impl std::error::Error for MeasureError {}

impl From<rusqlite::Error> for MeasureError {
    fn from(err: rusqlite::Error) -> Self {
        MeasureError::Sqlite(err)
    }
}

impl From<VerifyError> for MeasureError {
    fn from(err: VerifyError) -> Self {
        match err {
            VerifyError::Sqlite(inner) => MeasureError::Sqlite(inner),
            VerifyError::Violation(violation) => MeasureError::CheckViolation(violation),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs;
    use std::time::Duration;

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
