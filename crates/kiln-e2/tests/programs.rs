mod support;

use kiln_e2::accounts::{self, Account, AccountCall, AccountsState, Status};
use kiln_e2::inventory::{self, InventoryCall, StockState};
use kiln_e2::quota::{self, QuotaCall, QuotaState};
use kiln_ir::{
    AbortReason, Call, ColumnName, ColumnType, Comparison, IntegerWidth, Outcome, OverflowReason,
    ParameterName, Program, TableName,
};
use support::{aborted, committed, replay_all};

mod kiln_e2_15 {
    use super::*;

    /// A batch whose second call aborts, between two calls that commit.
    fn assert_abort_leaves_state_and_the_next_call_runs<C: Copy + Into<Call>>(
        program: &Program,
        initial: kiln_ir::Store,
        calls: [C; 3],
        aborted_name: &str,
    ) {
        let executions = replay_all(program, initial, &calls);
        assert_eq!(executions[1].outcome, aborted(aborted_name));
        assert_eq!(executions[1].store, executions[0].store);
        assert!(matches!(executions[0].outcome, Outcome::Committed { .. }));
        assert!(matches!(executions[2].outcome, Outcome::Committed { .. }));
        assert_ne!(executions[2].store, executions[1].store);
    }

    #[test]
    fn an_aborted_quota_call_changes_nothing_and_later_calls_run() {
        assert_abort_leaves_state_and_the_next_call_runs(
            &quota::program(),
            QuotaState { remaining: 500 }.to_store(),
            [
                QuotaCall::Allocate { quantity: 5 },
                QuotaCall::Refill { quantity: 51 },
                QuotaCall::Allocate { quantity: 5 },
            ],
            "invalid_quantity",
        );
    }

    #[test]
    fn an_aborted_inventory_call_changes_nothing_and_later_calls_run() {
        assert_abort_leaves_state_and_the_next_call_runs(
            &inventory::program(),
            StockState { quantity: 50 }.to_store(),
            [
                InventoryCall::OrderLine { quantity: 5 },
                InventoryCall::OrderLine { quantity: 11 },
                InventoryCall::OrderLine { quantity: 5 },
            ],
            "invalid_quantity",
        );
    }

    #[test]
    fn an_aborted_account_call_changes_nothing_and_later_calls_run() {
        let open = Account {
            balance: 10,
            status: Status::Open,
        };
        assert_abort_leaves_state_and_the_next_call_runs(
            &accounts::program(),
            AccountsState {
                accounts: [open; 4],
            }
            .to_store(),
            [
                AccountCall::Deposit {
                    account: 0,
                    amount: 5,
                },
                AccountCall::Withdraw {
                    account: 1,
                    amount: 11,
                },
                AccountCall::Deposit {
                    account: 0,
                    amount: 5,
                },
            ],
            "insufficient_funds",
        );
    }
}

mod kiln_e2_16 {
    use super::*;

    fn comparison_names(program: &Program) -> Vec<(String, String, Comparison, kiln_ir::Value)> {
        program
            .schema
            .check_constraints
            .iter()
            .map(|constraint| {
                (
                    constraint.table.0.clone(),
                    constraint.column.0.clone(),
                    constraint.comparison,
                    constraint.operand.clone(),
                )
            })
            .collect()
    }

    fn width_of(program: &Program, table: &str, column: &str) -> IntegerWidth {
        let declared = program
            .schema
            .column(&TableName(table.to_owned()), &ColumnName(column.to_owned()))
            .expect("the column is declared");
        match declared.kind {
            ColumnType::Integer {
                width,
                domain: None,
            } => width,
            other => panic!("{table}.{column} is not a plain integer: {other:?}"),
        }
    }

    fn constraint(
        table: &str,
        column: &str,
        comparison: Comparison,
        operand: i64,
    ) -> (String, String, Comparison, kiln_ir::Value) {
        (
            table.to_owned(),
            column.to_owned(),
            comparison,
            kiln_ir::Value::Integer(operand),
        )
    }

    #[test]
    fn the_quota_schema_is_i32_remaining_between_zero_and_the_ceiling() {
        let program = quota::program();
        assert_eq!(program.schema.validate(), Ok(()));
        assert_eq!(width_of(&program, "quota", "remaining"), IntegerWidth::I32);
        assert_eq!(
            comparison_names(&program),
            [
                constraint("quota", "remaining", Comparison::AtLeast, 0),
                constraint("quota", "remaining", Comparison::AtMost, 1000),
            ]
        );
    }

    #[test]
    fn the_inventory_schema_is_i16_quantity_between_ten_and_one_hundred() {
        let program = inventory::program();
        assert_eq!(program.schema.validate(), Ok(()));
        assert_eq!(width_of(&program, "stock", "quantity"), IntegerWidth::I16);
        assert_eq!(
            comparison_names(&program),
            [
                constraint("stock", "quantity", Comparison::AtLeast, 10),
                constraint("stock", "quantity", Comparison::AtMost, 100),
            ]
        );
    }

    #[test]
    fn the_accounts_schema_is_i64_balance_and_i8_status_with_bounds() {
        let program = accounts::program();
        assert_eq!(program.schema.validate(), Ok(()));
        assert_eq!(width_of(&program, "accounts", "balance"), IntegerWidth::I64);
        assert_eq!(width_of(&program, "accounts", "status"), IntegerWidth::I8);
        assert_eq!(
            comparison_names(&program),
            [
                constraint("accounts", "balance", Comparison::AtLeast, 0),
                constraint("accounts", "status", Comparison::AtLeast, 0),
                constraint("accounts", "status", Comparison::AtMost, 1),
            ]
        );
    }
}

mod kiln_e2_17 {
    use super::*;

    fn argument_overflow(parameter: &str) -> Outcome {
        Outcome::Aborted {
            reason: AbortReason::Overflow(OverflowReason::Argument {
                parameter: ParameterName(parameter.to_owned()),
            }),
        }
    }

    #[test]
    fn a_quota_quantity_beyond_i32_overflows_before_the_body_runs() {
        let executions = replay_all(
            &quota::program(),
            QuotaState { remaining: 500 }.to_store(),
            &[
                QuotaCall::Allocate {
                    quantity: i64::from(i32::MAX) + 1,
                },
                QuotaCall::Allocate { quantity: 5 },
            ],
        );
        assert_eq!(executions[0].outcome, argument_overflow("quantity"));
        assert_eq!(executions[1].outcome, committed(5));
    }

    #[test]
    fn an_order_quantity_beyond_i16_overflows_before_the_body_runs() {
        let executions = replay_all(
            &inventory::program(),
            StockState { quantity: 50 }.to_store(),
            &[InventoryCall::StockCheck {
                threshold: i64::from(i16::MAX) + 1,
            }],
        );
        assert_eq!(executions[0].outcome, argument_overflow("threshold"));
    }
}
