"""Self-tests for tools/export_reference.py. No flyvis import, no pytest.

Each test_* function is plain asserts. Tests that read real data under data/
skip when it is absent, unless SEMBLANCE_REQUIRE_FIXTURES=1.

Run by hand:  uv run --project tools tools/test_export_reference.py [test_name ...]
Validate an export:  uv run --project tools tools/test_export_reference.py --validate data/flyvis-reference
"""

import hashlib
import json
import os
import pathlib
import sys
import tempfile
import traceback

import numpy as np

import export_reference as er


class Skip(Exception):
    pass


def needs(*paths):
    for p in paths:
        if not p.exists():
            if os.environ.get("SEMBLANCE_REQUIRE_FIXTURES") == "1":
                raise AssertionError(f"SEMBLANCE_REQUIRE_FIXTURES=1 but {p} is missing")
            raise Skip(f"{p} not found")


def raises(fn, *needles):
    """fn() raises ExportError whose message contains every needle."""
    try:
        fn()
    except er.ExportError as e:
        for n in needles:
            assert n in str(e), f"{n!r} not in {str(e)!r}"
        return str(e)
    raise AssertionError("no ExportError raised")


def tmpdir() -> pathlib.Path:
    return pathlib.Path(tempfile.mkdtemp(prefix="export-reference-test-"))


# -- Task 1: command line, preflight, lock, parameter readers --------------------

def test_out_defaults_to_repo_data_and_inputs_never_follow_it():
    args = er.parse_args([])
    assert args.out == er.ROOT / "data" and args.force is False
    args = er.parse_args(["--out", "target/reref", "--force"])
    assert args.out == pathlib.Path.cwd() / "target" / "reref" and args.force is True
    for p in (er.RESULTS, er.ENSEMBLE_CSV, er.NODES_CSV, er.FLASHES_BIN, er.TIMING_JSON, er.PRETRAINED_RS):
        assert p.is_relative_to(er.ROOT), p
    assert er.DATA == er.ROOT / "data"


def test_preflight_passes_on_a_missing_or_empty_out():
    er.preflight(tmpdir() / "not-yet", False, {})
    er.preflight(tmpdir(), False, {})


def test_preflight_refuses_an_inherited_flyvis_root():
    raises(lambda: er.preflight(tmpdir(), False, {"FLYVIS_ROOT_DIR": "/elsewhere"}), "FLYVIS_ROOT_DIR")
    raises(lambda: er.preflight(tmpdir(), True, {"FLYVIS_ROOT_DIR": ""}), "FLYVIS_ROOT_DIR")


def test_preflight_refuses_existing_outputs_unless_forced():
    out = tmpdir()
    (out / "flyvis-reference").mkdir()
    raises(lambda: er.preflight(out, False, {}), str(out / "flyvis-reference"), "--force")
    er.preflight(out, True, {})


def test_preflight_names_a_lone_h80_file_without_the_reference_dir():
    # Review focus 4: a half-present previous set must still be refused.
    for name in er.H80_FILES:
        out = tmpdir()
        (out / "flyvis-stimuli").mkdir()
        (out / "flyvis-stimuli" / name).write_text("x")
        raises(lambda: er.preflight(out, False, {}), name)
        er.preflight(out, True, {})


def test_preflight_refuses_leftovers_even_when_forced():
    for prefix in er.LEFTOVER_PREFIXES:
        out = tmpdir()
        (out / f"{prefix}123").mkdir()
        msg = raises(lambda: er.preflight(out, True, {}), f"{prefix}123", "interrupted")
        assert "rename that .old-* back" in msg, msg


def test_a_second_export_cannot_take_the_lock():
    # Review focus 1: a second run must not wipe a running run's scratch root.
    path = tmpdir() / "sub" / "x.lock"
    first = er.acquire_lock(path)
    try:
        raises(lambda: er.acquire_lock(path), "another export is running")
    finally:
        first.close()
    er.acquire_lock(path).close()


SMALL_RS = '''
pub static CELL_TYPES: [&str; 65] = [
''' + "".join(f'    "T{i}",\n' for i in range(65)) + '''];

pub static CONN_TYPES: [(&str, &str); 604] = [
''' + "".join(f'    ("T{i % 65}", "T{(i + 1) % 65}"),\n' for i in range(604)) + '''];
'''


def _small_rs(counts):
    text = SMALL_RS
    for name, n in counts.items():
        vals = ", ".join(["0.5", "-1.25e-3", "0.0", "3.0"] * (n // 4) + ["1.0"] * (n % 4))
        text += f"\npub static {name}: [f32; {n}] = [\n    {vals},\n];\n"
    return text


def test_parse_pretrained_reads_exact_f32_bits():
    counts = {k: v for k, v in er.PRETRAINED_LENGTHS.items() if k not in ("CELL_TYPES", "CONN_TYPES")}
    p = er.parse_pretrained(_small_rs(counts))
    assert p["CELL_TYPES"][0] == "T0" and len(p["CELL_TYPES"]) == 65
    assert p["CONN_TYPES"][0] == ("T0", "T1") and len(p["CONN_TYPES"]) == 604
    assert p["BIAS"].dtype == np.float32 and len(p["BIAS"]) == 65
    assert p["BIAS"][1] == np.float32(-1.25e-3) and p["BIAS"][0] == np.float32(0.5)


def test_parse_pretrained_refuses_a_wrong_length():
    counts = {k: v for k, v in er.PRETRAINED_LENGTHS.items() if k not in ("CELL_TYPES", "CONN_TYPES")}
    counts["SIGN"] = 603
    raises(lambda: er.parse_pretrained(_small_rs(counts)), "SIGN[603]")


def test_parse_pretrained_real_file():
    p = er.parse_pretrained(er.PRETRAINED_RS.read_text())
    for name, n in er.PRETRAINED_LENGTHS.items():
        assert len(p[name]) == n, name
    assert p["CELL_TYPES"][0] == "R1" and p["CONN_TYPES"][0] == ("R1", "L1")
    assert p["BIAS"][0] == np.float32(0.44422865)
    assert p["SYN_COUNT"][-1] == np.float32(0.0) and p["SYN_COUNT"].dtype == np.float32


def _small_csv(kinds, n_models):
    lines = ["# a", "# b", "# c", "model,kind,index,name,value"]
    for m in range(n_models):
        for kind, n in kinds:
            lines += [f"{m:03d},{kind},{k},N{k},{0.1 * (m + 1):.7g}" for k in range(n)]
    return "\n".join(lines) + "\n"


def test_read_ensemble_checks_the_row_order():
    kinds = [("bias", 2), ("time_const", 2), ("syn_strength", 3)]
    e = er.read_ensemble(_small_csv(kinds, 2), kinds, 2)
    names, values = e["001"]["syn_strength"]
    assert names == ["N0", "N1", "N2"] and values.dtype == np.float32
    assert values[0] == np.float32(0.2)
    swapped = _small_csv(kinds, 2).replace("000,bias,1,", "000,bias,9,")
    raises(lambda: er.read_ensemble(swapped, kinds, 2), "expected 000,bias,1")
    raises(lambda: er.read_ensemble("# a\n# b\nmodel,kind,index,name,value\n", kinds, 2), "3 '#' lines")


def test_read_ensemble_real_file():
    needs(er.ENSEMBLE_CSV)
    text = er.ENSEMBLE_CSV.read_text()
    assert len(text.encode()) == 1_444_857
    e = er.read_ensemble(text)
    assert sorted(e) == [f"{m:03d}" for m in range(50)]
    p = er.parse_pretrained(er.PRETRAINED_RS.read_text())
    assert np.array_equal(e["000"]["bias"][1].view(np.uint32), p["BIAS"].view(np.uint32))
    assert e["000"]["bias"][0] == list(p["CELL_TYPES"])


# -- Task 2: storage layout and manifest -------------------------------------------

SIZES = {   # the spec's Sizes table
    "settled.f32.bin": 730_704, "settled.f64.bin": 1_461_408,
    "settling_centre.f32.bin": 130_000, "settling_centre.f64.bin": 260_000,
    "snapshots.f32.bin": 52_610_688, "snapshots.f64.bin": 105_221_376,
    "centre.f32.bin": 4_288_960, "centre.f64.bin": 8_577_920,
    "input.f64.bin": 47_574_464,
}


def _centre_nodes():
    return [(360 + 721 * i, f"T{i}") if i else (360, "R1") for i in range(65)]


def _manifest():
    files = er.file_layout()
    for f in files:
        f["sha256"] = "0" * 64
    return er.build_manifest(_centre_nodes(), files)


def test_file_layout_matches_the_sizes_table():
    files = er.file_layout()
    assert [f["name"] for f in files] == list(SIZES)
    assert {f["name"]: f["bytes"] for f in files} == SIZES
    assert sum(SIZES.values()) == 220_855_520
    assert sum(v for k, v in SIZES.items() if not k.startswith("input")) == 173_281_056


def test_blocks_are_contiguous_and_in_storage_order():
    for f in er.file_layout():
        offset = 0
        for b in f["blocks"]:
            assert b["offset"] == offset, (f["name"], b)
            assert b["bytes"] == int(np.prod(b["shape"])) * er.ITEMSIZE[f["dtype"]]
            offset += b["bytes"]
        assert offset == f["bytes"]
    centre = {f["name"]: f for f in er.file_layout()}["centre.f32.bin"]
    assert [(b["run"], b["sequence"]) for b in centre["blocks"][:13]] == [(0, k) for k in range(12)] + [(2, 0)]
    assert centre["blocks"][4]["shape"] == [525, 65] and centre["blocks"][12]["shape"] == [150, 65]
    inp = {f["name"]: f for f in er.file_layout()}["input.f64.bin"]
    assert [(b["run"], b["dt"], b["sequence"]) for b in inp["blocks"][11:13]] == [(None, "1/200", 11), (None, "1/50", 0)]


def test_sequence_table_matches_the_spec():
    seqs = er.sequence_table()
    assert len(seqs) == 12
    assert seqs[0]["args"] == {"baseline": 0.5, "intensity": 0.0, "radius": -1}
    assert seqs[3]["args"] == {"baseline": 0.5, "intensity": 1.0, "radius": 6}
    assert [(s["args"]["angle"], s["args"]["intensity"]) for s in seqs[4:]] == \
        [(0, 0.0), (0, 1.0), (30, 0.0), (30, 1.0), (90, 0.0), (90, 1.0), (150, 0.0), (150, 1.0)]
    assert seqs[4]["flyvis_index"] == 0 and seqs[11]["flyvis_index"] == 7
    assert seqs[5]["frames"]["1/50"] == {"n_frames": 131, "stimulus_frames": [50, 81],
                                         "snapshot_frames": [0, 26, 52, 78, 104, 130]}
    assert seqs[2]["frames"]["1/200"]["stimulus_frames"] == [200, 400]


def test_manifest_is_deterministic_and_valid():
    a, b = er.manifest_bytes(_manifest()), er.manifest_bytes(_manifest())
    assert a == b and a.endswith(b"}\n")
    m = json.loads(a)
    er.validate_manifest(m)
    assert list(m) == er.MANIFEST_KEYS
    assert m["dts"][0]["floor_dt_f32"] == 0.004999999888241291
    assert b'"floor_dt_f32": 0.019999999552965164' in a
    assert m["centre_nodes"][0] == {"node": 360, "cell_type": "R1"}
    assert "`" not in json.dumps(m["frame_semantics"]) and "**" not in json.dumps(m["frame_semantics"])


def test_validate_manifest_rejects_what_the_field_table_forbids():
    cases = [
        (lambda m: m["files"][0].update(dtype="<f8"), "dtype"),
        (lambda m: m["centre_nodes"].pop(), "64 centre nodes"),
        (lambda m: m["files"][2]["blocks"][1].update(offset=1), "block at 1"),
        (lambda m: m.update(extra=1), "top-level keys"),
        (lambda m: m["files"][0].update(sha256="ABC"), "sha256"),
        (lambda m: m["runs"].reverse(), "runs"),
        (lambda m: m.update(t_pre=1), "t_pre"),
    ]
    for mutate, needle in cases:
        m = json.loads(er.manifest_bytes(_manifest()))
        mutate(m)
        raises(lambda: er.validate_manifest(m), needle)


def test_validate_manifest_refuses_numpy_scalars():
    # Review focus 5: a numpy int or float would change the JSON or crash it.
    m = _manifest()
    m["files"][0]["blocks"][0]["shape"] = [np.int64(45669)]
    raises(lambda: er.validate_manifest(m), "int64")
    m = _manifest()
    m["sequences"][0]["args"]["intensity"] = np.float32(0.0)
    raises(lambda: er.validate_manifest(m), "float32")


def test_pack_file_keeps_bits_and_refuses_a_silent_cast():
    # Review focus 3: an f32 run's array must never be widened into the f64 file.
    entry = er._file_entry("x.f32.bin", "<f4", ["a"], [(0, "1/200", None, [2]), (2, "1/50", None, [1])])
    arrays = {(0, "1/200", None): np.array([1.0, -0.0], dtype=np.float32),
              (2, "1/50", None): np.array([0.5], dtype=np.float32)}
    data = er.pack_file(entry, arrays)
    assert data == bytes.fromhex("0000803f" "00000080" "0000003f")
    bad = dict(arrays)
    bad[(2, "1/50", None)] = np.array([0.5], dtype=np.float64)
    raises(lambda: er.pack_file(entry, bad), "float64", "<f4")
    bad[(2, "1/50", None)] = np.array([0.5, 0.5], dtype=np.float32)
    raises(lambda: er.pack_file(entry, bad), "shape")
    # The widening direction: a float32 array into an <f8 file is refused too.
    e64 = er._file_entry("x.f64.bin", "<f8", ["a"], [(1, "1/200", None, [1])])
    assert er.pack_file(e64, {(1, "1/200", None): np.array([0.5], dtype=np.float64)}) == bytes.fromhex("000000000000e03f")
    raises(lambda: er.pack_file(e64, {(1, "1/200", None): np.array([0.5], dtype=np.float32)}), "float32", "<f8")


def test_verify_files_catches_length_and_sha256():
    d = tmpdir()
    (d / "a.f32.bin").write_bytes(b"\0" * 8)
    entry = er._file_entry("a.f32.bin", "<f4", ["a"], [(0, "1/200", None, [2])])
    entry["sha256"] = er.sha256_file(d / "a.f32.bin")
    er.verify_files([entry], d)
    (d / "a.f32.bin").write_bytes(b"\0" * 7 + b"\1")
    raises(lambda: er.verify_files([entry], d), "sha256")
    (d / "a.f32.bin").write_bytes(b"\0" * 4)
    raises(lambda: er.verify_files([entry], d), "4 bytes")


def test_h80_sidecar_is_the_h9_format_plus_height():
    side = er.h80_sidecar()
    text = er.json_bytes(side).decode()
    assert '\n  "height": 80\n' in text, text[-40:]
    assert side["shape"] == [8, 21, 721] and side["dtype"] == "<f4"
    assert [(g["index"], g["angle"], g["intensity"]) for g in side["groups"][:3]] == [(0, 0, 0.0), (1, 0, 1.0), (2, 30, 0.0)]
    h9 = er.DATA / "flyvis-stimuli" / "moving_edge_offsets.json"
    needs(h9)
    assert json.loads(h9.read_text()) == {k: v for k, v in side.items() if k != "height"}


# -- Task 3: the atomic write --------------------------------------------------------

def _tiny_set(export_id="E1", h80_id=None, payload=b"\1\2\3\4"):
    entry = er._file_entry("a.f32.bin", "<f4", ["a"], [(0, "1/200", None, [1])])
    entry["sha256"] = hashlib.sha256(payload).hexdigest()
    manifest = {"files": [entry]}
    prov = er.json_bytes({"export_id": export_id})
    h80 = [("moving_edge_offsets_h80.bin", b"\0" * 4),
           ("moving_edge_offsets_h80.json", b"{}\n"),
           ("moving_edges_h80_provenance.json", er.json_bytes({"export_id": h80_id or export_id}))]
    return [("a.f32.bin", payload)], prov, manifest, h80


def _listing(root: pathlib.Path):
    return sorted((str(p.relative_to(root)), p.read_bytes() if p.is_file() else None)
                  for p in root.rglob("*"))


def _crash_at(point):
    def hook(p):
        if p == point:
            raise RuntimeError(f"crash at {p}")
    return hook


def test_write_into_a_missing_out_leaves_only_the_outputs():
    out = tmpdir() / "fresh"
    er.write_outputs(out, *_tiny_set(), "E1")
    names = sorted(str(p.relative_to(out)) for p in out.rglob("*"))
    assert names == ["flyvis-reference", "flyvis-reference/PROVENANCE.json", "flyvis-reference/a.f32.bin",
                     "flyvis-reference/manifest.json", "flyvis-stimuli",
                     "flyvis-stimuli/moving_edge_offsets_h80.bin", "flyvis-stimuli/moving_edge_offsets_h80.json",
                     "flyvis-stimuli/moving_edges_h80_provenance.json"], names
    assert (out / "flyvis-reference" / "manifest.json").read_bytes().endswith(b"\n")


def test_a_failure_before_the_swap_leaves_out_exactly_as_it_was():
    for point in ("staged", "verified"):
        out = tmpdir()
        (out / "flyvis-stimuli").mkdir()
        (out / "flyvis-stimuli" / "flashes.bin").write_bytes(b"keep")
        before = _listing(out)
        try:
            er.write_outputs(out, *_tiny_set(), "E1", hook=_crash_at(point))
        except RuntimeError:
            pass
        else:
            raise AssertionError("no crash")
        assert _listing(out) == before, point
    out = tmpdir() / "created-by-this-run"
    try:
        er.write_outputs(out, *_tiny_set(), "E1", hook=_crash_at("verified"))
    except RuntimeError:
        pass
    assert not out.exists()


def test_a_staged_file_that_differs_from_the_manifest_is_never_published():
    out = tmpdir()

    def corrupt(point):
        if point == "staged":
            staged = next(out.glob(".flyvis-reference.staging-*")) / "flyvis-reference" / "a.f32.bin"
            staged.write_bytes(b"\x09\x09\x09\x09")
    raises(lambda: er.write_outputs(out, *_tiny_set(), "E1", hook=corrupt), "sha256")
    assert _listing(out) == []


def test_force_replaces_the_previous_set_and_cleans_up():
    out = tmpdir()
    er.write_outputs(out, *_tiny_set("OLD", payload=b"\0\0\0\0"), "OLD")
    er.write_outputs(out, *_tiny_set("NEW"), "NEW")
    assert (out / "flyvis-reference" / "a.f32.bin").read_bytes() == b"\1\2\3\4"
    assert json.loads((out / "flyvis-stimuli" / "moving_edges_h80_provenance.json").read_text())["export_id"] == "NEW"
    assert sorted(p.name for p in out.iterdir()) == ["flyvis-reference", "flyvis-stimuli"]


def test_a_crash_between_the_two_renames_restores_the_previous_set():
    # Review focus 2.
    out = tmpdir()
    er.write_outputs(out, *_tiny_set("OLD", payload=b"\0\0\0\0"), "OLD")
    msg = raises(lambda: er.write_outputs(out, *_tiny_set("NEW"), "NEW", hook=_crash_at("old_moved")),
                 "crash at old_moved", "rename that .old-* back")
    assert (out / "flyvis-reference" / "a.f32.bin").read_bytes() == b"\0\0\0\0", msg
    left = sorted(p.name.rsplit("-", 1)[0] for p in out.iterdir() if p.name.startswith("."))
    assert left == [".flyvis-reference.incomplete", ".flyvis-reference.staging"], left
    raises(lambda: er.preflight(out, True, {}), "interrupted")


def test_a_crash_after_the_new_set_is_placed_deletes_nothing():
    out = tmpdir()
    er.write_outputs(out, *_tiny_set("OLD", payload=b"\0\0\0\0"), "OLD")
    raises(lambda: er.write_outputs(out, *_tiny_set("NEW"), "NEW", hook=_crash_at("reference_placed")),
           "crash at reference_placed")
    assert (out / "flyvis-reference" / "a.f32.bin").read_bytes() == b"\1\2\3\4"
    left = sorted(p.name.rsplit("-", 1)[0] for p in out.iterdir() if p.name.startswith("."))
    assert left == [".flyvis-reference.incomplete", ".flyvis-reference.old", ".flyvis-reference.staging"], left


def test_a_mismatched_export_id_pair_fails_the_final_check():
    out = tmpdir()
    raises(lambda: er.write_outputs(out, *_tiny_set("A", h80_id="B"), "A"), "export_id mismatch")
    assert any(p.name.startswith(".flyvis-reference.incomplete-") for p in out.iterdir())


def test_a_missing_stimuli_dir_is_created_only_at_step_5():
    out = tmpdir()
    seen = {}

    def look(point):
        seen[point] = (out / "flyvis-stimuli").exists()
    er.write_outputs(out, *_tiny_set(), "E1", hook=look)
    assert seen == {"staged": False, "verified": False, "old_moved": False,
                    "reference_placed": False, "h80_placed": True}, seen


def test_a_missing_parent_of_out_fails_before_anything_is_created():
    base = tmpdir()
    raises(lambda: er.write_outputs(base / "no" / "such" / "out", *_tiny_set(), "E1"), "parent")
    assert _listing(base) == []


# -- Task 4: models, checkpoints, freeze check -------------------------------------

def test_bits_equal_is_bitwise():
    a = np.array([0.0, 1.0], dtype=np.float32)
    assert er.bits_equal(a, a.copy())
    assert not er.bits_equal(a, np.array([-0.0, 1.0], dtype=np.float32))
    assert not er.bits_equal(a, a.astype(np.float64))
    assert not er.bits_equal(a, a[:1])
    nan = np.array([np.nan], dtype=np.float64)
    assert er.bits_equal(nan, nan.copy())


def test_max_abs_gap_widens_exactly():
    a = np.array([0.1], dtype=np.float32)
    assert er.max_abs_gap(a, a.astype(np.float64)) == 0.0
    assert er.max_abs_gap(np.array([1.0]), np.array([1.5])) == 0.5
    raises(lambda: er.max_abs_gap(np.zeros(2), np.zeros(3)), "shapes")


def _fake_ckpts():
    f = np.float32
    return {
        "000": {"time_const": np.array([0.019, 0.05, 0.02], f), "syn_strength": np.array([0, 0, 1e-5], f),
                "bias": np.array([-1, 2], f)},
        "001": {"time_const": np.array([0.019, 0.019, 0.5], f), "syn_strength": np.array([0, 0.1, 0.2], f),
                "bias": np.array([0, 0], f)},
        "049": {"time_const": np.array([0.03, 0.03, 0.03], f), "syn_strength": np.array([0, 1e-3, 0], f),
                "bias": np.array([0.5, 0.5], f)},
    }


def test_freeze_figures_counts_what_the_spec_names():
    clamps = {m: {"bias": None, "time_const": None, "syn_strength": "non_negative"} for m in ("000", "001", "049")}
    fig = er.freeze_figures(_fake_ckpts(), clamps)
    tc, ss, bias = fig["time_const"], fig["syn_strength"], fig["bias"]
    assert tc["below_f32_0.02_total"] == 3 and tc["below_f32_0.02_000"] == 1 and tc["below_f32_0.02_049"] == 0
    assert tc["below_f32_0.02_per_model"] == {"min": 0, "median": 1, "max": 2}
    assert tc["in_f32_0.02_to_0.02"] == 1          # f32(0.02) itself sits in the gap
    assert tc["largest_below_floor"] == float(np.float32(0.019))
    assert ss["exact_zeros"] == 5 and ss["zeros_000"] == 2 and ss["zeros_049"] == 2
    assert ss["conn_types_zero_in_some_model"] == 3 and ss["conn_types_zero_in_all_models"] == 1
    assert ss["smallest_positive"] == float(np.float32(1e-5))
    assert ss["positive_at_or_below_1e-4"] == 1 and ss["positive_at_or_below_1e-2"] == 2
    assert bias == {"min": -1.0, "max": 2.0, "clamp": ["None"]}
    assert ss["clamp"] == ["non_negative"]


def test_check_freeze_reports_the_first_mismatch():
    clamps = {m: {"bias": None, "time_const": None, "syn_strength": "non_negative"} for m in ("000", "001", "049")}
    fig = er.freeze_figures(_fake_ckpts(), clamps)
    raises(lambda: er.check_freeze(fig), "time_const.below_f32_0.02_total is 3, expected 2396")
    er.check_freeze(fig, {("syn_strength", "exact_zeros"): 5})


def test_copy_models_copies_only_the_load_path_and_checks_it():
    src, dst = tmpdir(), tmpdir()
    for mid in ("000", "049"):
        d = src / mid
        (d / "chkpts").mkdir(parents=True)
        (d / "validation").mkdir()
        (d / "__cache__").mkdir()
        (d / "chkpts" / "chkpt_00000").write_bytes(mid.encode())
        (d / "validation" / "loss.h5").write_bytes(b"l")
        for f in ("_meta.yaml", "best_chkpt", "validation_loss.h5"):
            (d / f).write_bytes(f.encode())
        (d / "__cache__" / "junk").write_bytes(b"j")
    shas = er.copy_models(src, dst, ["000", "049"])
    assert sorted(p.name for p in (dst / "000").iterdir()) == sorted(er.MODEL_ENTRIES)
    assert shas["049"]["original_sha256"] == shas["049"]["scratch_sha256"] == er.sha256_file(src / "049" / er.CHECKPOINT)


def test_models_rank_as_recorded():
    needs(er.RESULTS)
    losses = er.read_validation_losses(er.RESULTS)
    assert len(losses) == 50
    er.rank_models(losses)
    bad = dict(losses)
    bad["020"] = 0.0
    raises(lambda: er.rank_models(bad), "best 020")


def test_all_50_checkpoints_and_the_freeze_figures():
    needs(er.RESULTS, er.ENSEMBLE_CSV)
    pre = er.parse_pretrained(er.PRETRAINED_RS.read_text())
    ens = er.read_ensemble(er.ENSEMBLE_CSV.read_text())
    ids = sorted(er.read_validation_losses(er.RESULTS))
    ckpts = {m: er.load_checkpoint(er.RESULTS / m / er.CHECKPOINT) for m in ids}
    er.check_checkpoints(ckpts, ens, pre)
    fig = er.freeze_figures(ckpts, er.read_clamps(er.RESULTS, ids))
    er.check_freeze(fig)
    assert fig["time_const"]["largest_below_floor"] == float(np.float32(0.019996209))
    assert fig["time_const"]["smallest_at_or_above_floor"] == float(np.float32(0.020369984))
    ref = er.reference_params("049", pre, ens)
    assert er.bits_equal(ckpts["049"]["syn_strength"], ref["syn_strength"])
    ckpts["007"]["syn_strength"] = ckpts["007"]["syn_strength"].copy()
    ckpts["007"]["syn_strength"][0] = -0.0 if ckpts["007"]["syn_strength"][0] == 0 else 0.0
    raises(lambda: er.check_checkpoints(ckpts, ens, pre), "checkpoint 007 syn_strength")


# -- runner (keep last) ---------------------------------------------------------

def main(argv) -> int:
    if argv[:1] == ["--validate"]:
        return validate_dir(pathlib.Path(argv[1]))
    names = argv or sorted(n for n in globals() if n.startswith("test_"))
    passed = skipped = failed = 0
    for name in names:
        try:
            globals()[name]()
            passed += 1
            print(f"ok    {name}")
        except Skip as e:
            skipped += 1
            print(f"skip  {name}: {e}")
        except Exception:
            failed += 1
            print(f"FAIL  {name}")
            traceback.print_exc()
    print(f"{passed} passed, {skipped} skipped, {failed} failed")
    return 1 if failed else 0


def validate_dir(directory: pathlib.Path) -> int:
    """The manifest field table and every file's length and sha256, on a
    real export directory."""
    m = json.loads((directory / "manifest.json").read_text())
    er.validate_manifest(m)
    er.verify_files(m["files"], directory)
    names = sorted(p.name for p in directory.iterdir())
    want = sorted([f["name"] for f in m["files"]] + ["manifest.json", "PROVENANCE.json"])
    assert names == want, names
    print(f"{directory}: manifest valid, {len(m['files'])} files verified, {len(names)} files in all")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
