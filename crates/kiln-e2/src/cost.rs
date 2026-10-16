//! The quantities protocol.md section 3 records per prefix: the bytes of the
//! batch summary, the operations of the largest combine in the balanced
//! pairwise fold, and the operations of applying the batch summary.

use crate::summary::{OperationCount, Summary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub bytes: usize,
    /// `None` for a single call, which has no combine.
    pub largest_combine_operations: Option<u64>,
    pub apply_operations: u64,
}

/// `None` for an empty batch, which has no summary.
pub fn measure<S: Summary>(initial: S::State, calls: &[S::Call]) -> Option<Cost> {
    let mut level: Vec<S> = calls.iter().map(|&call| S::atomic(call)).collect();
    let mut largest_combine_operations = None;
    while level.len() > 1 {
        let mut earlier_summaries = level.into_iter();
        level = Vec::new();
        while let Some(earlier) = earlier_summaries.next() {
            let Some(later) = earlier_summaries.next() else {
                level.push(earlier);
                break;
            };
            let mut counter = OperationCount::default();
            level.push(earlier.combine(&later, &mut counter));
            largest_combine_operations = largest_combine_operations.max(Some(counter.total()));
        }
    }
    let batch_summary = level.pop()?;
    let mut counter = OperationCount::default();
    batch_summary.apply(initial, &mut counter);
    Some(Cost {
        bytes: batch_summary.size_in_bytes(),
        largest_combine_operations,
        apply_operations: counter.total(),
    })
}
