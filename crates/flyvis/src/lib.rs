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

pub mod groundtruth;
mod json;
pub mod expand;
#[cfg(test)]
mod expand_checks;
pub mod pretrained;
#[cfg(test)]
mod pretrained_checks;
pub mod spec;
#[cfg(test)]
mod test_support;

pub use expand::{expand, ExpandError, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL};
pub use spec::{ConnectomeSpec, EdgeSpec, NodeSpec, Offset, Pattern, SpecError, SpecErrorKind};
