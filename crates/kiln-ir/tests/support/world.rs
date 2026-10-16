//! A schema and builders for the compound-key tests: `lines` keyed by
//! `(w, d, o)`, a text-keyed `labels`, a keyless `events` and the indexes over
//! `lines`.

use kiln_ir::{
    AccessKind, AccessTarget, ClampDomain, Column, ColumnType, Effect, Execution, Expr,
    FunctionFault, Index, IndexName, IntegerWidth, Key, KeyDeclaration, Program, ProgramError, Row,
    Schema, Slot, Store, Table, Terminal,
};

use super::{
    call_under_test, column, columns_of, columns_of_values, integer, key_expr, program_with_schema,
    run_ok, table,
};

pub fn int_column(name: &str, width: IntegerWidth) -> Column {
    Column {
        name: column(name),
        kind: ColumnType::Integer {
            width,
            domain: None,
        },
    }
}

pub fn text_column(name: &str) -> Column {
    Column {
        name: column(name),
        kind: ColumnType::Text,
    }
}

pub fn key_of(columns: &[&str]) -> KeyDeclaration {
    KeyDeclaration::Columns(columns.iter().map(|name| column(name)).collect())
}

pub fn lines_columns() -> Vec<Column> {
    vec![
        int_column("w", IntegerWidth::I32),
        int_column("d", IntegerWidth::I8),
        int_column("o", IntegerWidth::I64),
        int_column("qty", IntegerWidth::I32),
    ]
}

/// A table keyed by `(w, d, o)` with the non-key column `qty`.
pub fn lines_table(name: &str) -> Table {
    Table {
        name: table(name),
        columns: lines_columns(),
        key: key_of(&["w", "d", "o"]),
    }
}

pub fn world_schema() -> Schema {
    let bucket = Column {
        name: column("bucket"),
        kind: ColumnType::Integer {
            width: IntegerWidth::I32,
            domain: Some(ClampDomain { min: 0, max: 10 }),
        },
    };
    Schema {
        tables: vec![
            lines_table("lines"),
            lines_table("seen"),
            Table {
                name: table("labels"),
                columns: vec![text_column("code"), text_column("note")],
                key: key_of(&["code"]),
            },
            Table {
                name: table("buckets"),
                columns: vec![bucket, int_column("size", IntegerWidth::I32)],
                key: key_of(&["bucket"]),
            },
            Table {
                name: table("events"),
                columns: vec![text_column("what")],
                key: KeyDeclaration::Keyless,
            },
        ],
        indexes: vec![
            Index {
                name: IndexName("lines_by_qty".to_owned()),
                table: table("lines"),
                columns: vec![column("qty")],
                unique: false,
            },
            Index {
                name: IndexName("lines_by_d_qty".to_owned()),
                table: table("lines"),
                columns: vec![column("d"), column("qty")],
                unique: false,
            },
        ],
        ..Default::default()
    }
}

pub fn line_key(w: i64, d: i64, o: i64) -> Key {
    Key(vec![integer(w), integer(d), integer(o)])
}

pub fn line_key_expr(w: i64, d: i64, o: i64) -> Vec<Expr> {
    key_expr(&line_key(w, d, o))
}

pub fn qty_row(qty: i64) -> Row {
    Row {
        columns: columns_of_values([("qty", integer(qty))]),
    }
}

pub fn lines_store(lines: &[(i64, i64, i64, i64)]) -> Store {
    Store::containing(
        lines
            .iter()
            .map(|&(w, d, o, qty)| (table("lines"), line_key(w, d, o), qty_row(qty))),
    )
}

pub fn literal(value: i64) -> Expr {
    Expr::Literal(integer(value))
}

pub fn returning_zero() -> Terminal {
    Terminal::Return(literal(0))
}

pub fn world_program(effects: Vec<Effect>, terminal: Terminal) -> Program {
    program_with_schema(world_schema(), vec![], effects, terminal)
}

pub fn run_world(effects: Vec<Effect>, store: &Store) -> Execution {
    run_ok(
        &world_program(effects, returning_zero()),
        &call_under_test(vec![]),
        store,
    )
}

pub fn rejection_of(program: &Program) -> ProgramError {
    kiln_ir::run(program, &call_under_test(vec![]), &Store::empty())
        .expect_err("the program is rejected before the call runs")
}

pub fn function_fault(fault: FunctionFault) -> ProgramError {
    ProgramError::Function {
        function: kiln_ir::FunctionName("under_test".to_owned()),
        fault,
    }
}

pub fn read_line(w: i64, d: i64, o: i64, into: Slot) -> Effect {
    Effect::Read {
        table: table("lines"),
        key: line_key_expr(w, d, o),
        into,
    }
}

pub fn insert_line(w: i64, d: i64, o: i64, qty: i64) -> Effect {
    Effect::Insert {
        table: table("lines"),
        key: line_key_expr(w, d, o),
        columns: columns_of([("qty", literal(qty))]),
    }
}

pub fn update_line_qty(w: i64, d: i64, o: i64, qty: i64) -> Effect {
    Effect::Update {
        table: table("lines"),
        key: line_key_expr(w, d, o),
        columns: columns_of([("qty", literal(qty))]),
    }
}

pub fn delete_line(w: i64, d: i64, o: i64) -> Effect {
    Effect::Delete {
        table: table("lines"),
        key: line_key_expr(w, d, o),
    }
}

pub fn read_lines_range(start: (i64, i64, i64), end: (i64, i64, i64), into: Slot) -> Effect {
    Effect::ReadRange {
        table: table("lines"),
        start: line_key_expr(start.0, start.1, start.2),
        end: line_key_expr(end.0, end.1, end.2),
        bound: literal(100),
        into,
    }
}

pub fn project(slot: Slot, name: &str) -> Expr {
    Expr::Projection {
        slot,
        column: column(name),
    }
}

/// Copies each row `source` bound into table "seen" under the key built from
/// the row's own `w`, `d` and `o`.
pub fn copy_keys_into_seen(source: Slot) -> Effect {
    let row = Slot(9);
    Effect::Iterate {
        source,
        row,
        body: vec![Effect::Insert {
            table: table("seen"),
            key: ["w", "d", "o"]
                .iter()
                .map(|name| project(row, name))
                .collect(),
            columns: Default::default(),
        }],
    }
}

pub fn keys_in_seen(execution: &Execution) -> Vec<Key> {
    execution
        .access_log
        .iter()
        .filter(|entry| entry.table == table("seen") && entry.kind == AccessKind::Insert)
        .map(|entry| match &entry.target {
            AccessTarget::Point(key) => key.clone(),
            other => panic!("expected a point target, got {other:?}"),
        })
        .collect()
}
