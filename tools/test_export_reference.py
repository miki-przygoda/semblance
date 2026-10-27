"""Self-tests for tools/export_reference.py. No flyvis import, no pytest.

Each test_* function is plain asserts. Tests that read real data under data/
skip when it is absent, unless SEMBLANCE_REQUIRE_FIXTURES=1.

Run by hand:  uv run --project tools tools/test_export_reference.py [test_name ...]
Validate an export:  uv run --project tools tools/test_export_reference.py --validate data/flyvis-reference
"""

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
