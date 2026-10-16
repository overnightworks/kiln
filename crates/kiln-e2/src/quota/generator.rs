use super::{Batch, QuotaCall, QuotaState, CEILING, QUANTITY_MAX};
use crate::rng::{batch_generator, BatchSize, Family, SplitMix64};

const ALLOCATE_SHARE: i64 = 75;
const MALFORMED_ONE_IN: i64 = 100;
const SMALL_MALFORMED_QUANTITY_LIMIT: i64 = 25;

/// Batch `batch_index` of `size` calls (inputs.md section 2.6).
pub fn generate(size: BatchSize, batch_index: u32) -> Batch {
    let mut generator = batch_generator(Family::Quota, size, batch_index);
    let initial = QuotaState {
        remaining: generator.uniform(0, CEILING),
    };
    let calls = (0..size.get()).map(|_| draw_call(&mut generator)).collect();
    Batch { initial, calls }
}

fn draw_call(generator: &mut SplitMix64) -> QuotaCall {
    let kind = generator.uniform(0, 99);
    let drawn = generator.uniform(1, QUANTITY_MAX);
    let malformed = generator.uniform(0, MALFORMED_ONE_IN - 1) == 0;
    let quantity = if !malformed {
        drawn
    } else if drawn <= SMALL_MALFORMED_QUANTITY_LIMIT {
        0
    } else {
        QUANTITY_MAX + 1
    };
    if kind < ALLOCATE_SHARE {
        QuotaCall::Allocate { quantity }
    } else {
        QuotaCall::Refill { quantity }
    }
}
