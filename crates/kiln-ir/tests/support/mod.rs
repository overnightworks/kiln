//! Shared builders for `tests/evaluator.rs` and `tests/arithmetic.rs`, so
//! each test states its behaviour and not its `Program`/`Call` plumbing.
//!
//! Not a test target itself: cargo only turns a file directly under
//! `tests/` into its own binary, so this module is free to be imported by
//! both without running as a third, empty test suite.
//!
//! Each of the two binaries compiles this module on its own and only some
//! of its helpers are called from any one binary, so a helper unused by one
//! binary is still live via the other — `dead_code` is allowed module-wide
//! rather than disabled per unused helper.
#![allow(dead_code)]

use std::collections::BTreeMap;

use kiln_ir::{
    run, Call, Column, ColumnName, Effect, Execution, Expr, Function, FunctionName, ParameterName,
    Program, Schema, Store, Table, TableName, Terminal, Value,
};
use proptest::prelude::*;

pub fn table(name: &str) -> TableName {
    TableName(name.to_owned())
}

pub fn column(name: &str) -> ColumnName {
    ColumnName(name.to_owned())
}

pub fn parameter(name: &str) -> ParameterName {
    ParameterName(name.to_owned())
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

pub fn single_function_program(
    parameters: Vec<ParameterName>,
    effects: Vec<Effect>,
    terminal: Terminal,
) -> Program {
    Program {
        schema: Default::default(),
        functions: vec![Function {
            name: FunctionName("under_test".to_owned()),
            parameters,
            body: kiln_ir::Block { effects, terminal },
        }],
    }
}

pub fn call_under_test(args: Vec<Value>) -> Call {
    Call {
        function: FunctionName("under_test".to_owned()),
        args,
    }
}

/// A program whose schema declares table "t" with exactly one column,
/// `declared_column`, plus the given function body — the shape every D3
/// test writes into.
pub fn program_with_column(
    declared_column: Column,
    parameters: Vec<ParameterName>,
    effects: Vec<Effect>,
    terminal: Terminal,
) -> Program {
    Program {
        schema: Schema {
            tables: vec![Table {
                name: table("t"),
                columns: vec![declared_column],
            }],
        },
        functions: vec![Function {
            name: FunctionName("under_test".to_owned()),
            parameters,
            body: kiln_ir::Block { effects, terminal },
        }],
    }
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
