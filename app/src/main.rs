//! `semblance` — a thin CLI that wires the crates into runnable pipelines so you can
//! confirm the whole thing works before touching the research logic.
//!
//! Subcommands (all run on a synthetic connectome unless you point them at data):
//!   demo       run the simulator once, print a behaviour summary
//!   roundtrip  encode -> decode with the lossless WeightCodec, print fidelity
//!   evolve     seed from the connectome's latent, run N generations of a toy
//!              fitness, print best-fitness per generation
//!
//! Arg parsing is hand-rolled (std only) to keep the dependency tree tiny. Swap in
//! `clap` if/when you want richer flags.

use std::env;
use std::process::ExitCode;

use semblance_codec::{Codec, Latent, WeightCodec};
use semblance_connectome::Connectome;
use semblance_evo::{EvoConfig, Fitness, Population};
use semblance_fidelity::{behavioural_distance, structural_distance, DriveProbe};
use semblance_sim::{ConstantDrive, Engine, Lif, SpikeReadout};

const DEFAULT_NEURONS: usize = 500;
const DEFAULT_DEGREE: usize = 8;
const SEED: u64 = 20260914;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("help");
    match cmd {
        "demo" => cmd_demo(),
        "roundtrip" => cmd_roundtrip(),
        "evolve" => cmd_evolve(),
        "help" | "-h" | "--help" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "semblance — connectome compress / evolve scaffold\n\n\
         USAGE:\n  semblance <command>\n\n\
         COMMANDS:\n  \
         demo        run the LIF simulator once on a synthetic connectome\n  \
         roundtrip   lossless encode->decode, report structural + behavioural fidelity\n  \
         evolve      seed from the connectome latent and run a toy evolution loop\n  \
         help        show this message\n\n\
         Everything runs on a synthetic graph so it works with no data. See\n\
         RESOURCES.md for how to load a real FlyWire / MaleCNS export instead."
    );
}

fn reference_connectome() -> Connectome {
    Connectome::synthetic(DEFAULT_NEURONS, DEFAULT_DEGREE, SEED)
}

fn cmd_demo() -> ExitCode {
    let c = reference_connectome();
    println!(
        "synthetic connectome: {} neurons, {} synapses",
        c.num_neurons(),
        c.num_synapses()
    );
    let mut engine = Engine::new(&c);
    let mut model = Lif::default();
    let stim = Box::new(ConstantDrive { k: 16, amplitude: 1.5 });
    let readout = Box::new(SpikeReadout::new(engine.num_neurons(), 8));
    let b = engine.run(&mut model, stim, readout, 300, 1.0);

    let total_spikes: u32 = b.spike_counts.iter().sum();
    let active = b.spike_counts.iter().filter(|&&s| s > 0).count();
    println!(
        "ran {} ticks | total spikes: {} | active neurons: {}/{} | motor channels: {}",
        b.ticks,
        total_spikes,
        active,
        c.num_neurons(),
        b.channels.len()
    );
    ExitCode::SUCCESS
}

fn cmd_roundtrip() -> ExitCode {
    let c = reference_connectome();
    let codec = WeightCodec::from_connectome(&c);

    let z: Latent = codec.encode(&c);
    let decoded = codec.decode(&z);

    let battery = vec![
        DriveProbe { k: 16, amplitude: 1.5, motor: 8, ticks: 200 },
        DriveProbe { k: 4, amplitude: 2.0, motor: 8, ticks: 200 },
    ];
    let sd = structural_distance(&c, &decoded);
    let bd = behavioural_distance(&c, &decoded, &battery);

    println!("codec: WeightCodec (lossless baseline)");
    println!("latent dim: {} (= num synapses)", codec.latent_dim(&c));
    println!("structural distance:  {sd:.6}");
    println!("behavioural distance: {bd:.6}   (0.0 == identical behaviour)");
    println!(
        "\nThis is your fidelity anchor. Implement ReducedCodec with latent_dim < {} and\n\
         watch how far behavioural distance rises as you shrink it — that curve is the result.",
        codec.latent_dim(&c)
    );
    ExitCode::SUCCESS
}

/// Toy fitness so `evolve` runs without a real environment: reward genomes whose
/// decoded network fires at a target overall rate under a fixed probe. Replace
/// this with "run in the environment, score how ant-like it was".
struct RateTargetFitness {
    codec: WeightCodec,
    target_spikes: f32,
    probe: DriveProbe,
}

impl Fitness for RateTargetFitness {
    fn score(&mut self, z: &Latent) -> f32 {
        let net = self.codec.decode(z);
        // run_probe replays the probe through a fresh engine and returns behaviour
        let b = semblance_fidelity::run_probe(&net, &self.probe);
        let total: u32 = b.spike_counts.iter().sum();
        -((total as f32) - self.target_spikes).abs()
    }
}

fn cmd_evolve() -> ExitCode {
    let c = reference_connectome();
    let codec = WeightCodec::from_connectome(&c);
    let seed: Latent = codec.encode(&c);

    let cfg = EvoConfig { pop_size: 60, elite: 8, mutation_rate: 0.25, mutation_std: 0.15 };
    let mut pop = Population::seeded(&seed, cfg, SEED);

    // Baseline fires ~120 spikes under this probe; ask evolution to roughly double
    // it. Reachable by finding more excitable weightings, so the loop visibly climbs.
    let mut fitness = RateTargetFitness {
        codec,
        target_spikes: 250.0,
        probe: DriveProbe { k: 16, amplitude: 1.5, motor: 8, ticks: 200 },
    };

    let generations = 20;
    println!("evolving {generations} generations (seeded from the reference latent)\n");
    println!("{:>4}  {:>14}", "gen", "best fitness");
    for _ in 0..generations {
        pop.step(&mut fitness);
        println!("{:>4}  {:>14.2}", pop.generation(), pop.best().fitness);
    }
    println!(
        "\nFitness here is a placeholder (hit a target spike count). The real work is\n\
         defining a fitness that rewards the behaviour you want — that's your environment."
    );
    ExitCode::SUCCESS
}
