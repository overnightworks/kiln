/// The type a validated term has (grammar §1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sort {
    Int,
    Bool,
    Cat(String),
    /// `Opt(Int)`: a row that is absent or present with an owner.
    Opt,
    Keyed(String),
    Map(String),
    Name,
    Outcome,
    Carrier,
}

/// What a check wanted where it found another sort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    Sort(Sort),
    AnyCategory,
    AnyMap,
    Keyed,
}
