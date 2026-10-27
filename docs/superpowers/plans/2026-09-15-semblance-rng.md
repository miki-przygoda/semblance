# semblance-rng Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the project's two inconsistent random sources with one frozen, auditable generator crate, and migrate `connectome` and `evo` onto it.

**Architecture:** A new zero-dependency crate `semblance-rng` exposes a single concrete `Rng` type — SplitMix64 expands a `u64` seed into xoshiro256++'s 256-bit state, xoshiro256++ produces the stream. Six methods, each earned by an existing call site. No trait, because the algorithm is frozen by contract rather than swappable. `connectome` drops its private `Lcg`; `evo` drops `rand` and its local Box–Muller.

**Tech Stack:** Rust, edition 2024, no external crates.

**Spec:** `docs/superpowers/specs/2026-09-15-semblance-rng-design.md`

## Global Constraints

- Rust **1.85** or newer; edition **2024** (workspace-wide, inherited).
- **No external dependencies.** After this work `cargo tree` must show only our own crates.
- The stream produced by a given seed is **frozen**. Any change to the algorithm, constants, per-method draw counts, or draw order is a breaking change requiring a major version bump.
- Crate package name `semblance-rng`; lib name `semblance_rng`.
- Package metadata is inherited from `[workspace.package]`; `readme = "../../README.md"` is set per-crate.
- Commit message style matches the repo: plain imperative subject, no `feat:`/`fix:` prefixes.
- Run all verification from the workspace root.

## File Structure

| File | Responsibility |
|---|---|
| `crates/rng/Cargo.toml` | new package manifest, metadata inherited |
| `crates/rng/src/lib.rs` | `Rng` and all six methods; `splitmix64` helper; unit tests |
| `crates/rng/tests/vectors.rs` | frozen stream vectors — the executable half of the stability contract |
| `Cargo.toml` | add `crates/rng` member + workspace dep; remove `rand` |
| `crates/connectome/Cargo.toml` | add `semblance-rng` |
| `crates/connectome/src/lib.rs` | delete `Lcg`; `synthetic()` uses `Rng` |
| `crates/evo/Cargo.toml` | `rand` → `semblance-rng` |
| `crates/evo/src/lib.rs` | delete local Box–Muller; use `chance`/`below`/`gaussian` |

`src/lib.rs` stays one file: ~150 lines with one cohesive responsibility. Splitting `splitmix64` (10 lines) into its own module would separate things that change together.

---

### Task 1: Crate skeleton and core generator

**Files:**
- Create: `crates/rng/Cargo.toml`
- Create: `crates/rng/src/lib.rs`
- Modify: `Cargo.toml` (members list, workspace dependencies)

**Interfaces:**
- Consumes: nothing (first task).
- Produces: `semblance_rng::Rng`, `Rng::seed_from_u64(seed: u64) -> Rng`, `Rng::next_u64(&mut self) -> u64`. Every later task builds on these two methods.

- [ ] **Step 1: Register the crate in the workspace**

In the root `Cargo.toml`, add `"crates/rng",` to `members` as the **first** entry (it is the dependency root), and add this line to `[workspace.dependencies]`:

```toml
semblance-rng = { path = "crates/rng", version = "0.1.0" }
```

Leave `rand = "0.10"` alone for now — Task 8 removes it, once `evo` no longer needs it.

- [ ] **Step 2: Create the package manifest**

Create `crates/rng/Cargo.toml`:

```toml
[package]
name = "semblance-rng"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true
keywords.workspace = true
categories.workspace = true
readme = "../../README.md"
description = "One frozen, auditable random source: SplitMix64 seeding xoshiro256++, with a stability contract backed by test vectors."

[lib]
name = "semblance_rng"
path = "src/lib.rs"

[dependencies]
```

The empty `[dependencies]` table is deliberate — it documents that this crate depends on nothing.

- [ ] **Step 3: Write the failing tests**

Create `crates/rng/src/lib.rs` containing ONLY this test module for now:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p semblance-rng`
Expected: FAIL to compile — `cannot find type 'Rng' in this scope`.

- [ ] **Step 5: Write the implementation**

Insert this ABOVE the `#[cfg(test)]` module in `crates/rng/src/lib.rs`:

```rust
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
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p semblance-rng`
Expected: PASS, 3 tests.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/rng
git commit -m "Add semblance-rng with SplitMix64-seeded xoshiro256++

Core generator only: seed_from_u64 and next_u64. Every u64 is a valid
seed, including 0x9E3779B97F4A7C15, which zeroed the generator this
crate replaces."
```

---

### Task 2: `next_f32`

**Files:**
- Modify: `crates/rng/src/lib.rs`

**Interfaces:**
- Consumes: `Rng::next_u64`.
- Produces: `Rng::next_f32(&mut self) -> f32`, uniform in `[0, 1)`. Used by Tasks 4, 5, 7.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `crates/rng/src/lib.rs`:

```rust
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
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p semblance-rng next_f32`
Expected: FAIL to compile — `no method named 'next_f32'`.

- [ ] **Step 3: Write the implementation**

Add inside `impl Rng`, after `next_u64`:

```rust
    /// A uniform draw in `[0, 1)`.
    ///
    /// Takes the top 24 bits, matching f32's mantissa width, so every
    /// representable step in the interval is reachable and none is favoured.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p semblance-rng next_f32`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rng/src/lib.rs
git commit -m "Add Rng::next_f32

Top 24 bits over 2^24, matching f32's mantissa width."
```

---

### Task 3: `below` — unbiased range sampling

**Files:**
- Modify: `crates/rng/src/lib.rs`

**Interfaces:**
- Consumes: `Rng::next_u64`.
- Produces: `Rng::below(&mut self, n: usize) -> usize`, uniform in `[0, n)`, panics when `n == 0`. Used by Tasks 7 and 8.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-rng below`
Expected: FAIL to compile — `no method named 'below'`.

- [ ] **Step 3: Write the implementation**

Add inside `impl Rng`, after `next_f32`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-rng below`
Expected: PASS, 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rng/src/lib.rs
git commit -m "Add Rng::below for unbiased range sampling

Rejection sampling rather than modulo, which favours low indices. Both
call sites this replaces (synthetic() and parent selection) used modulo."
```

---

### Task 4: `chance`

**Files:**
- Modify: `crates/rng/src/lib.rs`

**Interfaces:**
- Consumes: `Rng::next_f32`.
- Produces: `Rng::chance(&mut self, p: f32) -> bool`. Used by Task 8.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    #[test]
    fn chance_handles_both_extremes() {
        let mut rng = Rng::seed_from_u64(11);
        for _ in 0..10_000 {
            assert!(!rng.chance(0.0), "p = 0.0 fired");
            assert!(rng.chance(1.0), "p = 1.0 did not fire");
        }
    }

    #[test]
    fn chance_approximates_its_probability() {
        let mut rng = Rng::seed_from_u64(12);
        let draws = 100_000;
        let hits = (0..draws).filter(|_| rng.chance(0.25)).count();
        let rate = hits as f64 / draws as f64;
        assert!((rate - 0.25).abs() < 0.01, "observed rate {rate:.4}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-rng chance`
Expected: FAIL to compile — `no method named 'chance'`.

- [ ] **Step 3: Write the implementation**

Add inside `impl Rng`, after `below`:

```rust
    /// `true` with probability `p`.
    ///
    /// Correct at both ends without special-casing: `next_f32` is always
    /// `>= 0.0`, so `p <= 0.0` never fires, and always `< 1.0`, so `p >= 1.0`
    /// always fires.
    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-rng chance`
Expected: PASS, 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rng/src/lib.rs
git commit -m "Add Rng::chance"
```

---

### Task 5: `gaussian`

**Files:**
- Modify: `crates/rng/src/lib.rs`

**Interfaces:**
- Consumes: `Rng::next_f32`.
- Produces: `Rng::gaussian(&mut self) -> f32`, standard normal. Consumes exactly two `next_f32` draws per call. Used by Task 8.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    #[test]
    fn gaussian_has_standard_normal_moments() {
        let mut rng = Rng::seed_from_u64(4242);
        let n = 200_000;
        let xs: Vec<f32> = (0..n).map(|_| rng.gaussian()).collect();

        assert!(xs.iter().all(|x| x.is_finite()), "produced a non-finite value");

        let mean = xs.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
        let var = xs.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / n as f64;

        assert!(mean.abs() < 0.02, "mean {mean:.4} is not ~0");
        assert!((var.sqrt() - 1.0).abs() < 0.02, "sd {:.4} is not ~1", var.sqrt());
    }

    /// Box-Muller yields two normals per pair of uniforms; we return one and
    /// discard the other so the draw count per call never varies.
    #[test]
    fn gaussian_consumes_exactly_two_f32_draws() {
        let mut a = Rng::seed_from_u64(5150);
        let mut b = Rng::seed_from_u64(5150);

        a.gaussian();
        b.next_f32();
        b.next_f32();

        assert_eq!(a.next_u64(), b.next_u64(), "gaussian moved the stream by != 2 draws");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-rng gaussian`
Expected: FAIL to compile — `no method named 'gaussian'`.

- [ ] **Step 3: Write the implementation**

Add this constant next to `SPLITMIX_INCREMENT` near the top of the file:

```rust
/// Floor applied to Box-Muller's first uniform. `next_f32` can return exactly
/// `0.0`, and `ln(0)` is `-inf`. Part of the frozen contract, so it is a named
/// constant rather than a literal buried in the expression.
const GAUSSIAN_EPSILON: f32 = 1e-7;
```

Add inside `impl Rng`, after `chance`:

```rust
    /// A draw from the standard normal, N(0, 1).
    ///
    /// Box-Muller. It naturally produces two normals per pair of uniforms; we
    /// return one and discard the other on purpose. Caching the second would
    /// make the number of draws per call depend on how many times `gaussian`
    /// had been called before, which is exactly the kind of hidden stream
    /// dependency the stability contract has to rule out.
    pub fn gaussian(&mut self) -> f32 {
        let u1 = self.next_f32().max(GAUSSIAN_EPSILON);
        let u2 = self.next_f32();
        (-2.0 * u1.ln()).sqrt() * (core::f32::consts::TAU * u2).cos()
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-rng gaussian`
Expected: PASS, 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rng/src/lib.rs
git commit -m "Add Rng::gaussian via Box-Muller

Returns one normal per call and discards the second, so the draw count
per call is constant and the frozen stream stays predictable."
```

---

### Task 6: Freeze the stream with test vectors

**Files:**
- Create: `crates/rng/tests/vectors.rs`

**Interfaces:**
- Consumes: all six methods from Tasks 1–5.
- Produces: nothing consumed by later tasks. This task exists to make Tasks 7 and 8 unable to change the stream unnoticed.

These vectors are generated once from the implementation and then frozen. That
is the point: they are a characterisation test, not a derivation. Generate them,
paste them in, and from then on they fail loudly if the stream ever moves.

- [ ] **Step 1: Generate the vectors**

Create `crates/rng/tests/vectors.rs` with this temporary generator, run it, and keep the printed output:

```rust
use semblance_rng::Rng;

#[test]
fn print_vectors() {
    for seed in [0u64, 1, 42, 0x9E37_79B9_7F4A_7C15, u64::MAX] {
        let mut r = Rng::seed_from_u64(seed);
        let u: Vec<String> = (0..4).map(|_| format!("{}", r.next_u64())).collect();
        let mut r = Rng::seed_from_u64(seed);
        let f: Vec<String> = (0..4).map(|_| format!("{:?}", r.next_f32())).collect();
        let mut r = Rng::seed_from_u64(seed);
        let b: Vec<String> = (0..4).map(|_| format!("{}", r.below(100))).collect();
        let mut r = Rng::seed_from_u64(seed);
        let g: Vec<String> = (0..4).map(|_| format!("{:?}", r.gaussian())).collect();
        println!("({seed}u64, [{}], [{}], [{}], [{}]),",
                 u.join(", "), f.join(", "), b.join(", "), g.join(", "));
    }
}
```

Run: `cargo test -p semblance-rng --test vectors -- --nocapture`
Copy the five printed tuples.

- [ ] **Step 2: Replace the generator with the frozen test**

Overwrite `crates/rng/tests/vectors.rs` with the following, pasting the captured
tuples into `VECTORS` in place of the generator's output. Keep the tuple order:
`(seed, next_u64 x4, next_f32 x4, below(100) x4, gaussian x4)`.

```rust
//! Frozen stream vectors — the executable half of the stability contract in
//! the crate docs.
//!
//! These values were generated once from the implementation and then frozen.
//! If a change to `semblance-rng` makes this file fail, that change altered
//! the stream and is a breaking change: either revert it, or bump the major
//! version and regenerate these vectors deliberately.

use semblance_rng::Rng;

type Vector = (u64, [u64; 4], [f32; 4], [usize; 4], [f32; 4]);

const VECTORS: &[Vector] = &[
    // PASTE THE FIVE GENERATED TUPLES HERE
];

#[test]
fn stream_matches_frozen_vectors() {
    assert_eq!(VECTORS.len(), 5, "expected one vector per pinned seed");

    for &(seed, u64s, f32s, belows, gaussians) in VECTORS {
        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in u64s.iter().enumerate() {
            assert_eq!(r.next_u64(), want, "next_u64 #{i} changed for seed {seed:#x}");
        }

        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in f32s.iter().enumerate() {
            assert_eq!(r.next_f32(), want, "next_f32 #{i} changed for seed {seed:#x}");
        }

        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in belows.iter().enumerate() {
            assert_eq!(r.below(100), want, "below #{i} changed for seed {seed:#x}");
        }

        let mut r = Rng::seed_from_u64(seed);
        for (i, &want) in gaussians.iter().enumerate() {
            assert_eq!(r.gaussian(), want, "gaussian #{i} changed for seed {seed:#x}");
        }
    }
}
```

- [ ] **Step 3: Run the frozen test to verify it passes**

Run: `cargo test -p semblance-rng --test vectors`
Expected: PASS, 1 test. If it fails, the paste is wrong — regenerate rather than editing values by hand.

- [ ] **Step 4: Verify the test actually detects drift**

Temporarily change the rotate constant in `next_u64` from `.rotate_left(23)` to
`.rotate_left(24)` in `crates/rng/src/lib.rs`.

Run: `cargo test -p semblance-rng --test vectors`
Expected: FAIL on the very first `next_u64` assertion. A test that cannot fail is
not protecting anything — this step confirms it can.

Then revert to `.rotate_left(23)` and re-run to confirm PASS.

Do NOT use `GAUSSIAN_EPSILON` for this check. Perturbing it from `1e-7` to `1e-6`
looks like it should work but does not: `next_f32` has 24-bit resolution, so a
draw small enough for the floor to bite turns up roughly once in a million, and
these vectors take twenty draws. The test would pass and tell you nothing.

- [ ] **Step 5: Commit**

```bash
git add crates/rng/tests/vectors.rs
git commit -m "Freeze the semblance-rng stream with test vectors

Pins next_u64, next_f32, below and gaussian for five seeds including
the two that broke the previous generator. Verified the test fails when
the stream moves."
```

---

### Task 7: Migrate `connectome` off its private `Lcg`

**Files:**
- Modify: `crates/connectome/Cargo.toml`
- Modify: `crates/connectome/src/lib.rs` (delete `Lcg` at lines 178-199; rewrite `synthetic()` at lines 140-155)

**Interfaces:**
- Consumes: `Rng::seed_from_u64`, `Rng::below`, `Rng::next_f32`.
- Produces: no signature change. `Connectome::synthetic(num_neurons, avg_out_degree, seed)` keeps its exact signature; only the graph it returns changes.

- [ ] **Step 1: Add the dependency**

In `crates/connectome/Cargo.toml`, replace the empty `[dependencies]` table contents with:

```toml
[dependencies]
semblance-rng.workspace = true
```

- [ ] **Step 2: Write the failing test**

Add to the `tests` module in `crates/connectome/src/lib.rs`:

```rust
    /// Regression: the private `Lcg` this crate used to carry XORed its seed
    /// with 0x9E3779B97F4A7C15, so that exact seed zeroed its state and every
    /// draw returned 0 forever. The graph came out with every synapse
    /// targeting neuron 0 at weight -1.0, silently.
    #[test]
    fn synthetic_survives_the_seed_that_broke_the_old_rng() {
        let c = Connectome::synthetic(64, 4, 0x9E37_79B9_7F4A_7C15);

        let targets: std::collections::HashSet<NeuronId> =
            c.synapses().iter().map(|s| s.post).collect();
        assert!(
            targets.len() > 1,
            "every synapse landed on one neuron: the generator is degenerate"
        );

        let weights = c.weights();
        assert!(
            weights.windows(2).any(|w| w[0] != w[1]),
            "every weight is identical: the generator is degenerate"
        );
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p semblance-connectome synthetic_survives`
Expected: FAIL — the assertion trips, because `Lcg` is still in use and still degenerate for this seed.

- [ ] **Step 4: Rewrite `synthetic()` and delete `Lcg`**

Add to the imports at the top of `crates/connectome/src/lib.rs`:

```rust
use semblance_rng::Rng;
```

Replace the body of `synthetic()` with:

```rust
    pub fn synthetic(num_neurons: usize, avg_out_degree: usize, seed: u64) -> Connectome {
        let mut rng = Rng::seed_from_u64(seed);
        let mut c = Connectome::new(num_neurons);
        if num_neurons == 0 {
            return c;
        }
        for pre in 0..num_neurons {
            for _ in 0..avg_out_degree {
                let post = rng.below(num_neurons);
                // weights in [-1, 1]; sign stands in for excitatory/inhibitory.
                let w = (rng.next_f32() * 2.0) - 1.0;
                c.add_synapse(pre as NeuronId, post as NeuronId, w);
            }
        }
        c
    }
```

Note `rng.below(num_neurons)` replaces `(rng.next_u64() as usize) % num_neurons`, removing the modulo bias toward low-numbered neurons.

Then delete the entire `Lcg` struct and its `impl` block (the `/// Tiny deterministic RNG...` doc comment through the closing brace of `impl Lcg`, lines 178-199).

- [ ] **Step 5: Run the full crate test suite**

Run: `cargo test -p semblance-connectome`
Expected: PASS, 5 tests. The four pre-existing tests assert shape (`num_neurons`, `num_synapses`) and parsing, never specific edges or weights, so they are unaffected by the new stream.

- [ ] **Step 6: Commit**

```bash
git add crates/connectome/Cargo.toml crates/connectome/src/lib.rs Cargo.lock
git commit -m "Move connectome onto semblance-rng

Deletes the private Lcg, which was misnamed (it was xorshift64, not an
LCG) and had an absorbing zero state: seed 0x9E3779B97F4A7C15 produced
an all-zero stream and a graph where every synapse targeted neuron 0 at
weight -1.0, with no error.

synthetic() also picks targets with below() rather than modulo, removing
a bias toward low-numbered neurons."
```

---

### Task 8: Migrate `evo` and drop `rand` from the workspace

**Files:**
- Modify: `crates/evo/Cargo.toml`
- Modify: `crates/evo/src/lib.rs` (imports at 20-22; `Population.rng` field at 70; `seeded()` at 77-78; parent selection at 118; `perturb()` at 134-140; delete `gaussian()` at 142-147)
- Modify: `Cargo.toml` (remove `rand`)

**Interfaces:**
- Consumes: `Rng::seed_from_u64`, `Rng::below`, `Rng::chance`, `Rng::gaussian`.
- Produces: no signature change. `Population::seeded(seed, cfg, rng_seed)` keeps its exact signature.

- [ ] **Step 1: Swap the dependency**

In `crates/evo/Cargo.toml`, replace `rand.workspace = true` with:

```toml
semblance-rng.workspace = true
```

In the root `Cargo.toml`, delete the line `rand = "0.10"` from `[workspace.dependencies]`.

- [ ] **Step 2: Write the failing test**

Add to the `tests` module in `crates/evo/src/lib.rs`:

```rust
    #[test]
    fn seeded_populations_are_reproducible() {
        let seed_latent = vec![0.5f32; 32];
        let cfg = EvoConfig::default();

        let a = Population::seeded(&seed_latent, cfg.clone(), 7);
        let b = Population::seeded(&seed_latent, cfg, 7);

        let za: Vec<&Latent> = a.individuals.iter().map(|i| &i.genome.z).collect();
        let zb: Vec<&Latent> = b.individuals.iter().map(|i| &i.genome.z).collect();
        assert_eq!(za, zb, "same rng_seed must produce the same population");
    }
```

If `individuals` is private and the test cannot reach it, make the field
`pub(crate)` rather than adding a public accessor — the test lives in the same
crate and nothing outside it needs the field.

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p semblance-evo seeded_populations`
Expected: FAIL to compile — `rand` is gone from the manifest, so `use rand::...` cannot resolve.

- [ ] **Step 4: Rewrite the `rand` call sites**

Replace the two import lines (`use rand::rngs::StdRng;` and `use rand::{RngExt, SeedableRng};`) with:

```rust
use semblance_rng::Rng;
```

Change the `Population` field at line 70 from `rng: StdRng,` to:

```rust
    rng: Rng,
```

In `seeded()`, change `let mut rng = StdRng::seed_from_u64(rng_seed);` to:

```rust
        let mut rng = Rng::seed_from_u64(rng_seed);
```

At line 118, change `self.rng.random_range(0..elite)` to:

```rust
            let parent = &self.individuals[self.rng.below(elite)];
```

Replace `perturb()` entirely with:

```rust
/// Gaussian perturbation of a latent, in place. Each gene is perturbed with
/// probability `rate` by N(0, std).
fn perturb(z: &mut Latent, rate: f32, std: f32, rng: &mut Rng) {
    for g in z.iter_mut() {
        if rng.chance(rate) {
            *g += rng.gaussian() * std;
        }
    }
}
```

Then delete the local `gaussian()` function entirely (its doc comment through its closing brace) — `semblance-rng` owns that now.

- [ ] **Step 5: Run the full crate test suite**

Run: `cargo test -p semblance-evo`
Expected: PASS, 2 tests.

- [ ] **Step 6: Verify the workspace has no external dependencies**

Run: `cargo tree --workspace --edges normal`
Expected: only `semblance*` crates appear. No `rand`, `getrandom`, `libc`, `ppv-lite86`, `zerocopy`, or any other third-party name.

Run: `grep -rn "rand" Cargo.toml crates/*/Cargo.toml app/Cargo.toml`
Expected: no matches.

- [ ] **Step 7: Run the whole workspace and the CLI**

Run: `cargo test --workspace`
Expected: PASS. Count the totals — you should have 11 pre-existing tests plus the ones added in Tasks 1-8.

Run: `cargo run --quiet -- roundtrip`
Expected: `structural distance` and `behavioural distance` both `0.000000`. Losslessness is a property of the codec, not of the graph, so the new stream must not change it.

Run: `cargo run --quiet -- demo` and `cargo run --quiet -- evolve`
Expected: both succeed. Their printed numbers WILL differ from before — a different generator means a different synthetic graph and a different mutation stream. That is expected, not a regression.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates/evo
git commit -m "Move evo onto semblance-rng and drop rand

Replaces StdRng with semblance-rng and deletes the local Box-Muller,
which now lives in the rng crate. Parent selection uses below() rather
than random_range, removing a modulo bias.

This removes the workspace's last external dependency: cargo tree now
shows only our own crates.

Note: the synthetic graph and the mutation stream both change, so demo
and evolve print different numbers than before. roundtrip still reports
exactly 0.000000, because losslessness is a property of the codec."
```

---

## Verification checklist

Run from the workspace root after Task 8:

- [ ] `cargo build --workspace` — clean
- [ ] `cargo test --workspace` — all pass, zero failures
- [ ] `cargo clippy --workspace --all-targets` — no NEW warnings beyond the 5 pre-existing ones (4 `needless_range_loop` in `sim`, 1 manual char comparison in `connectome`)
- [ ] `cargo tree --workspace --edges normal` — only `semblance*` crates
- [ ] `cargo run --quiet -- roundtrip` — both distances `0.000000`
- [ ] `cargo publish --dry-run -p semblance-rng` — passes (requires a clean working tree)

## Out of scope

Deliberately NOT part of this plan, and not to be added by an implementer:

- Publishing to crates.io. The version bumps this work implies (a workspace-wide 0.2.0, plus `semblance-rng` at 0.1.0) are an unresolved release decision recorded in the spec.
- Any change to `semblance-sim`, `semblance-codec`, `semblance-fidelity`, or `app`. None of them draw random numbers today.
- The `ReducedCodec` and probe-battery research seams.
- The 5 pre-existing clippy warnings.
