//! The core evaluator: runs one [`Call`] against a [`Program`] and a
//! [`Store`], producing the store that results, the call's [`Outcome`], and
//! the [`AccessLogEntry`] list the reference meaning requires (D5).
//!
//! `run` is pure: it never mutates the `Store` it is given. It evaluates
//! against a private working copy and, on commit, returns that copy;
//! on abort, it returns a fresh clone of the original instead, discarding
//! every effect the aborted call produced (`docs/concept.md` §2.2).
//!
//! Split by responsibility: this module holds the outcome and access-log
//! types, the call-wide context, and `run` itself; [`resolve`] owns
//! expression resolution (constants, parameters, projections, checked
//! arithmetic); [`effects`] owns applying a function body's effects and
//! terminal against the store.

use std::collections::HashMap;

use crate::call::Call;
use crate::identifiers::{AbortName, ColumnName, ParameterName, TableName};
use crate::ir::{
    CheckConstraint, ForeignKey, Parameter, Program, Row, SchemaError, Slot, UniqueConstraint,
    Value,
};
use crate::store::Store;

mod check;
mod constraints;
mod effects;
mod foreign_key;
mod resolve;
mod unique;
mod written;

/// What a committed or aborted call produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Committed { response: Value },
    Aborted { reason: AbortReason },
}

/// Why a call aborted.
///
/// `MissingRow`, `DuplicateKey`, `Overflow` and `BoundExceeded` are the
/// evaluator's own structural rules for update/delete, insert, checked
/// arithmetic and bounded range iteration (`docs/concept.md` §2.2, D3, D6);
/// `Declared` is a transaction body's own explicit, named rejection. The
/// remaining variants are every other way a compiled program's own
/// expressions can fail to resolve against the data a call actually ran
/// against — never a panic, since whether an argument, a range bound, or a
/// projection resolves is a fact about that data, not something `run` can
/// rule out ahead of time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbortReason {
    Declared(AbortName),
    MissingRow {
        table: TableName,
        key: Value,
    },
    DuplicateKey {
        table: TableName,
        key: Value,
    },
    Overflow(OverflowReason),
    /// A range read on `table` matched `matched` rows, more than its
    /// declared `bound` — bounded iteration never runs open-ended (§2.1,
    /// §2.2 D6).
    BoundExceeded {
        table: TableName,
        bound: i64,
        matched: usize,
    },
    /// A range read on `table` resolved its `bound` expression to a value
    /// that was not an integer — a bound counts rows, so only an integer
    /// can satisfy it (§2.1, "never an open-ended loop").
    NonIntegerBound {
        table: TableName,
    },
    /// An arithmetic operand, or a write's own final value, resolved to a
    /// value that was not an integer — checked arithmetic and a column's
    /// width-or-clamp decision both run only over integers (D3), so a
    /// `Text` reaching either is a fact about the data this call ran
    /// against, never a panic.
    NonIntegerOperand,
    /// The argument bound to `parameter` is not the kind its declared
    /// [`ParameterType`](crate::ir::ParameterType) requires (an integer
    /// where text was declared, or text where an integer was declared).
    ArgumentTypeMismatch {
        parameter: ParameterName,
    },
    /// The call supplied no argument for `parameter`, whether or not the
    /// body references it.
    MissingArgument {
        parameter: ParameterName,
    },
    /// The call supplied more arguments than the function declares
    /// parameters; `position` is the first surplus argument.
    UnexpectedArgument {
        position: usize,
    },
    /// A projection has no single current row to read from: the slot it
    /// names was bound by a point read that found no row, or by a range
    /// read, whose slot never holds one current row on its own (a range's
    /// current row is only available inside an [`Effect::Iterate`](crate::ir::Effect::Iterate) body).
    ProjectionSourceAbsent {
        slot: Slot,
    },
    /// A projection named `column`, but the row bound to `slot` carries no
    /// column by that name.
    MissingColumn {
        slot: Slot,
        column: ColumnName,
    },
    /// A write gave an integer to `column` of `table`, which the schema
    /// declares as text.
    NonTextValue {
        table: TableName,
        column: ColumnName,
        key: Value,
    },
    /// The transaction's final state holds `value` in `constraint`'s column
    /// on more than one row (D2).
    UniqueViolation {
        constraint: UniqueConstraint,
        value: Value,
    },
    /// The transaction's final state holds a row whose `foreign_key` column
    /// names `value`, a key no row of the referenced table holds (D2).
    DanglingReference {
        foreign_key: ForeignKey,
        value: Value,
    },
    /// The transaction deleted the row at `key` of the table `foreign_key`
    /// references, while its final state still holds a row naming `key` (D2).
    ReferencedParentDeleted {
        foreign_key: ForeignKey,
        key: Value,
    },
    /// The transaction's final state holds the row at `key` violating
    /// `constraint` (D2).
    CheckViolation {
        constraint: CheckConstraint,
        key: Value,
    },
}

/// Why checked arithmetic overflowed, or a value fell outside its
/// declared width (D3): `i64` itself overflowed before any width-or-clamp
/// decision ever saw the result, a write's final value fell outside its
/// target column's declared width with no clamping domain to saturate
/// into instead, or an argument fell outside its parameter's declared
/// width — a column clamps, but a parameter, declaring no domain, never
/// does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowReason {
    /// Arithmetic overflowed `i64` itself — in a response, a key expression,
    /// or a nested sub-expression — before reaching any column write's
    /// width-or-clamp decision.
    Arithmetic,
    /// A write's final value fell outside `column`'s declared width, and
    /// `column` declares no clamping domain to saturate into instead.
    Width {
        table: TableName,
        column: ColumnName,
        key: Value,
    },
    /// The argument bound to `parameter` fell outside its declared width.
    Argument { parameter: ParameterName },
}

/// Which columns an access touched: every column of the row, or exactly the
/// named ones a write gave values for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessedColumns {
    EntireRow,
    Named(Vec<ColumnName>),
}

/// What kind of access an [`AccessLogEntry`] records (D5 names a point
/// read and a range read as one family of access kinds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessKind {
    PointRead,
    RangeRead,
    Insert,
    Update,
    Delete,
}

/// Which rows an [`AccessLogEntry`] names: one row by key, or every row
/// whose key falls in `[start, end]` — recorded even when the range
/// matches no row, since the range itself was still attempted
/// (`docs/concept.md` §2.2, D5: "range bounds (including empty ranges)").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessTarget {
    Point(Value),
    Range { start: Value, end: Value },
}

/// Who made an access: the transaction's own effects, or the end-of-
/// transaction check of a schema constraint (D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessOrigin {
    TransactionLogic,
    ConstraintCheck,
}

/// One attempted access, recorded whether or not the call that made it
/// went on to commit (D5: "attempted accesses are kept on abort").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessLogEntry {
    pub table: TableName,
    pub kind: AccessKind,
    pub target: AccessTarget,
    pub columns: AccessedColumns,
    pub origin: AccessOrigin,
}

/// The result of running one [`Call`]: the store it produced, its outcome,
/// and the accesses it attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub store: Store,
    pub outcome: Outcome,
    pub access_log: Vec<AccessLogEntry>,
}

/// The read-only context every step of one [`run`] resolves expressions
/// against: the schema a write checks its target column against, and the
/// call's own declared parameters and argument values (D1, D4). Bundled so
/// a function below takes one value instead of carrying all three
/// separately through every call in the recursion.
struct CallContext<'a> {
    schema: &'a crate::ir::Schema,
    parameters: &'a [Parameter],
    args: &'a [Value],
}

/// What a [`Slot`] holds once the effect that bound it has run: a single
/// row or its absence (`Effect::Read`), or the ordered sequence of rows a
/// range matched (`Effect::ReadRange`). [`Condition::RowPresent`](crate::ir::Condition::RowPresent) reads
/// either kind as one branchable fact — "did this read find anything" —
/// without the rest of `run` needing to know which effect produced it.
enum Binding {
    Row(Option<Row>),
    Range(Vec<Row>),
}

impl Binding {
    fn is_present(&self) -> bool {
        match self {
            Binding::Row(row) => row.is_some(),
            Binding::Range(rows) => !rows.is_empty(),
        }
    }
}

/// Runs `call` against `program` starting from `store`. Does not mutate
/// `store`.
///
/// Rejects `program`'s schema with a typed [`SchemaError`] before executing
/// anything if that schema is not internally satisfiable (D3,
/// [`Schema::validate`](crate::ir::Schema::validate)).
///
/// Panics, as out of scope for this boundary, on a static program fault:
/// a `call` naming a function `program` does not declare, a body referring to
/// a parameter its function did not declare, or an `Iterate` whose source
/// slot is unbound or was bound by a point read — `Program` and `Call` are
/// the compiler's trusted output, not raw input this boundary validates.
/// Whether a slot holds a row is data, never a fault: an unbound slot reads
/// as absent.
pub fn run(program: &Program, call: &Call, store: &Store) -> Result<Execution, SchemaError> {
    program.schema.validate()?;

    let function = program.function(&call.function);
    let ctx = CallContext {
        schema: &program.schema,
        parameters: &function.parameters,
        args: &call.args,
    };

    if let Some(reason) = check_arguments(function, &call.args) {
        return Ok(Execution {
            store: store.clone(),
            outcome: Outcome::Aborted { reason },
            access_log: Vec::new(),
        });
    }

    let mut working_store = store.clone();
    let mut bindings: HashMap<Slot, Binding> = HashMap::new();
    let mut access_log = Vec::new();

    let outcome = effects::execute_block(
        &function.body,
        &ctx,
        &mut working_store,
        &mut bindings,
        &mut access_log,
    );

    let outcome = match outcome {
        Outcome::Committed { .. } => {
            constraints::first_violation(&program.schema, &working_store, &mut access_log)
                .map_or(outcome, |reason| Outcome::Aborted { reason })
        }
        aborted @ Outcome::Aborted { .. } => aborted,
    };

    let final_store = match &outcome {
        Outcome::Committed { .. } => working_store,
        Outcome::Aborted { .. } => store.clone(),
    };

    Ok(Execution {
        store: final_store,
        outcome,
        access_log,
    })
}

/// Checks the arguments in `args` against the declared type of the
/// parameter at the same position, exactly as a write checks its value
/// against its target column's declared width (D3, `docs/concept.md` §2.2:
/// "the column's or argument's declared width"). Returns the typed abort
/// reason for the first argument that does not match, or `None` once every
/// argument has been checked.
///
/// A parameter declares no clamping domain, so an out-of-width or
/// wrong-kind argument always aborts — unlike a column, which may clamp an
/// out-of-width write instead.
///
/// The argument count must equal the parameter count, referenced or not:
/// a missing argument aborts with [`AbortReason::MissingArgument`], an extra
/// one with [`AbortReason::UnexpectedArgument`].
fn check_arguments(function: &crate::ir::Function, args: &[Value]) -> Option<AbortReason> {
    use crate::ir::ParameterType;

    if let Some(missing) = function.parameters.get(args.len()) {
        return Some(AbortReason::MissingArgument {
            parameter: missing.name.clone(),
        });
    }
    if args.len() > function.parameters.len() {
        return Some(AbortReason::UnexpectedArgument {
            position: function.parameters.len(),
        });
    }

    for (parameter, arg) in function.parameters.iter().zip(args) {
        match (parameter.kind, arg) {
            (ParameterType::Integer(width), Value::Integer(value)) => {
                if resolve::check_width(*value, width).is_err() {
                    return Some(AbortReason::Overflow(OverflowReason::Argument {
                        parameter: parameter.name.clone(),
                    }));
                }
            }
            (ParameterType::Text, Value::Text(_)) => {}
            (ParameterType::Integer(_), Value::Text(_))
            | (ParameterType::Text, Value::Integer(_)) => {
                return Some(AbortReason::ArgumentTypeMismatch {
                    parameter: parameter.name.clone(),
                });
            }
        }
    }
    None
}
