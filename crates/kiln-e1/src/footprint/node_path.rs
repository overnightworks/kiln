use std::fmt;

use kiln_ir::FunctionName;

/// The position of an IR node inside one function, rendered as a path such as
/// `place_order/body/terminal/when_present/effects[1]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodePath(String);

impl NodePath {
    pub fn body_of(function: &FunctionName) -> Self {
        Self(format!("{}/body", function.0))
    }

    pub fn effect(&self, index: usize) -> Self {
        Self(format!("{}/effects[{index}]", self.0))
    }

    pub fn iteration_body(&self) -> Self {
        Self(format!("{}/body", self.0))
    }

    pub fn terminal(&self) -> Self {
        Self(format!("{}/terminal", self.0))
    }

    pub fn when_present(&self) -> Self {
        Self(format!("{}/when_present", self.0))
    }

    pub fn when_absent(&self) -> Self {
        Self(format!("{}/when_absent", self.0))
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
