use super::{
    Batch, InventoryCall, StockState, ORDER_QUANTITY_MAX, STOCK_MAX, STOCK_MIN, THRESHOLD_MAX,
    THRESHOLD_MIN,
};
use crate::rng::{batch_generator, BatchSize, Family, SplitMix64};

/// 45 order lines to 4 stock checks, TPC-C's New-Order to Stock-Level ratio.
const MIX_SIZE: i64 = 49;
const ORDER_LINE_SHARE: i64 = 45;
const MALFORMED_ONE_IN: i64 = 100;
const SMALL_MALFORMED_QUANTITY_LIMIT: i64 = 5;
const SMALL_MALFORMED_THRESHOLD_LIMIT: i64 = 15;

/// Batch `batch_index` of `size` calls (inputs.md section 3.5).
pub fn generate(size: BatchSize, batch_index: u32) -> Batch {
    let mut generator = batch_generator(Family::Inventory, size, batch_index);
    let initial = StockState {
        quantity: generator.uniform(STOCK_MIN, STOCK_MAX),
    };
    let calls = (0..size.get()).map(|_| draw_call(&mut generator)).collect();
    Batch { initial, calls }
}

fn draw_call(generator: &mut SplitMix64) -> InventoryCall {
    if generator.uniform(0, MIX_SIZE - 1) < ORDER_LINE_SHARE {
        let drawn = generator.uniform(1, ORDER_QUANTITY_MAX);
        let quantity = malformed_or(generator, drawn, || {
            if drawn <= SMALL_MALFORMED_QUANTITY_LIMIT {
                0
            } else {
                ORDER_QUANTITY_MAX + 1
            }
        });
        InventoryCall::OrderLine { quantity }
    } else {
        let drawn = generator.uniform(THRESHOLD_MIN, THRESHOLD_MAX);
        let threshold = malformed_or(generator, drawn, || {
            if drawn <= SMALL_MALFORMED_THRESHOLD_LIMIT {
                THRESHOLD_MIN - 1
            } else {
                THRESHOLD_MAX + 1
            }
        });
        InventoryCall::StockCheck { threshold }
    }
}

/// Draws the malformed share `m`, and returns `malformed()` when it hits.
fn malformed_or(generator: &mut SplitMix64, drawn: i64, malformed: impl FnOnce() -> i64) -> i64 {
    if generator.uniform(0, MALFORMED_ONE_IN - 1) == 0 {
        malformed()
    } else {
        drawn
    }
}
