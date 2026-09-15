//! # semblance-evo
//!
//! Evolution in the compressed space. A genome IS a [`Latent`] code, so mutation
//! and crossover happen on the small vector, and you only `decode` (via a
//! `Codec`) when you need a network to score. Seeding the initial population from
//! the real fly's latent is the whole point: evolution starts from working wiring
//! instead of noise.
//!
//! What is DONE here (a working generational loop):
//! - gaussian mutation + truncation selection + elitism
//! - seeding a population around a reference latent
//! - a deterministic RNG-driven `step` you can call in a loop
//!
//! What is YOURS to fill in:
//! - [`Fitness`]: run the decoded network in your environment and return a score.
//!   This is the other big design surface (reward = "how ant-like was it?").
//! - smarter search if you need it: CMA-ES, novelty/quality-diversity, crossover.
//!   The trait boundary stays the same.

use semblance_codec::Latent;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

/// A candidate solution — just a latent code.
#[derive(Clone, Debug)]
pub struct Genome {
    pub z: Latent,
}

/// A scored genome.
#[derive(Clone, Debug)]
pub struct Individual {
    pub genome: Genome,
    pub fitness: f32,
}

/// Scores a latent. Implementors typically: `decode(z)` -> run in an environment
/// (`semblance-sim`) -> reduce the behaviour to a scalar reward.
pub trait Fitness {
    fn score(&mut self, z: &Latent) -> f32;
}

/// Evolution hyperparameters.
#[derive(Clone, Debug)]
pub struct EvoConfig {
    pub pop_size: usize,
    /// Number of top individuals kept verbatim and used as parents.
    pub elite: usize,
    /// Per-gene probability of being perturbed.
    pub mutation_rate: f32,
    /// Std-dev of the gaussian perturbation.
    pub mutation_std: f32,
}

impl Default for EvoConfig {
    fn default() -> Self {
        EvoConfig {
            pop_size: 64,
            elite: 8,
            mutation_rate: 0.1,
            mutation_std: 0.05,
        }
    }
}

/// A population that evolves latent codes.
pub struct Population {
    individuals: Vec<Individual>,
    cfg: EvoConfig,
    rng: StdRng,
    generation: u64,
}

impl Population {
    /// Seed `pop_size` genomes as jittered copies of `seed` (the real fly's
    /// latent). Individuals start unscored (fitness = -inf).
    pub fn seeded(seed: &Latent, cfg: EvoConfig, rng_seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(rng_seed);
        let mut individuals = Vec::with_capacity(cfg.pop_size);
        for _ in 0..cfg.pop_size {
            let mut z = seed.clone();
            perturb(&mut z, cfg.mutation_rate, cfg.mutation_std, &mut rng);
            individuals.push(Individual {
                genome: Genome { z },
                fitness: f32::NEG_INFINITY,
            });
        }
        Population { individuals, cfg, rng, generation: 0 }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Best individual seen in the current population. Call after at least one
    /// [`step`](Self::step).
    pub fn best(&self) -> &Individual {
        self.individuals
            .iter()
            .max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap_or(std::cmp::Ordering::Equal))
            .expect("population is never empty")
    }

    /// One generation: evaluate everyone, keep the elite, refill by mutating
    /// randomly-chosen elite parents.
    pub fn step<F: Fitness>(&mut self, fitness: &mut F) {
        // 1. evaluate
        for ind in &mut self.individuals {
            ind.fitness = fitness.score(&ind.genome.z);
        }
        // 2. sort best-first
        self.individuals
            .sort_by(|a, b| b.fitness.partial_cmp(&a.fitness).unwrap_or(std::cmp::Ordering::Equal));
        // 3. elitism + reproduction
        let elite = self.cfg.elite.max(1).min(self.individuals.len());
        let mut next: Vec<Individual> = self.individuals[..elite].to_vec();
        while next.len() < self.cfg.pop_size {
            let parent = &self.individuals[self.rng.random_range(0..elite)];
            let mut child = parent.genome.z.clone();
            perturb(&mut child, self.cfg.mutation_rate, self.cfg.mutation_std, &mut self.rng);
            next.push(Individual {
                genome: Genome { z: child },
                fitness: f32::NEG_INFINITY,
            });
        }
        self.individuals = next;
        self.generation += 1;
    }
}

/// Gaussian perturbation of a latent, in place. Each gene is perturbed with
/// probability `rate` by N(0, std). Uses Box–Muller so we only need `rand`'s
/// uniform sampler.
fn perturb(z: &mut Latent, rate: f32, std: f32, rng: &mut StdRng) {
    for g in z.iter_mut() {
        if rng.random::<f32>() < rate {
            *g += gaussian(rng) * std;
        }
    }
}

fn gaussian(rng: &mut StdRng) -> f32 {
    // Box–Muller; guard u1 away from 0 to avoid ln(0).
    let u1: f32 = rng.random::<f32>().max(1e-7);
    let u2: f32 = rng.random::<f32>();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toy fitness: maximise closeness to a fixed target vector. Lets us confirm
    /// the loop actually improves fitness over generations.
    struct TargetFitness {
        target: Latent,
    }
    impl Fitness for TargetFitness {
        fn score(&mut self, z: &Latent) -> f32 {
            let sse: f32 = z.iter().zip(&self.target).map(|(a, b)| (a - b) * (a - b)).sum();
            -sse
        }
    }

    #[test]
    fn evolution_improves_fitness() {
        let seed = vec![0.0f32; 32];
        let target = vec![0.5f32; 32];
        let cfg = EvoConfig { pop_size: 64, elite: 8, mutation_rate: 0.3, mutation_std: 0.1 };
        let mut pop = Population::seeded(&seed, cfg, 123);
        let mut fit = TargetFitness { target };

        pop.step(&mut fit);
        let first = pop.best().fitness;
        for _ in 0..40 {
            pop.step(&mut fit);
        }
        let last = pop.best().fitness;
        assert!(last > first, "fitness should improve: {first} -> {last}");
    }
}
