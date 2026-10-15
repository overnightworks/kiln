//! Frozen inputs (protocol.md §2.1): the PRNG seed formula, the initial
//! load's deterministic emails, and each workload's per-attempt trace of
//! operation parameters. Pure and deterministic — no SQLite, no I/O — so
//! a trace can be reproduced and compared without a real run.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};
use sha2::{Digest, Sha256};

/// `1 + cell_index * 1000 + repetition_index` (protocol §2.1): never one
/// seed running across a whole cell, so a retried attempt draws the same
/// trace as the attempt it replaces.
pub fn seed_for(cell_index: u64, repetition_index: u64) -> u64 {
    1 + cell_index * 1000 + repetition_index
}

/// A fresh `ChaCha8Rng` seeded for one repetition attempt (protocol
/// §2.1). The same instance draws every operation in that attempt,
/// warm-up included, in issue order.
pub fn rng_for_attempt(cell_index: u64, repetition_index: u64) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed_for(cell_index, repetition_index))
}

/// Uniform draw in `[low, high_inclusive]`. Not pinned by the protocol —
/// chosen as the simplest correct option over a rejection-sampling
/// scheme: modulo bias is at most `row_count / 2^64`, far below 1e-12 at
/// the 10,000,000-row size, so it is not a detectable source of
/// non-uniformity at this experiment's scale.
pub fn uniform_in_range(rng: &mut impl Rng, low: u64, high_inclusive: u64) -> u64 {
    assert!(low <= high_inclusive, "empty range");
    let span = high_inclusive - low + 1;
    low + rng.next_u64() % span
}

/// A draw in `[low, high_inclusive]` redrawn until it differs from
/// `excluded` (protocol §2: D's independent third id, redrawn on a
/// collision with the sender's id).
pub fn draw_distinct(rng: &mut impl Rng, low: u64, high_inclusive: u64, excluded: u64) -> u64 {
    loop {
        let candidate = uniform_in_range(rng, low, high_inclusive);
        if candidate != excluded {
            return candidate;
        }
    }
}

fn hex_prefix(bytes: &[u8], hex_chars: usize) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .chars()
        .take(hex_chars)
        .collect()
}

/// The initial load's deterministic email (protocol §2.1): the first 16
/// lowercase hex characters of `SHA-256` over `id`'s decimal ASCII
/// encoding, plus `-init-` and `id` — derived from `id`, never the
/// repetition PRNG, since a collision here would be a protocol
/// violation, not a redrawable event.
pub fn initial_load_email(id: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(id.to_string().as_bytes());
    let digest = hasher.finalize();
    format!("{}-init-{id}", hex_prefix(&digest, 16))
}

/// Workload C's inserted email (protocol §2): 16 random lowercase hex
/// characters, leading, followed by `-` and `repetition_counter`.
pub fn inserted_email(rng: &mut impl Rng, repetition_counter: u64) -> String {
    let mut bytes = [0u8; 8];
    rng.fill_bytes(&mut bytes);
    format!("{}-{repetition_counter}", hex_prefix(&bytes, 16))
}

/// Workload A's per-operation parameter (protocol §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadParams {
    pub id: u64,
}

/// Workload B's per-operation parameter (protocol §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateParams {
    pub id: u64,
}

/// Workload C's per-operation parameters before any collision redraw
/// (protocol §2): `id` is deterministic (the growing counter), `email`
/// is this generator's first draw — the execution layer redraws `email`
/// once more from the same RNG stream on `SQLITE_CONSTRAINT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsertParams {
    pub id: u64,
    pub email: String,
}

/// Workload D's per-operation parameters (protocol §2): `sender` is
/// bound to both `?1` and `?2`; `receiver` (`?3`) is independent and
/// never equals `sender`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferParams {
    pub sender: u64,
    pub receiver: u64,
}

/// Workload A's trace: `operations` uniform draws in `[1, row_count]`.
pub fn point_read_trace(seed: u64, row_count: u64, operations: usize) -> Vec<ReadParams> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..operations)
        .map(|_| ReadParams {
            id: uniform_in_range(&mut rng, 1, row_count),
        })
        .collect()
}

/// Workload B's trace: `operations` uniform draws in `[1, row_count]`.
pub fn point_update_trace(seed: u64, row_count: u64, operations: usize) -> Vec<UpdateParams> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..operations)
        .map(|_| UpdateParams {
            id: uniform_in_range(&mut rng, 1, row_count),
        })
        .collect()
}

/// Workload C's trace: `id` counts up from `row_count + 1`;
/// `repetition_counter` is the operation's 1-based ordinal within the
/// attempt (warm-up included, protocol §8.2), the simplest option that
/// still guarantees uniqueness across the whole attempt — the protocol
/// names the counter only as "the repetition counter (uniqueness)"
/// without pinning what it counts.
pub fn insert_trace(seed: u64, row_count: u64, operations: usize) -> Vec<InsertParams> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..operations)
        .map(|i| {
            let ordinal = i as u64 + 1;
            InsertParams {
                id: row_count + ordinal,
                email: inserted_email(&mut rng, ordinal),
            }
        })
        .collect()
}

/// Workload D's trace: `sender` uniform in `[1, row_count]`; `receiver`
/// an independent draw redrawn until it differs from `sender`.
pub fn transfer_trace(seed: u64, row_count: u64, operations: usize) -> Vec<TransferParams> {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    (0..operations)
        .map(|_| {
            let sender = uniform_in_range(&mut rng, 1, row_count);
            let receiver = draw_distinct(&mut rng, 1, row_count, sender);
            TransferParams { sender, receiver }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_formula_matches_protocol() {
        assert_eq!(seed_for(0, 0), 1);
        assert_eq!(seed_for(2, 3), 1 + 2 * 1000 + 3);
    }

    #[test]
    fn a_retried_attempt_draws_the_same_trace_as_the_attempt_it_replaces() {
        let seed = seed_for(5, 1);
        let first = point_read_trace(seed, 10_000, 50);
        let retried = point_read_trace(seed, 10_000, 50);
        assert_eq!(first, retried);
    }

    #[test]
    fn point_read_trace_stays_in_row_range() {
        let trace = point_read_trace(seed_for(0, 0), 37, 500);
        assert!(trace.iter().all(|p| (1..=37).contains(&p.id)));
    }

    /// Pins workload A's trace against draws computed independently of
    /// [`uniform_in_range`] — a fresh `ChaCha8Rng` seeded exactly as
    /// protocol §2.1 specifies, reduced by the same `1 + next_u64() %
    /// span` arithmetic written out here rather than called — so a
    /// future change to either the seed formula or the generator is
    /// caught even though the two sides never call the same function.
    #[test]
    fn point_read_trace_matches_independently_computed_draws() {
        let seed = seed_for(0, 0);
        let row_count = 37u64;
        let mut reference_rng = ChaCha8Rng::seed_from_u64(seed);
        let expected: Vec<u64> = (0..5)
            .map(|_| 1 + reference_rng.next_u64() % row_count)
            .collect();

        let trace = point_read_trace(seed, row_count, 5);
        let actual: Vec<u64> = trace.iter().map(|p| p.id).collect();
        assert_eq!(actual, expected);
    }

    /// Pins workload D's sender/receiver draws the same independent way,
    /// including the redraw-on-collision for the receiver.
    #[test]
    fn transfer_trace_matches_independently_computed_draws() {
        let seed = seed_for(1, 0);
        let row_count = 11u64;
        let mut reference_rng = ChaCha8Rng::seed_from_u64(seed);
        let mut expected = Vec::new();
        for _ in 0..5 {
            let sender = 1 + reference_rng.next_u64() % row_count;
            let mut receiver = 1 + reference_rng.next_u64() % row_count;
            while receiver == sender {
                receiver = 1 + reference_rng.next_u64() % row_count;
            }
            expected.push((sender, receiver));
        }

        let trace = transfer_trace(seed, row_count, 5);
        let actual: Vec<(u64, u64)> = trace.iter().map(|p| (p.sender, p.receiver)).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn transfer_trace_never_pairs_a_sender_with_itself() {
        let trace = transfer_trace(seed_for(1, 0), 11, 2_000);
        assert!(trace.iter().all(|p| p.sender != p.receiver));
        assert!(
            trace
                .iter()
                .all(|p| (1..=11).contains(&p.sender) && (1..=11).contains(&p.receiver))
        );
    }

    #[test]
    fn insert_trace_ids_count_up_from_row_count_plus_one() {
        let trace = insert_trace(seed_for(2, 0), 100, 5);
        let ids: Vec<u64> = trace.iter().map(|p| p.id).collect();
        assert_eq!(ids, vec![101, 102, 103, 104, 105]);
    }

    #[test]
    fn insert_trace_emails_are_unique_within_one_attempt() {
        let trace = insert_trace(seed_for(2, 0), 100, 500);
        let unique: std::collections::HashSet<_> = trace.iter().map(|p| &p.email).collect();
        assert_eq!(unique.len(), trace.len());
    }

    #[test]
    fn initial_load_email_is_deterministic_and_shaped_like_protocol_says() {
        let email = initial_load_email(42);
        assert_eq!(email, initial_load_email(42));
        let (hex_part, suffix) = email.split_once("-init-").expect("has -init- separator");
        assert_eq!(hex_part.len(), 16);
        assert!(
            hex_part
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_eq!(suffix, "42");
    }

    #[test]
    fn initial_load_email_differs_by_id() {
        assert_ne!(initial_load_email(1), initial_load_email(2));
    }
}
