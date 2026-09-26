//! Acceptance criteria for the expansion.
//!
//! Two gates against flyvis's own build (`tools/export_build.py`), and one
//! against the figures published in the paper. Kept out of `expand.rs` because
//! these are the criteria for the whole epic rather than unit tests of a
//! function.
//!
//! All of them skip when their data is absent. See `chore-and-fixes/` for the
//! CI gap that makes that a known hazard.

use crate::expand::{expand, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL};
use crate::spec::ConnectomeSpec;
use crate::test_support::{load_build_bytes, load_build_fixture, load_real_spec};
use semblance_connectome::TypedNetwork;

fn real_network() -> Option<TypedNetwork> {
    let raw = load_real_spec()?;
    let spec = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");
    Some(expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).expect("real spec must expand"))
}

/// Stage 3: the published figures.
#[test]
fn matches_the_published_figures() {
    let Some(n) = real_network() else { return };

    assert_eq!(n.num_columns(), 721, "distinct (u, v) lattice positions");
    assert_eq!(n.num_nodes(), 45_669, "neurons");
    assert_eq!(n.num_edges(), 1_513_231, "realised synapses");
    assert_eq!(n.conn_types().len(), 604, "realised connection types");
    assert_eq!(n.cell_types().len(), 65, "nodes in the spec");

    // CT1 appears twice, as two compartments of one biological cell.
    let biological: std::collections::BTreeSet<&str> = n
        .cell_types()
        .iter()
        .map(|t| t.split('(').next().unwrap())
        .collect();
    assert_eq!(biological.len(), 64, "biological cell types");

    // Realised offset groups: distinct (conn_type, du, dv).
    let groups: std::collections::BTreeSet<(u16, i16, i16)> = (0..n.num_edges())
        .map(|i| {
            let s = n.edge_source()[i] as usize;
            let t = n.edge_target()[i] as usize;
            (
                n.edge_conn_type()[i],
                n.node_u()[t] - n.node_u()[s],
                n.node_v()[t] - n.node_v()[s],
            )
        })
        .collect();
    assert_eq!(groups.len(), 2_355, "realised offset groups");
}

/// The declared-versus-realised gap, stated explicitly. This is where a smoke
/// test silently passes on the wrong invariant.
#[test]
fn one_declared_connection_type_realises_nothing() {
    let Some(raw) = load_real_spec() else { return };
    let spec = ConnectomeSpec::from_json_str(&raw).unwrap();
    let n = expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).unwrap();

    assert_eq!(spec.edges.len(), 605, "declared connection types");
    assert_eq!(n.conn_types().len(), 604, "realised connection types");

    let realised: std::collections::BTreeSet<(&str, &str)> = n
        .conn_types()
        .iter()
        .map(|&(a, b)| (n.cell_types()[a as usize].as_str(), n.cell_types()[b as usize].as_str()))
        .collect();
    assert!(!realised.contains(&("Lawf1", "Lawf1")), "Lawf1 -> Lawf1 must drop out");
}

/// Stage 2: every edge, against flyvis's own build.
#[test]
fn every_edge_matches_flyvis() {
    let (Some(n), Some(bytes)) = (real_network(), load_build_bytes("edges.bin")) else {
        return;
    };
    assert_eq!(bytes.len() % 12, 0, "edges.bin must be a whole number of records");
    assert_eq!(bytes.len() / 12, n.num_edges(), "edge count");

    for (i, rec) in bytes.chunks_exact(12).enumerate() {
        let src = u32::from_le_bytes(rec[0..4].try_into().unwrap());
        let tgt = u32::from_le_bytes(rec[4..8].try_into().unwrap());
        let n_syn = f32::from_le_bytes(rec[8..12].try_into().unwrap());
        assert_eq!(n.edge_source()[i], src, "source of edge {i}");
        assert_eq!(n.edge_target()[i], tgt, "target of edge {i}");
        assert!((n.edge_n_syn()[i] - n_syn).abs() < 1e-6, "n_syn of edge {i}");
    }
}

/// The connection type ordering, which the trained parameters index into.
#[test]
fn connection_type_ordering_matches_flyvis() {
    let (Some(n), Some(csv)) = (real_network(), load_build_fixture("conn_types.csv")) else {
        return;
    };
    let want: Vec<(String, String, f32)> = csv
        .lines()
        .skip(1)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            (f[1].to_string(), f[2].to_string(), f[3].trim().parse().unwrap())
        })
        .collect();

    assert_eq!(want.len(), n.conn_types().len());
    for (i, (a, b, sign)) in want.iter().enumerate() {
        let (ai, bi) = n.conn_types()[i];
        assert_eq!(&n.cell_types()[ai as usize], a, "source of connection type {i}");
        assert_eq!(&n.cell_types()[bi as usize], b, "target of connection type {i}");
        assert_eq!(n.conn_sign()[i], *sign, "sign of connection type {i}");
    }
}

/// Node ordering, which 0.3.0's per-neuron trace comparison depends on.
#[test]
fn node_ordering_matches_flyvis() {
    let (Some(n), Some(csv)) = (real_network(), load_build_fixture("nodes.csv")) else {
        return;
    };
    let rows: Vec<(String, i16, i16)> = csv
        .lines()
        .skip(1)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            (f[1].to_string(), f[2].parse().unwrap(), f[3].trim().parse().unwrap())
        })
        .collect();

    assert_eq!(rows.len(), n.num_nodes());
    for (i, (t, u, v)) in rows.iter().enumerate() {
        assert_eq!(&n.cell_types()[n.node_cell_type()[i] as usize], t, "type of node {i}");
        assert_eq!(n.node_u()[i], *u, "u of node {i}");
        assert_eq!(n.node_v()[i], *v, "v of node {i}");
    }
}

/// The trained parameters and the expanded network must agree on ordering, or
/// every weight attaches to the wrong synapses.
///
/// `pretrained::CELL_TYPES` and `CONN_TYPES` are generated by
/// `tools/export_params.py`. The expanded network's orderings come from
/// `expand()` and are checked against `tools/export_build.py`'s CSVs. Those
/// are **two separate export scripts**, and nothing else in the suite compares
/// them to each other. They derive their orderings the same way today --
/// first appearance over flyvis's built edge arrays -- but that agreement was
/// stated only in a doc comment until this test existed. A silent reordering
/// between the two scripts would misattribute all 604 trained `syn_strength`
/// values, and all 65 biases and time constants, onto a network that still
/// builds, still counts correctly, and still passes every other gate here.
#[test]
fn trained_parameter_ordering_matches_the_expanded_network() {
    let Some(n) = real_network() else { return };

    assert_eq!(
        n.cell_types().len(),
        crate::pretrained::CELL_TYPES.len(),
        "cell type count"
    );
    for (i, name) in n.cell_types().iter().enumerate() {
        assert_eq!(name, crate::pretrained::CELL_TYPES[i], "cell type {i}");
    }

    assert_eq!(
        n.conn_types().len(),
        crate::pretrained::CONN_TYPES.len(),
        "connection type count"
    );
    for (j, &(a, b)) in n.conn_types().iter().enumerate() {
        let (want_src, want_tar) = crate::pretrained::CONN_TYPES[j];
        assert_eq!(n.cell_types()[a as usize], want_src, "source of connection type {j}");
        assert_eq!(n.cell_types()[b as usize], want_tar, "target of connection type {j}");
    }
}

/// Weight derivation over the real network, using the real trained parameters.
///
/// Deliberately weak on its own: "finite, some positive, some negative" would
/// still hold even if `SYN_STRENGTH` were permuted onto the wrong connection
/// types. `trained_parameter_ordering_matches_the_expanded_network`, above, is
/// what makes these weights trustworthy -- it is the test that actually pins
/// the ordering `weights_into` relies on.
#[test]
fn derives_weights_over_the_real_network() {
    let Some(n) = real_network() else { return };
    let mut out = vec![0.0f32; n.num_edges()];
    n.weights_into(&crate::pretrained::SYN_STRENGTH, &mut out)
        .expect("604 trained strengths must fit 604 connection types");

    assert!(out.iter().all(|w| w.is_finite()), "every weight must be finite");
    assert!(out.iter().any(|&w| w > 0.0), "some weights must be excitatory");
    assert!(out.iter().any(|&w| w < 0.0), "some weights must be inhibitory");
}
