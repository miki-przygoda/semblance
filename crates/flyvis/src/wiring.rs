//! What the graded integrator needs from flyvis's build besides the network:
//! which synapse-count group each edge belongs to, which nodes receive the
//! visual input, and how one stimulus frame reaches them.
//!
//! Specs: docs/superpowers/specs/2026-10-28-graded-settling-design.md,
//! section 2; docs/superpowers/specs/2026-10-28-stimulus-traces-design.md,
//! section 1 (`scatter_hexals`).

use std::collections::HashMap;

use semblance_connectome::TypedNetwork;

use crate::pretrained::SYN_COUNT;
use crate::spec::ConnectomeSpec;

#[derive(Clone, Debug, PartialEq)]
pub enum WiringError {
    /// The network's (connection type, du, dv) groups do not number
    /// `SYN_COUNT.len()`, so `SYN_COUNT` cannot be positional over them.
    GroupCount { expected: usize, found: usize },
    /// An input unit names no cell type of the network.
    UnknownInputUnit(String),
    /// An input type's node list and the stimulus frame differ in length.
    FrameLength { expected: usize, found: usize },
    /// An input node is past the end of the buffer being written.
    NodeOutOfRange { node: u32, len: usize },
}

impl std::fmt::Display for WiringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WiringError::GroupCount { expected, found } =>
                write!(f, "{found} synapse-count groups, expected {expected}"),
            WiringError::UnknownInputUnit(u) => write!(f, "input unit {u:?} is not a cell type"),
            WiringError::FrameLength { expected, found } =>
                write!(f, "a frame of {found} hexals, but an input type has {expected} nodes"),
            WiringError::NodeOutOfRange { node, len } =>
                write!(f, "input node {node} is past the end of a buffer of {len}"),
        }
    }
}

impl std::error::Error for WiringError {}

/// Each edge's group, numbered by first appearance, and the group count.
fn group_edges(net: &TypedNetwork) -> (Vec<u32>, usize) {
    let mut index: HashMap<(u16, i16, i16), u32> = HashMap::new();
    let mut groups = Vec::with_capacity(net.num_edges());
    for e in 0..net.num_edges() {
        let s = net.edge_source()[e] as usize;
        let t = net.edge_target()[e] as usize;
        let key = (
            net.edge_conn_type()[e],
            net.node_u()[t] - net.node_u()[s],
            net.node_v()[t] - net.node_v()[s],
        );
        let next = index.len() as u32;
        groups.push(*index.entry(key).or_insert(next));
    }
    (groups, index.len())
}

/// For each edge, the index into `SYN_COUNT` (and the tables in
/// `syn_count_torch`) of its (connection type, du, dv) group. `SYN_COUNT` is
/// positional over those groups in first-appearance order over the built
/// edges.
pub fn edge_syn_groups(net: &TypedNetwork) -> Result<Vec<u32>, WiringError> {
    let (groups, found) = group_edges(net);
    if found != SYN_COUNT.len() {
        return Err(WiringError::GroupCount { expected: SYN_COUNT.len(), found });
    }
    Ok(groups)
}

/// The input nodes per input unit, in `spec.input_units` order, each in node
/// order: flyvis's [8, 721] `input_index` (`stimulus.py:128-130`).
pub fn input_nodes(spec: &ConnectomeSpec, net: &TypedNetwork) -> Result<Vec<Vec<u32>>, WiringError> {
    spec.input_units
        .iter()
        .map(|unit| {
            let t = net
                .cell_types()
                .iter()
                .position(|c| c == unit)
                .ok_or_else(|| WiringError::UnknownInputUnit(unit.clone()))?;
            Ok((0..net.num_nodes() as u32)
                .filter(|&i| net.node_cell_type()[i as usize] as usize == t)
                .collect())
        })
        .collect()
}

/// Writes one stimulus frame onto the input nodes: `out[nodes[k][h]] =
/// frame[h]` for every input type k and hexal h. Nodes not in `nodes` are left
/// as they are; the input nodes are overwritten, so a buffer zeroed once stays
/// zero elsewhere. flyvis `stimulus.py:206`.
///
/// `nodes` is [`input_nodes`]'s [8][721]. Everything is checked before
/// anything is written, so an error leaves `out` untouched.
pub fn scatter_hexals<T: Copy>(nodes: &[Vec<u32>], frame: &[T], out: &mut [T]) -> Result<(), WiringError> {
    for k in nodes {
        if k.len() != frame.len() {
            return Err(WiringError::FrameLength { expected: k.len(), found: frame.len() });
        }
        if let Some(&node) = k.iter().find(|&&i| i as usize >= out.len()) {
            return Err(WiringError::NodeOutOfRange { node, len: out.len() });
        }
    }
    for k in nodes {
        for (&i, &x) in k.iter().zip(frame) {
            out[i as usize] = x;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{load_stimulus_fixture, real_spec_and_network};
    use semblance_connectome::TypedNetworkParts;

    /// Types A and B on two columns; one connection type A -> B, realised at
    /// three different offsets.
    fn net() -> TypedNetwork {
        TypedNetwork::new(TypedNetworkParts {
            cell_types: vec!["A".into(), "B".into()],
            node_cell_type: vec![0, 0, 1, 1],
            node_u: vec![0, 1, 0, 1],
            node_v: vec![0, 0, 0, 0],
            conn_types: vec![(0, 1)],
            conn_sign: vec![1.0],
            edge_source: vec![0, 1, 0, 1],
            edge_target: vec![2, 3, 3, 2],
            edge_conn_type: vec![0, 0, 0, 0],
            edge_n_syn: vec![1.0; 4],
        })
        .unwrap()
    }

    fn spec(units: &str) -> ConnectomeSpec {
        let node = |n: &str| format!(
            r#"{{"name":"{n}","pattern":["stride",[1,1]],"activation":"relu","bias":0.0,
                "bias_fixed":false,"time_constant":0.05,"time_constant_fixed":false}}"#
        );
        ConnectomeSpec::from_json_str(&format!(
            r#"{{"nodes":[{},{}],"edges":[],"input_units":{units},"output_units":[]}}"#,
            node("A"),
            node("B")
        ))
        .unwrap()
    }

    #[test]
    fn groups_edges_by_type_and_offset_in_first_appearance_order() {
        // offsets (du, dv): (0,0), (0,0), (1,0), (-1,0)
        assert_eq!(group_edges(&net()), (vec![0, 0, 1, 2], 3));
    }

    #[test]
    fn refuses_a_network_whose_groups_do_not_fit_syn_count() {
        assert_eq!(
            edge_syn_groups(&net()),
            Err(WiringError::GroupCount { expected: 2355, found: 3 })
        );
    }

    #[test]
    fn input_nodes_keep_the_input_unit_order() {
        assert_eq!(input_nodes(&spec(r#"["B","A"]"#), &net()), Ok(vec![vec![2, 3], vec![0, 1]]));
    }

    #[test]
    fn an_input_unit_that_is_not_a_cell_type_is_an_error() {
        assert_eq!(
            input_nodes(&spec(r#"["C"]"#), &net()),
            Err(WiringError::UnknownInputUnit("C".into()))
        );
    }

    #[test]
    fn scatter_writes_every_type_and_leaves_other_nodes_alone() {
        let nodes = input_nodes(&spec(r#"["B","A"]"#), &net()).unwrap();
        let mut out = [-1i32; 4];
        assert_eq!(scatter_hexals(&nodes, &[7, 9], &mut out), Ok(()));
        assert_eq!(out, [7, 9, 7, 9], "hexal h lands on each type's h-th node");
        let mut out = [-1i32; 5];
        assert_eq!(scatter_hexals(&[vec![2, 3]], &[7, 9], &mut out), Ok(()));
        assert_eq!(out, [-1, -1, 7, 9, -1], "nodes outside `nodes` keep their values");
    }

    #[test]
    fn scatter_refuses_a_frame_of_the_wrong_length_and_writes_nothing() {
        let mut out = [-1i32; 4];
        assert_eq!(
            scatter_hexals(&[vec![0, 1], vec![2, 3]], &[7, 9, 11], &mut out),
            Err(WiringError::FrameLength { expected: 2, found: 3 })
        );
        assert_eq!(
            scatter_hexals(&[vec![0, 1], vec![2]], &[7, 9], &mut out),
            Err(WiringError::FrameLength { expected: 1, found: 2 }),
            "checked for every type before the first is written"
        );
        assert_eq!(out, [-1; 4]);
    }

    #[test]
    fn scatter_refuses_a_node_past_the_buffer_and_writes_nothing() {
        let mut out = [-1i32; 4];
        assert_eq!(
            scatter_hexals(&[vec![0, 1], vec![2, 4]], &[7, 9], &mut out),
            Err(WiringError::NodeOutOfRange { node: 4, len: 4 })
        );
        assert_eq!(out, [-1; 4], "the first type is valid, and still not written");
    }

    /// The input mapping, gated on flyvis's own lattice: a frame lit at
    /// hexal h alone lights exactly eight nodes, one in each input type, each
    /// at hex_coords.csv row h's (u, v). A transposed or reflected mapping
    /// fails here, before any dynamics run.
    #[test]
    fn input_mapping_matches_flyviss_lattice() {
        let Some(csv) = load_stimulus_fixture("hex_coords.csv") else { return };
        let Some((spec, net)) = real_spec_and_network() else { return };
        let mut lines = csv.lines();
        assert_eq!(lines.next(), Some("index,u,v"), "hex_coords.csv header");
        let rows: Vec<(i32, i32)> = lines
            .enumerate()
            .map(|(h, line)| {
                let f: Vec<i32> = line.split(',').map(|x| x.trim().parse().expect("an integer")).collect();
                assert_eq!((f.len(), f[0]), (3, h as i32), "hex_coords.csv row {h}");
                (f[1], f[2])
            })
            .collect();
        assert_eq!(rows.len(), 721);
        let nodes = input_nodes(&spec, &net).expect("input units are cell types");
        let input_types: Vec<u16> = nodes.iter().map(|k| net.node_cell_type()[k[0] as usize]).collect();
        let mut frame = vec![0u8; 721];
        let mut out = vec![0u8; net.num_nodes()];
        for (h, &(u, v)) in rows.iter().enumerate() {
            frame[h] = 1;
            out.fill(0);
            scatter_hexals(&nodes, &frame, &mut out).expect("721 hexals onto 8 x 721 nodes");
            frame[h] = 0;
            let lit: Vec<usize> = (0..out.len()).filter(|&i| out[i] == 1).collect();
            assert_eq!(lit.len(), 8, "hexal {h} lights {} nodes", lit.len());
            let mut types: Vec<u16> = lit.iter().map(|&i| net.node_cell_type()[i]).collect();
            types.sort_unstable();
            let mut want = input_types.clone();
            want.sort_unstable();
            assert_eq!(types, want, "hexal {h}: one node in each input type");
            for &i in &lit {
                assert_eq!(
                    (i32::from(net.node_u()[i]), i32::from(net.node_v()[i])),
                    (u, v),
                    "hexal {h} lands on node {i} ({})",
                    net.cell_types()[net.node_cell_type()[i] as usize]
                );
            }
        }
    }

    #[test]
    fn the_real_network_has_2355_groups_and_eight_input_types_of_721() {
        let Some((spec, n)) = real_spec_and_network() else { return };
        let groups = edge_syn_groups(&n).expect("2,355 groups");
        assert_eq!(groups.len(), 1_513_231);
        let inputs = input_nodes(&spec, &n).expect("input units are cell types");
        assert_eq!(inputs.len(), 8);
        for (k, nodes) in inputs.iter().enumerate() {
            assert_eq!(nodes.len(), 721, "input unit {k}");
            assert!(nodes.windows(2).all(|w| w[0] < w[1]), "input unit {k} ascending");
            let t = n.node_cell_type()[nodes[0] as usize] as usize;
            assert_eq!(n.cell_types()[t], spec.input_units[k]);
        }
    }
}
