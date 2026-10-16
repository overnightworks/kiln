use kiln_ir::{
    Block, CheckConstraint, Column, ColumnType, Comparison, Expr, Function, FunctionName,
    IntegerWidth, Program, Relation, Schema, Table, Value,
};

use super::{
    HOT_KEY, ORDER_QUANTITY_MAX, QUANTITY_COLUMN, RESTOCK_AMOUNT, STOCK_MAX, STOCK_MIN,
    STOCK_TABLE, THRESHOLD_MAX, THRESHOLD_MIN,
};
use crate::builders::{
    abort_unless_in_range, argument, branch, field, holds, integer_parameter, minus, number, plus,
    read, returning, sequence, update, Step,
};
use crate::rows::{column, key_column, key_declaration, key_literal, table};

pub fn program() -> Program {
    Program {
        schema: schema(),
        functions: vec![order_line(), stock_check()],
    }
}

fn schema() -> Schema {
    let quantity_bound = |comparison, operand| CheckConstraint {
        table: table(STOCK_TABLE),
        column: column(QUANTITY_COLUMN),
        comparison,
        operand: Value::Integer(operand),
    };
    Schema {
        tables: vec![Table {
            name: table(STOCK_TABLE),
            columns: vec![
                key_column(),
                Column {
                    name: column(QUANTITY_COLUMN),
                    kind: ColumnType::Integer {
                        width: IntegerWidth::I16,
                        domain: None,
                    },
                },
            ],
            key: key_declaration(),
        }],
        check_constraints: vec![
            quantity_bound(Comparison::AtLeast, STOCK_MIN),
            quantity_bound(Comparison::AtMost, STOCK_MAX),
        ],
        ..Default::default()
    }
}

fn stock() -> Expr {
    field(0, QUANTITY_COLUMN)
}

fn read_stock() -> kiln_ir::Effect {
    read(STOCK_TABLE, key_literal(HOT_KEY), 0)
}

fn function(
    name: &str,
    parameter: &str,
    steps: impl IntoIterator<Item = Step>,
    finish: Block,
) -> Function {
    Function {
        name: FunctionName(name.to_owned()),
        parameters: vec![integer_parameter(parameter, IntegerWidth::I16)],
        body: sequence(steps, finish),
    }
}

fn order_line() -> Function {
    let steps = abort_unless_in_range("quantity", 1, ORDER_QUANTITY_MAX, "invalid_quantity");
    let remainder = minus(stock(), argument("quantity"));
    let set_quantity = |value| update(STOCK_TABLE, key_literal(HOT_KEY), QUANTITY_COLUMN, value);
    let keeps_the_margin = returning(vec![set_quantity(remainder.clone())], stock());
    let restocks = returning(
        vec![set_quantity(plus(
            remainder.clone(),
            number(RESTOCK_AMOUNT),
        ))],
        stock(),
    );
    let finish = branch(
        vec![read_stock()],
        holds(remainder, Relation::GreaterOrEqual, number(STOCK_MIN)),
        keeps_the_margin,
        restocks,
    );
    function("order_line", "quantity", steps, finish)
}

fn stock_check() -> Function {
    let steps = abort_unless_in_range(
        "threshold",
        THRESHOLD_MIN,
        THRESHOLD_MAX,
        "invalid_threshold",
    );
    let finish = branch(
        vec![read_stock()],
        holds(stock(), Relation::Less, argument("threshold")),
        returning(Vec::new(), number(1)),
        returning(Vec::new(), number(0)),
    );
    function("stock_check", "threshold", steps, finish)
}
