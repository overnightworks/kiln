//! The per-repetition correctness checks (protocol.md §2.1): after B and
//! D no balance is negative, after D the sum of all balances is what it
//! was, and after C the row count grew by exactly the operations run. The
//! measurement child takes the "before" facts once its snapshot is
//! restored and verifies the rest after the last operation, outside the
//! timed region and outside the monitored window; a violation is an
//! execution failure, never a timing result. The queries themselves are
//! [`workloads::checks`], the one owner of what a check asks the database.

use std::fmt;

use rusqlite::Connection;

use crate::workloads::{self, Workload};

/// What a finished repetition got wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckViolation {
    NegativeBalance,
    BalanceSumChanged { before: i64, after: i64 },
    RowCountGrowth { expected: u64, found: u64 },
}

impl fmt::Display for CheckViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckViolation::NegativeBalance => {
                write!(
                    f,
                    "a balance is negative after the repetition (protocol §2.1)"
                )
            }
            CheckViolation::BalanceSumChanged { before, after } => write!(
                f,
                "the sum of balances changed from {before} to {after} (protocol §2.1)"
            ),
            CheckViolation::RowCountGrowth { expected, found } => write!(
                f,
                "the table should hold {expected} rows after the inserts, found {found} \
                 (protocol §2.1)"
            ),
        }
    }
}

/// What one repetition must leave true, captured before its first
/// operation.
#[derive(Debug)]
pub struct RepetitionChecks {
    balances_stay_non_negative: bool,
    balance_sum_before: Option<i64>,
    expected_row_count: Option<u64>,
}

impl RepetitionChecks {
    /// Captures the facts `workload` must preserve: `row_count` rows now,
    /// `operations` about to run (warm-up included).
    pub fn before(
        conn: &Connection,
        workload: Workload,
        row_count: u64,
        operations: usize,
    ) -> rusqlite::Result<Self> {
        let balance_sum_before = match workload {
            Workload::Transfer => Some(workloads::checks::sum_of_balances(conn)?),
            _ => None,
        };
        Ok(RepetitionChecks {
            balances_stay_non_negative: matches!(
                workload,
                Workload::PointUpdate | Workload::Transfer
            ),
            balance_sum_before,
            expected_row_count: (workload == Workload::Insert)
                .then(|| row_count + operations as u64),
        })
    }

    /// Checks the finished repetition's database against what
    /// [`before`](Self::before) captured.
    pub fn verify(&self, conn: &Connection) -> Result<(), VerifyError> {
        if self.balances_stay_non_negative && !workloads::checks::no_negative_balances(conn)? {
            return Err(CheckViolation::NegativeBalance.into());
        }
        if let Some(before) = self.balance_sum_before {
            let after = workloads::checks::sum_of_balances(conn)?;
            if after != before {
                return Err(CheckViolation::BalanceSumChanged { before, after }.into());
            }
        }
        if let Some(expected) = self.expected_row_count {
            let found = workloads::checks::row_count(conn)?;
            if found != expected {
                return Err(CheckViolation::RowCountGrowth { expected, found }.into());
            }
        }
        Ok(())
    }
}

/// Why a verification did not come out as a pass.
#[derive(Debug)]
pub enum VerifyError {
    Sqlite(rusqlite::Error),
    Violation(CheckViolation),
}

impl From<rusqlite::Error> for VerifyError {
    fn from(err: rusqlite::Error) -> Self {
        VerifyError::Sqlite(err)
    }
}

impl From<CheckViolation> for VerifyError {
    fn from(violation: CheckViolation) -> Self {
        VerifyError::Violation(violation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: u64 = 20;
    const OPERATIONS: usize = 4;

    fn loaded() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        workloads::create_schema(&conn).expect("schema");
        workloads::initial_load(&conn, ROWS).expect("load");
        conn
    }

    fn checks_for(conn: &Connection, workload: Workload) -> RepetitionChecks {
        RepetitionChecks::before(conn, workload, ROWS, OPERATIONS).expect("capture")
    }

    fn violation_of(checks: &RepetitionChecks, conn: &Connection) -> Option<CheckViolation> {
        match checks.verify(conn) {
            Ok(()) => None,
            Err(VerifyError::Violation(violation)) => Some(violation),
            Err(VerifyError::Sqlite(err)) => panic!("unexpected SQLite error: {err}"),
        }
    }

    mod kiln_e4_26 {
        use super::*;

        #[test]
        fn an_untouched_database_passes_every_workloads_checks() {
            let conn = loaded();
            for workload in [
                Workload::PointRead,
                Workload::PointUpdate,
                Workload::Transfer,
            ] {
                let checks = checks_for(&conn, workload);
                assert_eq!(violation_of(&checks, &conn), None, "{workload:?}");
            }
        }

        #[test]
        fn a_negative_balance_fails_point_update_and_transfer_but_not_point_read() {
            let conn = loaded();
            let checks = [
                Workload::PointRead,
                Workload::PointUpdate,
                Workload::Transfer,
            ]
            .map(|workload| (workload, checks_for(&conn, workload)));
            conn.execute("UPDATE accounts SET balance = -1 WHERE id = 3", [])
                .unwrap();

            for (workload, checks) in &checks {
                let expected =
                    (*workload != Workload::PointRead).then_some(CheckViolation::NegativeBalance);
                assert_eq!(violation_of(checks, &conn), expected, "{workload:?}");
            }
        }

        #[test]
        fn a_transfer_that_creates_or_loses_money_changes_the_sum_of_balances() {
            let conn = loaded();
            let checks = checks_for(&conn, Workload::Transfer);
            let before: i64 = workloads::checks::sum_of_balances(&conn).unwrap();
            conn.execute("UPDATE accounts SET balance = balance + 1 WHERE id = 1", [])
                .unwrap();

            assert_eq!(
                violation_of(&checks, &conn),
                Some(CheckViolation::BalanceSumChanged {
                    before,
                    after: before + 1
                })
            );
        }

        #[test]
        fn a_point_update_may_change_the_sum_of_balances() {
            let conn = loaded();
            let checks = checks_for(&conn, Workload::PointUpdate);
            conn.execute("UPDATE accounts SET balance = balance + 1 WHERE id = 1", [])
                .unwrap();

            assert_eq!(violation_of(&checks, &conn), None);
        }

        #[test]
        fn inserts_must_grow_the_table_by_exactly_the_operations_run() {
            for (extra_rows, expected_violation) in [
                (OPERATIONS as u64, None),
                (
                    OPERATIONS as u64 - 1,
                    Some(CheckViolation::RowCountGrowth {
                        expected: ROWS + OPERATIONS as u64,
                        found: ROWS + OPERATIONS as u64 - 1,
                    }),
                ),
                (
                    OPERATIONS as u64 + 1,
                    Some(CheckViolation::RowCountGrowth {
                        expected: ROWS + OPERATIONS as u64,
                        found: ROWS + OPERATIONS as u64 + 1,
                    }),
                ),
            ] {
                let conn = loaded();
                let checks = checks_for(&conn, Workload::Insert);
                for id in ROWS + 1..=ROWS + extra_rows {
                    conn.execute(
                        "INSERT INTO accounts (id, email, balance) VALUES (?1, ?2, 0)",
                        rusqlite::params![id as i64, format!("new-{id}")],
                    )
                    .unwrap();
                }

                assert_eq!(violation_of(&checks, &conn), expected_violation);
            }
        }

        #[test]
        fn a_violation_says_what_was_wrong() {
            for (violation, words) in [
                (CheckViolation::NegativeBalance, "negative"),
                (
                    CheckViolation::BalanceSumChanged {
                        before: 7,
                        after: 9,
                    },
                    "from 7 to 9",
                ),
                (
                    CheckViolation::RowCountGrowth {
                        expected: 30,
                        found: 31,
                    },
                    "30 rows",
                ),
            ] {
                assert!(violation.to_string().contains(words), "{violation}");
            }
        }
    }
}
