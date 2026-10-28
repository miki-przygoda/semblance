//! # semblance-sim
//!
//! Dynamics + the loop that runs them. A [`Connectome`] is inert; this crate
//! turns it into behaviour by (1) injecting sensory input, (2) integrating a
//! [`NeuronModel`] one tick at a time, and (3) reading motor output back out.
//!
//! What is DONE here (a working baseline you can run today):
//! - a leaky integrate-and-fire model ([`Lif`]) — the standard first choice
//! - an [`Engine`] tick loop using the connectome's CSR view
//! - trivial [`Stimulus`] / [`Readout`] impls so the pipeline runs end to end
//! - a [`Behaviour`] recording that fidelity metrics compare
//! - [`graded`]: flyvis's graded (non-spiking) dynamics, in f32 and f64,
//!   beside the spiking path and touching none of its traits
//!
//! What is YOURS to fill in later — this is where "make it act like an ant" lives:
//! - the sensory encoding ([`Stimulus`]): how a world state becomes input current,
//!   and which neurons receive it.
//! - the motor decoding ([`Readout`]): how spikes become actions/behaviour.
//! - richer dynamics if you outgrow LIF (adaptation, conductance-based, etc.).

pub mod graded;

use semblance_connectome::{Connectome, Csr, NeuronId};

/// A per-neuron dynamics model. Given the input current for each neuron this
/// tick, advance the membrane state and report which neurons spiked.
///
/// Implement your own to go beyond LIF; the engine only needs this trait.
pub trait NeuronModel {
    /// (Re)initialise internal state for `n` neurons.
    fn reset(&mut self, n: usize);
    /// Advance one timestep. `input[i]` is the total current into neuron `i`.
    /// Returns a spike mask of length `n`.
    fn step(&mut self, input: &[f32], dt: f32) -> &[bool];
}

/// Leaky integrate-and-fire. Membrane potential leaks toward rest, integrates
/// input, fires + resets when it crosses threshold, and is clamped for a
/// refractory period. Deliberately minimal and fast.
#[derive(Clone, Debug)]
pub struct Lif {
    pub tau: f32,        // membrane time constant
    pub v_rest: f32,     // resting potential
    pub v_thresh: f32,   // spike threshold
    pub v_reset: f32,    // post-spike reset
    pub refractory: f32, // refractory duration
    v: Vec<f32>,
    refr: Vec<f32>, // remaining refractory time per neuron
    spikes: Vec<bool>,
}

impl Default for Lif {
    fn default() -> Self {
        Lif {
            tau: 20.0,
            v_rest: 0.0,
            v_thresh: 1.0,
            v_reset: 0.0,
            refractory: 2.0,
            v: Vec::new(),
            refr: Vec::new(),
            spikes: Vec::new(),
        }
    }
}

impl NeuronModel for Lif {
    fn reset(&mut self, n: usize) {
        self.v = vec![self.v_rest; n];
        self.refr = vec![0.0; n];
        self.spikes = vec![false; n];
    }

    // sim is replaced in 0.4.0 (CLAUDE.md: do not invest in it); not rewriting these loops.
    #[allow(clippy::needless_range_loop)]
    fn step(&mut self, input: &[f32], dt: f32) -> &[bool] {
        debug_assert_eq!(input.len(), self.v.len());
        for i in 0..self.v.len() {
            self.spikes[i] = false;
            if self.refr[i] > 0.0 {
                self.refr[i] -= dt;
                self.v[i] = self.v_reset;
                continue;
            }
            // dv/dt = (-(v - v_rest) + input) / tau
            let dv = (-(self.v[i] - self.v_rest) + input[i]) / self.tau;
            self.v[i] += dv * dt;
            if self.v[i] >= self.v_thresh {
                self.v[i] = self.v_reset;
                self.refr[i] = self.refractory;
                self.spikes[i] = true;
            }
        }
        &self.spikes
    }
}

/// Turns a world/time into input current. This is half of the "interface" you
/// design when repurposing a brain: which neurons are sensory, and what they see.
pub trait Stimulus {
    /// Write this tick's input current into `out` (length = num_neurons).
    fn inject(&mut self, tick: u64, out: &mut [f32]);
}

/// Turns spikes into observable behaviour. The other half of the interface.
pub trait Readout {
    /// Called every tick with the current spike mask.
    fn observe(&mut self, tick: u64, spikes: &[bool]);
    /// Consume the accumulated observations into a [`Behaviour`] record.
    fn finish(self: Box<Self>) -> Behaviour;
}

/// A recording of what the network did. Fidelity is measured by comparing two
/// of these (original vs decoded, or two evolved variants). Keep whatever your
/// probes need here; the defaults record motor-channel activity over time plus
/// a per-neuron spike count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Behaviour {
    /// `channels[c][t]` = value of motor channel `c` at tick `t`.
    pub channels: Vec<Vec<f32>>,
    /// Total spikes per neuron over the whole run.
    pub spike_counts: Vec<u32>,
    pub ticks: u64,
}

/// The simulation loop. Holds the CSR wiring and per-neuron synaptic current.
///
/// Synapses are **current-based with exponential decay**: a presynaptic spike adds
/// its weight to the postsynaptic neuron's synaptic current, which decays by
/// `syn_decay` each tick. This lets sparse inputs summate over time — without it,
/// a single-tick kick just leaks away and recurrence is dead.
pub struct Engine {
    csr: Csr,
    syn: Vec<f32>,
    /// Per-tick multiplier on synaptic current (0..1). Higher = longer memory.
    pub syn_decay: f32,
    /// Scales all synaptic current into the membrane. Your quickest excitability knob.
    pub syn_gain: f32,
}

impl Engine {
    pub fn new(connectome: &Connectome) -> Self {
        let csr = connectome.to_csr();
        let n = csr.num_neurons;
        Engine { csr, syn: vec![0.0; n], syn_decay: 0.9, syn_gain: 1.0 }
    }

    pub fn num_neurons(&self) -> usize {
        self.csr.num_neurons
    }

    /// Run `ticks` steps and return the readout's [`Behaviour`].
    ///
    /// Each tick: decay synaptic current; input = synaptic current (× gain) +
    /// stimulus injection; integrate the model; add new spikes' weights to the
    /// postsynaptic synaptic current for subsequent ticks.
    // sim is replaced in 0.4.0 (CLAUDE.md: do not invest in it); not rewriting these loops.
    #[allow(clippy::needless_range_loop)]
    pub fn run<M: NeuronModel>(
        &mut self,
        model: &mut M,
        mut stimulus: Box<dyn Stimulus>,
        mut readout: Box<dyn Readout>,
        ticks: u64,
        dt: f32,
    ) -> Behaviour {
        let n = self.csr.num_neurons;
        model.reset(n);
        for v in self.syn.iter_mut() {
            *v = 0.0;
        }
        let mut input = vec![0.0f32; n];

        for t in 0..ticks {
            // 1. decay synaptic current, form this tick's input
            for i in 0..n {
                self.syn[i] *= self.syn_decay;
                input[i] = self.syn[i] * self.syn_gain;
            }
            stimulus.inject(t, &mut input);

            // 2. integrate
            let spikes = model.step(&input, dt);

            // 3. deposit new spikes into postsynaptic synaptic current
            for i in 0..n {
                if spikes[i] {
                    for (post, w) in self.csr.out_edges(i as NeuronId) {
                        self.syn[post as usize] += w;
                    }
                }
            }

            // 4. observe
            readout.observe(t, spikes);
        }
        readout.finish()
    }
}

/// Injects a fixed current into the first `k` neurons. Placeholder sensory drive
/// so the pipeline runs — replace with a real world->current encoding.
pub struct ConstantDrive {
    pub k: usize,
    pub amplitude: f32,
}

impl Stimulus for ConstantDrive {
    fn inject(&mut self, _tick: u64, out: &mut [f32]) {
        for v in out.iter_mut().take(self.k) {
            *v += self.amplitude;
        }
    }
}

/// Records spike counts for all neurons and treats the last `m` neurons as motor
/// channels (their per-tick spike as 0/1). Placeholder motor decoding.
pub struct SpikeReadout {
    num_neurons: usize,
    motor: usize,
    counts: Vec<u32>,
    channels: Vec<Vec<f32>>,
    ticks: u64,
}

impl SpikeReadout {
    pub fn new(num_neurons: usize, motor_channels: usize) -> Self {
        let motor = motor_channels.min(num_neurons);
        SpikeReadout {
            num_neurons,
            motor,
            counts: vec![0; num_neurons],
            channels: vec![Vec::new(); motor],
            ticks: 0,
        }
    }
}

impl Readout for SpikeReadout {
    // sim is replaced in 0.4.0 (CLAUDE.md: do not invest in it); not rewriting these loops.
    #[allow(clippy::needless_range_loop)]
    fn observe(&mut self, _tick: u64, spikes: &[bool]) {
        self.ticks += 1;
        for i in 0..self.num_neurons {
            if spikes[i] {
                self.counts[i] += 1;
            }
        }
        // last `motor` neurons are the motor channels
        for c in 0..self.motor {
            let idx = self.num_neurons - self.motor + c;
            self.channels[c].push(if spikes[idx] { 1.0 } else { 0.0 });
        }
    }

    fn finish(self: Box<Self>) -> Behaviour {
        Behaviour {
            channels: self.channels,
            spike_counts: self.counts,
            ticks: self.ticks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_runs_and_produces_behaviour() {
        let c = Connectome::synthetic(64, 6, 3);
        let mut engine = Engine::new(&c);
        let mut model = Lif::default();
        let stim = Box::new(ConstantDrive { k: 8, amplitude: 1.5 });
        let readout = Box::new(SpikeReadout::new(engine.num_neurons(), 4));
        let b = engine.run(&mut model, stim, readout, 200, 1.0);
        assert_eq!(b.ticks, 200);
        assert_eq!(b.spike_counts.len(), 64);
        assert_eq!(b.channels.len(), 4);
        // constant drive above threshold should make the driven neurons fire
        assert!(b.spike_counts.iter().sum::<u32>() > 0, "network should spike");
    }
}
