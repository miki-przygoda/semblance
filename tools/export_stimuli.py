"""Export flyvis's own rendered flash and dot stimuli as Rust test fixtures.

The Rust side regenerates these from parameters rather than loading them. Two
orderings decide whether that regeneration is right -- the 721-hexal lattice
order, and the order of sequences within each family -- and neither is
announced by the arrays themselves. flashes is intensity-major; dots is
centre-major. Both are easy to state confidently and wrongly, so this dumps
flyvis's own answer for the tests to compare against.

The dot family is 1,442 x 1,400 x 721 floats = 5.8 GB, far too large to dump.
A deterministic subset is exported instead: the first, middle (the origin) and
last centre in lattice order, both polarities each.

Run by hand:  uv run --project tools tools/export_stimuli.py
"""

import datetime
import json
import pathlib
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "flyvis-stimuli"

EXTENT = 15
DT = 1.0 / 200.0                 # what flyvis's own response analyses use
DOT_SUBSET_CENTRES = (0, 360, 720)   # first, the origin, last -- in lattice order


def main() -> int:
    import flyvis
    from flyvis.datasets.dots import Dots
    from flyvis.datasets.flashes import Flashes
    from flyvis.utils import hex_utils

    OUT.mkdir(parents=True, exist_ok=True)

    # -- the canonical hexal ordering ----------------------------------------
    u, v = hex_utils.get_hex_coords(EXTENT)
    assert len(u) == 721, len(u)
    assert (int(u[0]), int(v[0])) == (-15, 0), (u[0], v[0])
    centre = int(np.nonzero((u == 0) & (v == 0))[0][0])
    assert centre == 360, centre
    with (OUT / "hex_coords.csv").open("w") as f:
        f.write("index,u,v\n")
        for i, (uu, vv) in enumerate(zip(u, v)):
            f.write(f"{i},{int(uu)},{int(vv)}\n")
    print(f"  hex_coords.csv: {len(u)} rows, origin at index {centre}")

    # -- flashes -------------------------------------------------------------
    fl = Flashes(dt=DT)
    arr = np.asarray(fl.flashes_dir.flashes[:], dtype=np.float32)
    assert arr.shape == (4, 600, 721), arr.shape
    (OUT / "flashes.bin").write_bytes(arr.astype("<f4").tobytes())
    with (OUT / "flashes_args.csv").open("w") as f:
        f.write("index,baseline,intensity,radius\n")
        for i, r in fl.arg_df.iterrows():
            f.write(f"{i},{float(r.baseline)!r},{float(r.intensity)!r},{int(r.radius)}\n")
    print(f"  flashes.bin: {arr.shape}, {(OUT / 'flashes.bin').stat().st_size} bytes")
    print(f"  flashes_args.csv: {len(fl.arg_df)} rows")

    # -- dots: a deterministic subset ----------------------------------------
    d = Dots(dt=DT)
    assert len(d) == 1442, len(d)
    seqs = [c * 2 + p for c in DOT_SUBSET_CENTRES for p in (0, 1)]
    blobs, rows = [], []
    for s in seqs:
        item = np.asarray(d[s], dtype=np.float32)
        assert item.shape == (1400, 721), (s, item.shape)
        blobs.append(item.astype("<f4").tobytes())
        r = d.arg_df.iloc[s]
        rows.append((s, int(r.u), int(r.v), int(r.offset), float(r.intensity)))
    (OUT / "dots_subset.bin").write_bytes(b"".join(blobs))
    with (OUT / "dots_subset_args.csv").open("w") as f:
        f.write("order,sequence,u,v,offset,intensity\n")
        for i, (s, uu, vv, off, inten) in enumerate(rows):
            f.write(f"{i},{s},{uu},{vv},{off},{inten!r}\n")
    print(f"  dots_subset.bin: {len(seqs)} sequences, "
          f"{(OUT / 'dots_subset.bin').stat().st_size} bytes")

    # -- provenance ----------------------------------------------------------
    import torch

    (OUT / "PROVENANCE.json").write_text(
        json.dumps(
            {
                "exported": datetime.date.today().isoformat(),
                "flyvis": getattr(flyvis, "__version__", "?"),
                "torch": torch.__version__,
                "numpy": np.__version__,
                "extent": EXTENT,
                "dt": DT,
                "flashes_shape": list(arr.shape),
                "n_dot_sequences_total": len(d),
                "dot_subset_centres": list(DOT_SUBSET_CENTRES),
                "dot_subset_sequences": seqs,
            },
            indent=2,
        )
        + "\n"
    )
    print("  all assertions passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
