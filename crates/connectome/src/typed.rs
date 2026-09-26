//! An expanded, typed network: flat per-node and per-edge arrays plus the two
//! index maps that carry type-shared parameters onto them.
//!
//! This is the decoded form of a generative connectome spec. `Connectome`
//! stores a weight per synapse; this type deliberately does not, because the
//! parameter sharing -- one trained scalar per connection type, applied to
//! every synapse of that type -- is the representation under study. Weights
//! are derived on demand by [`TypedNetwork::weights_into`].

use std::collections::HashSet;

/// The arrays a builder hands to [`TypedNetwork::new`].
///
/// A plain struct rather than ten positional arguments: at this arity,
/// positional parameters of the same type are an invitation to transpose two
/// of them silently.
#[derive(Clone, Debug, PartialEq)]
pub struct TypedNetworkParts {
    /// Cell type names, in build order.
    pub cell_types: Vec<String>,
    /// Per node: index into `cell_types`.
    pub node_cell_type: Vec<u16>,
    /// Per node: hex lattice coordinate.
    pub node_u: Vec<i16>,
    /// Per node: hex lattice coordinate.
    pub node_v: Vec<i16>,
    /// Per connection type: (source cell type, target cell type).
    pub conn_types: Vec<(u16, u16)>,
    /// Per connection type: the synapse sign, +1.0 or -1.0.
    pub conn_sign: Vec<f32>,
    /// Per edge: presynaptic node index.
    pub edge_source: Vec<u32>,
    /// Per edge: postsynaptic node index.
    pub edge_target: Vec<u32>,
    /// Per edge: index into `conn_types`.
    pub edge_conn_type: Vec<u16>,
    /// Per edge: synapse count, from EM.
    pub edge_n_syn: Vec<f32>,
}

/// Why a set of parts does not describe a valid network.
#[derive(Clone, Debug, PartialEq)]
pub enum NetworkError {
    /// The per-node arrays are not all the same length.
    NodeArrayLengthMismatch { cell_type: usize, u: usize, v: usize },
    /// The per-edge arrays are not all the same length.
    EdgeArrayLengthMismatch { source: usize, target: usize, conn_type: usize, n_syn: usize },
    /// The per-connection-type arrays are not all the same length.
    ConnArrayLengthMismatch { types: usize, sign: usize },
    /// A node names a cell type that does not exist.
    CellTypeOutOfRange { node: usize, index: u16, count: usize },
    /// An edge names a connection type that does not exist.
    ConnTypeOutOfRange { edge: usize, index: u16, count: usize },
    /// An edge names a node that does not exist.
    NodeIndexOutOfRange { edge: usize, index: u32, count: usize },
    /// A connection type names a cell type that does not exist.
    ConnEndpointOutOfRange { conn_type: usize, index: u16, count: usize },
    /// A synapse sign was neither +1 nor -1.
    BadSign { conn_type: usize, value: f32 },
    /// A slice handed to [`TypedNetwork::weights_into`] was the wrong length.
    BufferLengthMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::NodeArrayLengthMismatch { cell_type, u, v } =>
                write!(f, "node arrays differ in length: cell_type {cell_type}, u {u}, v {v}"),
            NetworkError::EdgeArrayLengthMismatch { source, target, conn_type, n_syn } =>
                write!(f, "edge arrays differ in length: source {source}, target {target}, conn_type {conn_type}, n_syn {n_syn}"),
            NetworkError::ConnArrayLengthMismatch { types, sign } =>
                write!(f, "connection type arrays differ in length: types {types}, sign {sign}"),
            NetworkError::CellTypeOutOfRange { node, index, count } =>
                write!(f, "node {node} names cell type {index}, but only {count} exist"),
            NetworkError::ConnTypeOutOfRange { edge, index, count } =>
                write!(f, "edge {edge} names connection type {index}, but only {count} exist"),
            NetworkError::NodeIndexOutOfRange { edge, index, count } =>
                write!(f, "edge {edge} names node {index}, but only {count} exist"),
            NetworkError::ConnEndpointOutOfRange { conn_type, index, count } =>
                write!(f, "connection type {conn_type} names cell type {index}, but only {count} exist"),
            NetworkError::BadSign { conn_type, value } =>
                write!(f, "connection type {conn_type} has sign {value}, expected +1 or -1"),
            NetworkError::BufferLengthMismatch { expected, actual } =>
                write!(f, "expected a slice of {expected}, got {actual}"),
        }
    }
}

impl std::error::Error for NetworkError {}

/// An expanded network with type-shared parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct TypedNetwork {
    cell_types: Vec<String>,
    node_cell_type: Vec<u16>,
    node_u: Vec<i16>,
    node_v: Vec<i16>,
    conn_types: Vec<(u16, u16)>,
    conn_sign: Vec<f32>,
    edge_source: Vec<u32>,
    edge_target: Vec<u32>,
    edge_conn_type: Vec<u16>,
    edge_n_syn: Vec<f32>,
}

impl TypedNetwork {
    /// Validates `parts` and builds a network.
    ///
    /// Validation is not defensive politeness: a shifted connection-type index
    /// attributes trained parameters to the wrong synapses while producing a
    /// network that still runs and still counts correctly. Checking here is the
    /// only cheap place to catch it.
    pub fn new(parts: TypedNetworkParts) -> Result<Self, NetworkError> {
        let TypedNetworkParts {
            cell_types, node_cell_type, node_u, node_v,
            conn_types, conn_sign,
            edge_source, edge_target, edge_conn_type, edge_n_syn,
        } = parts;

        let n_nodes = node_cell_type.len();
        if node_u.len() != n_nodes || node_v.len() != n_nodes {
            return Err(NetworkError::NodeArrayLengthMismatch {
                cell_type: n_nodes, u: node_u.len(), v: node_v.len(),
            });
        }

        let n_conn = conn_types.len();
        if conn_sign.len() != n_conn {
            return Err(NetworkError::ConnArrayLengthMismatch { types: n_conn, sign: conn_sign.len() });
        }

        let n_edges = edge_source.len();
        if edge_target.len() != n_edges || edge_conn_type.len() != n_edges || edge_n_syn.len() != n_edges {
            return Err(NetworkError::EdgeArrayLengthMismatch {
                source: n_edges, target: edge_target.len(),
                conn_type: edge_conn_type.len(), n_syn: edge_n_syn.len(),
            });
        }

        let n_types = cell_types.len();
        for (i, &t) in node_cell_type.iter().enumerate() {
            if t as usize >= n_types {
                return Err(NetworkError::CellTypeOutOfRange { node: i, index: t, count: n_types });
            }
        }

        for (i, &(a, b)) in conn_types.iter().enumerate() {
            for end in [a, b] {
                if end as usize >= n_types {
                    return Err(NetworkError::ConnEndpointOutOfRange { conn_type: i, index: end, count: n_types });
                }
            }
        }

        for (i, &s) in conn_sign.iter().enumerate() {
            if s != 1.0 && s != -1.0 {
                return Err(NetworkError::BadSign { conn_type: i, value: s });
            }
        }

        for (i, &c) in edge_conn_type.iter().enumerate() {
            if c as usize >= n_conn {
                return Err(NetworkError::ConnTypeOutOfRange { edge: i, index: c, count: n_conn });
            }
        }

        for (i, (&s, &t)) in edge_source.iter().zip(edge_target.iter()).enumerate() {
            for end in [s, t] {
                if end as usize >= n_nodes {
                    return Err(NetworkError::NodeIndexOutOfRange { edge: i, index: end, count: n_nodes });
                }
            }
        }

        Ok(TypedNetwork {
            cell_types, node_cell_type, node_u, node_v,
            conn_types, conn_sign,
            edge_source, edge_target, edge_conn_type, edge_n_syn,
        })
    }

    /// Number of neurons.
    pub fn num_nodes(&self) -> usize { self.node_cell_type.len() }
    /// Number of synapses.
    pub fn num_edges(&self) -> usize { self.edge_source.len() }
    /// Distinct `(u, v)` lattice positions occupied by any neuron.
    pub fn num_columns(&self) -> usize {
        self.node_u.iter().zip(self.node_v.iter()).collect::<HashSet<_>>().len()
    }

    /// Cell type names, in build order.
    pub fn cell_types(&self) -> &[String] { &self.cell_types }
    /// Connection types as `(source cell type, target cell type)`.
    pub fn conn_types(&self) -> &[(u16, u16)] { &self.conn_types }
    /// Synapse sign per connection type.
    pub fn conn_sign(&self) -> &[f32] { &self.conn_sign }
    /// Cell type index per node.
    pub fn node_cell_type(&self) -> &[u16] { &self.node_cell_type }
    /// Hex lattice `u` per node.
    pub fn node_u(&self) -> &[i16] { &self.node_u }
    /// Hex lattice `v` per node.
    pub fn node_v(&self) -> &[i16] { &self.node_v }
    /// Presynaptic node index per edge.
    pub fn edge_source(&self) -> &[u32] { &self.edge_source }
    /// Postsynaptic node index per edge.
    pub fn edge_target(&self) -> &[u32] { &self.edge_target }
    /// Connection type index per edge.
    pub fn edge_conn_type(&self) -> &[u16] { &self.edge_conn_type }
    /// EM synapse count per edge.
    pub fn edge_n_syn(&self) -> &[f32] { &self.edge_n_syn }

    /// Writes `sign * n_syn * syn_strength` for every edge into `out`.
    ///
    /// `syn_strength` is one trained scalar per connection type. `out` is
    /// caller-owned so a parameter sweep allocates once and reuses it.
    pub fn weights_into(&self, syn_strength: &[f32], out: &mut [f32]) -> Result<(), NetworkError> {
        if syn_strength.len() != self.conn_types.len() {
            return Err(NetworkError::BufferLengthMismatch {
                expected: self.conn_types.len(), actual: syn_strength.len(),
            });
        }
        if out.len() != self.edge_source.len() {
            return Err(NetworkError::BufferLengthMismatch {
                expected: self.edge_source.len(), actual: out.len(),
            });
        }
        for ((slot, &ct), &n_syn) in out.iter_mut().zip(self.edge_conn_type.iter()).zip(self.edge_n_syn.iter()) {
            let ct = ct as usize;
            *slot = self.conn_sign[ct] * n_syn * syn_strength[ct];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two cell types, three nodes, two edges. Small enough to verify by hand.
    fn parts() -> TypedNetworkParts {
        TypedNetworkParts {
            cell_types: vec!["A".to_string(), "B".to_string()],
            node_cell_type: vec![0, 0, 1],
            node_u: vec![0, 1, 0],
            node_v: vec![0, 0, 0],
            conn_types: vec![(0, 1)],
            conn_sign: vec![-1.0],
            edge_source: vec![0, 1],
            edge_target: vec![2, 2],
            edge_conn_type: vec![0, 0],
            edge_n_syn: vec![4.0, 6.0],
        }
    }

    #[test]
    fn accepts_a_consistent_network() {
        let n = TypedNetwork::new(parts()).expect("consistent parts must build");
        assert_eq!(n.num_nodes(), 3);
        assert_eq!(n.num_edges(), 2);
        assert_eq!(n.cell_types(), ["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn counts_distinct_lattice_positions() {
        // nodes sit at (0,0), (1,0), (0,0) -- two distinct positions.
        assert_eq!(TypedNetwork::new(parts()).unwrap().num_columns(), 2);
    }

    #[test]
    fn derives_weights_as_sign_times_n_syn_times_strength() {
        let n = TypedNetwork::new(parts()).unwrap();
        let mut out = vec![0.0; 2];
        n.weights_into(&[2.0], &mut out).unwrap();
        assert_eq!(out, vec![-8.0, -12.0]);
    }

    #[test]
    fn rejects_a_cell_type_index_out_of_range() {
        let mut p = parts();
        p.node_cell_type[1] = 7;
        assert_eq!(
            TypedNetwork::new(p),
            Err(NetworkError::CellTypeOutOfRange { node: 1, index: 7, count: 2 })
        );
    }

    #[test]
    fn rejects_an_edge_endpoint_past_the_node_count() {
        let mut p = parts();
        p.edge_target[0] = 9;
        assert_eq!(
            TypedNetwork::new(p),
            Err(NetworkError::NodeIndexOutOfRange { edge: 0, index: 9, count: 3 })
        );
    }

    #[test]
    fn rejects_a_connection_type_index_out_of_range() {
        let mut p = parts();
        p.edge_conn_type[1] = 3;
        assert_eq!(
            TypedNetwork::new(p),
            Err(NetworkError::ConnTypeOutOfRange { edge: 1, index: 3, count: 1 })
        );
    }

    #[test]
    fn rejects_a_sign_that_is_not_plus_or_minus_one() {
        let mut p = parts();
        p.conn_sign[0] = 0.0;
        assert_eq!(
            TypedNetwork::new(p),
            Err(NetworkError::BadSign { conn_type: 0, value: 0.0 })
        );
    }

    #[test]
    fn rejects_ragged_edge_arrays() {
        let mut p = parts();
        p.edge_n_syn.push(1.0);
        assert!(matches!(
            TypedNetwork::new(p),
            Err(NetworkError::EdgeArrayLengthMismatch { .. })
        ));
    }

    #[test]
    fn weights_into_rejects_a_wrong_sized_buffer() {
        let n = TypedNetwork::new(parts()).unwrap();
        let mut out = vec![0.0; 1];
        assert!(n.weights_into(&[2.0], &mut out).is_err());
    }

    #[test]
    fn weights_into_rejects_a_wrong_sized_parameter_vector() {
        let n = TypedNetwork::new(parts()).unwrap();
        let mut out = vec![0.0; 2];
        assert!(n.weights_into(&[2.0, 3.0], &mut out).is_err());
    }
}
