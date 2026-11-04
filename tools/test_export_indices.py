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


def set_example():
    """A small, self-consistent set: its data files and their manifest."""
    flashes = [ei.flash_entry(k, 0.5, i, r, 600) for k, (i, r) in enumerate([(0, -1), (0, 6), (1, -1), (1, 6)])]
    edges = [ei.edge_entry(0, 0, 0, 2.4, 80, 1078)]
    frames = np.stack([np.full(721, 0.5), np.ones(721), np.zeros(721)])
    index = np.zeros(4 * 600 + 1078, dtype="<u4")
    index[5] = 2
    files = {
        "frames.f64.bin": frames.astype("<f8").tobytes(),
        "frame_index.u32.bin": index.tobytes(),
        "fri.f32.bin": np.zeros((50, 65), dtype="<f4").tobytes(),
        "fri.f64.bin": np.zeros((2, 65), dtype="<f8").tobytes(),
    }
    arrays = [ei.array_entry("fri", "f32", "<f4", [50, 65]), ei.array_entry("fri", "f64", "<f8", [2, 65])]
    m = ei.build_manifest(
        cell_types=[f"T{i}" for i in range(65)],
        flashes=flashes,
        edges=edges,
        n_frames_rows=3,
        frame_index_blocks=[ei.index_block("flashes", k, 600 * k, 600) for k in range(4)]
        + [ei.index_block("edges", 0, 2400, 1078)],
        arrays=arrays,
        sha256={name: ei.hashlib_sha256(data) for name, data in files.items()},
    )
    return files, m


def manifest_example():
    return set_example()[1]


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


def test_validate_manifest_refuses_an_incomplete_sha256_map():
    m = manifest_example()
    del m["sha256"]["fri.f64.bin"]
    refuses(lambda: ei.validate_manifest(m), "sha256")


def test_validate_manifest_refuses_a_frame_index_that_skips_a_stimulus():
    m = manifest_example()
    del m["frame_index"]["blocks"][2]
    refuses(lambda: ei.validate_manifest(m), "frame_index")


def test_validate_manifest_refuses_a_leading_axis_that_is_not_the_models():
    m = manifest_example()
    m["arrays"][1] = ei.array_entry("fri", "f64", "<f8", [50, 65])
    refuses(lambda: ei.validate_manifest(m), "leading axis")


def test_write_set_refuses_a_frame_index_past_the_frames():
    files, m = set_example()
    index = np.frombuffer(files["frame_index.u32.bin"], dtype="<u4").copy()
    index[7] = 3
    files["frame_index.u32.bin"] = index.tobytes()
    m["sha256"]["frame_index.u32.bin"] = ei.hashlib_sha256(files["frame_index.u32.bin"])
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        refuses(lambda: ei.write_set(out, files, m, {}, force=False), "frame_index reaches row 3")
        assert not out.exists()


def test_write_set_refuses_a_file_that_differs_from_its_hash():
    files, m = set_example()
    files["fri.f32.bin"] = b"\x01" + files["fri.f32.bin"][1:]
    with tempfile.TemporaryDirectory() as d:
        refuses(lambda: ei.write_set(pathlib.Path(d) / "set", files, m, {}, force=False), "fri.f32.bin")


def test_write_set_refuses_a_populated_out_without_force():
    files, m = set_example()
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        out.mkdir()
        (out / "old").write_text("x")
        refuses(lambda: ei.write_set(out, files, m, {}, force=False), "not empty")
        assert (out / "old").exists() and not (out / "fri.f32.bin").exists()


def test_write_set_replaces_the_whole_set_with_force():
    files, m = set_example()
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "set"
        out.mkdir()
        (out / "old").write_text("x")
        ei.write_set(out, files, m, {"host": "h"}, force=True)
        assert sorted(p.name for p in out.iterdir()) == sorted([*files, "PROVENANCE.json", "manifest.json"])
        assert not list(pathlib.Path(d).glob("set.tmp*"))


def test_check_valid_frames_finds_nan_inside_and_values_after():
    r = np.zeros((2, 10, 3))
    r[:, 6:, :] = np.nan
    ei.check_valid_frames(r, [6, 6], "edges")            # exact: passes
    bad = r.copy()
    bad[1, 2, 0] = np.nan
    for x, n in [(bad, [6, 6]), (r, [6, 7]), (r, [5, 6])]:
        try:
            ei.check_valid_frames(x, n, "edges")
        except ei.ExportError:
            continue
        raise AssertionError(f"accepted {n}")


# -- shards and the merge -----------------------------------------------------------

def refuses(fn, fragment):
    try:
        fn()
    except ei.ExportError as e:
        assert fragment in str(e), f"refused, but for another reason: {e}"
    else:
        raise AssertionError(f"accepted; expected a refusal mentioning {fragment!r}")


PASSED = {"windows": "passed", "link_to_fixtures2": "passed", "thread_invariance": "passed"}


def fake_shard(name, models, grey=0.5, checks=True):
    """A shard as run_models returns it, with one flash, one edge and two
    small arrays per model: fri [65] and dsi_to_known [] (float64 at f32,
    as flyvis returns it)."""
    arrays = {}
    for p, ids in models.items():
        for mid in ids:
            dt = np.float32 if p == "f32" else np.float64
            arrays.setdefault("fri", {}).setdefault(p, {})[mid] = np.full(65, int(mid) / 100, dtype=dt)
            arrays.setdefault("dsi_to_known", {}).setdefault(p, {})[mid] = np.asarray(int(mid) / 10, dtype=np.float64)
    ran_000 = [p for p, ids in models.items() if "000" in ids] if checks else []
    return {
        "name": name, "models": models, "arrays": arrays,
        "cell_types": [f"T{i}" for i in range(65)], "tuning_types": list(ei.PD_TO_KNOWN_TYPES),
        "tables": {"flashes": [ei.flash_entry(0, 0.5, 1, 6, 401)], "edges": [ei.edge_entry(0, 0, 1, 25.0, 80, 300)]},
        "inputs": [np.full((401, 721), grey), np.vstack([np.full((100, 721), 0.5), np.ones((200, 721))])],
        "timing": {f"{m}.{p}": 1.0 for p, ids in models.items() for m in ids}, "check_seconds": {},
        "checks": {p: dict(PASSED) for p in ran_000},
        "batch4": {p: {"fri": {"bit_equal": True, "max_abs_gap": 0.0, "nonfinite_mismatches": 0}} for p in ran_000},
        "environment": {"host": "h", "machine": "m", "flyvis": "1.2.0", "torch": "t", "numpy": np.__version__,
                        "uv_lock_sha256": "0" * 64, "git": {"head": "abc", "dirty": False}, "threads": 1,
                        "batch_size": 1, "models_sha256": {}, "seconds": 0.0},
    }


def complete_shards():
    return [fake_shard("a", {"f32": ei.MODELS[:25]}),
            fake_shard("b", {"f32": ei.MODELS[25:]}),
            fake_shard("c", {"f64": ["000", "049"]})]


def merge_from_disk(d, shards):
    dirs = []
    for sh in shards:
        ei.save_shard(pathlib.Path(d) / sh["name"], sh)
        dirs.append(pathlib.Path(d) / sh["name"])
    return ei.merge([ei.load_shard(x) for x in dirs])


def test_merge_writes_a_complete_set_in_model_order():
    with tempfile.TemporaryDirectory() as d:
        files, m, prov = merge_from_disk(d, complete_shards())
        out = pathlib.Path(d) / "set"
        ei.write_set(out, files, m, prov, force=False)
        m = json.loads((out / "manifest.json").read_text())
        ei.validate_manifest(m)
        assert m["models"] == {"f32": ei.MODELS, "f64": ["000", "049"]}
        shapes = {(a["name"], a["precision"]): (a["shape"], a["dtype"]) for a in m["arrays"]}
        assert shapes[("fri", "f32")] == ([50, 65], "<f4") and shapes[("fri", "f64")] == ([2, 65], "<f8")
        assert shapes[("dsi_to_known", "f32")] == ([50], "<f8")
        fri = np.fromfile(out / "fri.f32.bin", dtype="<f4").reshape(50, 65)
        assert fri[37, 0] == np.float32(0.37) and fri[49, 0] == np.float32(0.49)
        assert m["frames"]["rows"] == 2           # grey and white
        assert prov["checks"]["f64"]["shard"] == "c" and prov["f32_f64_gap"]["models"] == ["000", "049"]


def test_merge_refuses_a_missing_model():
    shards = complete_shards()
    shards[1] = fake_shard("b", {"f32": ei.MODELS[25:49]})
    with tempfile.TemporaryDirectory() as d:
        refuses(lambda: merge_from_disk(d, shards), "missing ['049']")


def test_merge_refuses_a_duplicate_model():
    shards = complete_shards()
    shards[1] = fake_shard("b", {"f32": ei.MODELS[24:]})
    with tempfile.TemporaryDirectory() as d:
        refuses(lambda: merge_from_disk(d, shards), "model 024 at f32")


def test_merge_refuses_mismatched_inputs():
    shards = complete_shards()
    shards[2] = fake_shard("c", {"f64": ["000", "049"]}, grey=0.25)
    with tempfile.TemporaryDirectory() as d:
        refuses(lambda: merge_from_disk(d, shards), "inputs")


def test_merge_refuses_a_missing_model_000_check():
    shards = complete_shards()
    shards[2] = fake_shard("c", {"f64": ["000", "049"]}, checks=False)
    with tempfile.TemporaryDirectory() as d:
        refuses(lambda: merge_from_disk(d, shards), "model 000's checks at f64")


def test_save_shard_never_overwrites():
    with tempfile.TemporaryDirectory() as d:
        out = pathlib.Path(d) / "s"
        out.mkdir()
        refuses(lambda: ei.save_shard(out, fake_shard("s", {"f32": ["000"]})), "never overwritten")


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
