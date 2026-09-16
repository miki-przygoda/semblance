# semblance

A Rust scaffold for one idea: **take a real fly connectome, compress it into a
small latent code, evolve in that compressed space, and measure how faithfully
the decoded brain still *behaves* like the original.**

A *semblance* is the outward appearance of being something — looking and acting
like the original without being identical underneath. That is exactly what this
project measures: two different weightings can behave the same, so the test is
behavioural, not structural.

This repo is a clean starting point, not a finished system. The plumbing —
graph representation, a working spiking simulator, an evolution loop, a fidelity
harness, and a lossless codec baseline — is built and tested so you can go
straight to the parts that are actually research: **the reduced encoding** and
**the behavioural-fidelity metric**.

## The thesis in one line

Every ingredient already exists in the literature (connectome embeddings;
indirect/generative encodings like HyperNEAT; evolving in latent space). What's
underexplored is assembling them **on a real, measured connectome and judging the
round-trip behaviourally rather than structurally**. See `DESIGN.md` for the gap,
`RESOURCES.md` for the (link-checked) prior art.

## Quickstart

Requires **Rust 1.85 or newer** — the workspace is on edition 2024.
Check with `rustc --version`; update with `rustup update`. The workspace has
no external dependencies — everything it needs ships in the standard library
plus the crates in this repo.

```bash
cargo build            # builds the whole workspace
cargo test             # all crates green
cargo run -- demo      # run the LIF simulator once on a synthetic connectome
cargo run -- roundtrip # lossless encode->decode, report structural + behavioural fidelity
cargo run -- evolve    # seed from the connectome latent, run a toy evolution loop
```

Everything runs on a **synthetic** connectome out of the box, so you don't need to
download anything to see the pipeline work. Wire in real data when you're ready
(`RESOURCES.md` → "Getting the data").

## What runs today vs. what's yours

| Crate | Built and working | Left for you (the research) |
|-------|-------------------|------------------------------|
| `rng` | `semblance-rng` — SplitMix64-seeded xoshiro256++, frozen stream pinned by test vectors | — |
| `connectome` | graph type, CSR view, edge-list loader, synthetic generator | load a real FlyWire/MaleCNS export |
| `sim` | LIF neurons, decaying-current synapses, tick loop | sensory encoding + motor decoding ("make it act like an ant") |
| `codec` | `WeightCodec` — lossless baseline, directly evolvable | `ReducedCodec` — the actual compression (`todo!()`) |
| `evo` | gaussian mutation + truncation selection + elitism, seedable | your `Fitness` (environment + reward) |
| `fidelity` | structural + behavioural distance, size-vs-fidelity curve harness | the **probe battery** design |
| `app` | CLI wiring the three flows together | — |

The two places to start are marked with `todo!()` and `TODO(you)` in
`crates/codec/src/lib.rs` and the probe-battery note in
`crates/fidelity/src/lib.rs`.

## Design notes

- **`Latent = Vec<f32>`** everywhere on purpose: the thing the codec produces is
  exactly the thing evolution mutates. "Evolve in the compressed space" = mutate
  the latent, then `decode` to get a network to score.
- **`WeightCodec` is the anchor, not the goal.** It stores topology and treats the
  weight vector as the latent, so it round-trips exactly (fidelity = 0 distance)
  and is trivially evolvable. It's your control point at
  `(latent_dim = num_synapses, fidelity = perfect)`. `ReducedCodec` is where you
  push `latent_dim` down and watch fidelity fall — that curve is the result.
- **Behaviour, not structure, is the metric.** Two different weightings can behave
  identically; `fidelity::structural_distance` is only a fast sanity check.
  `behavioural_distance` replays a probe battery through both networks and compares
  what they *do*.

## License

Apache-2.0 — see [LICENSE](LICENSE).

Copyright 2026 Mikolaj Mikuliszyn.
