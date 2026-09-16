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
}
