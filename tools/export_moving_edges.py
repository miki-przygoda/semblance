"""Export flyvis's own moving-edge renderings as Rust test fixtures.

The Rust side renders these analytically -- it rotates the sampling geometry
rather than the raster -- so unlike the flash and dot fixtures these are NOT
expected to match element-wise. They are the reference for a tolerance gate,
and the gate's job is to report the observed distribution, not to clear a
threshold quietly.

Three artifacts, for three different questions:

  moving_edge_offsets  what flyvis's renderer produces, for four angles chosen
                       to cover its three numerical regimes: 0 (no rotation at
                       all), 90 (axis-aligned), 30 and 150 (bilinear).
  hex_eye_owner        which ommatidium flyvis assigns each monitor pixel to.
                       flyvis builds this in float16 and we build it in f64, so
                       this localises a gate failure to the mask rather than to
                       the bar.
  moving_edge_timing   the resample indices and pad lengths. These ARE checked
                       element-wise -- only the geometry carries a tolerance.

Run by hand:  uv run --project tools tools/export_moving_edges.py
"""

import datetime
import inspect
import json
import pathlib
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "flyvis-stimuli"

DT = 1.0 / 200.0                 # what flyvis's own response analyses use
GATE_ANGLES = [0, 30, 90, 150]   # no rotation / bilinear / axis-aligned / bilinear
N_OMMATIDIA = 721
PPO = 25
MONITOR = 775


def main() -> int:
    import torch

    import flyvis
    from flyvis.datasets.moving_bar import MovingEdge
    from flyvis.datasets.rendering.eye import HexEye

    OUT.mkdir(parents=True, exist_ok=True)

    # -- flyvis's pixel-to-ommatidium mask ------------------------------------
    eye = HexEye(N_OMMATIDIA, PPO)
    is_inside = eye.is_inside
    assert tuple(is_inside.shape) == (MONITOR * MONITOR, N_OMMATIDIA), is_inside.shape
    n_owners = is_inside.sum(dim=1).cpu().numpy()
    assert n_owners.max() <= 1, f"a pixel is inside {n_owners.max()} ommatidia"
    owner = is_inside.float().argmax(dim=1).cpu().numpy().astype(np.int64)
    owner[n_owners == 0] = 65535
    (OUT / "hex_eye_owner.bin").write_bytes(owner.astype("<u2").tobytes())

    kernel_sum = eye.kernel_sum.cpu().numpy().astype(int)
    assert kernel_sum.shape == (N_OMMATIDIA,), kernel_sum.shape
    assert kernel_sum.min() > 0, "an ommatidium owns no pixels"
    (OUT / "hex_eye_kernel_sum.json").write_text(
        json.dumps(kernel_sum.tolist()) + "\n"
    )
    print(f"  hex_eye_owner.bin: {int((owner != 65535).sum())} owned, "
          f"{int((owner == 65535).sum())} unowned")
    print(f"  hex_eye_kernel_sum.json: min {kernel_sum.min()} max {kernel_sum.max()} "
          f"sum {kernel_sum.sum()}")

    # -- the rendered offsets, for four angles --------------------------------
    edge = MovingEdge(dt=DT, angles=GATE_ANGLES)
    offsets = np.asarray(edge.wrap.offsets[:], dtype=np.float64)
    # RenderedOffsets stacks `product(angles, widths, intensities)`; MovingEdge
    # has one width, so this is angle-major, intensity-minor.
    assert offsets.shape == (len(GATE_ANGLES) * 2, 21, N_OMMATIDIA), offsets.shape
    (OUT / "moving_edge_offsets.bin").write_bytes(
        offsets.astype("<f4").tobytes()
    )
    groups = [
        {"index": i, "angle": int(a), "intensity": float(v)}
        for i, (a, v) in enumerate((a, v) for a in GATE_ANGLES for v in (0.0, 1.0))
    ]
    (OUT / "moving_edge_offsets.json").write_text(
        json.dumps(
            {"shape": list(offsets.shape), "groups": groups, "dtype": "<f4"}, indent=2
        )
        + "\n"
    )
    print(f"  moving_edge_offsets.bin: {offsets.shape}, "
          f"{(OUT / 'moving_edge_offsets.bin').stat().st_size} bytes")

    # flyvis's background is 127/255, not 0.5 -- its image is cast to uint8
    # before rotation and 255 * 0.5 = 127.5 truncates. Assert it, so the gate's
    # expected floor is recorded rather than remembered.
    angle0_bright = offsets[1]          # angle 0, intensity 1
    assert np.isclose(angle0_bright.min(), 0.5, atol=1e-6), angle0_bright.min()
    angle30_bright = offsets[3]         # angle 30, intensity 1
    quantised = float(angle30_bright.min())
    print(f"  background at angle 0: {angle0_bright.min():.6f} (unrotated, exact)")
    print(f"  background at angle 30: {quantised:.6f} (127/255 = {127/255:.6f})")

    # -- timing ---------------------------------------------------------------
    n_angles = len(inspect.signature(MovingEdge.__init__).parameters["angles"].default)
    n_intensities = len(edge.intensities)
    assert n_angles == 12, f"flyvis's own MovingEdge.__init__ default has {n_angles} angles"
    assert n_intensities == 2, f"flyvis's own MovingEdge instance has {n_intensities} intensities"

    speeds = [float(s) for s in edge.speeds]
    t_stim = [float(t) for t in edge.t_stim]
    n_frames = int(next(iter(edge.sequences.values())).shape[-2])
    timing = []
    for speed, t in zip(speeds, t_stim):
        seq = edge.sequences[speed]
        finite = ~torch.isnan(seq[0]).any(dim=-1)
        n_valid = int(finite.sum())
        assert bool(finite[:n_valid].all()), "the NaN tail is not contiguous"
        n_stim = int(edge.indices[speed].shape[0])
        timing.append({
            "speed": speed,
            "t_stim": t,
            "n_stim": n_stim,
            "n_pre": int((t + edge.t_pre) / DT) - n_stim,
            "n_valid": n_valid,
            "indices": [int(i) for i in edge.indices[speed].cpu().numpy()],
        })
    (OUT / "moving_edge_timing.json").write_text(
        json.dumps(
            {
                "dt": DT,
                "n_frames": n_frames,
                "t_stim_max": float(edge.t_stim_max),
                "n_sequences_full_family": n_angles * n_intensities * len(speeds),
                "speeds": timing,
            },
            indent=2,
        )
        + "\n"
    )
    for row in timing:
        print(f"  speed {row['speed']:>5}: t_stim {row['t_stim']:.6f} "
              f"stim {row['n_stim']:>4} pre {row['n_pre']:>3} valid {row['n_valid']:>5} "
              f"of {n_frames}")

    # -- provenance -----------------------------------------------------------
    (OUT / "moving_edges_provenance.json").write_text(
        json.dumps(
            {
                "exported": datetime.date.today().isoformat(),
                "flyvis": getattr(flyvis, "__version__", "?"),
                "torch": torch.__version__,
                "numpy": np.__version__,
                "dt": DT,
                "gate_angles": GATE_ANGLES,
                "n_ommatidia": N_OMMATIDIA,
                "ppo": PPO,
                "monitor_px": MONITOR,
                "offsets_shape": list(offsets.shape),
            },
            indent=2,
        )
        + "\n"
    )
    print("  all assertions passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
