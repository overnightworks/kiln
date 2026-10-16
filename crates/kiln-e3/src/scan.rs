use std::collections::BTreeSet;

use kiln_ir::{
    AbortName, AbortReason, Block, ColumnType, Condition, Effect, Expr, Key, KeyDeclaration,
    OverflowReason, ParameterType, Program, Store, Terminal, Value,
};

/// What the grammar reads from a `Program` (grammar §1.2, §1.5).
pub(crate) struct ProgramFacts {
    pub literal_pool: BTreeSet<i64>,
    pub names: Vec<AbortReason>,
}

/// The facts of `program`, whose stored keys are read from `store`.
/// Keys are assumed to be one integer component.
pub(crate) fn scan(program: &Program, store: &Store) -> ProgramFacts {
    let mut scan = Scan::default();
    scan.literals.extend([0, 1]);
    scan.declared_column_bounds(program);
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
    scan.width_overflows(program, store);
    ProgramFacts {
        literal_pool: scan.literals,
        names: scan.names,
    }
}

fn bounds((low, high): (i64, i64)) -> [i64; 2] {
    [low, high]
}

#[derive(Default)]
struct Scan {
    literals: BTreeSet<i64>,
    names: Vec<AbortReason>,
}

impl Scan {
    fn name(&mut self, reason: AbortReason) {
        if !self.names.contains(&reason) {
            self.names.push(reason);
        }
    }

    fn value(&mut self, value: &Value) {
        if let Value::Integer(number) = value {
            self.literals.insert(*number);
        }
    }

    fn declared_column_bounds(&mut self, program: &Program) {
        for table in &program.schema.tables {
            for column in &table.columns {
                if let ColumnType::Integer { width, domain } = &column.kind {
                    self.literals.extend(bounds(width.range()));
                    if let Some(domain) = domain {
                        self.literals.extend([domain.min, domain.max]);
                    }
                }
            }
        }
    }

    fn width_overflows(&mut self, program: &Program, store: &Store) {
        let all_keys = (
            Key(vec![Value::Integer(i64::MIN)]),
            Key(vec![Value::Integer(i64::MAX)]),
        );
        for table in &program.schema.tables {
            let key_columns = match &table.key {
                KeyDeclaration::Columns(columns) => columns.as_slice(),
                KeyDeclaration::Keyless => &[],
            };
            let stored = store.rows_in_range(&table.name, &all_keys.0, &all_keys.1);
            for column in &table.columns {
                let clamps = matches!(
                    column.kind,
                    ColumnType::Integer {
                        domain: Some(_),
                        ..
                    }
                );
                let unclamped_integer =
                    matches!(column.kind, ColumnType::Integer { .. }) && !clamps;
                if unclamped_integer && !key_columns.contains(&column.name) {
                    for row in &stored {
                        self.name(AbortReason::Overflow(OverflowReason::Width {
                            table: table.name.clone(),
                            column: column.name.clone(),
                            key: row.key.clone(),
                        }));
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
            Terminal::Abort(AbortName(name)) => {
                self.name(AbortReason::Declared(AbortName(name.clone())));
            }
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
