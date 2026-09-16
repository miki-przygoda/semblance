//! # semblance-rng
//!
//! One random source for the whole project. Every seeded draw in `semblance`
//! comes from here, so a seed means exactly one thing workspace-wide.
//!
//! SplitMix64 expands a `u64` seed into xoshiro256++'s 256-bit state, and
//! xoshiro256++ produces the stream. Period 2^256 - 1; passes BigCrush.
//!
//! ## Stability contract
//!
//! The stream produced by a given seed is **frozen**. Any change to it — the
//! algorithm, the constants, the number of draws a method consumes, or the
//! order of draws within a method — is a breaking change requiring a major
//! version bump.
//!
//! This crate exists because `rand` 0.9 changed `StdRng`'s algorithm and
//! silently reset this project's reproducibility baseline. The vectors in
//! `tests/vectors.rs` make an accidental repeat of that fail loudly.

/// The golden-ratio constant, as SplitMix64's additive increment.
///
/// Worth noting: the generator this crate replaces used this same constant as
/// an XOR mask over its seed, which is what created its absorbing zero state.
/// Here it increments a counter, and a counter cannot get stuck — which is
/// precisely why seeding is safe for every possible `u64`.
const SPLITMIX_INCREMENT: u64 = 0x9E37_79B9_7F4A_7C15;

/// A seeded, deterministic random source.
///
/// See the module docs for the stability contract. Construct with
/// [`Rng::seed_from_u64`]; there is deliberately no unseeded constructor,
/// because every run in this project must be reproducible from its seed.
#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seed the generator. Every `u64` is a valid seed.
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut state = seed;
        let mut s = [0u64; 4];
        for slot in s.iter_mut() {
            *slot = splitmix64(&mut state);
        }
        Rng { s }
    }

    /// The raw 64-bit draw. Everything else in this type is built on it.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[0]
            .wrapping_add(self.s[3])
            .rotate_left(23)
            .wrapping_add(self.s[0]);

        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);

        result
    }

    /// A uniform draw in `[0, 1)`.
    ///
    /// Takes the top 24 bits, matching f32's mantissa width, so every
    /// representable step in the interval is reachable and none is favoured.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A uniform index in `[0, n)`.
    ///
    /// Rejection sampling, not Lemire's multiply: `zone` is the largest
    /// multiple of `n` that fits in a `u64`, and draws at or above it are
    /// discarded. Marginally slower, and unbiased by inspection — which is
    /// what matters when the stream is frozen by contract.
    ///
    /// # Panics
    ///
    /// If `n == 0`. An empty range has no valid answer, and returning 0 would
    /// hide the caller's bug.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) has no valid result");
        let n = n as u64;
        let zone = (u64::MAX / n) * n;
        loop {
            let x = self.next_u64();
            if x < zone {
                return (x % n) as usize;
            }
        }
    }
}

/// SplitMix64. Used only to expand a seed into xoshiro's state.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(SPLITMIX_INCREMENT);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_reproduces_the_stream() {
        let mut a = Rng::seed_from_u64(12345);
        let mut b = Rng::seed_from_u64(12345);
        for _ in 0..64 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::seed_from_u64(1);
        let mut b = Rng::seed_from_u64(2);
        let xs: Vec<u64> = (0..16).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..16).map(|_| b.next_u64()).collect();
        assert_ne!(xs, ys);
    }

    /// The bug this crate exists to prevent. The old `Lcg` in `connectome`
    /// XORed its seed with 0x9E3779B97F4A7C15, so that exact seed zeroed the
    /// state and every subsequent output was 0, forever.
    #[test]
    fn pathological_seeds_produce_healthy_streams() {
        for seed in [0u64, 0x9E37_79B9_7F4A_7C15, u64::MAX] {
            let mut rng = Rng::seed_from_u64(seed);
            let xs: Vec<u64> = (0..32).map(|_| rng.next_u64()).collect();
            assert!(
                xs.iter().any(|&x| x != 0),
                "seed {seed:#x} produced an all-zero stream"
            );
            assert!(
                xs.windows(2).any(|w| w[0] != w[1]),
                "seed {seed:#x} produced a constant stream"
            );
        }
    }

    #[test]
    fn next_f32_stays_in_the_unit_interval() {
        let mut rng = Rng::seed_from_u64(7);
        let mut saw_low = false;
        let mut saw_high = false;
        for _ in 0..100_000 {
            let x = rng.next_f32();
            assert!((0.0..1.0).contains(&x), "{x} outside [0, 1)");
            if x < 0.25 {
                saw_low = true;
            }
            if x > 0.75 {
                saw_high = true;
            }
        }
        assert!(saw_low && saw_high, "values clustered in the middle");
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::seed_from_u64(99);
        for n in [1usize, 2, 7, 50, 1000] {
            for _ in 0..10_000 {
                assert!(rng.below(n) < n);
            }
        }
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut rng = Rng::seed_from_u64(3);
        for _ in 0..1000 {
            assert_eq!(rng.below(1), 0);
        }
    }

    #[test]
    #[should_panic(expected = "below(0)")]
    fn below_zero_panics() {
        let mut rng = Rng::seed_from_u64(3);
        rng.below(0);
    }

    /// The old `synthetic()` used `next_u64() % n`, which favours low indices.
    /// Every bucket should land within 5% of the expected share.
    #[test]
    fn below_is_unbiased_across_buckets() {
        const N: usize = 17;
        const DRAWS: usize = 340_000;
        let expected = (DRAWS / N) as f64;

        let mut rng = Rng::seed_from_u64(2024);
        let mut counts = [0usize; N];
        for _ in 0..DRAWS {
            counts[rng.below(N)] += 1;
        }

        for (bucket, &count) in counts.iter().enumerate() {
            let drift = (count as f64 - expected).abs() / expected;
            assert!(
                drift < 0.05,
                "bucket {bucket} drifted {:.1}% from uniform ({count} vs {expected:.0})",
                drift * 100.0
            );
        }
    }
}
