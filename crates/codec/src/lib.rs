//! # semblance-codec
//!
//! The round-trip you sketched: `connectome -> latent -> connectome`. This crate
//! is the heart of the project, and it's deliberately the least filled-in, because
//! the interesting research is the encoding you choose.
//!
//! A [`Latent`] is a flat `Vec<f32>` on purpose: it's exactly what the evolution
//! crate mutates. So "evolve in the compressed space" = mutate the latent, then
//! `decode` to get a network to evaluate.
//!
//! What is DONE here:
//! - the [`Codec`] trait (the seam everything else is written against)
//! - [`WeightCodec`], a **lossless** baseline: fixed topology, latent = the weight
//!   vector. This is your fidelity anchor (round-trips exactly) and is directly
//!   evolvable (mutate weights on real fly wiring). Use it to get the whole
//!   pipeline green before you compress anything.
//!
//! What is YOURS to fill in — this is the project:
//! - [`ReducedCodec`]: a latent SMALLER than the weight count. This is where the
//!   size-vs-fidelity trade-off you care about actually happens. Ideas to try, in
//!   rough order of effort (see `RESOURCES.md` for the papers behind each):
//!     * linear projection / PCA-style basis over the weight vector
//!     * a learned autoencoder (train offline, load weights here)
//!     * an *indirect* / generative encoding (HyperNEAT/CPPN-style): store a tiny
//!       function that GROWS the wiring from neuron coordinates. Best fit for
//!       "small genome, big brain, inspect the full network any time".
//! - non-uniform budgets: compress redundant structure hard, keep load-bearing
//!   parts near-exact (this is how you get faithful AND small rather than a
//!   uniform blur).

use semblance_connectome::{Connectome, NeuronId};

/// A compact, evolvable representation of a connectome.
pub type Latent = Vec<f32>;

/// Encode a connectome to a latent code and decode it back.
///
/// The contract you're implicitly optimising:
/// - `decode(encode(c))` should behave like `c` (measured in `semblance-fidelity`)
/// - `latent_dim` should be much smaller than `c.num_synapses()` to be worth it
/// - `decode` must be cheap enough to call once per fitness evaluation
pub trait Codec {
    /// Size of the latent this codec produces for `c`. This is the "size" axis of
    /// your size-vs-fidelity curve.
    fn latent_dim(&self, c: &Connectome) -> usize;
    /// Compress.
    fn encode(&self, c: &Connectome) -> Latent;
    /// Reconstruct. For codecs with a fixed substrate (topology held in the
    /// codec), this rebuilds weights onto that substrate.
    fn decode(&self, z: &Latent) -> Connectome;
}

/// Lossless baseline. The codec stores the topology (pre/post pairs + neuron
/// count); the latent is just the weights, in edge order. `decode(encode(c)) == c`.
///
/// This is not compression — it's the control that anchors your curve at
/// (latent_dim = num_synapses, fidelity = 1.0). Start here.
#[derive(Clone, Debug)]
pub struct WeightCodec {
    num_neurons: usize,
    topology: Vec<(NeuronId, NeuronId)>,
}

impl WeightCodec {
    /// Fix the substrate from a reference connectome (typically the real fly).
    pub fn from_connectome(c: &Connectome) -> Self {
        let topology = c.synapses().iter().map(|s| (s.pre, s.post)).collect();
        WeightCodec { num_neurons: c.num_neurons(), topology }
    }

    fn rebuild(&self, weights: &[f32]) -> Connectome {
        assert_eq!(weights.len(), self.topology.len(), "latent dim mismatch");
        let mut c = Connectome::new(self.num_neurons);
        for (&(pre, post), &w) in self.topology.iter().zip(weights) {
            c.add_synapse(pre, post, w);
        }
        c
    }
}

impl Codec for WeightCodec {
    fn latent_dim(&self, _c: &Connectome) -> usize {
        self.topology.len()
    }
    fn encode(&self, c: &Connectome) -> Latent {
        c.weights()
    }
    fn decode(&self, z: &Latent) -> Connectome {
        self.rebuild(z)
    }
}

/// Skeleton for a genuinely-compressing codec: `latent_dim < num_synapses`.
///
/// The struct holds whatever your scheme needs (a fixed substrate to decode onto,
/// a projection basis, learned decoder weights, a CPPN, ...). Fields left open on
/// purpose. Fill in `encode`/`decode` and you have a research result: plot the
/// fidelity of `decode(encode(c))` as you shrink `latent`.
pub struct ReducedCodec {
    /// Fixed substrate to decode weights onto (one simple design choice). Keep or
    /// replace depending on whether you also compress topology.
    pub substrate: WeightCodec,
    /// Target latent size — the knob you sweep for the size-vs-fidelity curve.
    pub latent_dim: usize,
    // TODO(you): projection matrices / autoencoder params / CPPN genome / ...
}

impl ReducedCodec {
    pub fn new(reference: &Connectome, latent_dim: usize) -> Self {
        ReducedCodec {
            substrate: WeightCodec::from_connectome(reference),
            latent_dim,
        }
    }
}

impl Codec for ReducedCodec {
    fn latent_dim(&self, _c: &Connectome) -> usize {
        self.latent_dim
    }

    fn encode(&self, _c: &Connectome) -> Latent {
        // TODO(you): compress the weight vector (and maybe topology) down to
        // `self.latent_dim` numbers. Simplest first step: a fixed random or
        // PCA projection of `_c.weights()`.
        todo!("implement your reduced encoding")
    }

    fn decode(&self, _z: &Latent) -> Connectome {
        // TODO(you): expand `_z` back to a full weight vector, then
        // `self.substrate.decode(&full_weights)`.
        todo!("implement your reduced decoding")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_connectome::Connectome;

    #[test]
    fn weight_codec_is_lossless() {
        let c = Connectome::synthetic(80, 5, 11);
        let codec = WeightCodec::from_connectome(&c);
        let z = codec.encode(&c);
        assert_eq!(z.len(), c.num_synapses());
        let back = codec.decode(&z);
        assert_eq!(back.synapses(), c.synapses());
    }

    #[test]
    fn latent_dim_matches_synapses_for_weight_codec() {
        let c = Connectome::synthetic(40, 4, 2);
        let codec = WeightCodec::from_connectome(&c);
        assert_eq!(codec.latent_dim(&c), c.num_synapses());
    }
}
