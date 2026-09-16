//! # semblance-connectome
//!
//! The substrate. A [`Connectome`] is just neurons + weighted directed synapses.
//! It carries no dynamics — that lives in `semblance-sim`. Keeping the wiring and the
//! dynamics separate is what lets you compress/evolve the wiring independently of
//! how you choose to simulate it.
//!
//! What is DONE here (usable as-is):
//! - a compact edge-list graph type with add/query helpers
//! - a CSR (compressed sparse row) view for a cache-friendly tick loop
//! - a deterministic synthetic-graph generator so you can develop before wiring real data
//! - an edge-list loader (`pre,post,weight` per line)
//!
//! What is YOURS to fill in later:
//! - loading a real FlyWire / MaleCNS export (see `RESOURCES.md`); most exports are
//!   an edge list plus a neuron table, so `from_edge_list` is the natural seam.
//! - per-neuron metadata (neurotransmitter sign, cell type) if your model needs it.

use std::io::{self, BufRead};

use semblance_rng::Rng;

/// Index of a neuron. `u32` keeps edge lists small; a fly brain is ~1.4e5 neurons.
pub type NeuronId = u32;

/// A directed, weighted connection `pre -> post`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Synapse {
    pub pre: NeuronId,
    pub post: NeuronId,
    pub weight: f32,
}

/// A wiring diagram. Edges are stored as a flat list; call [`Connectome::to_csr`]
/// when you want to iterate outgoing edges fast.
#[derive(Clone, Debug, Default)]
pub struct Connectome {
    num_neurons: usize,
    synapses: Vec<Synapse>,
}

impl Connectome {
    pub fn new(num_neurons: usize) -> Self {
        Self { num_neurons, synapses: Vec::new() }
    }

    pub fn num_neurons(&self) -> usize {
        self.num_neurons
    }

    pub fn num_synapses(&self) -> usize {
        self.synapses.len()
    }

    pub fn synapses(&self) -> &[Synapse] {
        &self.synapses
    }

    /// Add one edge. Grows the neuron count if an id is out of current range so
    /// that loaders don't have to know the count up front.
    pub fn add_synapse(&mut self, pre: NeuronId, post: NeuronId, weight: f32) {
        let max_id = pre.max(post) as usize + 1;
        if max_id > self.num_neurons {
            self.num_neurons = max_id;
        }
        self.synapses.push(Synapse { pre, post, weight });
    }

    /// The weight vector, in stored edge order. This is the natural thing a
    /// weight-only codec encodes (see `semblance-codec`).
    pub fn weights(&self) -> Vec<f32> {
        self.synapses.iter().map(|s| s.weight).collect()
    }

    /// Rebuild with the same topology but new weights (same length/order as
    /// [`weights`]). Panics on length mismatch — that mismatch is always a bug.
    pub fn with_weights(&self, weights: &[f32]) -> Connectome {
        assert_eq!(
            weights.len(),
            self.synapses.len(),
            "weight vector length must match synapse count"
        );
        let synapses = self
            .synapses
            .iter()
            .zip(weights)
            .map(|(s, &w)| Synapse { weight: w, ..*s })
            .collect();
        Connectome { num_neurons: self.num_neurons, synapses }
    }

    /// Build a cache-friendly outgoing-edge view for the simulation loop.
    pub fn to_csr(&self) -> Csr {
        let n = self.num_neurons;
        let mut out_counts = vec![0u32; n + 1];
        for s in &self.synapses {
            out_counts[s.pre as usize + 1] += 1;
        }
        for i in 0..n {
            out_counts[i + 1] += out_counts[i];
        }
        let offsets = out_counts;
        let mut post = vec![0 as NeuronId; self.synapses.len()];
        let mut weight = vec![0f32; self.synapses.len()];
        let mut cursor = offsets.clone();
        for s in &self.synapses {
            let slot = cursor[s.pre as usize] as usize;
            post[slot] = s.post;
            weight[slot] = s.weight;
            cursor[s.pre as usize] += 1;
        }
        Csr { num_neurons: n, offsets, post, weight }
    }

    /// Load from an edge list: one `pre,post,weight` per line, `#` comments and
    /// blank lines ignored. This is the seam for real data — most FlyWire /
    /// MaleCNS connectivity exports reduce to exactly this plus a neuron table.
    pub fn from_edge_list<R: BufRead>(reader: R) -> io::Result<Connectome> {
        let mut c = Connectome::default();
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split(|ch| ch == ',' || ch == '\t' || ch == ' ').filter(|t| !t.is_empty());
            let parse_err = || io::Error::new(io::ErrorKind::InvalidData, format!("bad edge line: {line}"));
            let pre: NeuronId = it.next().ok_or_else(parse_err)?.parse().map_err(|_| parse_err())?;
            let post: NeuronId = it.next().ok_or_else(parse_err)?.parse().map_err(|_| parse_err())?;
            let weight: f32 = match it.next() {
                Some(w) => w.parse().map_err(|_| parse_err())?,
                None => 1.0,
            };
            c.add_synapse(pre, post, weight);
        }
        Ok(c)
    }

    /// A deterministic synthetic connectome for tests and local dev. Not
    /// biologically meaningful — just gives you a graph of a chosen size and
    /// density so the whole pipeline runs before you plug in a real export.
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
}

/// Compressed-sparse-row view of outgoing edges. Iterate neuron `i`'s targets
/// with `post[offsets[i]..offsets[i+1]]` and the parallel `weight` slice.
#[derive(Clone, Debug)]
pub struct Csr {
    pub num_neurons: usize,
    pub offsets: Vec<u32>,
    pub post: Vec<NeuronId>,
    pub weight: Vec<f32>,
}

impl Csr {
    #[inline]
    pub fn out_edges(&self, neuron: NeuronId) -> impl Iterator<Item = (NeuronId, f32)> + '_ {
        let i = neuron as usize;
        let start = self.offsets[i] as usize;
        let end = self.offsets[i + 1] as usize;
        (start..end).map(move |k| (self.post[k], self.weight[k]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn synthetic_has_expected_shape() {
        let c = Connectome::synthetic(100, 4, 42);
        assert_eq!(c.num_neurons(), 100);
        assert_eq!(c.num_synapses(), 400);
    }

    #[test]
    fn csr_round_trips_edges() {
        let c = Connectome::synthetic(50, 3, 7);
        let csr = c.to_csr();
        let total: usize = (0..c.num_neurons() as NeuronId)
            .map(|n| csr.out_edges(n).count())
            .sum();
        assert_eq!(total, c.num_synapses());
    }

    #[test]
    fn with_weights_preserves_topology() {
        let c = Connectome::synthetic(20, 2, 1);
        let zeros = vec![0.0f32; c.num_synapses()];
        let z = c.with_weights(&zeros);
        assert!(z.synapses().iter().all(|s| s.weight == 0.0));
        assert_eq!(z.num_synapses(), c.num_synapses());
    }

    #[test]
    fn edge_list_parses() {
        let data = "# comment\n0,1,0.5\n1 2 -0.25\n2\t0\n";
        let c = Connectome::from_edge_list(Cursor::new(data)).unwrap();
        assert_eq!(c.num_synapses(), 3);
        assert_eq!(c.num_neurons(), 3);
        assert_eq!(c.synapses()[2].weight, 1.0); // defaulted
    }

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
}
