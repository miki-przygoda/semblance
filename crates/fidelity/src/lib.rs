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
//! - [`compare_behaviour`]: distance between two [`Behaviour`] recordings; a
//!   shape mismatch or an empty battery is a [`FidelityError`], never a score
//! - [`size_fidelity_curve`]: the harness that produces your headline result — how
//!   fidelity falls as the latent shrinks, with encode/decode timing
//! - [`indices`]: flyvis's response indices (FRI, DSI, preferred direction,
//!   tuning correlation), their time windows and the paper's agreement rules,
//!   as pure functions over `semblance_sim::response::Responses`
//! - [`similarity`]: debiased linear CKA over each cell type's central
//!   response time course (the representational axis), and Zhou & Hasler's
//!   peak-voltage RDM variant, reported beside it
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

pub mod indices;
pub mod similarity;

/// Why two behaviours could not be compared. Each is a refusal: before
/// 0.4.0 every one of these scored as a perfect 0 (chore defects 2 and 3).
#[derive(Clone, Debug, PartialEq)]
pub enum FidelityError {
    ChannelCount { a: usize, b: usize },
    NeuronCount { a: usize, b: usize },
    TickCount { a: u64, b: u64 },
    ChannelLength { channel: usize, a: usize, b: usize },
    /// Neither recording has a channel sample or a neuron.
    NothingCompared,
    /// A battery with no probes.
    EmptyBattery,
}

impl std::fmt::Display for FidelityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FidelityError::ChannelCount { a, b } => write!(f, "the recordings have {a} and {b} motor channels"),
            FidelityError::NeuronCount { a, b } => write!(f, "the recordings have {a} and {b} neurons"),
            FidelityError::TickCount { a, b } => write!(f, "the recordings ran {a} and {b} ticks"),
            FidelityError::ChannelLength { channel, a, b } => write!(f, "motor channel {channel} has {a} and {b} samples"),
            FidelityError::NothingCompared => f.write_str("the recordings have no channel samples and no neurons to compare"),
            FidelityError::EmptyBattery => f.write_str("a battery with no probes measures nothing"),
        }
    }
}

impl std::error::Error for FidelityError {}

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
/// (mean squared difference over channels and ticks) and a rate term
/// (difference in per-neuron spike counts). Lower = more faithful.
///
/// The recordings must have the same shape: a different channel count,
/// neuron count, tick count or channel length is a [`FidelityError`], as is
/// a pair with nothing to compare. Never a score.
pub fn compare_behaviour(a: &Behaviour, b: &Behaviour) -> Result<f32, FidelityError> {
    if a.channels.len() != b.channels.len() {
        return Err(FidelityError::ChannelCount { a: a.channels.len(), b: b.channels.len() });
    }
    if a.spike_counts.len() != b.spike_counts.len() {
        return Err(FidelityError::NeuronCount { a: a.spike_counts.len(), b: b.spike_counts.len() });
    }
    if a.ticks != b.ticks {
        return Err(FidelityError::TickCount { a: a.ticks, b: b.ticks });
    }
    for (c, (x, y)) in a.channels.iter().zip(&b.channels).enumerate() {
        if x.len() != y.len() {
            return Err(FidelityError::ChannelLength { channel: c, a: x.len(), b: y.len() });
        }
    }

    let (channel_term, count) = {
        let mut acc = 0.0f32;
        let mut count = 0usize;
        for (x, y) in a.channels.iter().zip(&b.channels) {
            for (p, q) in x.iter().zip(y) {
                let d = p - q;
                acc += d * d;
                count += 1;
            }
        }
        (if count == 0 { 0.0 } else { acc / count as f32 }, count)
    };
    if count == 0 && a.spike_counts.is_empty() {
        return Err(FidelityError::NothingCompared);
    }

    let rate_term = {
        let n = a.spike_counts.len();
        if n == 0 {
            0.0
        } else {
            let mut acc = 0.0f32;
            for (&p, &q) in a.spike_counts.iter().zip(&b.spike_counts) {
                let d = p as f32 - q as f32;
                acc += d * d;
            }
            (acc / n as f32).sqrt()
        }
    };

    Ok(channel_term + 0.01 * rate_term)
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
/// battery of probes: the mean [`compare_behaviour`] distance (0 = perfect).
/// An empty battery measures nothing, so it is an error, never 0.
pub fn behavioural_distance<P: Probe>(original: &Connectome, decoded: &Connectome, battery: &[P]) -> Result<f32, FidelityError> {
    if battery.is_empty() {
        return Err(FidelityError::EmptyBattery);
    }
    let mut total = 0.0f32;
    for probe in battery {
        let ob = run_probe(original, probe);
        let db = run_probe(decoded, probe);
        total += compare_behaviour(&ob, &db)?;
    }
    Ok(total / battery.len() as f32)
}

/// Somewhere to collect your probes. Left minimal on purpose — designing the
/// battery is the research decision, not boilerplate.
pub struct ProbeBattery<P: Probe> {
    pub probes: Vec<P>,
}

impl<P: Probe> ProbeBattery<P> {
    /// An empty battery.
    pub fn new() -> Self {
        ProbeBattery { probes: Vec::new() }
    }

    /// Add a probe, returning `self` so a battery reads as one expression.
    pub fn with(mut self, probe: P) -> Self {
        self.probes.push(probe);
        self
    }

    pub fn len(&self) -> usize {
        self.probes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.probes.is_empty()
    }

    /// The probes, ready to hand to [`behavioural_distance`].
    pub fn as_slice(&self) -> &[P] {
        &self.probes
    }
}

impl<P: Probe> Default for ProbeBattery<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Probe> From<Vec<P>> for ProbeBattery<P> {
    fn from(probes: Vec<P>) -> Self {
        ProbeBattery { probes }
    }
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
    /// Behavioural distance (lower = more faithful; `0.0` = identical behaviour).
    /// A codec whose `encode`/`decode` are still `todo!()` panics rather than
    /// producing a point here — there is no "not measured" value.
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
///
/// A shape mismatch or an empty battery is a [`FidelityError`].
pub fn size_fidelity_curve<P: Probe>(
    reference: &Connectome,
    codecs: &[Box<dyn Codec>],
    battery: &[P],
) -> Result<Vec<CurvePoint>, FidelityError> {
    let mut points = Vec::with_capacity(codecs.len());
    for codec in codecs {
        let t0 = Instant::now();
        let z: Latent = codec.encode(reference);
        let encode_ns = t0.elapsed().as_nanos();

        let t1 = Instant::now();
        let decoded = codec.decode(&z);
        let decode_ns = t1.elapsed().as_nanos();

        let bd = behavioural_distance(reference, &decoded, battery)?;
        points.push(CurvePoint {
            latent_dim: codec.latent_dim(reference),
            behavioural_distance: bd,
            encode_ns,
            decode_ns,
        });
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_codec::WeightCodec;

    /// The probes these tests replay. Uses the public [`DriveProbe`] rather than a
    /// private near-duplicate of it.
    fn test_battery() -> Vec<DriveProbe> {
        vec![DriveProbe { k: 8, amplitude: 1.5, motor: 4, ticks: 150 }]
    }

    #[test]
    fn lossless_codec_has_zero_behavioural_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let codec = WeightCodec::from_connectome(&c);
        let z = codec.encode(&c);
        let decoded = codec.decode(&z);

        let battery = test_battery();
        assert_eq!(behavioural_distance(&c, &decoded, &battery), Ok(0.0), "a lossless round-trip must behave identically");
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
        let bd = behavioural_distance(&c, &perturbed, &test_battery()).unwrap();
        assert!(bd > 0.0, "changed dynamics should change behaviour");
    }

    #[test]
    fn probe_battery_collects_probes_for_behavioural_distance() {
        let c = Connectome::synthetic(80, 5, 9);
        let mut w = c.weights();
        for x in w.iter_mut() {
            *x += 0.5;
        }
        let different = c.with_weights(&w);
        let battery = ProbeBattery::new()
            .with(DriveProbe { k: 8, amplitude: 1.5, motor: 4, ticks: 150 })
            .with(DriveProbe { k: 4, amplitude: 2.0, motor: 4, ticks: 150 });

        assert_eq!(battery.len(), 2);
        assert!(!battery.is_empty());
        let bd = behavioural_distance(&c, &different, battery.as_slice()).unwrap();
        assert!(bd > 0.0, "a different network behaves differently: {bd}");
        assert_eq!(behavioural_distance(&c, &c, battery.as_slice()), Ok(0.0));
    }

    /// Chore defect 2: a busy recording against an empty one scored 0.
    #[test]
    fn a_busy_recording_against_an_empty_one_is_an_error() {
        let busy = Behaviour { channels: vec![vec![1.0; 50]; 4], spike_counts: vec![7; 500], ticks: 50 };
        assert_eq!(compare_behaviour(&busy, &Behaviour::default()), Err(FidelityError::ChannelCount { a: 4, b: 0 }));
        let mut fewer = busy.clone();
        fewer.spike_counts.pop();
        assert_eq!(compare_behaviour(&busy, &fewer), Err(FidelityError::NeuronCount { a: 500, b: 499 }));
        let mut shorter = busy.clone();
        shorter.ticks = 49;
        assert_eq!(compare_behaviour(&busy, &shorter), Err(FidelityError::TickCount { a: 50, b: 49 }));
        let mut ragged = busy.clone();
        ragged.channels[2].pop();
        assert_eq!(compare_behaviour(&busy, &ragged), Err(FidelityError::ChannelLength { channel: 2, a: 50, b: 49 }));
        assert_eq!(compare_behaviour(&Behaviour::default(), &Behaviour::default()), Err(FidelityError::NothingCompared));
    }

    /// Equal shapes: the arithmetic is the old one.
    #[test]
    fn equal_shapes_score_as_before() {
        let a = Behaviour { channels: vec![vec![1.0, 0.0]], spike_counts: vec![2], ticks: 2 };
        let b = Behaviour { channels: vec![vec![0.0, 0.0]], spike_counts: vec![0], ticks: 2 };
        assert_eq!(compare_behaviour(&a, &b), Ok(1.0f32 / 2.0 + 0.01 * 2.0));
    }

    /// Chore defect 3: an empty battery scored 0 against a very different network.
    #[test]
    fn an_empty_battery_is_an_error_not_a_perfect_score() {
        let c = Connectome::synthetic(80, 5, 9);
        let w: Vec<f32> = c.weights().iter().map(|x| x + 5.0).collect();
        let shifted = c.with_weights(&w);
        let none: Vec<DriveProbe> = Vec::new();
        assert_eq!(behavioural_distance(&c, &shifted, &none), Err(FidelityError::EmptyBattery));
        assert!(behavioural_distance(&c, &shifted, &test_battery()).unwrap() > 0.0);
        let codecs: Vec<Box<dyn Codec>> = vec![Box::new(WeightCodec::from_connectome(&c))];
        assert_eq!(size_fidelity_curve(&c, &codecs, &none).map(|v| v.len()), Err(FidelityError::EmptyBattery));
        assert_eq!(size_fidelity_curve(&c, &codecs, &test_battery()).map(|v| v[0].behavioural_distance), Ok(0.0));
    }

    #[test]
    fn errors_display_as_registered() {
        let cases = [
            (FidelityError::ChannelCount { a: 4, b: 0 }, "the recordings have 4 and 0 motor channels"),
            (FidelityError::NeuronCount { a: 500, b: 499 }, "the recordings have 500 and 499 neurons"),
            (FidelityError::TickCount { a: 50, b: 49 }, "the recordings ran 50 and 49 ticks"),
            (FidelityError::ChannelLength { channel: 2, a: 50, b: 49 }, "motor channel 2 has 50 and 49 samples"),
            (FidelityError::NothingCompared, "the recordings have no channel samples and no neurons to compare"),
            (FidelityError::EmptyBattery, "a battery with no probes measures nothing"),
        ];
        for (e, want) in cases {
            assert_eq!(e.to_string(), want);
        }
    }
}
