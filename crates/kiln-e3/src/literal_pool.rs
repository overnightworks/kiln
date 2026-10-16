use std::collections::BTreeSet;

use kiln_ir::{
    Block, ColumnType, Condition, Effect, Expr, KeyDeclaration, ParameterType, Program, Terminal,
    Value,
};

/// The constant pool `P` of grammar §1.5: the body literals, the check
/// operands, the width bounds of non-key value columns and of parameters, and
/// `0` and `1`. Key columns are left out: their `I64` width is a
/// representation convention, not part of the family.
pub(crate) fn of(program: &Program) -> BTreeSet<i64> {
    let mut scan = Scan::default();
    scan.literals.extend([0, 1]);
    scan.value_column_bounds(program);
    for function in &program.functions {
        for parameter in &function.parameters {
            if let ParameterType::Integer(width) = parameter.kind {
                scan.literals.extend(bounds(width.range()));
            }
        }
        scan.block(&function.body);
    }
    for constraint in &program.schema.check_constraints {
        scan.value(&constraint.operand);
    }
    scan.literals
}

fn bounds((low, high): (i64, i64)) -> [i64; 2] {
    [low, high]
}

#[derive(Default)]
struct Scan {
    literals: BTreeSet<i64>,
}

impl Scan {
    fn value(&mut self, value: &Value) {
        if let Value::Integer(number) = value {
            self.literals.insert(*number);
        }
    }

    fn value_column_bounds(&mut self, program: &Program) {
        for table in &program.schema.tables {
            let key_columns = match &table.key {
                KeyDeclaration::Columns(columns) => columns.as_slice(),
                KeyDeclaration::Keyless => &[],
            };
            for column in &table.columns {
                if key_columns.contains(&column.name) {
                    continue;
                }
                if let ColumnType::Integer { width, domain } = &column.kind {
                    self.literals.extend(bounds(width.range()));
                    if let Some(domain) = domain {
                        self.literals.extend([domain.min, domain.max]);
                    }
                }
            }
        }
    }

    fn block(&mut self, block: &Block) {
        for effect in &block.effects {
            self.effect(effect);
        }
        match &block.terminal {
            Terminal::Return(expr) => self.expr(expr),
            Terminal::Abort(_) => {}
            Terminal::Branch {
                condition,
                when_present,
                when_absent,
            } => {
                self.condition(condition);
                self.block(when_present);
                self.block(when_absent);
            }
        }
    }

    fn effect(&mut self, effect: &Effect) {
        match effect {
            Effect::Read { key, .. } | Effect::Delete { key, .. } => self.exprs(key),
            Effect::Insert { key, columns, .. } | Effect::Update { key, columns, .. } => {
                self.exprs(key);
                columns.values().for_each(|expr| self.expr(expr));
            }
            Effect::ReadRange {
                start, end, bound, ..
            }
            | Effect::ReadIndexRange {
                start, end, bound, ..
            } => {
                self.exprs(start);
                self.exprs(end);
                self.expr(bound);
            }
            Effect::Iterate { body, .. } => body.iter().for_each(|inner| self.effect(inner)),
        }
    }

    fn exprs(&mut self, exprs: &[Expr]) {
        exprs.iter().for_each(|expr| self.expr(expr));
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(value) => self.value(value),
            Expr::Parameter(_) | Expr::Projection { .. } => {}
            Expr::Arithmetic(_, left, right) => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                self.condition(condition);
                self.expr(when_true);
                self.expr(when_false);
            }
        }
    }

    fn condition(&mut self, condition: &Condition) {
        match condition {
            Condition::RowPresent(_) => {}
            Condition::Compare { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
        }
    }
}
