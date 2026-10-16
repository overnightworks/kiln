//! Shared builders for `tests/evaluator.rs`, `tests/arithmetic.rs`, and
//! `tests/arithmetic_boundaries.rs`, so each test states its behaviour and
//! not its `Program`/`Call` plumbing.
//!
//! Not a test target itself: cargo only turns a file directly under
//! `tests/` into its own binary, so this module is free to be imported by
//! all three without running as a fourth, empty test suite.
//!
//! Each binary compiles this module on its own and only some of its
//! helpers are called from any one binary, so a helper unused by one is
//! still live via another — `dead_code` is allowed module-wide rather than
//! disabled per unused helper.
#![allow(dead_code)]

pub mod world;

use std::collections::{BTreeMap, BTreeSet};

use kiln_ir::{
    replay, run, Call, Column, ColumnName, ColumnType, Effect, Execution, Expr, Function,
    FunctionName, IntegerWidth, Key, KeyDeclaration, KeyInterval, Parameter, ParameterName,
    ParameterType, Program, Row, Schema, Store, Table, TableName, Terminal, Value,
};
use proptest::prelude::*;

pub fn table(name: &str) -> TableName {
    TableName(name.to_owned())
}

pub fn column(name: &str) -> ColumnName {
    ColumnName(name.to_owned())
}

/// The key column `id` that [`keyed_by_id`] gives every table.
pub fn id_column() -> Column {
    Column {
        name: column("id"),
        kind: ColumnType::Integer {
            width: IntegerWidth::I64,
            domain: None,
        },
    }
}

/// A table keyed by an integer column `id` alongside `columns`.
pub fn keyed_by_id(name: &str, columns: Vec<Column>) -> Table {
    Table {
        name: table(name),
        columns: std::iter::once(id_column()).chain(columns).collect(),
        key: KeyDeclaration::Columns(vec![column("id")]),
    }
}

/// The one-part key holding `component`.
pub fn one_part(component: i64) -> Key {
    Key(vec![Value::Integer(component)])
}

/// A key expression of one literal component.
pub fn literal_key(component: i64) -> Vec<Expr> {
    vec![Expr::Literal(integer(component))]
}

/// An inclusive interval between two literals.
pub fn literal_interval(from: i64, to: i64) -> Option<KeyInterval> {
    Some(KeyInterval {
        from: Expr::Literal(integer(from)),
        to: Expr::Literal(integer(to)),
    })
}

/// A key expression whose components are the literals of `key`.
pub fn key_expr(key: &Key) -> Vec<Expr> {
    key.0.iter().cloned().map(Expr::Literal).collect()
}

pub fn arbitrary_key() -> impl Strategy<Value = Key> {
    any::<i64>().prop_map(one_part)
}

pub fn parameter(name: &str) -> ParameterName {
    ParameterName(name.to_owned())
}

/// A declared parameter named `name` whose argument is checked against
/// `width`, exactly as a write is checked against its target column's
/// declared width (D3).
pub fn typed_parameter(name: &str, width: IntegerWidth) -> Parameter {
    Parameter {
        name: parameter(name),
        kind: ParameterType::Integer(width),
    }
}

/// A declared parameter named `name` whose argument must be text.
pub fn text_parameter(name: &str) -> Parameter {
    Parameter {
        name: parameter(name),
        kind: ParameterType::Text,
    }
}

pub fn columns_of(
    pairs: impl IntoIterator<Item = (&'static str, Expr)>,
) -> BTreeMap<ColumnName, Expr> {
    pairs
        .into_iter()
        .map(|(name, expr)| (column(name), expr))
        .collect()
}

pub fn columns_of_values(
    pairs: impl IntoIterator<Item = (&'static str, Value)>,
) -> BTreeMap<ColumnName, Value> {
    pairs
        .into_iter()
        .map(|(name, value)| (column(name), value))
        .collect()
}

/// A program with one function, `under_test`, over a schema declaring every
/// table its body names as keyed by an integer `id` and nothing else: the
/// columns a test writes stay undeclared.
pub fn single_function_program(
    parameters: Vec<Parameter>,
    effects: Vec<Effect>,
    terminal: Terminal,
) -> Program {
    let mut names = BTreeSet::new();
    collect_table_names(&effects, &terminal, &mut names);
    Program {
        schema: Schema {
            tables: names.iter().map(|name| keyed_by_id(name, vec![])).collect(),
            ..Default::default()
        },
        functions: vec![Function {
            name: FunctionName("under_test".to_owned()),
            parameters,
            body: kiln_ir::Block { effects, terminal },
        }],
    }
}

fn collect_table_names(effects: &[Effect], terminal: &Terminal, names: &mut BTreeSet<String>) {
    for effect in effects {
        match effect {
            Effect::Read { table, .. }
            | Effect::Insert { table, .. }
            | Effect::Update { table, .. }
            | Effect::Delete { table, .. }
            | Effect::ReadRange { table, .. } => {
                names.insert(table.0.clone());
            }
            Effect::Iterate { body, .. } => collect_table_names(body, terminal, names),
            Effect::ReadIndexRange { .. } => {}
        }
    }
    if let Terminal::Branch {
        when_present,
        when_absent,
        ..
    } = terminal
    {
        for arm in [when_present, when_absent] {
            collect_table_names(&arm.effects, &arm.terminal, names);
        }
    }
}

pub fn call_under_test(args: Vec<Value>) -> Call {
    Call {
        function: FunctionName("under_test".to_owned()),
        args,
    }
}

/// A program over `schema` with one function, `under_test`.
pub fn program_with_schema(
    schema: Schema,
    parameters: Vec<Parameter>,
    effects: Vec<Effect>,
    terminal: Terminal,
) -> Program {
    Program {
        schema,
        functions: vec![Function {
            name: FunctionName("under_test".to_owned()),
            parameters,
            body: kiln_ir::Block { effects, terminal },
        }],
    }
}

/// A program whose schema declares table "t" keyed by `id` with exactly one
/// other column, `declared_column`, plus the given function body — the shape
/// every D3 test writes into.
pub fn program_with_column(
    declared_column: Column,
    parameters: Vec<Parameter>,
    effects: Vec<Effect>,
    terminal: Terminal,
) -> Program {
    let schema = Schema {
        tables: vec![keyed_by_id("t", vec![declared_column])],
        ..Default::default()
    };
    program_with_schema(schema, parameters, effects, terminal)
}

pub fn arbitrary_value() -> impl Strategy<Value = Value> {
    prop_oneof![
        any::<i64>().prop_map(Value::Integer),
        "[a-z]{1,6}".prop_map(Value::Text),
    ]
}

/// Runs `call` against `program` and `store`, asserting the schema itself
/// is valid — every test in this crate builds a schema it expects `run` to
/// accept; a dedicated D3 schema-validation test calls `Schema::validate`
/// or `run` directly instead of this helper to see the typed rejection.
pub fn run_ok(program: &Program, call: &Call, store: &Store) -> Execution {
    run(program, call, store).expect("test schema is valid")
}

pub fn committed_response(execution: &Execution) -> &Value {
    match &execution.outcome {
        kiln_ir::Outcome::Committed { response } => response,
        kiln_ir::Outcome::Aborted { reason } => {
            panic!("expected a commit, got an abort: {reason:?}")
        }
    }
}

/// Which write effect `run_write` makes: an `Insert` of a fresh key, or an
/// `Update` of a key the store already carries that same column on — so a
/// D3 case can prove itself through both write kinds (`tests/arithmetic.rs`,
/// `tests/arithmetic_boundaries.rs`).
#[derive(Debug, Clone, Copy)]
pub enum WriteVia {
    Insert,
    Update,
}

/// Runs a single write of `write_expr` into the column named "c", declared
/// as `declared_column`, via `via` — the one shape every D3 arithmetic,
/// width, and clamp test builds on.
pub fn run_write(declared_column: Column, write_expr: Expr, via: WriteVia) -> Execution {
    assert_eq!(
        declared_column.name,
        column("c"),
        "run_write always writes into the column named \"c\""
    );
    let store = match via {
        WriteVia::Insert => Store::empty(),
        WriteVia::Update => Store::containing([(
            table("t"),
            one_part(1),
            Row {
                columns: columns_of_values([("c", Value::Integer(0))]),
            },
        )]),
    };
    let effect = match via {
        WriteVia::Insert => Effect::Insert {
            table: table("t"),
            key: literal_key(1),
            columns: columns_of([("c", write_expr)]),
        },
        WriteVia::Update => Effect::Update {
            table: table("t"),
            key: literal_key(1),
            columns: columns_of([("c", write_expr)]),
        },
    };
    let program = program_with_column(
        declared_column,
        vec![],
        vec![effect],
        Terminal::Return(Expr::Literal(Value::Integer(0))),
    );

    run_ok(&program, &call_under_test(vec![]), &store)
}

pub fn integer(value: i64) -> Value {
    Value::Integer(value)
}

pub fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

pub fn insert_row(
    table_name: &str,
    key: i64,
    values: impl IntoIterator<Item = (&'static str, Value)>,
) -> Effect {
    Effect::Insert {
        table: table(table_name),
        key: literal_key(key),
        columns: literal_columns(values),
    }
}

pub fn update_row(
    table_name: &str,
    key: i64,
    values: impl IntoIterator<Item = (&'static str, Value)>,
) -> Effect {
    Effect::Update {
        table: table(table_name),
        key: literal_key(key),
        columns: literal_columns(values),
    }
}

pub fn delete_row(table_name: &str, key: i64) -> Effect {
    Effect::Delete {
        table: table(table_name),
        key: literal_key(key),
    }
}

fn literal_columns(
    values: impl IntoIterator<Item = (&'static str, Value)>,
) -> BTreeMap<ColumnName, Expr> {
    values
        .into_iter()
        .map(|(name, value)| (column(name), Expr::Literal(value)))
        .collect()
}

/// One row to put in a test store: its table, key and column values.
pub type RowSpec = (&'static str, i64, Vec<(&'static str, Value)>);

pub fn store_of_rows(rows: &[RowSpec]) -> Store {
    Store::containing(rows.iter().map(|(table_name, key, values)| {
        (
            table(table_name),
            one_part(*key),
            Row {
                columns: columns_of_values(values.clone()),
            },
        )
    }))
}

pub fn aborted_reason(execution: &Execution) -> &kiln_ir::AbortReason {
    match &execution.outcome {
        kiln_ir::Outcome::Aborted { reason } => reason,
        kiln_ir::Outcome::Committed { response } => {
            panic!("expected an abort, got a commit with {response:?}")
        }
    }
}

pub fn constraint_entries(execution: &Execution) -> Vec<kiln_ir::AccessLogEntry> {
    execution
        .access_log
        .iter()
        .filter(|entry| entry.origin == kiln_ir::AccessOrigin::ConstraintCheck)
        .cloned()
        .collect()
}

/// Replays `calls` against `program` and `store`, asserting the schema is
/// valid so a test sees executions and not the typed rejection.
pub fn replayed(program: &Program, calls: &[Call], store: &Store) -> Vec<Execution> {
    replay(program, calls, store).expect("test schema is valid")
}
