use crate::identifiers::{FunctionName, ParameterName};

use super::block::Block;
use super::schema::{IntegerWidth, Schema};
use super::validation::{check_block, ProgramError};

/// A function parameter's declared type: an integer at a mandatory width,
/// or text. Every parameter declares one of the two — there is no
/// undeclared or optionally-typed parameter, so an argument's kind is
/// always known ahead of checking its value (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterType {
    Integer(IntegerWidth),
    Text,
}

/// A function's declared parameter: its name and declared type.
///
/// An argument bound to `name` is checked against `kind` exactly as a
/// write is checked against its target column's declared width (D3,
/// `docs/concept.md` §2.2: "Arithmetic runs over checked integers at the
/// column's or argument's declared width"): an argument whose own kind does
/// not match `kind` aborts, and an [`IntegerWidth`]-declared argument
/// outside that width's representable range aborts too. Unlike `Column`, a
/// declared parameter carries no clamping domain: D3 clamps a column's own
/// write, never a call's argument, so an out-of-width argument always
/// aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: ParameterName,
    pub kind: ParameterType,
}

/// A named transaction function: its parameters and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: FunctionName,
    pub parameters: Vec<Parameter>,
    pub body: Block,
}

/// A schema plus its named transaction functions (D1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub schema: Schema,
    pub functions: Vec<Function>,
}

impl Program {
    /// Rejects a program whose schema is invalid or whose function bodies
    /// name a table, a key or a written column the schema does not allow
    /// (D8, D9). `run` calls this before executing any call.
    pub fn validate(&self) -> Result<(), ProgramError> {
        self.schema.validate()?;
        for function in &self.functions {
            check_block(&self.schema, &function.body).map_err(|fault| ProgramError::Function {
                function: function.name.clone(),
                fault,
            })?;
        }
        Ok(())
    }

    /// The function named `name`.
    ///
    /// Panics if the program declares no such function: a [`Call`](crate::call::Call)
    /// naming an undeclared function is a contract violation by whatever
    /// built the call, not a semantic outcome `run` reports as an [`AbortReason`](crate::evaluator::AbortReason).
    pub fn function(&self, name: &FunctionName) -> &Function {
        self.functions
            .iter()
            .find(|function| &function.name == name)
            .unwrap_or_else(|| panic!("program declares no function named {name:?}"))
    }
}
