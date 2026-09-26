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
    # (largest known-points-minus-hull-vertices).
    chosen = {
        max(fill_stats, key=lambda r: r[1])[0],
        min(fill_stats, key=lambda r: r[1])[0],
        max(fill_stats, key=lambda r: r[1] - r[2])[0],
    }
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
