//! SplitMix64 and the seed derivation of inputs.md section 5. All randomness
//! of the generators comes through here.

use std::fmt;

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const FIRST_MULTIPLIER: u64 = 0xBF58_476D_1CE4_E5B9;
const SECOND_MULTIPLIER: u64 = 0x94D0_49BB_1331_11EB;

const FAMILY_SHIFT: u32 = 48;
const SIZE_SHIFT: u32 = 32;

/// The largest batch size the freeze generates.
pub const BATCH_SIZE_MAX: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn from_state(state: u64) -> Self {
        Self { state }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(FIRST_MULTIPLIER);
        z = (z ^ (z >> 27)).wrapping_mul(SECOND_MULTIPLIER);
        z ^ (z >> 31)
    }

    /// `lo + (next mod (hi - lo + 1))`, computed in `u64`; the modulo bias is
    /// part of the frozen definition.
    pub fn uniform(&mut self, lo: i64, hi: i64) -> i64 {
        let span = u64::try_from(hi - lo + 1).expect("hi is not below lo");
        lo + i64::try_from(self.next_u64() % span).expect("a residue below an i64 span fits")
    }
}

/// The families, numbered as the seed derivation numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Quota = 1,
    Inventory = 2,
    Accounts = 3,
}

/// The generator of a family's batch `batch_index` of `size` calls: the seed
/// is `(family << 48) | (size << 32) | batch_index`.
pub fn batch_generator(family: Family, size: BatchSize, batch_index: u32) -> SplitMix64 {
    SplitMix64::from_state(
        ((family as u64) << FAMILY_SHIFT)
            | (u64::from(size.0) << SIZE_SHIFT)
            | u64::from(batch_index),
    )
}

/// A batch size in `1..=BATCH_SIZE_MAX`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSize(u32);

impl BatchSize {
    pub fn new(size: u32) -> Result<Self, BatchSizeOutOfRange> {
        if (1..=BATCH_SIZE_MAX).contains(&size) {
            Ok(Self(size))
        } else {
            Err(BatchSizeOutOfRange(size))
        }
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchSizeOutOfRange(pub u32);

impl fmt::Display for BatchSizeOutOfRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "batch size {} is outside 1..={BATCH_SIZE_MAX}",
            self.0
        )
    }
}

impl std::error::Error for BatchSizeOutOfRange {}
