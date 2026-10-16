use kiln_ir::{
    Block, CheckConstraint, Column, ColumnType, Comparison, Expr, Function, FunctionName,
    IntegerWidth, Program, Relation, Schema, Table, Value,
};

use super::{AccountsAbort, Status, ACCOUNTS_TABLE, AMOUNT_MAX, BALANCE_COLUMN, STATUS_COLUMN};
use crate::builders::{
    abort_if, abort_if_absent, abort_unless_in_range, argument, field, integer_parameter, minus,
    number, plus, read, returning, sequence, update, Step,
};
use crate::rows::{column, key_column, key_declaration, table};

pub fn program() -> Program {
    Program {
        schema: schema(),
        functions: vec![deposit(), withdraw(), transfer(), freeze(), unfreeze()],
    }
}

fn schema() -> Schema {
    let bound = |column_name, comparison, operand| CheckConstraint {
        table: table(ACCOUNTS_TABLE),
        column: column(column_name),
        comparison,
        operand: Value::Integer(operand),
    };
    let integer_column = |name, width| Column {
        name: column(name),
        kind: ColumnType::Integer {
            width,
            domain: None,
        },
    };
    Schema {
        tables: vec![Table {
            name: table(ACCOUNTS_TABLE),
            columns: vec![
                key_column(),
                integer_column(BALANCE_COLUMN, IntegerWidth::I64),
                integer_column(STATUS_COLUMN, IntegerWidth::I8),
            ],
            key: key_declaration(),
        }],
        check_constraints: vec![
            bound(BALANCE_COLUMN, Comparison::AtLeast, 0),
            bound(STATUS_COLUMN, Comparison::AtLeast, Status::Open as i64),
            bound(STATUS_COLUMN, Comparison::AtMost, Status::Frozen as i64),
        ],
        ..Default::default()
    }
}

fn function(
    name: &str,
    parameters: &[&str],
    steps: impl IntoIterator<Item = Step>,
    finish: Block,
) -> Function {
    Function {
        name: FunctionName(name.to_owned()),
        parameters: parameters
            .iter()
            .map(|parameter| integer_parameter(parameter, IntegerWidth::I64))
            .collect(),
        body: sequence(steps, finish),
    }
}

fn read_account(account: &str, slot: u32) -> kiln_ir::Effect {
    read(ACCOUNTS_TABLE, vec![argument(account)], slot)
}

fn set(account: &str, column_name: &str, value: Expr) -> kiln_ir::Effect {
    update(ACCOUNTS_TABLE, vec![argument(account)], column_name, value)
}

fn balance(slot: u32) -> Expr {
    field(slot, BALANCE_COLUMN)
}

fn amount_in_range() -> [Step; 2] {
    abort_unless_in_range("amount", 1, AMOUNT_MAX, AccountsAbort::InvalidAmount.name())
}

fn account_must_exist(account: &str, slot: u32) -> Step {
    abort_if_absent(slot, AccountsAbort::AccountNotFound.name()).after(read_account(account, slot))
}

fn abort_if_status(slot: u32, status: Status, name: &'static str) -> Step {
    abort_if(
        field(slot, STATUS_COLUMN),
        Relation::Equal,
        number(status as i64),
        name,
    )
}

fn abort_if_short_of(slot: u32) -> Step {
    abort_if(
        balance(slot),
        Relation::Less,
        argument("amount"),
        AccountsAbort::InsufficientFunds.name(),
    )
}

fn deposit() -> Function {
    let steps = amount_in_range().into_iter().chain([
        account_must_exist("account", 0),
        abort_if_status(0, Status::Frozen, AccountsAbort::AccountFrozen.name()),
    ]);
    let credited = plus(balance(0), argument("amount"));
    let finish = returning(
        vec![set("account", BALANCE_COLUMN, credited.clone())],
        credited,
    );
    function("deposit", &["account", "amount"], steps, finish)
}

fn withdraw() -> Function {
    let steps = amount_in_range().into_iter().chain([
        account_must_exist("account", 0),
        abort_if_status(0, Status::Frozen, AccountsAbort::AccountFrozen.name()),
        abort_if_short_of(0),
    ]);
    let debited = minus(balance(0), argument("amount"));
    let finish = returning(
        vec![set("account", BALANCE_COLUMN, debited.clone())],
        debited,
    );
    function("withdraw", &["account", "amount"], steps, finish)
}

fn transfer() -> Function {
    let steps = amount_in_range().into_iter().chain([
        abort_if(
            argument("from"),
            Relation::Equal,
            argument("to"),
            AccountsAbort::SameAccount.name(),
        ),
        account_must_exist("from", 0),
        account_must_exist("to", 1),
        abort_if_status(0, Status::Frozen, AccountsAbort::AccountFrozen.name()),
        abort_if_status(1, Status::Frozen, AccountsAbort::AccountFrozen.name()),
        abort_if_short_of(0),
    ]);
    let debited = minus(balance(0), argument("amount"));
    let finish = returning(
        vec![
            set("from", BALANCE_COLUMN, debited.clone()),
            set("to", BALANCE_COLUMN, plus(balance(1), argument("amount"))),
        ],
        debited,
    );
    function("transfer", &["from", "to", "amount"], steps, finish)
}

/// Sets the status to `to`; an account already there aborts with `already`.
fn status_change(name: &str, to: Status, already: &'static str) -> Function {
    let steps = [
        account_must_exist("account", 0),
        abort_if_status(0, to, already),
    ];
    let finish = returning(
        vec![set("account", STATUS_COLUMN, number(to as i64))],
        balance(0),
    );
    function(name, &["account"], steps, finish)
}

fn freeze() -> Function {
    status_change(
        "freeze",
        Status::Frozen,
        AccountsAbort::AlreadyFrozen.name(),
    )
}

fn unfreeze() -> Function {
    status_change("unfreeze", Status::Open, AccountsAbort::NotFrozen.name())
}
