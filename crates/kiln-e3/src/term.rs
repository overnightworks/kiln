use kiln_ir::{AbortReason, Relation};

/// The operand a carrier field is read from: the left and right operands of a
/// combine, or the summary an apply is given (grammar §1.3, `O ∈ {S, T, s}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Left,
    Right,
    Summary,
}

/// Every production of grammar §1.3, checker-only ones included, with the
/// sort left to validation: a malformed term is representable on purpose.
///
/// Where the grammar leaves a term's category implicit (`name(v)`, `table`,
/// `keyed`, `of`), the variant names it, so a term's sort never depends on the
/// context it is written in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    // Int
    Lit(i64),
    Param(String),
    Key,
    State(String),
    Code(Box<Term>),
    Get(Box<Term>, Box<Term>),
    Add(Box<Term>, Box<Term>),
    Sub(Box<Term>, Box<Term>),
    Min(Box<Term>, Box<Term>),
    Max(Box<Term>, Box<Term>),
    Clamp(Box<Term>, Box<Term>, Box<Term>),
    Field {
        operand: Operand,
        field: String,
    },
    Applied,
    Mod(Box<Term>, i64),
    // Bool
    True,
    False,
    Not(Box<Term>),
    And(Box<Term>, Box<Term>),
    Or(Box<Term>, Box<Term>),
    Compare(Box<Term>, Relation, Box<Term>),
    CategoryEqual(Box<Term>, Box<Term>),
    IsPresent(Box<Term>),
    // Cat
    CategoryValue {
        category: String,
        code: usize,
    },
    Lookup(Box<Term>, Box<Term>),
    Of {
        category: String,
        code: Box<Term>,
    },
    // Opt
    Absent,
    Held(Box<Term>),
    StateRow {
        component: String,
        key: Box<Term>,
    },
    AppliedRow(Box<Term>),
    // Keyed and Map
    Keyed {
        category: String,
        rows: Vec<Term>,
    },
    Table {
        category: String,
        entries: Vec<Term>,
    },
    // any sort: both arms share it
    Ite(Box<Term>, Box<Term>, Box<Term>),
    // Carrier
    Tuple(Vec<Term>),
    Whole(Operand),
    When(Box<Term>, Vec<Term>),
    // Outcome
    Commit(Box<Term>),
    Abort(Box<Term>),
    Case {
        scrutinee: Box<Term>,
        arms: Vec<Term>,
    },
    // Name
    Name(AbortReason),
}
