//! Expands the generative connectome spec into a realised network.
//!
//! The spec describes 65 node types and 605 connection types as rules; tiling
//! them over a hexagonal lattice produces 45,669 neurons and 1,513,231
//! synapses. This module reimplements flyvis's own build (flyvis 1.2.0,
//! `connectome/connectome.py`) closely enough to match it element for element,
//! because 0.3.0 validates a port by comparing per-neuron traces and a
//! differently-ordered network would fail with no indication why.

use crate::spec::{ConnectomeSpec, NodeSpec, Pattern};
use semblance_connectome::{NetworkError, TypedNetwork, TypedNetworkParts};
use std::collections::HashMap;

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

/// Cross product of `ob` and `oa`, in `i64` so lattice coordinates cannot
/// overflow.
fn cross(o: (i32, i32), a: (i32, i32), b: (i32, i32)) -> i64 {
    (a.0 as i64 - o.0 as i64) * (b.1 as i64 - o.1 as i64)
        - (a.1 as i64 - o.1 as i64) * (b.0 as i64 - o.0 as i64)
}

/// Convex hull of integer points, counter-clockwise, collinear points dropped.
///
/// Andrew's monotone chain, in exact integer arithmetic. Duplicates in the
/// input are tolerated.
///
/// flyvis obtains this from qhull with the `QJ` joggle option, which **does**
/// retain collinear boundary points as vertices where this chain drops them:
/// measured against the real spec, the two disagree on 91 of the 243 filled
/// edge types. That difference provably does not change what `fill_hull`
/// returns, so `hull_is_a_subset_of_scipys_vertices` asserts only that every
/// vertex produced here is one scipy also found. The offset-level comparison
/// in `fill_hull`'s tests is what actually pins correctness.
pub(crate) fn convex_hull(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut pts: Vec<(i32, i32)> = points.to_vec();
    pts.sort_unstable();
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }

    let mut hull: Vec<(i32, i32)> = Vec::with_capacity(pts.len() * 2);
    for &p in &pts {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len() + 1;
    // `skip(1)`: the last sorted point already ends the lower chain. Including
    // it again pushes a duplicate that the pop at the end does not remove.
    for &p in pts.iter().rev().skip(1) {
        while hull.len() >= lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0 {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull
}

use crate::spec::Offset;

/// Point-in-polygon by Haines' `CrossingsMultiplyTest` (*Graphics Gems IV*).
///
/// This is the algorithm behind matplotlib's `Path.contains_points`, which is
/// what flyvis's `fill_hull` uses. Checked against matplotlib over every grid
/// point of every filled edge type in the real spec: 0 mismatches in 3,425.
///
/// **Do not replace this with the division-based `pnpoly` formulation.** That
/// is a different algorithm at the boundary -- it disagreed with matplotlib on
/// 877 of those 3,425 points -- and it cannot reproduce the winding-sensitivity
/// that `(lhs >= rhs) == yflag1` gives, which matplotlib has.
///
/// Arithmetic is `i64` and therefore exact. Every product is a difference of
/// lattice coordinates, well inside the range where `f64` represents integers
/// exactly, so this agrees with matplotlib's floating-point evaluation rather
/// than merely approximating it.
pub(crate) fn contains_point(poly: &[(i32, i32)], p: (i32, i32)) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let (tx, ty) = (p.0 as i64, p.1 as i64);
    let mut vtx0 = poly[poly.len() - 1];
    let mut yflag0 = vtx0.1 as i64 >= ty;
    let mut inside = false;
    for &vtx1 in poly {
        let yflag1 = vtx1.1 as i64 >= ty;
        if yflag0 != yflag1 {
            let lhs = (vtx1.1 as i64 - ty) * (vtx0.0 as i64 - vtx1.0 as i64);
            let rhs = (vtx1.0 as i64 - tx) * (vtx0.1 as i64 - vtx1.1 as i64);
            if (lhs >= rhs) == yflag1 {
                inside = !inside;
            }
        }
        yflag0 = yflag1;
        vtx0 = vtx1;
    }
    inside
}

/// Adds an offset at every lattice point inside the convex hull of the
/// declared offsets, with `n_syn_fill` synapses each.
///
/// This is how flyvis reconstructs edges across gaps in the EM data. Declared
/// offsets are returned first and unchanged; invented ones follow in `u`-major
/// grid order, matching `np.mgrid`. That ordering is part of the contract,
/// because edge construction iterates offsets in this order.
///
/// The caller is responsible for only invoking this when there are 3 or more
/// declared offsets, which is flyvis's own condition.
pub(crate) fn fill_hull(offsets: &[Offset], n_syn_fill: f32) -> Vec<Offset> {
    let pts: Vec<(i32, i32)> = offsets.iter().map(|o| (o.du, o.dv)).collect();
    let declared: std::collections::HashSet<(i32, i32)> = pts.iter().copied().collect();
    let poly = convex_hull(&pts);

    let (min_u, max_u) = (
        pts.iter().map(|p| p.0).min().unwrap(),
        pts.iter().map(|p| p.0).max().unwrap(),
    );
    let (min_v, max_v) = (
        pts.iter().map(|p| p.1).min().unwrap(),
        pts.iter().map(|p| p.1).max().unwrap(),
    );

    let mut out = offsets.to_vec();
    for u in min_u..=max_u {
        for v in min_v..=max_v {
            if !declared.contains(&(u, v)) && contains_point(&poly, (u, v)) {
                out.push(Offset { du: u, dv: v, n_syn: n_syn_fill });
            }
        }
    }
    out
}

/// Why a spec could not be expanded.
#[derive(Clone, Debug, PartialEq)]
pub enum ExpandError {
    /// An edge names a cell type the node list does not declare.
    UnknownCellType { edge_spec_index: usize, name: String },
    /// More node or connection types than a `u16` index can address.
    TooManyTypes { count: usize },
    /// The expansion produced arrays the network type rejected.
    Network(NetworkError),
}

impl std::fmt::Display for ExpandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpandError::UnknownCellType { edge_spec_index, name } =>
                write!(f, "edges[{edge_spec_index}] names unknown cell type {name:?}"),
            ExpandError::TooManyTypes { count } =>
                write!(f, "{count} types exceeds the u16 index range"),
            ExpandError::Network(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ExpandError {}

impl From<NetworkError> for ExpandError {
    fn from(e: NetworkError) -> Self {
        ExpandError::Network(e)
    }
}

/// Expands the generative spec into the realised network.
///
/// Use [`DEFAULT_EXTENT`] and [`DEFAULT_N_SYN_FILL`] to reproduce flyvis's own
/// build, which yields 45,669 neurons and 1,513,231 synapses.
///
/// Edge types that realise no edges are dropped, and **do not consume a
/// connection type index**. That is not a cosmetic count correction: the
/// trained `syn_strength` vector is positional, so a retained zero-realising
/// type would shift every later index by one and silently attribute every
/// subsequent weight to the wrong connection type.
pub fn expand(
    spec: &ConnectomeSpec,
    extent: i32,
    n_syn_fill: f32,
) -> Result<TypedNetwork, ExpandError> {
    if spec.nodes.len() > u16::MAX as usize {
        return Err(ExpandError::TooManyTypes { count: spec.nodes.len() });
    }

    let placed = place_nodes(&spec.nodes, extent);
    let type_of: HashMap<&str, u16> = spec
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i as u16))
        .collect();

    // Where each node sits, and which nodes belong to each type.
    let mut at: HashMap<(u16, i32, i32), u32> = HashMap::with_capacity(placed.len());
    let mut by_type: Vec<Vec<u32>> = vec![Vec::new(); spec.nodes.len()];
    for (i, p) in placed.iter().enumerate() {
        at.insert((p.cell_type, p.u, p.v), i as u32);
        by_type[p.cell_type as usize].push(i as u32);
    }

    let mut conn_types: Vec<(u16, u16)> = Vec::new();
    let mut conn_sign: Vec<f32> = Vec::new();
    let (mut edge_source, mut edge_target) = (Vec::new(), Vec::new());
    let (mut edge_conn_type, mut edge_n_syn) = (Vec::new(), Vec::new());

    for (ei, e) in spec.edges.iter().enumerate() {
        let src_t = *type_of.get(e.src.as_str()).ok_or(ExpandError::UnknownCellType {
            edge_spec_index: ei,
            name: e.src.clone(),
        })?;
        let tar_t = *type_of.get(e.tar.as_str()).ok_or(ExpandError::UnknownCellType {
            edge_spec_index: ei,
            name: e.tar.clone(),
        })?;

        let offsets = if n_syn_fill > 0.0 && e.offsets.len() >= 3 {
            fill_hull(&e.offsets, n_syn_fill)
        } else {
            e.offsets.clone()
        };

        // Assigned on the first realised edge, not up front.
        let mut ct: Option<u16> = None;

        for o in &offsets {
            for &src in &by_type[src_t as usize] {
                let p = &placed[src as usize];
                let Some(&tgt) = at.get(&(tar_t, p.u + o.du, p.v + o.dv)) else {
                    continue;
                };
                let index = match ct {
                    Some(c) => c,
                    None => {
                        if conn_types.len() >= u16::MAX as usize {
                            return Err(ExpandError::TooManyTypes { count: conn_types.len() + 1 });
                        }
                        conn_types.push((src_t, tar_t));
                        conn_sign.push(e.alpha as f32);
                        let c = (conn_types.len() - 1) as u16;
                        ct = Some(c);
                        c
                    }
                };
                edge_source.push(src);
                edge_target.push(tgt);
                edge_conn_type.push(index);
                edge_n_syn.push(o.n_syn);
            }
        }
    }

    Ok(TypedNetwork::new(TypedNetworkParts {
        cell_types: spec.nodes.iter().map(|n| n.name.clone()).collect(),
        node_cell_type: placed.iter().map(|p| p.cell_type).collect(),
        node_u: placed.iter().map(|p| p.u as i16).collect(),
        node_v: placed.iter().map(|p| p.v as i16).collect(),
        conn_types,
        conn_sign,
        edge_source,
        edge_target,
        edge_conn_type,
        edge_n_syn,
    })?)
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

    use crate::test_support::{load_build_fixture, load_real_spec};
    use crate::spec::ConnectomeSpec;

    #[test]
    fn hull_of_a_square_is_its_four_corners() {
        let pts = [(0, 0), (2, 0), (2, 2), (0, 2), (1, 1)];
        let mut h = convex_hull(&pts);
        h.sort();
        assert_eq!(h, vec![(0, 0), (0, 2), (2, 0), (2, 2)]);
    }

    #[test]
    fn hull_drops_points_collinear_on_an_edge() {
        let pts = [(0, 0), (1, 0), (2, 0), (2, 2), (0, 2)];
        let h = convex_hull(&pts);
        assert!(!h.contains(&(1, 0)), "midpoint of an edge is not a vertex: {h:?}");
        assert_eq!(h.len(), 4);
    }

    #[test]
    fn hull_of_collinear_points_is_the_two_extremes() {
        let h = convex_hull(&[(0, 0), (1, 1), (2, 2)]);
        assert_eq!(h.len(), 2);
        assert!(h.contains(&(0, 0)) && h.contains(&(2, 2)));
    }

    #[test]
    fn hull_is_counter_clockwise() {
        let h = convex_hull(&[(0, 0), (2, 0), (2, 2), (0, 2)]);
        // Shoelace: positive area means counter-clockwise.
        let area: i64 = (0..h.len())
            .map(|i| {
                let (x1, y1) = h[i];
                let (x2, y2) = h[(i + 1) % h.len()];
                x1 as i64 * y2 as i64 - x2 as i64 * y1 as i64
            })
            .sum();
        assert!(area > 0, "expected counter-clockwise, got area {area}");
    }

    /// scipy runs qhull with the `QJ` joggle, which RETAINS collinear boundary
    /// points as vertices; a monotone chain drops them. Measured across all 243
    /// filled edge types while this plan was written: our vertex set is always a
    /// *subset* of scipy's, and the difference never changes what `fill_hull`
    /// returns. Asserting set equality would fail on 91 of 243 for reasons that
    /// provably do not affect the output. The real gate is
    /// `filled_offsets_match_flyvis_exactly` in the next task.
    #[test]
    fn hull_is_a_subset_of_scipys_vertices() {
        let (Some(raw), Some(hulls)) = (load_real_spec(), load_build_fixture("hulls.csv")) else {
            return;
        };
        let spec = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");

        let mut scipy: std::collections::BTreeMap<usize, std::collections::BTreeSet<(i32, i32)>> =
            Default::default();
        for line in hulls.lines().skip(1).filter(|l| !l.is_empty()) {
            let f: Vec<&str> = line.split(',').collect();
            let ei: usize = f[0].parse().unwrap();
            let du: i32 = f[2].parse().unwrap();
            let dv: i32 = f[3].trim().parse().unwrap();
            scipy.entry(ei).or_default().insert((du, dv));
        }
        assert_eq!(scipy.len(), 243, "edge types with 3+ offsets");

        for (ei, want) in scipy {
            let pts: Vec<(i32, i32)> =
                spec.edges[ei].offsets.iter().map(|o| (o.du, o.dv)).collect();
            for v in convex_hull(&pts) {
                assert!(want.contains(&v), "edge spec index {ei}: {v:?} is not a scipy vertex");
            }
        }
    }

    fn off(du: i32, dv: i32, n_syn: f32) -> Offset {
        Offset { du, dv, n_syn }
    }

    #[test]
    fn a_point_well_inside_is_contained() {
        let sq = [(0, 0), (4, 0), (4, 4), (0, 4)];
        assert!(contains_point(&sq, (2, 2)));
    }

    #[test]
    fn a_point_well_outside_is_not_contained() {
        let sq = [(0, 0), (4, 0), (4, 4), (0, 4)];
        assert!(!contains_point(&sq, (9, 9)));
        assert!(!contains_point(&sq, (-1, 2)));
    }

    /// Verified against matplotlib: for a counter-clockwise axis-aligned
    /// square, both vertical edges and the top are inside; the bottom is not.
    #[test]
    fn the_boundary_rule_matches_matplotlib() {
        let sq = [(0, 0), (4, 0), (4, 4), (0, 4)];
        assert!(contains_point(&sq, (0, 2)), "left edge is inside");
        assert!(contains_point(&sq, (4, 2)), "right edge is inside");
        assert!(contains_point(&sq, (2, 4)), "top edge is inside");
        assert!(!contains_point(&sq, (2, 0)), "bottom edge is outside");
    }

    /// The multiply form compares against `yflag1`, so reversing the winding
    /// changes boundary verdicts. matplotlib does the same. A division-based
    /// crossing test cannot reproduce this -- which is how the wrong variant
    /// gets chosen and then disagrees on 26% of boundary points.
    #[test]
    fn the_rule_is_winding_sensitive() {
        let ccw = [(0, 0), (4, 0), (4, 4), (0, 4)];
        let cw = [(0, 0), (0, 4), (4, 4), (4, 0)];
        let differing = (-1..=5)
            .flat_map(|u| (-1..=5).map(move |v| (u, v)))
            .filter(|&p| contains_point(&ccw, p) != contains_point(&cw, p))
            .count();
        assert_eq!(differing, 8, "reversing the winding must change boundary verdicts");
    }

    /// A collinear vertex inserted on an edge changes nothing. This is why the
    /// monotone chain may drop the points qhull's joggle keeps.
    #[test]
    fn a_collinear_vertex_does_not_change_the_verdict() {
        let tri = [(0, 0), (2, 0), (0, 2)];
        let with_extra = [(0, 0), (1, 0), (2, 0), (0, 2)];
        for u in 0..=2 {
            for v in 0..=2 {
                assert_eq!(
                    contains_point(&tri, (u, v)),
                    contains_point(&with_extra, (u, v)),
                    "({u}, {v})"
                );
            }
        }
    }

    #[test]
    fn fill_hull_leaves_declared_offsets_first_and_in_order() {
        let declared = [off(0, 0, 5.0), off(2, 0, 6.0), off(0, 2, 7.0)];
        let filled = fill_hull(&declared, DEFAULT_N_SYN_FILL);
        assert_eq!(&filled[..3], &declared[..]);
    }

    /// Pins the u-major grid order flyvis's `np.mgrid` produces, which the next
    /// task's edge construction depends on.
    ///
    /// The fixture is chosen so the two added points come out in a DIFFERENT
    /// order under v-major iteration -- u-major gives [(0,2), (1,1)], v-major
    /// gives [(1,1), (0,2)] -- so swapping the loop nesting makes this test
    /// fail. An earlier fixture used two points sharing `v`, which both
    /// orderings visit identically; it asserted an order it could not detect.
    /// Expected values verified against matplotlib.
    #[test]
    fn fill_hull_adds_exactly_the_contained_lattice_points() {
        let declared = [off(0, 1, 5.0), off(0, 3, 6.0), off(2, 0, 7.0)];
        let filled = fill_hull(&declared, DEFAULT_N_SYN_FILL);
        let added: Vec<(i32, i32)> = filled[3..].iter().map(|o| (o.du, o.dv)).collect();
        assert_eq!(added, vec![(0, 2), (1, 1)], "u-major; v-major would give [(1,1), (0,2)]");
        assert!(filled[3..].iter().all(|o| o.n_syn == DEFAULT_N_SYN_FILL));
    }

    #[test]
    fn fill_hull_never_duplicates_a_declared_offset() {
        let declared = [off(0, 0, 5.0), off(2, 0, 6.0), off(0, 2, 7.0)];
        let filled = fill_hull(&declared, DEFAULT_N_SYN_FILL);
        let mut seen = std::collections::HashSet::new();
        assert!(filled.iter().all(|o| seen.insert((o.du, o.dv))), "duplicate offset");
    }

    /// The rule, in isolation: matplotlib's verdict for every grid point of
    /// three representative edge types, evaluated on **scipy's own polygon**
    /// rather than ours. Using our hull here would conflate a rule error with
    /// the known-and-harmless collinear vertex difference; this test must fail
    /// only when `contains_point` itself is wrong.
    #[test]
    fn containment_matches_matplotlib_point_for_point() {
        let (Some(oracle), Some(hulls)) = (
            load_build_fixture("containment_oracle.csv"),
            load_build_fixture("hulls.csv"),
        ) else {
            return;
        };

        let mut polys: std::collections::BTreeMap<usize, Vec<(i32, i32)>> = Default::default();
        for line in hulls.lines().skip(1).filter(|l| !l.is_empty()) {
            let f: Vec<&str> = line.split(',').collect();
            let ei: usize = f[0].parse().unwrap();
            let du: i32 = f[2].parse().unwrap();
            let dv: i32 = f[3].trim().parse().unwrap();
            polys.entry(ei).or_default().push((du, dv));
        }

        let mut checked = 0usize;
        let mut types = std::collections::BTreeSet::new();
        for line in oracle.lines().skip(1).filter(|l| !l.is_empty()) {
            let f: Vec<&str> = line.split(',').collect();
            let ei: usize = f[0].parse().unwrap();
            let u: i32 = f[1].parse().unwrap();
            let v: i32 = f[2].parse().unwrap();
            let want = f[3].trim() == "1";

            let poly = &polys[&ei];
            assert_eq!(
                contains_point(poly, (u, v)), want,
                "containment mismatch at ({u}, {v}) for edge spec index {ei}"
            );
            checked += 1;
            types.insert(ei);
        }
        assert_eq!(types.len(), 3, "oracle should cover 3 edge types");
        assert!(checked > 0);
    }

    /// The stage 1 gate: every filled offset, for every edge type, against
    /// flyvis's own post-fill_hull output.
    #[test]
    fn filled_offsets_match_flyvis_exactly() {
        let (Some(raw), Some(csv)) = (load_real_spec(), load_build_fixture("offsets.csv")) else {
            return;
        };
        let spec = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");

        let mut expected: std::collections::BTreeMap<usize, Vec<(i32, i32, f32)>> = Default::default();
        for line in csv.lines().skip(1).filter(|l| !l.is_empty()) {
            let f: Vec<&str> = line.split(',').collect();
            let ei: usize = f[0].parse().unwrap();
            let du: i32 = f[3].parse().unwrap();
            let dv: i32 = f[4].parse().unwrap();
            let n: f32 = f[5].parse().unwrap();
            expected.entry(ei).or_default().push((du, dv, n));
        }
        assert_eq!(expected.values().map(Vec::len).sum::<usize>(), 2378, "total offsets");

        for (ei, want) in expected {
            let e = &spec.edges[ei];
            let got: Vec<(i32, i32, f32)> = if e.offsets.len() >= 3 {
                fill_hull(&e.offsets, DEFAULT_N_SYN_FILL)
            } else {
                e.offsets.clone()
            }
            .iter()
            .map(|o| (o.du, o.dv, o.n_syn))
            .collect();
            assert_eq!(got.len(), want.len(), "offset count for edge spec index {ei}");
            for (g, w) in got.iter().zip(&want) {
                assert_eq!((g.0, g.1), (w.0, w.1), "offset position for edge spec index {ei}");
                assert!((g.2 - w.2).abs() < 1e-6, "n_syn for edge spec index {ei}");
            }
        }
    }

    use crate::spec::EdgeSpec;

    fn edge(src: &str, tar: &str, alpha: i32, offsets: Vec<Offset>) -> EdgeSpec {
        EdgeSpec {
            src: src.to_string(),
            tar: tar.to_string(),
            offsets,
            alpha,
            alpha_fixed: true,
            alpha_references: vec![],
            time_constant: None,
            time_constant_fixed: false,
            lambda_mult: 1.0,
            edge_type: "chem".to_string(),
        }
    }

    fn tiny_spec(edges: Vec<EdgeSpec>) -> ConnectomeSpec {
        ConnectomeSpec {
            nodes: vec![
                node("A", Pattern::Stride { u: 1, v: 1 }),
                node("B", Pattern::Stride { u: 1, v: 1 }),
            ],
            edges,
            input_units: vec!["A".to_string()],
            output_units: vec!["B".to_string()],
        }
    }

    #[test]
    fn expands_a_single_offset_into_one_edge_per_realisable_source() {
        // extent 1 gives 7 columns; a (1, 0) shift lands inside for some of them.
        let s = tiny_spec(vec![edge("A", "B", 1, vec![off(0, 0, 3.0)])]);
        let n = expand(&s, 1, DEFAULT_N_SYN_FILL).unwrap();
        assert_eq!(n.num_nodes(), 14, "7 columns x 2 types");
        assert_eq!(n.num_edges(), 7, "a (0,0) offset always lands");
        assert!(n.edge_n_syn().iter().all(|&x| x == 3.0));
    }

    #[test]
    fn drops_an_edge_type_that_realises_nothing() {
        // A tiles with stride 3, so a (1, 0) shift never lands on another A.
        let mut s = tiny_spec(vec![
            edge("A", "A", 1, vec![off(1, 0, 2.0)]),
            edge("A", "B", -1, vec![off(0, 0, 4.0)]),
        ]);
        s.nodes[0].pattern = Pattern::Stride { u: 3, v: 2 };
        let n = expand(&s, 15, DEFAULT_N_SYN_FILL).unwrap();
        assert_eq!(n.conn_types().len(), 1, "the zero-realising type must not appear");
        assert_eq!(n.conn_sign(), [-1.0], "and the surviving type keeps its own sign");
    }

    /// The trap this whole design guards against: connection type indices are
    /// assigned on first *realised* edge, so a zero-realising type must not
    /// consume an index. If it did, every syn_strength past it would attach to
    /// the wrong connection type.
    #[test]
    fn connection_type_indices_follow_first_realised_edge() {
        let mut s = tiny_spec(vec![
            edge("A", "A", 1, vec![off(1, 0, 2.0)]),   // realises nothing
            edge("B", "B", 1, vec![off(0, 0, 5.0)]),   // realises
            edge("A", "B", -1, vec![off(0, 0, 4.0)]),  // realises
        ]);
        s.nodes[0].pattern = Pattern::Stride { u: 3, v: 2 };
        let n = expand(&s, 15, DEFAULT_N_SYN_FILL).unwrap();
        assert_eq!(n.conn_types().len(), 2);
        assert_eq!(n.conn_types()[0], (1, 1), "B->B takes index 0, not 1");
        assert_eq!(n.conn_types()[1], (0, 1), "A->B takes index 1");
        assert_eq!(n.conn_sign(), [1.0, -1.0]);
    }

    #[test]
    fn edges_are_ordered_offset_outer_source_inner() {
        let s = tiny_spec(vec![edge("A", "B", 1, vec![off(0, 0, 1.0), off(1, 0, 2.0)])]);
        let n = expand(&s, 1, DEFAULT_N_SYN_FILL).unwrap();
        let counts = n.edge_n_syn();
        let first = counts.iter().filter(|&&x| x == 1.0).count();
        // All offset-(0,0) edges must come before any offset-(1,0) edge.
        assert!(counts[..first].iter().all(|&x| x == 1.0));
        assert!(counts[first..].iter().all(|&x| x == 2.0));
    }

    #[test]
    fn rejects_an_edge_naming_an_unknown_cell_type() {
        let s = tiny_spec(vec![edge("A", "Z", 1, vec![off(0, 0, 1.0)])]);
        assert_eq!(
            expand(&s, 1, DEFAULT_N_SYN_FILL),
            Err(ExpandError::UnknownCellType { edge_spec_index: 0, name: "Z".to_string() })
        );
    }

    /// At extent 1 the lattice is 7 columns: (0,0), (0,1), (0,-1), (1,0),
    /// (1,-1), (-1,0), (-1,1). Offset (0,0) lands for all 7; offset (1,0)
    /// lands only for (0,0), (0,1), (-1,0), (-1,1) -- 4 of them. 11 in total,
    /// and no hull filling because two offsets is below flyvis's threshold.
    #[test]
    fn skips_hull_filling_below_three_offsets() {
        let s = tiny_spec(vec![edge("A", "B", 1, vec![off(0, 0, 1.0), off(1, 0, 1.0)])]);
        let n = expand(&s, 1, DEFAULT_N_SYN_FILL).unwrap();
        assert_eq!(n.num_edges(), 11);
    }
}
