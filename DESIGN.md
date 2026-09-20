# Design & research framing

This document is the "why" behind the scaffold: what the project is really
claiming, where the open problem is, and how each crate maps onto it.

## The idea, precisely

1. Start from a **real connectome** (fly — FlyWire female brain or Janelia
   MaleCNS). It's a static, weighted, directed graph of ~10^5 neurons.
2. **Compress** it: `encode : Connectome -> Latent`, with `|Latent| ≪ num_synapses`.
3. **Evolve in the compressed space**: mutate/select over `Latent`, seeding the
   initial population from the real connectome's own latent so evolution starts
   from working wiring instead of noise.
4. **Decode** any latent back to a full network to inspect or simulate it.
5. **Judge the round-trip behaviourally**: `decode(encode(c))` should *act* like
   `c` under a battery of probes. Plot fidelity against latent size.

The headline deliverable is not "an evolved fly". It's the **size-vs-fidelity
curve**: how much can you shrink the representation before behaviour degrades, and
where. Evolution is what the compact space is *for*, and bolts on cleanly once the
encode/decode/measure loop exists.

## Why this is worth doing (the gap)

Each piece is established; the combination on a measured connectome, judged
behaviourally, is not. From the link-checked survey in `RESOURCES.md`:

- **Connectome compression / low-dim structure exists.** Connectome embedding
  spaces, and the "hourglass" result that biological circuits funnel through a
  low-dimensional core, both say the wiring has far fewer effective degrees of
  freedom than edges. So compression *should* be possible without wrecking
  function.
- **Indirect / generative encoding is a whole field.** HyperNEAT/CPPN, GENE,
  HyperNCA, developmental encodings — all are "small genome grows a big network".
  That's precisely your compress→decode step, already benchmarked, just never fed a
  traced brain as the thing to reproduce.
- **Evolving in latent space is live** (e.g. Evolutionary Planning in Latent
  Space; VAE-based model compression). Also never applied to a real connectome.
- **The fly-connectome modelling work goes the other way.** `flyvis` keeps the
  wiring *fixed* and optimises weights to predict recorded activity; embodied
  models (`flybody`) attach a body but don't compress or evolve the wiring.

Nobody (as far as the search went — you can't prove a negative) has done
"**seed from real fly wiring → compress → evolve in the latent → measure
behavioural round-trip fidelity**". That's the sentence your project owns.

## The two hard parts (deliberately left to you)

Everything else is plumbing. These two are the research, so the scaffold gives you
clean seams and gets out of the way.

### 1. The reduced encoding — `crates/codec/src/lib.rs::ReducedCodec`

The knob is `latent_dim`. Options, roughly increasing in effort and payoff:

- **Linear projection / PCA** over the weight vector. Cheapest; a good first
  point on the curve and a sanity check that the harness works end to end.
- **Learned autoencoder** — train offline (Python is fine), load the decoder
  weights here, decode in Rust. Non-linear, usually much better fidelity per bit.
- **Indirect / generative encoding** (CPPN/HyperNEAT-style): store a tiny function
  that *generates* connectivity from neuron coordinates. Best fit for "small
  genome, big brain, inspect the full network any time", and the most likely to
  give a striking compression ratio.

A note you already arrived at: don't spend the latent budget uniformly. Compress
redundant structure hard and keep load-bearing structure near-exact — that's how
you get *faithful and small* rather than a uniform blur. Non-uniform allocation is
itself a result worth showing.

### 2. The behavioural-fidelity metric — `crates/fidelity/src/lib.rs`

`compare_behaviour` is provided (motor-channel MSE + a rate term). The open
decision is the **probe battery**: the set of stimuli you replay through both
networks before comparing. A single stimulus is easy to fit and proves nothing; a
diverse, well-chosen battery is what makes "behaves the same" trustworthy. That
choice depends on what behaviour you care about, so it's yours to design
(`ProbeBattery` is the placeholder). This is the load-bearing decision in the whole
project — if the metric is weak, every fidelity number downstream is meaningless.

## Crate-by-crate

- **`rng`** — the workspace's one random source. SplitMix64 expands a `u64`
  seed into xoshiro256++'s state; the resulting stream is frozen and pinned by
  test vectors, so `connectome`'s `synthetic()` and `evo`'s mutation are both
  reproducible from a single seed. It's also what let this branch drop the
  workspace's last external dependency.
- **`connectome`** — the substrate, no dynamics. Edge list + CSR view. The
  `from_edge_list` loader is the seam for real data: most exports reduce to
  `pre,post,weight` plus a neuron table. `synthetic()` lets the whole pipeline run
  before you touch real data.
- **`flyvis`** — reads the published flyvis connectome spec (Lappalainen et al.,
  Nature 2024) faithfully: a private minimal JSON reader, a typed literal view of
  the spec, and flyvis's curated ground truth as compiled constants. No external
  dependencies. It reads the file; expanding it into a `connectome` graph is a
  separate, later step.
- **`sim`** — dynamics and the loop. LIF is implemented; synapses are current-based
  with exponential decay so sparse inputs summate (without that, recurrence is
  dead and weights don't matter). `Stimulus`/`Readout` are the input/output
  interface — the crate boundary where "repurpose this brain for a different body
  or world" actually happens.
- **`codec`** — the round-trip. `WeightCodec` (lossless) anchors the curve;
  `ReducedCodec` is the research.
- **`evo`** — genome = latent. Gaussian mutation, truncation selection, elitism,
  seedable from the real latent. Swap in CMA-ES or quality-diversity later without
  changing the trait boundary. `Fitness` is yours: decode → run in an environment
  → score.
- **`fidelity`** — structural distance (fast, not the target), behavioural distance
  (the target), and `size_fidelity_curve` (the experiment harness).

## Suggested first milestones

1. `cargo run -- roundtrip` on synthetic data — done, it's your green baseline.
2. Load one real FlyWire/MaleCNS subgraph via `from_edge_list`; confirm `roundtrip`
   still reports zero behavioural distance with `WeightCodec`.
3. Implement the PCA `ReducedCodec`; produce the first real `size_fidelity_curve`.
4. Design a real probe battery; re-measure. This is where the project earns its
   claim.
5. Only then: turn on `evolve` over the reduced latent with a real fitness.
