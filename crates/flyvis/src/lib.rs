//! Reads the published flyvis connectome spec, carries its curated ground
//! truth, and expands the spec into a realised network.
//!
//! flyvis (Lappalainen et al., Nature 2024) distributes its connectome as
//! generative rules -- 65 cell types and 605 connection types tiled over a
//! hexagonal lattice -- rather than as an edge list. This crate reads that
//! file faithfully and reproduces flyvis's own build of it.
//!
//! The expanded network type itself lives in `semblance-connectome`, not
//! here: the crates that compress and score a network should not have to
//! depend on the importer to name the thing they operate on.
//!
//! # Parity with flyvis
//!
//! [`parity`] runs `semblance-sim`'s graded integrator through flyvis's
//! settling and its twelve reference stimuli, and compares every value with
//! flyvis's own reference export: exactly, with torch's `exp` of the synapse
//! counts ([`syn_count_torch`]), and within bounds fixed in advance, with
//! Rust's. The crate's `#[cfg(test)]` gates and the `semblance validate`
//! command both run it. The result is recorded in
//! `docs/results/0.3.0-flyvis-parity.md`.
//!
//! # Licensing
//!
//! [`pretrained`], [`syn_count_torch`] and [`groundtruth`] are flyvis's data,
//! under flyvis's MIT licence, and parts of this crate port flyvis's
//! MIT-licensed code (the connectome expansion, the hex-eye fp16 mask
//! construction, the ground-truth readers). The package is licensed
//! `Apache-2.0 AND MIT`; NOTICE lists the files and carries flyvis's licence
//! text.

pub mod groundtruth;
mod json;
pub mod ensemble;
pub mod expand;
#[cfg(test)]
mod expand_checks;
#[cfg(test)]
mod gate_support;
pub mod hex_eye;
pub mod moving_edge;
#[cfg(test)]
mod moving_edge_checks;
pub mod parity;
pub mod pretrained;
#[cfg(test)]
mod pretrained_checks;
pub mod reference;
#[cfg(test)]
mod settling_checks;
pub mod spec;
#[cfg(test)]
mod test_support;
pub mod stimuli;
#[cfg(test)]
mod stimuli_checks;
pub mod syn_count_torch;
#[cfg(test)]
mod syn_count_torch_checks;
#[cfg(test)]
mod trace_checks;
pub mod wiring;

pub use expand::{expand, ExpandError, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL};
pub use ensemble::{Ensemble, EnsembleError, EnsembleErrorKind, ModelParams};
pub use spec::{ConnectomeSpec, EdgeSpec, NodeSpec, Offset, Pattern, SpecError, SpecErrorKind};
pub use stimuli::{
    DotParams, Dots, FlashParams, Flashes, Stimulus, StimulusError, FULL_FIELD,
};
pub use moving_edge::{MovingEdge, MovingEdgeParams};
pub use wiring::{edge_syn_groups, input_nodes, scatter_hexals, WiringError};
