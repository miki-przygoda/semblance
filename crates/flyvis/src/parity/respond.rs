//! Responses for any network: settle it as the 0.3.0 gates do, play a
//! battery from the settled state, and record each cell type's central
//! value at every frame.
//!
//! Spec: docs/superpowers/specs/2026-11-04-graded-responses-design.md,
//! section 2.

use super::run::mismatch;
use super::{Fixtures, ParityError};

/// For each cell type, in network order, its node at `(u, v) = (0, 0)`:
/// the central column flyvis's indices read. Derived from the network, so
/// scoring needs no reference traces, and checked against the manifest's
/// `centre_nodes`, which must name the same nodes in the same order.
pub fn central_nodes(f: &Fixtures) -> Result<Vec<usize>, ParityError> {
    let net = &f.net;
    let mut found: Vec<Option<usize>> = vec![None; net.cell_types().len()];
    let at_origin = net.node_u().iter().zip(net.node_v()).zip(net.node_cell_type()).enumerate();
    for (node, ((&u, &v), &t)) in at_origin {
        if u == 0 && v == 0 && found[t as usize].replace(node).is_some() {
            return Err(mismatch(format!("cell type {} has more than one node at (0, 0)", net.cell_types()[t as usize])));
        }
    }
    let nodes = found
        .iter()
        .enumerate()
        .map(|(t, n)| n.ok_or_else(|| mismatch(format!("cell type {} has no node at (0, 0)", net.cell_types()[t]))))
        .collect::<Result<Vec<usize>, ParityError>>()?;
    let manifest: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    if manifest != nodes {
        return Err(mismatch(format!(
            "the network's central nodes {nodes:?} differ from the manifest's centre_nodes {manifest:?}"
        )));
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_support::fixture;

    #[test]
    fn the_central_nodes_are_the_manifests_centre_nodes() {
        let Some(f) = fixture() else { return };
        let nodes = central_nodes(f).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(nodes.len(), f.network().cell_types().len());
        for (k, c) in f.manifest().centre_nodes.iter().enumerate() {
            assert_eq!(f.network().cell_types()[f.network().node_cell_type()[nodes[k]] as usize], c.cell_type);
        }
    }
}
