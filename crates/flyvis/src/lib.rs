//! Reads the published flyvis connectome spec and carries its curated ground
//! truth.
//!
//! flyvis (Lappalainen et al., Nature 2024) distributes its connectome as
//! generative rules -- 65 cell types and 605 connection types tiled over a
//! hexagonal lattice -- rather than as an edge list. This crate reads that file
//! faithfully; expanding it into a graph belongs elsewhere.

pub mod groundtruth;
mod json;
pub mod spec;
#[cfg(test)]
mod test_support;

pub use spec::{ConnectomeSpec, EdgeSpec, NodeSpec, Offset, Pattern, SpecError, SpecErrorKind};
