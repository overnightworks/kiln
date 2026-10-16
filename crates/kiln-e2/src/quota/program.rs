use kiln_ir::{
    Block, CheckConstraint, Column, ColumnType, Comparison, Function, FunctionName, IntegerWidth,
    Program, Relation, Schema, Table, Value,
};

use super::{CEILING, HOT_KEY, QUANTITY_MAX, QUOTA_TABLE, REMAINING_COLUMN};
use crate::builders::{
    abort_if, abort_unless_in_range, argument, branch, field, holds, integer_parameter, minus,
    number, plus, read, returning, sequence, update,
};
use crate::rows::{column, key_column, key_declaration, key_literal, table};

const QUANTITY: &str = "quantity";

pub fn program() -> Program {
    Program {
        schema: schema(),
        functions: vec![allocate(), refill()],
    }
}

fn schema() -> Schema {
    let remaining_bound = |comparison, operand| CheckConstraint {
        table: table(QUOTA_TABLE),
        column: column(REMAINING_COLUMN),
        comparison,
        operand: Value::Integer(operand),
    };
    Schema {
        tables: vec![Table {
            name: table(QUOTA_TABLE),
            columns: vec![
                key_column(),
                Column {
                    name: column(REMAINING_COLUMN),
                    kind: ColumnType::Integer {
                        width: IntegerWidth::I32,
                        domain: None,
                    },
                },
            ],
            key: key_declaration(),
        }],
        check_constraints: vec![
            remaining_bound(Comparison::AtLeast, 0),
            remaining_bound(Comparison::AtMost, CEILING),
        ],
        ..Default::default()
    }
}

fn remaining() -> kiln_ir::Expr {
    field(0, REMAINING_COLUMN)
}

fn set_remaining(value: kiln_ir::Expr) -> kiln_ir::Effect {
    update(QUOTA_TABLE, key_literal(HOT_KEY), REMAINING_COLUMN, value)
}

fn quantity_function(name: &str, body: Block) -> Function {
    Function {
        name: FunctionName(name.to_owned()),
        parameters: vec![integer_parameter(QUANTITY, IntegerWidth::I32)],
        body,
    }
}

fn allocate() -> Function {
    let steps = abort_unless_in_range(QUANTITY, 1, QUANTITY_MAX, "invalid_quantity")
        .into_iter()
        .chain([
            abort_if(remaining(), Relation::Equal, number(0), "quota_exhausted").after(read(
                QUOTA_TABLE,
                key_literal(HOT_KEY),
                0,
            )),
        ]);
    let grants_in_full = returning(
        vec![set_remaining(minus(remaining(), argument(QUANTITY)))],
        argument(QUANTITY),
    );
    let grants_what_is_left = returning(vec![set_remaining(number(0))], remaining());
    let finish = branch(
        Vec::new(),
        holds(remaining(), Relation::GreaterOrEqual, argument(QUANTITY)),
        grants_in_full,
        grants_what_is_left,
    );
    quantity_function("allocate", sequence(steps, finish))
}

fn refill() -> Function {
    let steps = abort_unless_in_range(QUANTITY, 1, QUANTITY_MAX, "invalid_quantity")
        .into_iter()
        .chain([
            abort_if(remaining(), Relation::Equal, number(CEILING), "quota_full").after(read(
                QUOTA_TABLE,
                key_literal(HOT_KEY),
                0,
            )),
        ]);
    let adds_in_full = returning(
        vec![set_remaining(plus(remaining(), argument(QUANTITY)))],
        argument(QUANTITY),
    );
    let adds_up_to_the_ceiling = returning(
        vec![set_remaining(number(CEILING))],
        minus(number(CEILING), remaining()),
    );
    let finish = branch(
        Vec::new(),
        holds(
            plus(remaining(), argument(QUANTITY)),
            Relation::LessOrEqual,
            number(CEILING),
        ),
        adds_in_full,
        adds_up_to_the_ceiling,
    );
    quantity_function("refill", sequence(steps, finish))
}
