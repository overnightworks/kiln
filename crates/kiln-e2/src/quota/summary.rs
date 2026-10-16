//! Quota's summary algebra (inputs.md section 2.4): every call is a clipped
//! translation `x ↦ clamp(x + a, l, u)` on `[0, 1000]`.

use std::collections::BTreeMap;
use std::mem::size_of;

use super::{Batch, QuotaAbort, QuotaCall, QuotaState, CEILING, QUANTITY_MAX};
use crate::corpus::prefix_corpus;
use crate::cost::measure;
use crate::run::CallOutcome;
use crate::summary::{OperationCounter, Summary};

/// `(a, l, u)` with `0 <= l <= u <= 1000`. The representation is not
/// canonical: distinct triples can denote the same transformer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaSummary {
    shift: i64,
    low: i64,
    high: i64,
}

const REPRESENTATION_BYTES: usize = 3 * size_of::<i64>();

impl QuotaSummary {
    fn translation(shift: i64) -> Self {
        Self {
            shift,
            low: 0,
            high: CEILING,
        }
    }
}

fn add<C: OperationCounter>(counter: &mut C, left: i64, right: i64) -> i64 {
    counter.count();
    left + right
}

fn clamp<C: OperationCounter>(counter: &mut C, value: i64, low: i64, high: i64) -> i64 {
    counter.count();
    value.max(low).min(high)
}

fn is_valid_quantity(quantity: i64) -> bool {
    (1..=QUANTITY_MAX).contains(&quantity)
}

impl Summary for QuotaSummary {
    type Call = QuotaCall;
    type State = QuotaState;
    type Abort = QuotaAbort;

    fn atomic(call: QuotaCall) -> Self {
        match call {
            QuotaCall::Allocate { quantity } if is_valid_quantity(quantity) => {
                Self::translation(-quantity)
            }
            QuotaCall::Refill { quantity } if is_valid_quantity(quantity) => {
                Self::translation(quantity)
            }
            QuotaCall::Allocate { .. } | QuotaCall::Refill { .. } => Self::translation(0),
        }
    }

    fn combine<C: OperationCounter>(&self, later: &Self, counter: &mut C) -> Self {
        let low = add(counter, self.low, later.shift);
        let high = add(counter, self.high, later.shift);
        Self {
            shift: add(counter, self.shift, later.shift),
            low: clamp(counter, low, later.low, later.high),
            high: clamp(counter, high, later.low, later.high),
        }
    }

    fn apply<C: OperationCounter>(&self, state: QuotaState, counter: &mut C) -> QuotaState {
        let shifted = add(counter, state.remaining, self.shift);
        QuotaState {
            remaining: clamp(counter, shifted, self.low, self.high),
        }
    }

    fn response(call: QuotaCall, pre_state: QuotaState) -> CallOutcome<QuotaAbort> {
        let remaining = pre_state.remaining;
        let (QuotaCall::Allocate { quantity } | QuotaCall::Refill { quantity }) = call;
        if !is_valid_quantity(quantity) {
            return CallOutcome::declared(QuotaAbort::InvalidQuantity);
        }
        let (room, abort) = match call {
            QuotaCall::Allocate { .. } => (remaining, QuotaAbort::QuotaExhausted),
            QuotaCall::Refill { .. } => (CEILING - remaining, QuotaAbort::QuotaFull),
        };
        match room {
            0 => CallOutcome::declared(abort),
            room => CallOutcome::committed(quantity.min(room)),
        }
    }

    fn size_in_bytes(&self) -> usize {
        REPRESENTATION_BYTES
    }
}

/// What the prefix corpus measured at one batch size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeReport {
    pub size: u32,
    pub bytes: usize,
    pub largest_combine_operations: Option<u64>,
    pub apply_operations: u64,
}

/// The largest bytes and operation counts over the seeds, per batch size.
pub fn cost_report() -> Vec<SizeReport> {
    let mut by_size: BTreeMap<u32, SizeReport> = BTreeMap::new();
    for case in prefix_corpus(super::generate, |batch: &mut Batch, size| {
        batch.calls.truncate(size)
    }) {
        let cost = measure::<QuotaSummary>(case.batch.initial, &case.batch.calls)
            .expect("a protocol size is at least one call");
        let report = by_size.entry(case.size).or_insert(SizeReport {
            size: case.size,
            bytes: cost.bytes,
            largest_combine_operations: cost.largest_combine_operations,
            apply_operations: cost.apply_operations,
        });
        report.bytes = report.bytes.max(cost.bytes);
        report.largest_combine_operations = report
            .largest_combine_operations
            .max(cost.largest_combine_operations);
        report.apply_operations = report.apply_operations.max(cost.apply_operations);
    }
    by_size.into_values().collect()
}
