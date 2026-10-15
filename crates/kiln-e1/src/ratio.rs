use std::cmp::Ordering;
use std::fmt;
use std::num::NonZeroU128;

/// A share that cannot be represented exactly in 128 bits. The verdict
/// refuses rather than round (`docs/experiments.md`, "Gating numerator and
/// verdict": the thresholds are exact).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overflow;

impl fmt::Display for Overflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a share needs more than 128 bits to stay exact")
    }
}

impl std::error::Error for Overflow {}

/// An exact non-negative fraction, kept reduced so that equal values are equal
/// structurally. Floating point appears only when a share is displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ratio {
    numerator: u128,
    denominator: NonZeroU128,
}

impl Ratio {
    pub fn new(numerator: u128, denominator: NonZeroU128) -> Self {
        let divisor = greatest_common_divisor(numerator, denominator.get());
        Self {
            numerator: numerator / divisor,
            denominator: NonZeroU128::new(denominator.get() / divisor)
                .expect("a divisor of a non-zero number leaves a non-zero quotient"),
        }
    }

    pub fn from_percent(percent: u8) -> Self {
        Self::new(
            u128::from(percent),
            NonZeroU128::new(100).expect("100 is not zero"),
        )
    }

    pub fn checked_add(self, other: Self) -> Result<Self, Overflow> {
        let numerator = self
            .numerator
            .checked_mul(other.denominator.get())
            .zip(other.numerator.checked_mul(self.denominator.get()))
            .and_then(|(left, right)| left.checked_add(right));
        Ok(Self::new(
            numerator.ok_or(Overflow)?,
            self.denominator_product(other)?,
        ))
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, Overflow> {
        Ok(Self::new(
            self.numerator
                .checked_mul(other.numerator)
                .ok_or(Overflow)?,
            self.denominator_product(other)?,
        ))
    }

    pub fn checked_half(self) -> Result<Self, Overflow> {
        self.checked_mul(Self::new(1, NonZeroU128::new(2).expect("2 is not zero")))
    }

    fn denominator_product(self, other: Self) -> Result<NonZeroU128, Overflow> {
        self.denominator
            .checked_mul(other.denominator)
            .ok_or(Overflow)
    }

    /// The share in percent, for display only.
    fn as_percent_for_display(self) -> f64 {
        100.0 * self.numerator as f64 / self.denominator.get() as f64
    }
}

impl PartialOrd for Ratio {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compares by continued fractions, which is exact and cannot overflow where
/// cross-multiplying 128-bit parts could.
impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_fractions(
            (self.numerator, self.denominator.get()),
            (other.numerator, other.denominator.get()),
        )
    }
}

fn compare_fractions(left: (u128, u128), right: (u128, u128)) -> Ordering {
    let (left_whole, left_rest) = (left.0 / left.1, left.0 % left.1);
    let (right_whole, right_rest) = (right.0 / right.1, right.0 % right.1);
    left_whole
        .cmp(&right_whole)
        .then_with(|| match (left_rest, right_rest) {
            (0, 0) => Ordering::Equal,
            (0, _) => Ordering::Less,
            (_, 0) => Ordering::Greater,
            // a/b < c/d exactly when b/a > d/c, for the proper-fraction parts.
            _ => compare_fractions((right.1, right_rest), (left.1, left_rest)),
        })
}

fn greatest_common_divisor(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

impl fmt::Display for Ratio {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:.4} %", self.as_percent_for_display())
    }
}
