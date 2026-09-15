//! # semblance-fidelity
//!
//! The question everything else leans on: *did the round-trip preserve what
//! matters?* Two networks can have different weights and behave identically, so
//! structural diffs are the cheap-but-wrong answer and behavioural comparison is
//! the real one.
//!
//! What is DONE here:
//! - [`structural_distance`]: fast L2 over shared-topology weights (a sanity check,
//!   not the target metric)
//! - [`compare_behaviour`]: distance between two [`Behaviour`] recordings
//! - [`size_fidelity_curve`]: the harness that produces your headline result — how
//!   fidelity falls as the latent shrinks, with encode/decode timing
//!
//! What is YOURS to fill in:
//! - the **probe battery**: the set of stimuli you replay through both networks
//!   before comparing behaviour. A single stimulus is easy to fit; a diverse
//!   battery is what makes "behaves the same" trustworthy. That design choice is
//!   the crux, so it's left to you (see [`ProbeBattery`]).

use semblance_codec::{Codec, Latent};
use semblance_connectome::Connectome;
use semblance_sim::{Behaviour, Engine, Lif, Readout, Stimulus};
use std::time::Instant;

/// Cheap structural check: L2 distance between weight vectors, assuming the two
/// connectomes share topology/order (as they do when both come from the same
/// substrate codec). Returns `f32::INFINITY` if lengths differ.
///
/// Useful as a fast regression guard. NOT your fidelity target — two behaviourally
/// identical networks can score badly here, and vice versa.
pub fn structural_distance(a: &Connectome, b: &Connectome) -> f32 {
    let wa = a.weights();
    let wb = b.weights();
    if wa.len() != wb.len() {
        return f32::INFINITY;
    }
    wa.iter().zip(&wb).map(|(x, y)| (x - y) * (x - y)).sum::<f32>().sqrt()
}

/// Distance between two behaviour recordings. Combines a motor-channel term
/// (mean squared difference over aligned channels/ticks) and a rate term
/// (difference in per-neuron spike counts). Lower = more faithful.
///
/// This is a reasonable default; tune the weighting, or swap in a metric that
/// matches what you actually care about (timing? trajectory? final state?).
pub fn compare_behaviour(a: &Behaviour, b: &Behaviour) -> f32 {
    let channel_term = {
        let nch = a.channels.len().min(b.channels.len());
        let mut acc = 0.0f32;
        let mut count = 0usize;
        for c in 0..nch {
            let nt = a.channels[c].len().min(b.channels[c].len());
            for t in 0..nt {
                let d = a.channels[c][t] - b.channels[c][t];
                acc += d * d;
                count += 1;
            }
        }
        if count == 0 { 0.0 } else { acc / count as f32 }
    };

    let rate_term = {
        let n = a.spike_counts.len().min(b.spike_counts.len());
        if n == 0 {
            0.0
        } else {
            let mut acc = 0.0f32;
            for i in 0..n {
                let d = a.spike_counts[i] as f32 - b.spike_counts[i] as f32;
                acc += d * d;
            }
            (acc / n as f32).sqrt()
        }
    };

    channel_term + 0.01 * rate_term
}

/// A stimulus/readout pair to replay identically through two networks. Implement
/// this to define what "the same situation" means for your comparison. The
/// factory shape (returning fresh boxed trait objects) is so the same probe can
/// be run against multiple networks without shared mutable state.
pub trait Probe {
    fn stimulus(&self) -> Box<dyn Stimulus>;
    fn readout(&self, num_neurons: usize) -> Box<dyn Readout>;
    fn ticks(&self) -> u64;
    fn dt(&self) -> f32 {
        1.0
    }
}

/// Run one probe through one network and record its behaviour.
pub fn run_probe<P: Probe>(connectome: &Connectome, probe: &P) -> Behaviour {
    let mut engine = Engine::new(connectome);
    let mut model = Lif::default();
    let n = engine.num_neurons();
    engine.run(&mut model, probe.stimulus(), probe.readout(n), probe.ticks(), probe.dt())
}

/// Behavioural fidelity of a decoded network vs the original, averaged over a
/// battery of probes. Returns the mean [`compare_behaviour`] distance (0 = perfect).
pub fn behavioural_distance<P: Probe>(original: &Connectome, decoded: &Connectome, battery: &[P]) -> f32 {
    if battery.is_empty() {
        return 0.0;
    }
    let mut total = 0.0f32;
    for probe in battery {
        let ob = run_probe(original, probe);
        let db = run_probe(decoded, probe);
        total += compare_behaviour(&ob, &db);
    }
    total / battery.len() as f32
}

/// Somewhere to collect your probes. Left minimal on purpose — designing the
/// battery is the research decision, not boilerplate.
pub struct ProbeBattery<P: Probe> {
    pub probes: Vec<P>,
}

/// A ready-made probe: constant drive into the first `k` neurons, spikes read
/// from the last `motor` neurons. Enough to run the pipeline end to end; replace
/// with something world-shaped when you design your real battery.
pub struct DriveProbe {
    pub k: usize,
    pub amplitude: f32,
    pub motor: usize,
    pub ticks: u64,
}

impl Default for DriveProbe {
    fn default() -> Self {
        DriveProbe { k: 8, amplitude: 1.5, motor: 4, ticks: 200 }
    }
}

impl Probe for DriveProbe {
    fn stimulus(&self) -> Box<dyn Stimulus> {
        Box::new(semblance_sim::ConstantDrive { k: self.k, amplitude: self.amplitude })
    }
    fn readout(&self, num_neurons: usize) -> Box<dyn Readout> {
        Box::new(semblance_sim::SpikeReadout::new(num_neurons, self.motor))
    }
    fn ticks(&self) -> u64 {
        self.ticks
    }
}

/// One point on the size-vs-fidelity curve.
#[derive(Clone, Debug)]
pub struct CurvePoint {
    pub latent_dim: usize,
    /// Behavioural distance (lower = more faithful). `None` if the codec isn't
    /// implemented yet (e.g. `ReducedCodec` still `todo!()`).
    pub behavioural_distance: f32,
    pub encode_ns: u128,
    pub decode_ns: u128,
}

/// The headline experiment. For each codec in `codecs` (typically the same scheme
/// at decreasing latent sizes), encode+decode the reference and measure behavioural
/// fidelity and round-trip timing. Plot `latent_dim` vs `behavioural_distance` and
/// you have your result.
///
/// `codecs` is boxed so you can mix schemes; timing uses a single encode/decode —
/// average over repeats yourself if you need tighter numbers.
pub fn size_fidelity_curve<P: Probe>(
    reference: &Connectome,
    codecs: &[Box<dyn Codec>],
    battery: &[P],
) -> Vec<CurvePoint> {
    let mut points = Vec::with_capacity(codecs.len());
    for codec in codecs {
        let t0 = Instant::now();
        let z: Latent = codec.encode(reference);
        let encode_ns = t0.elapsed().as_nanos();

        let t1 = Instant::now();
        let decoded = codec.decode(&z);
        let decode_ns = t1.elapsed().as_nanos();

        let bd = behavioural_distance(reference, &decoded, battery);
        points.push(CurvePoint {
            latent_dim: codec.latent_dim(reference),
            behavioural_distance: bd,
            encode_ns,
            decode_ns,
        });
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_codec::WeightCodec;
    use semblance_sim::{ConstantDrive, SpikeReadout};

    struct DrivePorbe {
        k: usize,
        motor: usize,
        ticks: u64,
    }
    impl Probe for DrivePorbe {
        fn stimulus(&self) -> Box<dyn Stimulus> {
            Box::new(ConstantDrive { k: self.k, amplitude: 1.5 })
        }
        fn readout(&self, n: usize) -> Box<dyn Readout> {
            Box::new(SpikeReadout::new(n, self.motor))
        }
        fn ticks(&self) -> u64 {
            self.ticks
        }
    }

    #[test]
    fn lossless_codec_has_zero_behavioural_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let codec = WeightCodec::from_connectome(&c);
        let z = codec.encode(&c);
        let decoded = codec.decode(&z);

        let battery = vec![DrivePorbe { k: 8, motor: 4, ticks: 150 }];
        let bd = behavioural_distance(&c, &decoded, &battery);
        assert_eq!(bd, 0.0, "a lossless round-trip must behave identically");
        assert_eq!(structural_distance(&c, &decoded), 0.0);
    }

    #[test]
    fn perturbing_weights_increases_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let mut w = c.weights();
        for x in w.iter_mut() {
            *x += 0.5;
        }
        let perturbed = c.with_weights(&w);
        let battery = vec![DrivePorbe { k: 8, motor: 4, ticks: 150 }];
        let bd = behavioural_distance(&c, &perturbed, &battery);
        assert!(bd > 0.0, "changed dynamics should change behaviour");
    }
}
