//! The freeze's function notation (inputs.md section 1) as constructors: a body
//! reads as the ordered list of its checks followed by what it does when
//! every check passes.

use kiln_ir::{
    AbortName, ArithmeticOp, Block, Condition, Effect, Expr, IntegerWidth, Parameter,
    ParameterName, ParameterType, Relation, Slot, Terminal,
};

use crate::rows::{column, table};

pub(crate) fn integer_parameter(name: &str, width: IntegerWidth) -> Parameter {
    Parameter {
        name: ParameterName(name.to_owned()),
        kind: ParameterType::Integer(width),
    }
}

pub(crate) fn argument(name: &str) -> Expr {
    Expr::Parameter(ParameterName(name.to_owned()))
}

pub(crate) fn number(value: i64) -> Expr {
    Expr::Literal(kiln_ir::Value::Integer(value))
}

/// `sN.column`
pub(crate) fn field(slot: u32, column_name: &str) -> Expr {
    Expr::Projection {
        slot: Slot(slot),
        column: column(column_name),
    }
}

pub(crate) fn plus(left: Expr, right: Expr) -> Expr {
    Expr::Arithmetic(ArithmeticOp::Add, Box::new(left), Box::new(right))
}

pub(crate) fn minus(left: Expr, right: Expr) -> Expr {
    Expr::Arithmetic(ArithmeticOp::Subtract, Box::new(left), Box::new(right))
}

pub(crate) fn holds(left: Expr, relation: Relation, right: Expr) -> Condition {
    Condition::Compare {
        left,
        relation,
        right,
    }
}

/// `read table[key] -> sN`
pub(crate) fn read(table_name: &str, key: Expr, slot: u32) -> Effect {
    Effect::Read {
        table: table(table_name),
        key,
        into: Slot(slot),
    }
}

/// `update table[key].column = value`
pub(crate) fn update(table_name: &str, key: Expr, column_name: &str, value: Expr) -> Effect {
    Effect::Update {
        table: table(table_name),
        key,
        columns: [(column(column_name), value)].into(),
    }
}

pub(crate) fn returning(effects: Vec<Effect>, response: Expr) -> Block {
    Block {
        effects,
        terminal: Terminal::Return(response),
    }
}

/// `if condition: when_true; otherwise when_false`, after `effects`.
pub(crate) fn branch(
    effects: Vec<Effect>,
    condition: Condition,
    when_true: Block,
    when_false: Block,
) -> Block {
    Block {
        effects,
        terminal: Terminal::Branch {
            condition,
            when_present: Box::new(when_true),
            when_absent: Box::new(when_false),
        },
    }
}

fn aborting(name: &str) -> Block {
    Block {
        effects: Vec::new(),
        terminal: Terminal::Abort(AbortName(name.to_owned())),
    }
}

enum Check {
    AbortIf(Condition, &'static str),
    AbortIfAbsent(Slot, &'static str),
}

/// One test of a body, with the reads that precede it: the first failing
/// step decides the outcome.
pub(crate) struct Step {
    effects: Vec<Effect>,
    check: Check,
}

/// `if left <relation> right: abort name`
pub(crate) fn abort_if(left: Expr, relation: Relation, right: Expr, name: &'static str) -> Step {
    Step {
        effects: Vec::new(),
        check: Check::AbortIf(holds(left, relation, right), name),
    }
}

/// `if absent sN: abort name`
pub(crate) fn abort_if_absent(slot: u32, name: &'static str) -> Step {
    Step {
        effects: Vec::new(),
        check: Check::AbortIfAbsent(Slot(slot), name),
    }
}

/// The two argument tests that every call with a bounded argument starts with.
pub(crate) fn abort_unless_in_range(
    parameter: &str,
    lowest: i64,
    highest: i64,
    name: &'static str,
) -> [Step; 2] {
    [
        abort_if(argument(parameter), Relation::Less, number(lowest), name),
        abort_if(
            argument(parameter),
            Relation::Greater,
            number(highest),
            name,
        ),
    ]
}

impl Step {
    /// The step runs `effect` before its test.
    pub(crate) fn after(mut self, effect: Effect) -> Self {
        self.effects.push(effect);
        self
    }

    fn before(self, rest: Block) -> Block {
        let terminal = match self.check {
            Check::AbortIf(condition, name) => Terminal::Branch {
                condition,
                when_present: Box::new(aborting(name)),
                when_absent: Box::new(rest),
            },
            Check::AbortIfAbsent(slot, name) => Terminal::Branch {
                condition: Condition::RowPresent(slot),
                when_present: Box::new(rest),
                when_absent: Box::new(aborting(name)),
            },
        };
        Block {
            effects: self.effects,
            terminal,
        }
    }
}

/// A body that runs `steps` in order and then `finish`.
pub(crate) fn sequence(steps: impl IntoIterator<Item = Step>, finish: Block) -> Block {
    let steps: Vec<Step> = steps.into_iter().collect();
    steps
        .into_iter()
        .rev()
        .fold(finish, |rest, step| step.before(rest))
}
