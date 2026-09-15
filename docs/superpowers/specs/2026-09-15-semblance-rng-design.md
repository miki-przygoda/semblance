# `semblance-rng` — one frozen, auditable random source

**Date:** 2026-09-15
**Status:** approved, not yet implemented

## Why

The project draws random numbers in two places, from two different generators,
with two different reproducibility stories.

`crates/connectome/src/lib.rs` carries a private 22-line generator whose own
doc comment describes the split:

> Tiny deterministic RNG (xorshift-ish LCG). Kept local so this crate has zero
> dependencies; the evolution crate uses the real `rand` instead.

`crates/evo` uses `rand::StdRng`. Three problems follow.

### 1. `rand` moved the stream underneath us

`rand` 0.9 changed `StdRng`'s algorithm. Upgrading 0.8 → 0.10 (commit `0666bc7`)
therefore reset the project's reproducibility baseline: a given seed no longer
reproduces pre-upgrade runs. For a project whose headline deliverable is a
reproducible size-vs-fidelity curve, an RNG that can change between minor
releases of a third-party crate is a standing liability.

### 2. The local generator is misnamed

`x ^= x << 13; x ^= x >> 7; x ^= x << 17` is Marsaglia's **xorshift64**. There
is no linear congruential step in it. The name `Lcg` and the comment
"xorshift-ish LCG" describe two different algorithms, neither of them accurately.

### 3. The local generator silently degenerates on one seed

`Lcg::new` computes `seed ^ 0x9E3779B97F4A7C15`. Seeding with exactly
`0x9E3779B97F4A7C15` zeroes the state, and xorshift cannot leave zero. Verified
by running the code as written:

```
seed 0x9E3779B97F4A7C15:
  first 6 next_u64 : [0, 0, 0, 0, 0, 0]
  first 6 next_f32 : [0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
```

The failure is silent and total. `Connectome::synthetic(n, d, 0x9E3779B97F4A7C15)`
returns a graph in which every synapse targets neuron 0 with weight exactly
`-1.0`, because `next_u64() % n == 0` and `0.0 * 2.0 - 1.0 == -1.0`. Nothing
panics. The pipeline will measure fidelity against it without complaint.

Separately, `synthetic()` picks targets with `(rng.next_u64() as usize) % num_neurons`,
which is modulo-biased toward low-numbered neurons. `evo` has the same bias in
its parent selection. The bias is small for typical sizes, but it is baked into
the test substrate.

## Goals

- One generator, used everywhere, so a seed means one thing across the workspace.
- A stream that is **frozen by contract** and guarded by committed test vectors.
- No absorbing states, for any seed.
- Unbiased integer range sampling.
- No external dependencies.

## Non-goals

- Cryptographic security. Nothing here is adversarial.
- Swappable generators. A trait would imply the algorithm can change; it cannot.
- Parallel stream splitting. No call site needs it. Revisit if the evolution
  loop is ever parallelised.

## Algorithm

**SplitMix64 for seeding, xoshiro256++ for the stream.**

`seed_from_u64` runs SplitMix64 four times to fill xoshiro256++'s 256-bit state:

```
z  = (x += 0x9E3779B97F4A7C15)
z  = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
z  = (z ^ (z >> 27)) * 0x94D049BB133111EB
out = z ^ (z >> 31)
```

Note the irony worth preserving in a comment: `0x9E3779B97F4A7C15` is the same
golden-ratio constant the old `Lcg` misused as an XOR mask. Here it is an
*additive increment on a counter*, which is precisely why SplitMix64 cannot
stall — it has no state to get trapped in.

The stream is stock xoshiro256++:

```
result = rotl(s[0] + s[3], 23) + s[0]
t      = s[1] << 17
s[2] ^= s[0];  s[3] ^= s[1];  s[1] ^= s[2];  s[0] ^= s[3]
s[2] ^= t
s[3]  = rotl(s[3], 45)
```

Period 2^256 − 1. Passes BigCrush, TestU01's hardest battery. Used by `rand`'s
own `SmallRng` on 64-bit targets, so the stream quality is independently studied
rather than something we are asserting about our own code.

A zero state is unreachable: it would require all four SplitMix64 outputs to be
zero simultaneously, and SplitMix64 is a bijection over a counter.

## API

Six methods. Each one exists because a current call site needs it.

```rust
pub struct Rng { s: [u64; 4] }

impl Rng {
    pub fn seed_from_u64(seed: u64) -> Self;
    pub fn next_u64(&mut self) -> u64;
    pub fn next_f32(&mut self) -> f32;           // [0, 1)
    pub fn below(&mut self, n: usize) -> usize;  // [0, n), unbiased
    pub fn chance(&mut self, p: f32) -> bool;
    pub fn gaussian(&mut self) -> f32;           // N(0, 1)
}
```

| Method | Replaces | Caller |
|---|---|---|
| `seed_from_u64` | `Lcg::new`, `StdRng::seed_from_u64` | both |
| `next_u64` | `Lcg::next_u64` | primitive / extension point |
| `next_f32` | `Lcg::next_f32`, `rng.random::<f32>()` | connectome weights, evo rate draw |
| `below` | `% num_neurons`, `random_range(0..elite)` | connectome targets, evo parents |
| `chance` | `random::<f32>() < rate` | evo mutation rate |
| `gaussian` | evo's local Box–Muller | evo perturbation |

### `next_f32`

Top 24 bits over 2^24, matching f32's mantissa width:
`(x >> 40) as f32 / (1u64 << 24) as f32`. Yields `[0, 1)`. This is what the
existing `Lcg` does and it is correct, so the conversion carries over unchanged.
The resulting values differ regardless, because the `u64` feeding it comes from a
different generator.

### `below`

Rejection sampling, not Lemire's multiply:

```rust
let zone = (u64::MAX / n) * n;   // largest multiple of n that fits
loop {
    let x = self.next_u64();
    if x < zone { return (x % n) as usize; }
}
```

Marginally slower than Lemire's, and obviously unbiased on inspection. When the
behaviour is frozen permanently, auditability beats a few nanoseconds.

`below(0)` panics — an empty range has no valid answer, and returning 0 would
hide the caller's bug. `below(1)` returns 0 and still consumes one draw: the loop
is not special-cased, so stream position stays predictable from the sequence of
calls alone.

### `chance`

`self.next_f32() < p`. Correct at both ends without special-casing: `p <= 0.0`
is never true because `next_f32() >= 0.0`, and `p >= 1.0` is always true because
`next_f32() < 1.0`.

### `gaussian`

Box–Muller, returning **one** value per call and discarding the second:

```rust
let u1 = self.next_f32().max(GAUSSIAN_EPSILON);  // ln(0) guard
let u2 = self.next_f32();
(-2.0 * u1.ln()).sqrt() * (core::f32::consts::TAU * u2).cos()
```

The guard is necessary: `next_f32` can return exactly `0.0`, and `ln(0)` is
`-inf`. `GAUSSIAN_EPSILON` is a named constant (`1e-7`, matching the value evo
already uses) rather than a literal, because it is part of the frozen contract.

Box–Muller naturally produces two normals per pair of uniforms. We discard the
second deliberately. Caching it would make the number of `next_u64` draws per
`gaussian()` call depend on how many times `gaussian()` had been called before —
exactly the kind of hidden stream dependency that makes a freeze hard to honour.
Constant cost per call is worth one wasted cosine.

## Stability contract

This is the crate's reason for existing, and it goes in the module docs:

> The stream produced by a given seed is frozen. Any change to it — algorithm,
> constants, draw counts per method, or the order of draws within a method — is
> a breaking change requiring a major version bump. The test vectors in
> `tests/vectors.rs` exist to make an accidental change fail loudly.

Test vectors pin exact outputs for a fixed set of seeds across every method.
They are the executable half of the promise; the prose above is the other half.

## Migration

| File | Change |
|---|---|
| `crates/rng/` | new crate |
| `Cargo.toml` | add `crates/rng` to members; drop `rand`; add `semblance-rng` |
| `crates/connectome/Cargo.toml` | add `semblance-rng` |
| `crates/connectome/src/lib.rs` | delete `Lcg` (22 lines); `synthetic()` uses `below()` and `next_f32()` |
| `crates/evo/Cargo.toml` | `rand` → `semblance-rng` |
| `crates/evo/src/lib.rs` | delete local Box–Muller (12 lines); use `chance()`, `below()`, `gaussian()` |

After this the workspace has **zero external dependencies** — `cargo tree` shows
nothing but our own crates.

### What this does and does not break

No existing test breaks. Every connectome test asserts shape (`num_neurons`,
`num_synapses`), never specific edges or weights. The fidelity tests assert
invariants (`== 0.0` for a lossless round-trip, `> 0.0` for a perturbed one) that
hold for any substrate. The evo test asserts that fitness improves.

What does change is the *content* of `synthetic()` output, and therefore the
numbers `demo`, `roundtrip`, and `evolve` print. `roundtrip` will still report
exactly `0.000000`, because losslessness is a property of the codec, not the graph.

### Release implications

All six crates are published at 0.1.0. This work changes observable behaviour in
`connectome` and `evo`, which under 0.x semver means 0.2.0 for those, and version
bumps for everything that depends on them — effectively a workspace-wide 0.2.0,
plus `semblance-rng` entering at 0.1.0.

Worth deciding separately: 0.2.0 was previously earmarked for the first working
`ReducedCodec`. Either this work takes 0.2.0 and `ReducedCodec` takes 0.3.0, or
publishing waits until both have landed. Not a blocker for implementation.

## Testing

- **Frozen vectors** — fixed seeds, exact expected outputs, every method.
- **Pathological seeds** — `0`, `0x9E3779B97F4A7C15` (the seed that broke the old
  generator), and `u64::MAX` all produce healthy, varied streams.
- **`below` is unbiased** — bucket counts over many draws stay within tolerance;
  `below(1) == 0`; `below(0)` panics.
- **`next_f32` bounds** — every draw in `[0, 1)` over a large sample.
- **`gaussian` moments** — mean ≈ 0, standard deviation ≈ 1 over a large sample.
- **Determinism** — same seed reproduces the sequence exactly; different seeds
  diverge.

## Publishing

Seventh crate, same manifest metadata as the rest (inherited from
`[workspace.package]`), name confirmed free on crates.io.
