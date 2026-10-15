//! A single invocation of a declared function (D4).

use crate::identifiers::FunctionName;
use crate::ir::Value;

/// A named function invoked with concrete argument values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub function: FunctionName,
    pub args: Vec<Value>,
}
