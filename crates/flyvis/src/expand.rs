//! Expands the generative connectome spec into a realised network.
//!
//! The spec describes 65 node types and 605 connection types as rules; tiling
//! them over a hexagonal lattice produces 45,669 neurons and 1,513,231
//! synapses. This module reimplements flyvis's own build (flyvis 1.2.0,
//! `connectome/connectome.py`) closely enough to match it element for element,
//! because 0.3.0 validates a port by comparing per-neuron traces and a
//! differently-ordered network would fail with no indication why.

use crate::spec::{NodeSpec, Pattern};

/// flyvis's default lattice radius, in columns.
pub const DEFAULT_EXTENT: i32 = 15;

/// flyvis's default synapse count for edges invented by hull filling.
pub const DEFAULT_N_SYN_FILL: f32 = 1.0;

/// One neuron, placed on the lattice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PlacedNode {
    /// Index into the spec's node list.
    pub cell_type: u16,
    /// Oblique lattice coordinate.
    pub u: i32,
    /// Oblique lattice coordinate.
    pub v: i32,
}

/// Resolves a pattern to `((u_stride, v_stride), extent)`.
///
/// `Single` uses extent 0, not a stride: that is how flyvis places exactly one
/// cell at the origin rather than one cell per stride step.
fn strides_and_extent(pattern: Pattern, extent: i32) -> ((i32, i32), i32) {
    match pattern {
        Pattern::Stride { u, v } => ((u, v), extent),
        Pattern::Tile(n) => ((n, n), extent),
        Pattern::Single => ((1, 1), 0),
    }
}

/// Tiles every node type over the lattice, in flyvis's build order.
///
/// Ordering is spec entry, then `u` ascending, then `v` ascending. A node's
/// position in the returned vector is its index everywhere downstream.
///
/// Note on `%`: flyvis tests `u % u_stride == 0`, and Python's remainder is
/// non-negative where Rust's is signed (`-4 % 3` is `2` in Python, `-1` in
/// Rust). The two disagree on the value and agree on the test, because
/// divisibility is sign-agnostic. Do not "fix" this with `rem_euclid`.
pub(crate) fn place_nodes(nodes: &[NodeSpec], extent: i32) -> Vec<PlacedNode> {
    let mut out = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let ((u_stride, v_stride), ext) = strides_and_extent(node.pattern, extent);
        for u in -ext..=ext {
            let lo = (-ext).max(-ext - u);
            let hi = ext.min(ext - u);
            for v in lo..=hi {
                if u % u_stride == 0 && v % v_stride == 0 {
                    out.push(PlacedNode { cell_type: index as u16, u, v });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{NodeSpec, Pattern};

    fn node(name: &str, pattern: Pattern) -> NodeSpec {
        NodeSpec {
            name: name.to_string(),
            pattern,
            activation: "relu".to_string(),
            bias: 0.0,
            bias_fixed: false,
            time_constant: None,
            time_constant_fixed: false,
        }
    }

    #[test]
    fn an_unstrided_type_fills_the_whole_hexagon() {
        let placed = place_nodes(&[node("A", Pattern::Stride { u: 1, v: 1 })], DEFAULT_EXTENT);
        // 3n^2 + 3n + 1 for n = 15
        assert_eq!(placed.len(), 721);
    }

    #[test]
    fn a_single_pattern_places_exactly_one_node_at_the_origin() {
        let placed = place_nodes(&[node("A", Pattern::Single)], DEFAULT_EXTENT);
        assert_eq!(placed.len(), 1);
        assert_eq!((placed[0].u, placed[0].v), (0, 0));
    }

    /// Lawf1 and Lawf2 use stride (3, 2) and have 123 cells. This number is
    /// load-bearing: the (3, 2) stride is why Lawf1 -> Lawf1 realises zero
    /// edges, which is what turns 605 declared connection types into 604.
    #[test]
    fn a_three_two_stride_places_the_documented_lawf_count() {
        let placed = place_nodes(&[node("Lawf1", Pattern::Stride { u: 3, v: 2 })], DEFAULT_EXTENT);
        assert_eq!(placed.len(), 123);
    }

    #[test]
    fn tile_is_the_same_as_an_equal_stride_in_both_axes() {
        let tiled = place_nodes(&[node("A", Pattern::Tile(3))], DEFAULT_EXTENT);
        let strided = place_nodes(&[node("A", Pattern::Stride { u: 3, v: 3 })], DEFAULT_EXTENT);
        assert_eq!(tiled.len(), strided.len());
        assert!(tiled.iter().zip(&strided).all(|(a, b)| (a.u, a.v) == (b.u, b.v)));
    }

    /// The v bounds make the region a hexagon rather than a rhombus: at the
    /// extreme u, only half the v range is in the lattice.
    #[test]
    fn the_v_range_narrows_at_the_edges_of_u() {
        let placed = place_nodes(&[node("A", Pattern::Stride { u: 1, v: 1 })], 15);
        let at = |u: i32| placed.iter().filter(|p| p.u == u).count();
        assert_eq!(at(0), 31);
        assert_eq!(at(15), 16);
        assert_eq!(at(-15), 16);
    }

    #[test]
    fn nodes_are_ordered_by_spec_entry_then_u_then_v() {
        let placed = place_nodes(
            &[node("A", Pattern::Single), node("B", Pattern::Stride { u: 15, v: 15 })],
            15,
        );
        assert_eq!(placed[0].cell_type, 0);
        let b: Vec<(i32, i32)> =
            placed.iter().filter(|p| p.cell_type == 1).map(|p| (p.u, p.v)).collect();
        let mut sorted = b.clone();
        sorted.sort();
        assert_eq!(b, sorted, "u-major, v-minor ordering");
    }
}
