//! The protocol's paired corpus (protocol.md section 2): for each seed, the
//! prefixes of the batch `generate(256, seed)` at every protocol size, each
//! keeping the batch's initial state.

use crate::rng::{BatchSize, BATCH_SIZE_MAX};

pub const PROTOCOL_SIZES: [u32; 9] = [1, 2, 4, 8, 16, 32, 64, 128, 256];
pub const PROTOCOL_SEEDS: u32 = 100;

pub struct PrefixCase<B> {
    pub size: u32,
    pub seed: u32,
    pub batch: B,
}

/// How a family's batch is cut to its first calls.
pub type Truncate<B> = fn(&mut B, usize);

pub fn prefix_corpus<B: Clone>(
    generate: fn(BatchSize, u32) -> B,
    truncate: Truncate<B>,
) -> impl Iterator<Item = PrefixCase<B>> {
    let largest = BatchSize::new(BATCH_SIZE_MAX).expect("the largest batch size is valid");
    (0..PROTOCOL_SEEDS).flat_map(move |seed| {
        let full = generate(largest, seed);
        PROTOCOL_SIZES.into_iter().map(move |size| {
            let mut batch = full.clone();
            truncate(&mut batch, size as usize);
            PrefixCase { size, seed, batch }
        })
    })
}
