//! Builders for the text-form tests (`tests/text.rs`,
//! `tests/text_validation.rs`): text expressions, and a response run through
//! `kiln_ir::run`.

use kiln_ir::{
    AbortReason, Condition, Expr, Outcome, Parameter, ProgramError, Store, Terminal, Value,
};

use super::{call_under_test, integer, parameter, single_function_program, text};
use kiln_ir::run;

pub fn text_literal(value: &str) -> Expr {
    Expr::Literal(text(value))
}

pub fn integer_literal(value: i64) -> Expr {
    Expr::Literal(integer(value))
}

pub fn argument(name: &str) -> Expr {
    Expr::Parameter(parameter(name))
}

pub fn concat(left: Expr, right: Expr) -> Expr {
    Expr::Concat(Box::new(left), Box::new(right))
}

pub fn truncate(text: Expr, length: Expr) -> Expr {
    Expr::Truncate {
        text: Box::new(text),
        length: Box::new(length),
    }
}

pub fn contains(text: Expr, part: Expr) -> Condition {
    Condition::Contains { text, part }
}

pub fn when(condition: Condition, when_true: Expr, when_false: Expr) -> Expr {
    Expr::Conditional {
        condition: Box::new(condition),
        when_true: Box::new(when_true),
        when_false: Box::new(when_false),
    }
}

pub fn response_result(
    parameters: Vec<Parameter>,
    arguments: Vec<Value>,
    response: Expr,
) -> Result<Outcome, ProgramError> {
    let program = single_function_program(parameters, vec![], Terminal::Return(response));
    run(&program, &call_under_test(arguments), &Store::empty()).map(|execution| execution.outcome)
}

pub fn response_with(parameters: Vec<Parameter>, arguments: Vec<Value>, response: Expr) -> Outcome {
    response_result(parameters, arguments, response).expect("the program validates")
}

pub fn response_of(response: Expr) -> Outcome {
    response_with(vec![], vec![], response)
}

pub fn committed_text(value: &str) -> Outcome {
    Outcome::Committed {
        response: text(value),
    }
}

pub fn aborted(reason: AbortReason) -> Outcome {
    Outcome::Aborted { reason }
}
