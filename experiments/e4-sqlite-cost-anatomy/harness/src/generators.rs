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

/// Workload C's collision fallback (protocol §2, §5): protocol §5
/// forbids PRNG work inside the timed window, but the redraw is only
/// known to be needed once the primary `INSERT`, inside that window,
/// reports a collision. [`prepare_insert_fallback`] draws it ahead of
/// time, from a *clone* of the attempt's RNG, so the real stream
/// advances only if the fallback is actually used —
/// [`commit`](Self::commit) adopts the clone's post-draw state into it;
/// dropped unused (no collision), `rng` is left exactly as if the
/// fallback had never been drawn.
pub struct InsertFallback<R> {
    pub email: String,
    advanced: R,
}

impl<R> InsertFallback<R> {
    /// Adopts the clone's post-draw state into `rng`, since the
    /// fallback was actually used to commit a row.
    pub fn commit(self, rng: &mut R) {
        *rng = self.advanced;
    }
}

/// Clones `rng` and draws the fallback email from the clone alone, never
/// advancing `rng` itself — see [`InsertFallback`].
pub fn prepare_insert_fallback<R: Rng + Clone>(
    rng: &R,
    repetition_counter: u64,
) -> InsertFallback<R> {
    let mut advanced = rng.clone();
    let email = inserted_email(&mut advanced, repetition_counter);
    InsertFallback { email, advanced }
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

    /// Frozen golden traces for workloads A/B/C/D: literal values, not
    /// recomputed at test time by a second implementation of the seed
    /// formula or the modulo arithmetic — the gap review finding #9
    /// named ("the new trace tests recompute expectations using the
    /// same PRNG, production `seed_for`, and duplicated modulo
    /// algorithm... not frozen, independently established traces").
    ///
    /// Produced once: a throwaway `#[ignore]`d test called
    /// `point_read_trace`/`point_update_trace`/`insert_trace`/
    /// `transfer_trace` directly against the seeds and sizes below and
    /// printed the result with `--nocapture`, run against `rand_chacha`
    /// 0.10.0 (the version `Cargo.lock` pins) on 2026-10-15; the printed
    /// values were copied here verbatim and the throwaway test deleted.
    /// A future change to the seed formula, the PRNG, or a generator's
    /// arithmetic must change these literals by hand, deliberately, not
    /// make the test pass by recomputing alongside it.
    ///
    /// These literals were printed by the production generators
    /// themselves at freeze time, so this test guards against a future
    /// regression in that output — it does not independently prove the
    /// generators conform to the protocol. That conformance was checked
    /// by review, by reading the seed formula and the key-range mapping
    /// directly against protocol §2.1 ("Inputs, frozen") rather than by
    /// recomputing these values a second way: `seed_for`'s
    /// `1 + cell_index × 1000 + repetition_index` matches §2.1's seed
    /// formula verbatim, and every id this module draws is checked
    /// against the live row range (§3.1's "uniform random over the live
    /// key range"), both confirmed by inspection of `seed_for` and the
    /// range-bounded draw here, not by this fixture.
    mod golden {
        use super::*;

        const SEED_A: u64 = 424242;
        const SEED_B: u64 = 424243;
        const SEED_C: u64 = 424244;
        const SEED_D: u64 = 424245;
        const ROW_COUNT: u64 = 1000;
        const N: usize = 10;

        const EXPECTED_A_IDS: [u64; N] = [845, 223, 410, 920, 392, 896, 688, 321, 548, 732];
        const EXPECTED_B_IDS: [u64; N] = [690, 285, 355, 971, 861, 768, 838, 215, 634, 323];
        const EXPECTED_C: [(u64, &str); N] = [
            (1001, "2a99d71e1cac944a-1"),
            (1002, "7d186a203adba3fc-2"),
            (1003, "cf0bc8141eb2a5ec-3"),
            (1004, "e7192d5c027e119c-4"),
            (1005, "dee8bb0d6d7de4a8-5"),
            (1006, "842379b8711c744b-6"),
            (1007, "2d3d78c4ab866370-7"),
            (1008, "9d3449931fa47996-8"),
            (1009, "92cadffd5feec7d1-9"),
            (1010, "81993d19856ac9dd-10"),
        ];
        const EXPECTED_D: [(u64, u64); N] = [
            (984, 827),
            (31, 212),
            (536, 921),
            (649, 568),
            (836, 673),
            (101, 408),
            (266, 821),
            (480, 881),
            (801, 784),
            (750, 657),
        ];

        #[test]
        fn point_read_matches_the_frozen_trace() {
            let actual: Vec<u64> = point_read_trace(SEED_A, ROW_COUNT, N)
                .iter()
                .map(|p| p.id)
                .collect();
            assert_eq!(actual, EXPECTED_A_IDS);
        }

        #[test]
        fn point_update_matches_the_frozen_trace() {
            let actual: Vec<u64> = point_update_trace(SEED_B, ROW_COUNT, N)
                .iter()
                .map(|p| p.id)
                .collect();
            assert_eq!(actual, EXPECTED_B_IDS);
        }

        #[test]
        fn insert_matches_the_frozen_trace() {
            let actual: Vec<(u64, String)> = insert_trace(SEED_C, ROW_COUNT, N)
                .iter()
                .map(|p| (p.id, p.email.clone()))
                .collect();
            let expected: Vec<(u64, String)> = EXPECTED_C
                .iter()
                .map(|(id, email)| (*id, email.to_string()))
                .collect();
            assert_eq!(actual, expected);
        }

        #[test]
        fn transfer_matches_the_frozen_trace() {
            let actual: Vec<(u64, u64)> = transfer_trace(SEED_D, ROW_COUNT, N)
                .iter()
                .map(|p| (p.sender, p.receiver))
                .collect();
            assert_eq!(actual, EXPECTED_D);
        }
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
