# Connectome Expansion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand flyvis's generative connectome spec into a typed 45,669-neuron / 1,513,231-synapse network in Rust, matching flyvis's own build element for element.

**Architecture:** A new `TypedNetwork` type in `semblance-connectome` holds the expanded network as flat arrays plus two index maps (`edge -> conn_type`, `node -> cell_type`) that carry the 734 trained parameters. The expansion itself lives in `semblance-flyvis`, which gains `semblance-connectome` as a dependency, because the tiling rules and hull conventions are flyvis's quirks and do not belong in a general graph type. Three gates, from most isolating to least: the convex hull and point-in-polygon rule are checked against oracles dumped from scipy and matplotlib; then the full network element-wise; then the published figures.

**Tech Stack:** Rust 1.85, edition 2024, zero external crates. Python via `uv` in `tools/` for fixture export only, run by hand.

**Spec:** `docs/superpowers/specs/2026-09-25-connectome-expansion-design.md`

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` must stay at exactly 8 `semblance*` packages. `std` is not external; `HashMap` is fine. Adding any crates.io dependency, even for tests, is forbidden.
- **Rust 1.85+, edition 2024.** `gen` is a reserved keyword.
- **Commit style: plain imperative subject, no `feat:`/`fix:`/`chore:` prefixes.** Match the existing log.
- **Do not run `cargo fmt`.** The repo is not rustfmt-clean and the tool reformats unrelated crates. Format new code by hand to match surrounding style.
- **Do not modify** `Connectome`, `codec`, `fidelity`, `sim`, `evo` or `app`. This work is purely additive. If an existing signature seems to need changing, stop and report it.
- **Clippy has 5 pre-existing warnings** (4 `needless_range_loop` in `sim`, 1 manual char comparison in `connectome`). Do not count those as new; do not introduce more.
- **Tests that need gitignored data must skip, not fail**, following `crates/flyvis/src/test_support.rs`.
- `data/` is gitignored. Never `git add` anything under it.

---

### Task 1: Export flyvis's build fixtures

**Files:**
- Create: `tools/export_build.py`
- Modify: `tools/README.md` (add the new script to its list)
- Output (gitignored): `data/flyvis-build/{nodes.csv,conn_types.csv,offsets.csv,hulls.csv,containment_oracle.csv,edges.bin,PROVENANCE.json}`

**Interfaces:**
- Consumes: nothing.
- Produces: the fixture files above. Later tasks read them. Exact formats are fixed here and must not be changed later without updating every reader.

Formats, all UTF-8, LF, with a header row for CSVs:

| File | Columns / layout |
|---|---|
| `nodes.csv` | `index,cell_type,u,v` — 45,669 rows, build order |
| `conn_types.csv` | `index,source_type,target_type,sign` — 604 rows, first-appearance order |
| `offsets.csv` | `edge_spec_index,src,tar,du,dv,n_syn,filled` — 2,378 rows; `filled` is `0` for declared, `1` for hull-filled |
| `hulls.csv` | `edge_spec_index,vertex_order,du,dv` — scipy's hull vertices, in scipy's order. Supplies the polygon for the containment oracle, and the superset our hull is checked against |
| `containment_oracle.csv` | `edge_spec_index,u,v,inside` — every grid point and matplotlib's verdict, for 3 chosen edge types |
| `edges.bin` | little-endian, repeating `u32 source, u32 target, f32 n_syn` — 1,513,231 records, 18,158,772 bytes |

- [ ] **Step 1: Write the export script**

```python
"""Export flyvis's own built connectome as fixtures for the Rust expansion.

The Rust side reimplements flyvis's build. Two pieces of it cannot be derived
from first principles -- scipy's convex hull and matplotlib's point-in-polygon
rule -- so this script dumps their outputs directly. That turns "we hope the
hull matches" into a test that names the exact offset group that disagrees.

Run by hand:  uv run --project tools tools/export_build.py
"""

import datetime
import json
import pathlib
import struct
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "flyvis-build"

EXTENT = 15
N_SYN_FILL = 1


def hull_of(known_pts):
    """Exactly what flyvis's fill_hull computes, exposed so we can dump it."""
    import scipy.spatial as ss

    hull = ss.ConvexHull((1 + 1e-6) * known_pts, False, "QJ")
    return known_pts[hull.vertices]


def grid_of(known_pts):
    """The integer grid fill_hull tests, in fill_hull's own u-major order."""
    return np.concatenate(
        np.dstack(
            np.mgrid[
                known_pts[:, 0].min() : known_pts[:, 0].max() + 1,
                known_pts[:, 1].min() : known_pts[:, 1].max() + 1,
            ]
        )
    )


def main() -> int:
    import matplotlib.path as mp

    import flyvis
    from flyvis.connectome import ConnectomeFromAvgFilters

    OUT.mkdir(parents=True, exist_ok=True)
    spec = json.loads(pathlib.Path(flyvis.connectome_file).read_text())
    c = ConnectomeFromAvgFilters(extent=EXTENT, n_syn_fill=N_SYN_FILL)

    dec = lambda a: [
        x.decode() if isinstance(x, (bytes, np.bytes_)) else str(x) for x in a
    ]

    # -- nodes ---------------------------------------------------------------
    types, us, vs = dec(c.nodes.type[:]), c.nodes.u[:], c.nodes.v[:]
    with (OUT / "nodes.csv").open("w") as f:
        f.write("index,cell_type,u,v\n")
        for i, (t, u, v) in enumerate(zip(types, us, vs)):
            f.write(f"{i},{t},{u},{v}\n")
    print(f"  nodes.csv: {len(types)} rows")

    # -- connection types, in first-appearance order over built edges --------
    src_t, tar_t = dec(c.edges.source_type[:]), dec(c.edges.target_type[:])
    signs = c.edges.sign[:]
    conn, seen = [], {}
    for a, b, s in zip(src_t, tar_t, signs):
        if (a, b) not in seen:
            seen[(a, b)] = len(conn)
            conn.append((a, b, int(s)))
    with (OUT / "conn_types.csv").open("w") as f:
        f.write("index,source_type,target_type,sign\n")
        for i, (a, b, s) in enumerate(conn):
            f.write(f"{i},{a},{b},{s}\n")
    print(f"  conn_types.csv: {len(conn)} rows")

    # -- offsets, hulls, containment oracle ----------------------------------
    off_rows, hull_rows, fill_stats = [], [], []
    for ei, e in enumerate(spec["edges"]):
        declared = {tuple(o[0]) for o in e["offsets"]}
        for (du, dv), n_syn in e["offsets"]:
            off_rows.append((ei, e["src"], e["tar"], du, dv, n_syn, 0))
        if N_SYN_FILL > 0 and len(e["offsets"]) >= 3:
            known = np.array([o[0] for o in e["offsets"]])
            verts = hull_of(known)
            for vi, (du, dv) in enumerate(verts):
                hull_rows.append((ei, vi, int(du), int(dv)))
            grid = grid_of(known)
            inside = mp.Path(verts).contains_points(grid)
            for (u, v) in grid[inside]:
                if (u, v) not in declared:
                    off_rows.append((ei, e["src"], e["tar"], int(u), int(v), N_SYN_FILL, 1))
            fill_stats.append((ei, len(known), len(verts), len(grid)))

    with (OUT / "offsets.csv").open("w") as f:
        f.write("edge_spec_index,src,tar,du,dv,n_syn,filled\n")
        for r in off_rows:
            f.write("{},{},{},{},{},{!r},{}\n".format(*r))
    print(f"  offsets.csv: {len(off_rows)} rows")

    with (OUT / "hulls.csv").open("w") as f:
        f.write("edge_spec_index,vertex_order,du,dv\n")
        for r in hull_rows:
            f.write("{},{},{},{}\n".format(*r))
    print(f"  hulls.csv: {len(hull_rows)} rows over {len(fill_stats)} edge types")

    # Three oracle cases, chosen by deterministic rule so this is reproducible:
    # most offsets, fewest offsets, and the most collinear-looking hull
    # (largest known-points-minus-hull-vertices). Two criteria can pick the
    # same edge type -- they did on the real data -- so a criterion that
    # collides takes its next-best candidate instead. `sorted` is stable, so
    # ties fall back to edge-spec order and the choice stays reproducible.
    ranked = [
        sorted(fill_stats, key=lambda r: -r[1]),           # most offsets
        sorted(fill_stats, key=lambda r: r[1]),            # fewest offsets
        sorted(fill_stats, key=lambda r: -(r[1] - r[2])),  # most collinear
    ]
    chosen = []
    for order in ranked:
        for r in order:
            if r[0] not in chosen:
                chosen.append(r[0])
                break
    assert len(chosen) == 3, chosen
    with (OUT / "containment_oracle.csv").open("w") as f:
        f.write("edge_spec_index,u,v,inside\n")
        for ei in sorted(chosen):
            known = np.array([o[0] for o in spec["edges"][ei]["offsets"]])
            verts = hull_of(known)
            grid = grid_of(known)
            inside = mp.Path(verts).contains_points(grid)
            for (u, v), ins in zip(grid, inside):
                f.write(f"{ei},{u},{v},{int(ins)}\n")
    print(f"  containment_oracle.csv: edge types {sorted(chosen)}")

    # -- edges ---------------------------------------------------------------
    si, ti = c.edges.source_index[:], c.edges.target_index[:]
    ns = c.edges.n_syn[:]
    with (OUT / "edges.bin").open("wb") as f:
        for a, b, n in zip(si, ti, ns):
            f.write(struct.pack("<IIf", int(a), int(b), float(n)))
    print(f"  edges.bin: {len(si)} records, {(OUT / 'edges.bin').stat().st_size} bytes")

    # -- provenance ----------------------------------------------------------
    import scipy
    import matplotlib

    (OUT / "PROVENANCE.json").write_text(
        json.dumps(
            {
                "exported": datetime.date.today().isoformat(),
                "flyvis": getattr(flyvis, "__version__", "?"),
                "scipy": scipy.__version__,
                "matplotlib": matplotlib.__version__,
                "numpy": np.__version__,
                "extent": EXTENT,
                "n_syn_fill": N_SYN_FILL,
                "connectome_file": str(flyvis.connectome_file),
                "n_nodes": len(types),
                "n_edges": len(si),
                "n_conn_types": len(conn),
                "n_offsets": len(off_rows),
                "oracle_edge_types": sorted(chosen),
            },
            indent=2,
        )
        + "\n"
    )

    # -- self-check against the published figures ----------------------------
    assert len(types) == 45669, len(types)
    assert len(si) == 1513231, len(si)
    assert len(conn) == 604, len(conn)
    assert len(off_rows) == 2378, len(off_rows)
    assert len(spec["edges"]) == 605, len(spec["edges"])
    assert len(set(types)) == 65, len(set(types))
    print("  all published figures match")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 2: Run it**

Run: `uv run --project tools tools/export_build.py`
Expected: prints row counts for each file and `all published figures match`, exit 0.

If an assert fires, **stop and report** — a mismatch here means the reference build differs from the published figures and nothing downstream can be trusted.

- [ ] **Step 3: Verify the outputs on disk**

Run:
```bash
ls -la data/flyvis-build/
wc -l data/flyvis-build/*.csv
head -3 data/flyvis-build/offsets.csv data/flyvis-build/hulls.csv
```
Expected: `edges.bin` is exactly 18158772 bytes; `nodes.csv` 45,670 lines (header + rows); `offsets.csv` 2,379; `conn_types.csv` 605.

- [ ] **Step 4: Note the script in `tools/README.md`**

Add a row describing `export_build.py` alongside the existing scripts, matching the file's existing table or list style.

- [ ] **Step 5: Commit**

```bash
git add tools/export_build.py tools/README.md
git commit -m "Add an exporter for flyvis's own built connectome"
```

Do **not** add anything under `data/`.

---

### Task 2: `TypedNetwork` in `semblance-connectome`

**Files:**
- Create: `crates/connectome/src/typed.rs`
- Modify: `crates/connectome/src/lib.rs` (add `pub mod typed;` and a re-export)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `semblance_connectome::typed::TypedNetwork`
  - `semblance_connectome::typed::TypedNetworkParts` — a plain struct of the 10 arrays, all fields `pub`
  - `semblance_connectome::typed::NetworkError`
  - `TypedNetwork::new(parts: TypedNetworkParts) -> Result<TypedNetwork, NetworkError>`
  - `TypedNetwork::weights_into(&self, syn_strength: &[f32], out: &mut [f32]) -> Result<(), NetworkError>`
  - accessors: `num_nodes`, `num_edges`, `num_columns`, `cell_types`, `conn_types`, `conn_sign`, `node_cell_type`, `node_u`, `node_v`, `edge_source`, `edge_target`, `edge_conn_type`, `edge_n_syn`

- [ ] **Step 1: Write the failing tests**

Create `crates/connectome/src/typed.rs` with only the test module for now:

```rust
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
```

Add to `crates/connectome/src/lib.rs`, near the other module-level items:

```rust
pub mod typed;

pub use typed::{NetworkError, TypedNetwork, TypedNetworkParts};
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-connectome`
Expected: FAIL to compile — `TypedNetwork`, `TypedNetworkParts`, `NetworkError` not found.

- [ ] **Step 3: Write the implementation**

Put this **above** the test module in `crates/connectome/src/typed.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-connectome`
Expected: PASS, all tests including the pre-existing ones.

- [ ] **Step 5: Check clippy**

Run: `cargo clippy -p semblance-connectome --all-targets`
Expected: the 1 pre-existing manual-char-comparison warning only. No new warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/connectome/src/typed.rs crates/connectome/src/lib.rs
git commit -m "Add TypedNetwork, an expanded network with type-shared parameters"
```

---

### Task 3: Hex-lattice node placement

**Files:**
- Create: `crates/flyvis/src/expand.rs`
- Modify: `crates/flyvis/src/lib.rs` (add `pub mod expand;`)
- Modify: `crates/flyvis/Cargo.toml` (add the `semblance-connectome` dependency)

**Interfaces:**
- Consumes: `crate::spec::{NodeSpec, Pattern}` from Task 2's untouched existing code.
- Produces:
  - `pub(crate) struct PlacedNode { pub cell_type: u16, pub u: i32, pub v: i32 }`
  - `pub(crate) fn place_nodes(nodes: &[NodeSpec], extent: i32) -> Vec<PlacedNode>`
  - `pub const DEFAULT_EXTENT: i32 = 15;`
  - `pub const DEFAULT_N_SYN_FILL: f32 = 1.0;`

- [ ] **Step 1: Add the dependency**

In `crates/flyvis/Cargo.toml`, under `[dependencies]`:

```toml
semblance-connectome.workspace = true
```

`semblance-connectome` is already declared in the workspace `[workspace.dependencies]`, so nothing else changes. `Cargo.lock` must still show 8 packages.

- [ ] **Step 2: Write the failing tests**

Create `crates/flyvis/src/expand.rs` with only the test module:

```rust
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
```

Add to `crates/flyvis/src/lib.rs`, after `mod json;`:

```rust
pub mod expand;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis expand`
Expected: FAIL to compile — `place_nodes`, `PlacedNode`, `DEFAULT_EXTENT` not found.

- [ ] **Step 4: Write the implementation**

Put this **above** the test module in `crates/flyvis/src/expand.rs`:

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis expand`
Expected: PASS, 6 tests.

- [ ] **Step 6: Verify the lockfile did not grow**

Run: `grep -c '^name = ' Cargo.lock`
Expected: `8`.

- [ ] **Step 7: Commit**

```bash
git add crates/flyvis/src/expand.rs crates/flyvis/src/lib.rs crates/flyvis/Cargo.toml Cargo.lock
git commit -m "Place flyvis's node types on the hex lattice"
```

---

### Task 4: Convex hull, gated against scipy's own vertices

**Files:**
- Modify: `crates/flyvis/src/expand.rs`
- Modify: `crates/flyvis/src/test_support.rs` (add a fixture loader)

**Interfaces:**
- Consumes: `place_nodes`, `PlacedNode` from Task 3.
- Produces:
  - `pub(crate) fn convex_hull(points: &[(i32, i32)]) -> Vec<(i32, i32)>` — counter-clockwise, no collinear interior points, starting at the lexicographically smallest point.
  - `pub(crate) fn load_build_fixture(name: &str) -> Option<String>` in `test_support`

**Why the gate:** scipy's qhull runs with the `QJ` joggle, which perturbs input points and can therefore admit collinear boundary points as vertices where a plain monotone chain would drop them. That interacts with the half-open containment rule exactly at boundary points, which on an integer lattice are common. Rather than reason about it, compare against what scipy actually produced.

- [ ] **Step 1: Add the fixture loader**

Append to `crates/flyvis/src/test_support.rs`:

```rust
/// Loads a fixture exported from flyvis's own build, or `None` if the export
/// has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../data/flyvis-build/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_build.py` to generate it."
            );
            None
        }
    }
}
```

- [ ] **Step 2: Write the failing tests**

Add to the `mod tests` block in `crates/flyvis/src/expand.rs`:

```rust
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
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis expand`
Expected: FAIL to compile — `convex_hull` not found.

- [ ] **Step 4: Write the implementation**

Add to `crates/flyvis/src/expand.rs`, after `place_nodes`:

```rust
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis expand -- --nocapture`
Expected: PASS. If `data/flyvis-build/` is absent the scipy gate prints its skip message and returns — that is expected on a machine without the export, but **the implementer must run Task 1 first**, so it should execute here.

If `hull_is_a_subset_of_scipys_vertices` fails, **stop and report the edge spec index**. Do not adjust the `<=` in `cross` to make it pass -- a *superset* would mean the chain is keeping points qhull rejected, which is a different bug from the known collinear difference.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/expand.rs crates/flyvis/src/test_support.rs
git commit -m "Add a convex hull over lattice offsets, checked against scipy"
```

---

### Task 5: Haines containment and `fill_hull`

**Files:**
- Modify: `crates/flyvis/src/expand.rs`

**Interfaces:**
- Consumes: `convex_hull` from Task 4, `crate::spec::Offset`.
- Produces:
  - `pub(crate) fn contains_point(poly: &[(i32, i32)], p: (i32, i32)) -> bool`
  - `pub(crate) fn fill_hull(offsets: &[Offset], n_syn_fill: f32) -> Vec<Offset>`

**The rule being implemented** is Haines' `CrossingsMultiplyTest` from *Graphics Gems IV*, which is what matplotlib's `Path.contains_points` uses. It was checked against matplotlib over every grid point of every filled edge type while this plan was written: **0 mismatches in 3,425 points.**

Two properties are load-bearing and easy to lose:

- **It multiplies; it does not divide.** The familiar `pnpoly` formulation that computes `x < (xj-xi)*(y-yi)/(yj-yi) + xi` is a *different* algorithm at the boundary. Substituting it disagrees with matplotlib on **877 of those same 3,425 points**.
- **It is winding-sensitive.** The `== yflag1` comparison means reversing a polygon's orientation changes which boundary points count as inside. matplotlib behaves identically. A division-based test cannot reproduce this at all.

Boundary behaviour, verified against matplotlib for a counter-clockwise axis-aligned square: both vertical edges inside, top inside, bottom outside. That asymmetry is why the edge count is 1,513,231 rather than 1,517,786 or 1,453,484.

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `crates/flyvis/src/expand.rs`. Note `Offset` is
imported at module scope in Step 3 and reaches the test module via `use super::*`,
so do not import it again here:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis expand`
Expected: FAIL to compile — `contains_point`, `fill_hull` not found.

- [ ] **Step 3: Write the implementation**

Add to `crates/flyvis/src/expand.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis expand -- --nocapture`
Expected: PASS.

If `filled_offsets_match_flyvis_exactly` fails, the message names the edge spec index. **Stop and report it** with the first few disagreeing offsets. Do not tune the comparison operators until the totals line up — that is fitting to the answer, and the two gates above exist precisely so the cause can be localised instead.

- [ ] **Step 5: Check clippy**

Run: `cargo clippy -p semblance-flyvis --all-targets`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/expand.rs
git commit -m "Fill offset hulls using matplotlib's crossings-multiply rule"
```

---

### Task 6: Edge construction and `expand`

**Files:**
- Modify: `crates/flyvis/src/expand.rs`
- Modify: `crates/flyvis/src/lib.rs` (re-export `expand`)

**Interfaces:**
- Consumes: `place_nodes`, `fill_hull`, `DEFAULT_EXTENT`, `DEFAULT_N_SYN_FILL` from Tasks 3-5; `TypedNetwork`, `TypedNetworkParts`, `NetworkError` from Task 2.
- Produces:
  - `pub fn expand(spec: &ConnectomeSpec, extent: i32, n_syn_fill: f32) -> Result<TypedNetwork, ExpandError>`
  - `pub enum ExpandError { UnknownCellType { edge_spec_index: usize, name: String }, TooManyTypes { count: usize }, Network(NetworkError) }`

- [ ] **Step 1: Write the failing tests**

Add to the `mod tests` block in `crates/flyvis/src/expand.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis expand`
Expected: FAIL to compile — `expand`, `ExpandError` not found.

- [ ] **Step 3: Write the implementation**

Add to `crates/flyvis/src/expand.rs`:

```rust
use crate::spec::ConnectomeSpec;
use semblance_connectome::{NetworkError, TypedNetwork, TypedNetworkParts};
use std::collections::HashMap;

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
```

Add to `crates/flyvis/src/lib.rs`, at the `pub use` line:

```rust
pub use expand::{expand, ExpandError, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL};
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p semblance-flyvis expand`
Expected: PASS.

- [ ] **Step 5: Check clippy and the whole workspace**

Run:
```bash
cargo clippy -p semblance-flyvis --all-targets
cargo test --workspace
```
Expected: no new clippy warnings; all workspace tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/flyvis/src/expand.rs crates/flyvis/src/lib.rs
git commit -m "Expand the flyvis spec into a typed network"
```

---

### Task 7: The acceptance gates

**Files:**
- Create: `crates/flyvis/src/expand_checks.rs`
- Modify: `crates/flyvis/src/lib.rs` (add `#[cfg(test)] mod expand_checks;`)
- Modify: `CLAUDE.md` (update the test count)
- Modify: `docs/project-outline/0.2.0-flyvis-import/README.md` (mark tasks 6 and 7 done)

**Interfaces:**
- Consumes: `expand`, `DEFAULT_EXTENT`, `DEFAULT_N_SYN_FILL`, `load_real_spec`, `load_build_fixture`.
- Produces: nothing consumed by later tasks.

A separate file, following `pretrained_checks.rs`: these are acceptance criteria for the whole expansion, and keeping them out of the implementation module makes that visible.

- [ ] **Step 1: Write the failing tests**

Create `crates/flyvis/src/expand_checks.rs`:

```rust
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
/// Deliberately weak on its own -- it pins only finiteness and that both signs
/// occur. What makes the weights trustworthy is
/// `trained_parameter_ordering_matches_the_expanded_network` above, which is
/// what guarantees `SYN_STRENGTH[j]` reaches connection type `j`.
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
```

- [ ] **Step 2: Add the binary fixture loader**

Append to `crates/flyvis/src/test_support.rs`:

```rust
/// Loads a binary fixture exported from flyvis's own build, or `None` if the
/// export has not been run. See `tools/export_build.py`.
pub(crate) fn load_build_bytes(name: &str) -> Option<Vec<u8>> {
    let path = format!(
        "{}/../../data/flyvis-build/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!(
                "skipping: {path} not found. Run \
                 `uv run --project tools tools/export_build.py` to generate it."
            );
            None
        }
    }
}
```

`expand_checks.rs` already imports it.

Add to `crates/flyvis/src/lib.rs`, beside the other `#[cfg(test)]` module:

```rust
#[cfg(test)]
mod expand_checks;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p semblance-flyvis expand_checks`
Expected: FAIL to compile first; once compiling, any assertion failure names the exact edge, node or connection type that disagrees.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p semblance-flyvis -- --nocapture`
Expected: PASS. `every_edge_matches_flyvis` walks 1,513,231 records, so allow it time in debug; if it is unreasonably slow, run `cargo test -p semblance-flyvis --release`.

If any gate fails, **stop and report** which one and the first disagreement. Do not relax an assertion.

- [ ] **Step 5: Run the whole workspace**

Run:
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run --quiet -- roundtrip
```
Expected: all pass; `roundtrip` prints exactly `0.000000` for both distances; clippy shows only the 5 pre-existing warnings.

- [ ] **Step 6: Update the documented test count**

Count the tests actually reported by `cargo test --workspace` and update the two numbers in `CLAUDE.md`:
- the `cargo test --workspace # 57 tests, 0 failures` line
- the "4 of those tests skip when `data/flyvis-supplementary/` is absent" line — the new gates also skip without `data/flyvis-build/`, so state both directories.

**Do not guess these numbers.** Read them from the test output.

- [ ] **Step 7: Mark the epic tasks done**

In `docs/project-outline/0.2.0-flyvis-import/README.md`, mark Task 6 and Task 7 as **DONE** with their commit hashes, matching how Tasks 4 and 5 are recorded. This file is gitignored — edit it, but do not `git add` it.

- [ ] **Step 8: Commit**

```bash
git add crates/flyvis/src/expand_checks.rs crates/flyvis/src/test_support.rs crates/flyvis/src/lib.rs CLAUDE.md
git commit -m "Check the expanded network against flyvis and the published figures"
```
