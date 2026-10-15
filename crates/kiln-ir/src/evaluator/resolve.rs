//! Expression resolution: turning an [`Expr`] into a concrete [`Value`]
//! against one call's [`CallContext`] and the [`Binding`]s its reads have
//! made so far, including D3's checked arithmetic and a write's own
//! width-or-clamp decision.

use std::collections::HashMap;

use crate::identifiers::ColumnName;
use crate::ir::{ArithmeticOp, ClampDomain, Column, ColumnType, Expr, IntegerWidth, Slot, Value};

use super::{AbortReason, Binding, CallContext, OverflowReason};

/// Why [`resolve`] could not produce a value for an expression: `i64`
/// arithmetic overflowed before any column or argument was reached (D3), an
/// operand resolved to a value of the wrong kind for the context that
/// needed it, or a projection expression could not find the
/// value it names in the data this call actually ran against. Every variant
/// maps to a typed [`AbortReason`] through
/// [`resolve_or_abort`](super::effects::resolve_or_abort) or
/// [`resolve_for_write`]'s own caller — never a panic, since each is a fact
/// about that data, not a malformed expression.
pub(super) enum ResolveError {
    Overflow(OverflowReason),
    NonIntegerOperand,
    ProjectionSourceAbsent { slot: Slot },
    MissingColumn { slot: Slot, column: ColumnName },
}

impl From<ResolveError> for AbortReason {
    fn from(error: ResolveError) -> Self {
        match error {
            ResolveError::Overflow(reason) => AbortReason::Overflow(reason),
            ResolveError::NonIntegerOperand => AbortReason::NonIntegerOperand,
            ResolveError::ProjectionSourceAbsent { slot } => {
                AbortReason::ProjectionSourceAbsent { slot }
            }
            ResolveError::MissingColumn { slot, column } => {
                AbortReason::MissingColumn { slot, column }
            }
        }
    }
}

/// Resolves an expression to a concrete value: a constant, the argument
/// bound to a declared parameter (its arity was checked before the call ran),
/// a column or the primary key projected from an earlier read's row, or an arithmetic operation
/// over two such values.
///
/// Arithmetic here is checked at `i64`'s own width and returns
/// [`ResolveError::Overflow`] if it overflows — true outside any column
/// write (a response, a key expression) and inside a nested sub-expression
/// alike, so every intermediate step of a write's own expression is checked
/// the same way before [`resolve_for_write`] ever judges the expression's
/// one final value against its target column (D3).
///
/// A [`Slot`] no read has bound (or that an iteration released) resolves as
/// an absent projection source, since whether a read ran is a fact about the
/// data. Panics if `expr` refers to a parameter `ctx.parameters` does not
/// declare — a contract violation by whatever built the expression, not a
/// semantic outcome this evaluator reports.
pub(super) fn resolve(
    expr: &Expr,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
) -> Result<Value, ResolveError> {
    match expr {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::Parameter(name) => {
            let index = ctx
                .parameters
                .iter()
                .position(|parameter| &parameter.name == name)
                .unwrap_or_else(|| panic!("function has no declared parameter named {name:?}"));
            Ok(ctx.args[index].clone())
        }
        Expr::Projection { slot, column } => {
            let Some(Binding::Row(Some(row))) = bindings.get(slot) else {
                return Err(ResolveError::ProjectionSourceAbsent { slot: *slot });
            };
            row.row
                .columns
                .get(column)
                .cloned()
                .ok_or_else(|| ResolveError::MissingColumn {
                    slot: *slot,
                    column: column.clone(),
                })
        }
        Expr::PrimaryKey { slot } => {
            let Some(Binding::Row(Some(row))) = bindings.get(slot) else {
                return Err(ResolveError::ProjectionSourceAbsent { slot: *slot });
            };
            Ok(row.key.clone())
        }
        Expr::Arithmetic(op, lhs, rhs) => {
            let lhs = as_integer(resolve(lhs, ctx, bindings)?)?;
            let rhs = as_integer(resolve(rhs, ctx, bindings)?)?;
            let result = checked_apply(*op, lhs, rhs)
                .ok_or(ResolveError::Overflow(OverflowReason::Arithmetic))?;
            Ok(Value::Integer(result))
        }
    }
}

/// Why [`resolve_for_write`] could not produce a value for its target
/// column: [`resolve`] itself failed before the column was ever reached, or
/// the expression's final value fell outside the column's declared width
/// with no clamping domain to saturate into instead (D3), or an integer was
/// written to a text column.
///
/// [`apply_effect`](super::effects) is the only caller, and it already
/// knows which table, column and key the write targeted — this type carries
/// just enough to tell its D3 width outcome apart from every other
/// resolution failure, leaving that context to the caller that already has
/// it.
pub(super) enum WriteOverflow {
    Resolve(ResolveError),
    Width,
    NonText,
}

/// Resolves `expr` for a write into a column declared as `declared_column`,
/// applying D3's width-or-clamp decision to the expression's one final
/// value. Nested arithmetic inside `expr` was already checked at `i64` by
/// `resolve`; this only ever judges the result `resolve` already produced,
/// never an intermediate step of a computed write.
///
/// An undeclared column (`declared_column` is `None`) carries no width or
/// domain, so its write resolves exactly as [`resolve`] would on its own —
/// the behavior this evaluator has always had outside this slice's reach.
pub(super) fn resolve_for_write(
    expr: &Expr,
    ctx: &CallContext,
    bindings: &HashMap<Slot, Binding>,
    declared_column: Option<&Column>,
) -> Result<Value, WriteOverflow> {
    let value = resolve(expr, ctx, bindings).map_err(WriteOverflow::Resolve)?;
    let Some(column) = declared_column else {
        return Ok(value);
    };

    match (column.kind, value) {
        (ColumnType::Text, text @ Value::Text(_)) => Ok(text),
        (ColumnType::Text, Value::Integer(_)) => Err(WriteOverflow::NonText),
        (ColumnType::Integer { width, domain }, value) => {
            let raw = as_integer(value).map_err(WriteOverflow::Resolve)?;
            let raw = match domain {
                Some(domain) => clamp_to_domain(raw, domain),
                None => check_width(raw, width).map_err(|_| WriteOverflow::Width)?,
            };
            Ok(Value::Integer(raw))
        }
    }
}

/// Extracts the integer an arithmetic operand, or a write's own final
/// value, must resolve to.
///
/// Returns [`ResolveError::NonIntegerOperand`] on a text value: D3's
/// arithmetic and a write's width-or-clamp decision both run only over
/// checked integers, so a non-integer operand is a fact about the data this
/// call ran against, not a contract violation — never a panic.
fn as_integer(value: Value) -> Result<i64, ResolveError> {
    match value {
        Value::Integer(value) => Ok(value),
        Value::Text(_) => Err(ResolveError::NonIntegerOperand),
    }
}

/// `lhs op rhs`, checked at `i64`'s own width: `None` on true `i64` overflow.
fn checked_apply(op: ArithmeticOp, lhs: i64, rhs: i64) -> Option<i64> {
    match op {
        ArithmeticOp::Add => lhs.checked_add(rhs),
        ArithmeticOp::Subtract => lhs.checked_sub(rhs),
    }
}

/// `value` if it lies within `width`'s representable range, `Err(())`
/// otherwise (D3).
pub(super) fn check_width(value: i64, width: IntegerWidth) -> Result<i64, ()> {
    let (min, max) = width.range();
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(())
    }
}

/// Saturates `value` into `domain`'s inclusive bound (D3): a value already
/// inside the domain is left unchanged.
fn clamp_to_domain(value: i64, domain: ClampDomain) -> i64 {
    value.clamp(domain.min, domain.max)
}
