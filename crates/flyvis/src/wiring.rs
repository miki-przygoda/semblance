//! What the graded integrator needs from flyvis's build besides the network:
//! which synapse-count group each edge belongs to, and which nodes receive
//! the visual input.
//!
//! Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 2.

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
}

impl std::fmt::Display for WiringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WiringError::GroupCount { expected, found } =>
                write!(f, "{found} synapse-count groups, expected {expected}"),
            WiringError::UnknownInputUnit(u) => write!(f, "input unit {u:?} is not a cell type"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::real_spec_and_network;
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
