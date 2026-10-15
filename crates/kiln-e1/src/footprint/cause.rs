use std::fmt;

use kiln_ir::{ColumnName, IndexName, TableName};

/// Why a function's footprint is none of static, one-hop or domain-bounded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnknownCause {
    /// A range reads an index the schema does not declare (Q2.5).
    UndeclaredIndex(IndexName),
    /// A read's column reaches an access of `to` with no declared foreign key
    /// into it.
    NoDeclaredForeignKey {
        from: TableName,
        column: ColumnName,
        to: TableName,
    },
    /// The primary key of a row of `from` is used as a key of `to`.
    KeyOfAnotherTable { from: TableName, to: TableName },
    /// A primary-key range is bounded by a read result, which names no index.
    PrimaryRangeBoundedByRead,
    /// `count` resolving reads, none of which depends on another.
    IndependentResolutions { count: usize },
    /// `count` resolving reads, one of which depends on another, not confined
    /// to one argument-selected partition.
    ChainedResolutions { count: usize },
}

impl fmt::Display for UnknownCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndeclaredIndex(index) => {
                write!(
                    formatter,
                    "range over index {} the schema does not declare",
                    index.0
                )
            }
            Self::NoDeclaredForeignKey { from, column, to } => write!(
                formatter,
                "{}.{} reaches {} with no declared foreign key",
                from.0, column.0, to.0
            ),
            Self::KeyOfAnotherTable { from, to } => write!(
                formatter,
                "the key of a {} row is used as a key of {}",
                from.0, to.0
            ),
            Self::PrimaryRangeBoundedByRead => {
                formatter.write_str("a primary-key range is bounded by a read result")
            }
            Self::IndependentResolutions { count } => {
                write!(
                    formatter,
                    "{count} independent resolving reads exceed one hop"
                )
            }
            Self::ChainedResolutions { count } => write!(
                formatter,
                "{count} chained resolving reads, not confined to one argument-selected partition"
            ),
        }
    }
}
