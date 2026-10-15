use std::fmt;

/// TPC-C's transaction types with E1's frozen mix, which the freeze fixes in
/// `inputs.md` §1; it is deliberately not configurable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TpccTransaction {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
}

impl TpccTransaction {
    pub const ALL: [Self; 5] = [
        Self::NewOrder,
        Self::Payment,
        Self::OrderStatus,
        Self::Delivery,
        Self::StockLevel,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::NewOrder => "new-order",
            Self::Payment => "payment",
            Self::OrderStatus => "order-status",
            Self::Delivery => "delivery",
            Self::StockLevel => "stock-level",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|transaction| transaction.name() == name)
    }

    /// Percent of the frozen mix: `inputs.md` §1, "Frozen TPC-C weights".
    pub fn weight_percent(self) -> f64 {
        match self {
            Self::NewOrder => 45.0,
            Self::Payment => 43.0,
            Self::OrderStatus | Self::Delivery | Self::StockLevel => 4.0,
        }
    }
}

impl fmt::Display for TpccTransaction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}
