"""Self-tests for tools/export_indices.py. No flyvis import, no pytest.

Run by hand:  uv run --project tools tools/test_export_indices.py [test_name ...]
"""

import json
import pathlib
import sys
import tempfile
import traceback

import numpy as np

import export_indices as ei


def test_dedup_shares_identical_rows_without_merging_stimuli():
    grey = np.full(721, 0.5)
    on = np.ones(721)
    a = np.stack([grey, grey, on, grey])
    b = np.stack([on, on])
    frames, index = ei.dedup_frames([a, b])
    assert frames.shape == (2, 721) and frames.dtype == np.float64
    assert index[0].tolist() == [0, 0, 1, 0] and index[0].dtype == np.uint32
    assert index[1].tolist() == [1, 1]
    for x, ix in zip([a, b], index):
        assert np.array_equal(frames[ix], x)


def test_dedup_distinguishes_bitwise_not_numerically():
    pos, neg = np.zeros(721), np.zeros(721)
    neg[3] = -0.0
    frames, index = ei.dedup_frames([np.stack([pos, neg])])
    assert frames.shape[0] == 2 and index[0].tolist() == [0, 1]


def test_edge_tables_cover_every_speed():
    assert sorted(ei.EDGE_WINDOW_END) == sorted(ei.SPEEDS) == sorted(ei.EDGE_VALID_FRAMES)
    assert ei.EDGE_WINDOW_END == {2.4: 878, 4.8: 539, 9.7: 367, 13: 325, 19: 285, 25: 265}
    assert ei.EDGE_VALID_FRAMES == {2.4: 1078, 4.8: 739, 9.7: 567, 13: 525, 19: 485, 25: 465}
    for s in ei.SPEEDS:
        assert ei.EDGE_WINDOW_END[s] < ei.EDGE_VALID_FRAMES[s]


def manifest_example():
    flashes = [ei.flash_entry(k, 0.5, i, r, 600) for k, (i, r) in enumerate([(0, -1), (0, 6), (1, -1), (1, 6)])]
    edges = [ei.edge_entry(0, 0, 0, 2.4, 80, 1078)]
    arrays = [ei.array_entry("fri", "f32", "<f4", [50, 65])]
    return ei.build_manifest(
        cell_types=[f"T{i}" for i in range(65)],
        flashes=flashes,
        edges=edges,
        n_frames_rows=3,
        frame_index_blocks=[ei.index_block("flashes", k, 600 * k, 600) for k in range(4)]
        + [ei.index_block("edges", 0, 2400, 1078)],
        arrays=arrays,
        sha256={"frames.f64.bin": "0" * 64},
    )


def test_manifest_round_trips_and_validates():
    m = manifest_example()
    ei.validate_manifest(m)
    again = json.loads(ei.manifest_bytes(m))
    assert again == m
    assert m["batteries"]["flashes"][1]["window"] == [200, 400]
    assert m["batteries"]["edges"][0]["window"] == [200, 878]


def test_validate_manifest_refuses_a_shape_that_disagrees_with_its_bytes():
    m = manifest_example()
    m["arrays"][0]["bytes"] = 1
    try:
        ei.validate_manifest(m)
    except ei.ExportError as e:
        assert "fri" in str(e)
    else:
        raise AssertionError("a wrong byte count was accepted")


def test_write_set_refuses_a_populated_out_without_force():
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        out.mkdir()
        (out / "old").write_text("x")
        try:
            ei.write_set(out, {"a.bin": b"\x00"}, manifest_example(), {}, force=False)
        except ei.ExportError:
            pass
        else:
            raise AssertionError("overwrote a populated --out")
        assert (out / "old").exists() and not (out / "a.bin").exists()


def test_write_set_replaces_the_whole_set_with_force():
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        out.mkdir()
        (out / "old").write_text("x")
        ei.write_set(out, {"a.bin": b"\x00"}, manifest_example(), {"host": "h"}, force=True)
        assert sorted(p.name for p in out.iterdir()) == ["PROVENANCE.json", "a.bin", "manifest.json"]
        assert not list(pathlib.Path(d).glob("set.tmp*"))


def main(argv):
    names = argv[1:] or [n for n in globals() if n.startswith("test_")]
    failed = 0
    for n in names:
        try:
            globals()[n]()
            print(f"ok   {n}")
        except Exception:
            failed += 1
            print(f"FAIL {n}")
            traceback.print_exc()
    print(f"{len(names) - failed} passed, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
