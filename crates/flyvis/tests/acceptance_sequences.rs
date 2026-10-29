//! Acceptance tests for behaviour group `sequences` of epic stimulus-traces:
//! reading the reference manifest's `sequences`.
//!
//! One test per behaviour id (SEQUENCES-01 .. SEQUENCES-09), each driving the
//! crate's public `semblance_flyvis::reference` API (`parse_sequences`,
//! `SequenceInfo`, `SequenceFrames`, `Manifest::parse`).
//!
//! SEQUENCES-07 and SEQUENCES-08 also run the crate's own tests through
//! cargo, in a target directory under `CARGO_TARGET_TMPDIR`: SEQUENCES-07
//! runs the real-manifest fixture test in a copy of the workspace's sources
//! whose `data/flyvis-reference/manifest.json` gives one sequence five
//! snapshot frames (the real `data/` is never written); SEQUENCES-08 runs
//! `reference::tests` and `acceptance_reference.rs` as they stand. Tests that
//! read the real manifest skip without it unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

use semblance_flyvis::reference::{parse_sequences, Manifest, ReferenceError, SequenceFrames, SequenceInfo};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const MANIFEST: &str = "flyvis-reference/manifest.json";

fn data_path(rel: &str) -> PathBuf {
    Path::new(WORKSPACE).join("data").join(rel)
}

fn fixtures_required() -> bool {
    std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1")
}

fn read_fixture(rel: &str) -> Option<String> {
    let path = data_path(rel);
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {} is missing", path.display());
            eprintln!("skipping: {} not found; run ci/fetch-fixtures.sh", path.display());
            None
        }
    }
}

/// Two sequences: a flash with both dts, and a moving edge whose args mix
/// numeric and non-numeric entries.
const SEQUENCES: &str = r#"{"sequences":[
 {"index":0,"family":"flashes","flyvis_index":0,
  "args":{"baseline":0.5,"intensity":0.0,"radius":-1},
  "frames":{"1/200":{"n_frames":600,"stimulus_frames":[200,400],"snapshot_frames":[0,119,239,359,479,599]},
            "1/50":{"n_frames":150,"stimulus_frames":[50,100],"snapshot_frames":[0,29,59,89,119,149]}}},
 {"index":5,"family":"moving_edge","flyvis_index":1,
  "args":{"angle":0,"intensity":1.0,"speed":13.0,"width":80,"height":80,"offsets":[-10,11],
          "t_pre":1.0,"t_post":1.0,"post_pad_mode":"continue","bar_loc_horizontal":0.0},
  "frames":{"1/200":{"n_frames":525,"stimulus_frames":[200,325],"snapshot_frames":[0,104,209,314,419,524]},
            "1/50":{"n_frames":131,"stimulus_frames":[50,81],"snapshot_frames":[0,26,52,78,104,130]}}}]}"#;

fn parsed() -> Vec<SequenceInfo> {
    parse_sequences(SEQUENCES).expect("the synthetic sequences are valid")
}

fn refused(text: &str, what: &str) -> ReferenceError {
    match parse_sequences(text) {
        Ok(s) => panic!("{what}: must be refused, parsed {s:?}"),
        Err(e) => e,
    }
}

/// `SEQUENCES` with `from` replaced once by `to`, checking that it occurs.
fn edit(from: &str, to: &str) -> String {
    assert!(SEQUENCES.contains(from), "test bug: {from:?} not in SEQUENCES");
    SEQUENCES.replacen(from, to, 1)
}

// ---------------------------------------------------------------------------
// SEQUENCES-01 .. SEQUENCES-07: the parser
// ---------------------------------------------------------------------------

/// SEQUENCES-01: one `SequenceInfo` per entry, with one `SequenceFrames` per
/// dt key holding `n_frames`, `stimulus_frames` [start, end) and
/// `snapshot_frames`.
#[test]
fn sequences_01_one_sequence_info_per_entry_with_one_frame_layout_per_dt() {
    let s = parsed();
    assert_eq!(s.len(), 2, "one SequenceInfo per entry");

    assert_eq!((s[0].index, s[0].family.as_str(), s[0].flyvis_index), (0, "flashes", 0));
    assert_eq!((s[1].index, s[1].family.as_str(), s[1].flyvis_index), (5, "moving_edge", 1));

    assert_eq!(
        s[0].frames,
        vec![
            SequenceFrames {
                dt: "1/200".into(),
                n_frames: 600,
                stimulus_frames: (200, 400),
                snapshot_frames: vec![0, 119, 239, 359, 479, 599],
            },
            SequenceFrames {
                dt: "1/50".into(),
                n_frames: 150,
                stimulus_frames: (50, 100),
                snapshot_frames: vec![0, 29, 59, 89, 119, 149],
            },
        ]
    );
    assert_eq!(
        s[1].frames,
        vec![
            SequenceFrames {
                dt: "1/200".into(),
                n_frames: 525,
                stimulus_frames: (200, 325),
                snapshot_frames: vec![0, 104, 209, 314, 419, 524],
            },
            SequenceFrames {
                dt: "1/50".into(),
                n_frames: 131,
                stimulus_frames: (50, 81),
                snapshot_frames: vec![0, 26, 52, 78, 104, 130],
            },
        ]
    );

    // An empty list is zero sequences, not an error.
    assert_eq!(parse_sequences(r#"{"sequences":[]}"#), Ok(vec![]));
}

/// SEQUENCES-02: `args` holds every numeric entry in file order; the
/// non-numeric ones (`offsets`, `post_pad_mode`) are skipped.
#[test]
fn sequences_02_args_keep_numeric_entries_in_file_order_and_skip_the_rest() {
    let s = parsed();
    let names = |q: &SequenceInfo| q.args.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();

    assert_eq!(s[0].args, vec![("baseline".into(), 0.5), ("intensity".into(), 0.0), ("radius".into(), -1.0)]);
    assert_eq!(
        s[1].args,
        vec![
            ("angle".into(), 0.0),
            ("intensity".into(), 1.0),
            ("speed".into(), 13.0),
            ("width".into(), 80.0),
            ("height".into(), 80.0),
            ("t_pre".into(), 1.0),
            ("t_post".into(), 1.0),
            ("bar_loc_horizontal".into(), 0.0),
        ]
    );
    assert!(!names(&s[1]).iter().any(|k| k == "offsets" || k == "post_pad_mode"), "non-numeric args are skipped");

    // File order, not sorted order: reversing the entries reverses `args`.
    let reversed = edit(r#""baseline":0.5,"intensity":0.0,"radius":-1"#, r#""radius":-1,"intensity":0.0,"baseline":0.5"#);
    let r = parse_sequences(&reversed).expect("valid");
    assert_eq!(names(&r[0]), ["radius", "intensity", "baseline"]);

    // Other non-numeric kinds (string, bool, null, object) are skipped too,
    // and do not make the sequence an error.
    let mixed = edit(r#""baseline":0.5,"#, r#""note":"x","flag":true,"none":null,"nested":{"a":1},"baseline":0.5,"#);
    let m = parse_sequences(&mixed).expect("non-numeric args are not an error");
    assert_eq!(names(&m[0]), ["baseline", "intensity", "radius"]);
}

/// SEQUENCES-03: `arg(name)` and `frames(dt)` return the named argument and
/// the dt's `SequenceFrames`, or `None` when absent.
#[test]
fn sequences_03_arg_and_frames_look_up_by_name_or_return_none() {
    let s = parsed();
    let edge = &s[1];

    assert_eq!(edge.arg("angle"), Some(0.0));
    assert_eq!(edge.arg("speed"), Some(13.0));
    assert_eq!(edge.arg("height"), Some(80.0));
    assert_eq!(s[0].arg("radius"), Some(-1.0));
    assert_eq!(edge.arg("radius"), None, "an argument the sequence does not have");
    assert_eq!(edge.arg("offsets"), None, "a skipped non-numeric argument");
    assert_eq!(edge.arg("post_pad_mode"), None, "a skipped non-numeric argument");

    let f = edge.frames("1/50").expect("1/50 is present");
    assert_eq!((f.dt.as_str(), f.n_frames, f.stimulus_frames), ("1/50", 131, (50, 81)));
    assert_eq!(f.snapshot_frames, [0, 26, 52, 78, 104, 130]);
    let f = edge.frames("1/200").expect("1/200 is present");
    assert_eq!((f.n_frames, f.stimulus_frames), (525, (200, 325)));
    assert_eq!(edge.frames("1/100"), None, "a dt the sequence does not have");

    let only_50 = edit(
        r#""1/200":{"n_frames":600,"stimulus_frames":[200,400],"snapshot_frames":[0,119,239,359,479,599]},"#,
        "",
    );
    // Whether parse_sequences accepts this is SEQUENCES-04's question; when
    // it does, the absent dt must read as None.
    if let Ok(q) = parse_sequences(&only_50) {
        assert_eq!(q[0].frames("1/200"), None);
        assert!(q[0].frames("1/50").is_some());
    }
}

/// SEQUENCES-04: a missing listed field, a wrongly typed field, or a missing
/// dt entry is a `ReferenceError`.
#[test]
#[ignore = "acceptance-failing: SEQUENCES-04"]
fn sequences_04_a_missing_or_mistyped_field_or_a_missing_dt_entry_is_an_error() {
    use ReferenceError::{Missing, WrongType};

    // Missing fields.
    assert_eq!(refused(r#"{"runs":[]}"#, "no sequences list"), Missing("manifest.sequences".into()));
    for field in ["index", "family", "flyvis_index", "args", "frames"] {
        let text = SEQUENCES.replacen(&format!(r#""{field}":"#), r#""renamed":"#, 1);
        assert_eq!(refused(&text, field), Missing(format!("sequences[0].{field}")), "missing {field}");
    }
    for field in ["n_frames", "stimulus_frames", "snapshot_frames"] {
        let text = SEQUENCES.replacen(&format!(r#""{field}":"#), r#""renamed":"#, 1);
        assert_eq!(refused(&text, field), Missing(format!("sequences[0].frames.1/200.{field}")), "missing {field}");
    }

    // Wrongly typed fields.
    for (from, to, at) in [
        (r#""index":0"#, r#""index":"0""#, "sequences[0].index"),
        (r#""index":0"#, r#""index":-1"#, "sequences[0].index"),
        (r#""index":0"#, r#""index":0.5"#, "sequences[0].index"),
        (r#""family":"flashes""#, r#""family":1"#, "sequences[0].family"),
        (r#""flyvis_index":0"#, r#""flyvis_index":null"#, "sequences[0].flyvis_index"),
        (r#""args":{"baseline""#, r#""args":[],"x":{"baseline""#, "sequences[0].args"),
        (r#""frames":{"1/200""#, r#""frames":[],"x":{"1/200""#, "sequences[0].frames"),
        (r#""n_frames":600"#, r#""n_frames":"600""#, "sequences[0].frames.1/200.n_frames"),
        (r#""n_frames":600"#, r#""n_frames":-600"#, "sequences[0].frames.1/200.n_frames"),
        (r#""stimulus_frames":[200,400]"#, r#""stimulus_frames":[200]"#, "sequences[0].frames.1/200.stimulus_frames"),
        (r#""stimulus_frames":[200,400]"#, r#""stimulus_frames":[200,400,500]"#, "sequences[0].frames.1/200.stimulus_frames"),
        (r#""stimulus_frames":[200,400]"#, r#""stimulus_frames":[200,"400"]"#, "sequences[0].frames.1/200.stimulus_frames"),
        (r#""stimulus_frames":[200,400]"#, r#""stimulus_frames":{"a":1}"#, "sequences[0].frames.1/200.stimulus_frames"),
        (r#""snapshot_frames":[0,119"#, r#""snapshot_frames":["0",119"#, "sequences[0].frames.1/200.snapshot_frames"),
        (r#""snapshot_frames":[0,119"#, r#""snapshot_frames":[-1,119"#, "sequences[0].frames.1/200.snapshot_frames"),
    ] {
        assert_eq!(refused(&edit(from, to), to), WrongType(at.into()), "{from} -> {to}");
    }
    assert_eq!(refused(r#"{"sequences":{}}"#, "sequences not a list"), WrongType("manifest.sequences".into()));
    assert!(matches!(refused("{", "not JSON"), ReferenceError::Json { .. }));

    // A missing dt entry: the first sequence has no 1/200 layout, while the
    // second has both.
    let only_50 = edit(
        r#""1/200":{"n_frames":600,"stimulus_frames":[200,400],"snapshot_frames":[0,119,239,359,479,599]},"#,
        "",
    );
    refused(&only_50, "a sequence missing a dt entry");
    // And with no dt entries at all.
    let none = edit(
        r#"{"1/200":{"n_frames":600,"stimulus_frames":[200,400],"snapshot_frames":[0,119,239,359,479,599]},
            "1/50":{"n_frames":150,"stimulus_frames":[50,100],"snapshot_frames":[0,29,59,89,119,149]}}"#,
        "{}",
    );
    refused(&none, "a sequence with no dt entries");
}

/// SEQUENCES-05: `snapshot_frames` not ascending, or with a frame outside
/// `0..n_frames`, is an error.
#[test]
fn sequences_05_snapshot_frames_out_of_order_or_out_of_range_are_an_error() {
    let at = |seq: usize, dt: &str| ReferenceError::FrameRange(format!("sequences[{seq}].frames.{dt}.snapshot_frames"));

    for (from, to, seq, dt, what) in [
        ("[0,119,239,359,479,599]", "[0,239,119,359,479,599]", 0, "1/200", "two frames swapped"),
        ("[0,119,239,359,479,599]", "[0,119,119,359,479,599]", 0, "1/200", "a repeated frame (not strictly ascending)"),
        ("[0,119,239,359,479,599]", "[599,479,359,239,119,0]", 0, "1/200", "descending"),
        ("[0,119,239,359,479,599]", "[0,119,239,359,479,600]", 0, "1/200", "a frame at n_frames"),
        ("[0,119,239,359,479,599]", "[0,119,239,359,479,10000]", 0, "1/200", "a frame far past n_frames"),
        ("[0,26,52,78,104,130]", "[0,26,52,78,104,131]", 1, "1/50", "a frame at n_frames, second sequence"),
    ] {
        assert_eq!(refused(&edit(from, to), what), at(seq, dt), "{what}");
    }

    // Boundaries that are inside the rules still parse: the last frame,
    // n_frames - 1, and an empty list.
    assert!(parse_sequences(&edit("[0,26,52,78,104,130]", "[130]")).is_ok(), "n_frames - 1 is inside");
    assert!(parse_sequences(&edit("[0,26,52,78,104,130]", "[]")).is_ok(), "no snapshot frames is structurally valid");
}

/// SEQUENCES-06: a `stimulus_frames` window with start > end, or end >
/// `n_frames`, is an error.
#[test]
fn sequences_06_a_window_reversed_or_past_n_frames_is_an_error() {
    let at = |seq: usize, dt: &str| ReferenceError::FrameRange(format!("sequences[{seq}].frames.{dt}.stimulus_frames"));

    for (from, to, seq, dt, what) in [
        ("[200,400]", "[400,200]", 0, "1/200", "start > end"),
        ("[200,400]", "[201,200]", 0, "1/200", "start one past end"),
        ("[200,400]", "[200,601]", 0, "1/200", "end past n_frames"),
        ("[50,81]", "[50,132]", 1, "1/50", "end past n_frames, second sequence"),
        ("[50,81]", "[132,132]", 1, "1/50", "an empty window past n_frames"),
    ] {
        assert_eq!(refused(&edit(from, to), what), at(seq, dt), "{what}");
    }

    // [start, end) with end exclusive: end == n_frames and start == end are
    // both allowed.
    assert!(parse_sequences(&edit("[200,400]", "[200,600]")).is_ok(), "end == n_frames");
    assert!(parse_sequences(&edit("[50,81]", "[131,131]")).is_ok(), "empty window at n_frames");
    assert!(parse_sequences(&edit("[200,400]", "[0,0]")).is_ok(), "empty window at 0");
}

// ---------------------------------------------------------------------------
// Running the crate's own tests through cargo
// ---------------------------------------------------------------------------

fn cargo(dir: &Path, target: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(option_env!("CARGO").unwrap_or("cargo"))
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target)
        .args(args)
        .output()
        .expect("cargo runs");
    let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), output)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "target" || name == "data" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            copy_tree(&src, &dst);
        } else if ty.is_file() {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

#[cfg(unix)]
fn link(src: &Path, dst: &Path) {
    std::os::unix::fs::symlink(src, dst).unwrap();
}

/// A copy of the workspace's sources whose `data/flyvis-reference/` links the
/// real reference files except `manifest.json`, which it writes.
struct Shadow {
    root: PathBuf,
    target: PathBuf,
}

impl Shadow {
    fn new() -> Shadow {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-sequences");
        let root = base.join("workspace");
        match std::fs::remove_dir_all(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("removing the old shadow {}: {e}", root.display()),
        }
        std::fs::create_dir_all(&root).unwrap();
        let ws = Path::new(WORKSPACE);
        for f in ["Cargo.toml", "Cargo.lock", "README.md"] {
            std::fs::copy(ws.join(f), root.join(f)).unwrap();
        }
        copy_tree(&ws.join("crates"), &root.join("crates"));
        copy_tree(&ws.join("app"), &root.join("app"));
        Shadow { root, target: base.join("target") }
    }

    fn manifest(&self, text: &str) {
        let dir = self.root.join("data/flyvis-reference");
        if dir.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        for entry in std::fs::read_dir(data_path("flyvis-reference")).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() != "manifest.json" {
                link(&entry.path(), &dir.join(entry.file_name()));
            }
        }
        std::fs::write(dir.join("manifest.json"), text).unwrap();
    }

    /// Runs the real-manifest sequence test alone; returns (passed, output).
    fn gate(&self) -> (bool, String) {
        const GATE: &str = "reference::tests::the_real_manifest_lists_the_twelve_reference_sequences";
        let (passed, output) = cargo(
            &self.root,
            &self.target,
            &["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--exact", GATE, "--nocapture"],
        );
        assert!(output.contains("running 1 test"), "the gate must run exactly once:\n{output}");
        // Under SEMBLANCE_REQUIRE_FIXTURES=1 a missing manifest would fail
        // too; make sure the shadow read it rather than skipping.
        assert!(!output.contains("skipping:"), "the gate must read the shadow manifest:\n{output}");
        (passed, output)
    }
}

/// `text` with the first `snapshot_frames` list inside `sequences` cut to its
/// first four entries and its last: five frames, still ascending and in range.
fn five_snapshots(text: &str) -> String {
    let seqs = text.find("\"sequences\"").expect("manifest has sequences");
    let key = seqs + text[seqs..].find("\"snapshot_frames\"").expect("a snapshot_frames list");
    let open = key + text[key..].find('[').unwrap();
    let close = open + text[open..].find(']').unwrap();
    let frames: Vec<&str> = text[open + 1..close].split(',').map(str::trim).collect();
    assert_eq!(frames.len(), 6, "the real manifest has six snapshot frames");
    let five = [frames[0], frames[1], frames[2], frames[3], frames[5]].join(", ");
    format!("{}[{five}]{}", &text[..open], &text[close + 1..])
}

/// SEQUENCES-07: a sequence with 5 snapshot frames is rejected when the
/// sequences are read. The parser checks structure only (spec section 2),
/// so the rejection is the real-manifest fixture test's: run it against a
/// manifest identical to the real one but for one sequence's five snapshots.
#[test]
fn sequences_07_a_sequence_with_five_snapshot_frames_is_rejected() {
    let Some(text) = read_fixture(MANIFEST) else { return };
    let altered = five_snapshots(&text);
    assert!(parse_sequences(&altered).is_ok(), "five snapshots are structurally valid; the content check rejects them");

    let shadow = Shadow::new();

    // Control: the unaltered manifest passes the gate in the shadow.
    shadow.manifest(&text);
    let (passed, output) = shadow.gate();
    assert!(passed, "control: the real manifest must pass:\n{output}");

    shadow.manifest(&altered);
    let (passed, output) = shadow.gate();
    assert!(!passed, "five snapshot frames must be rejected:\n{output}");
    assert!(output.contains("5 snapshot frames"), "rejected for its snapshot count:\n{output}");
}

// ---------------------------------------------------------------------------
// SEQUENCES-08: Manifest::parse untouched
// ---------------------------------------------------------------------------

/// A manifest `Manifest::parse` accepts, with `sequences` given verbatim
/// (or left out when `None`).
fn manifest_with(sequences: Option<&str>) -> String {
    let seq = sequences.map(|s| format!(r#""sequences":{s},"#)).unwrap_or_default();
    format!(
        r#"{{"schema_version":1,"byte_order":"little","array_order":"C","n_nodes":7,"t_pre":1.0,"pre_stim_value":0.5,
  "dts":[{{"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}}],
  "runs":[{{"index":0,"model":"000","dt":"1/50","precision":"f64"}}],
  "centre_nodes":[{{"node":2,"cell_type":"R1"}}],
  {seq}
  "files":[{{"name":"b.f64.bin","dtype":"<f8","axes":["seq"],"bytes":16,"sha256":"0",
            "blocks":[{{"run":null,"dt":"1/50","sequence":0,"offset":0,"bytes":16,"shape":[2]}}]}}]}}"#
    )
}

/// The epic's base: the last commit on main before stimulus-traces.
const BASE: &str = "ee4626c";

/// SEQUENCES-08: `Manifest::parse` still accepts manifests with no or partial
/// `sequences`; `Manifest::parse`, its tests and `acceptance_reference.rs`
/// are unchanged and pass.
#[test]
fn sequences_08_manifest_parse_its_tests_and_acceptance_reference_are_unchanged_and_pass() {
    // No or partial sequences: Manifest::parse does not read them, even
    // where parse_sequences refuses them.
    for (what, seqs) in [
        ("no sequences", None),
        ("an empty list", Some("[]")),
        ("a partial entry", Some(r#"[{"index":0,"family":"flashes","frames":{"1/50":{"n_frames":3}}}]"#)),
        ("a mistyped list", Some(r#"{"x":1}"#)),
    ] {
        let text = manifest_with(seqs);
        let m = Manifest::parse(&text).unwrap_or_else(|e| panic!("{what}: Manifest::parse must accept it: {e}"));
        assert_eq!((m.n_nodes, m.runs.len(), m.files.len()), (7, 1, 1), "{what}");
        if seqs != Some("[]") {
            assert!(parse_sequences(&text).is_err(), "{what}: parse_sequences refuses it, separately");
        }
    }

    // Unchanged since the epic's base: no line of reference.rs removed or
    // altered (only additions), and acceptance_reference.rs identical.
    let ws = Path::new(WORKSPACE);
    let git = |args: &[&str]| Command::new("git").current_dir(ws).args(args).output();
    match git(&["cat-file", "-e", &format!("{BASE}^{{commit}}")]) {
        Ok(o) if o.status.success() => {
            let o = git(&["diff", "--quiet", BASE, "--", "crates/flyvis/tests/acceptance_reference.rs"]).unwrap();
            assert!(o.status.success(), "acceptance_reference.rs changed since {BASE}");
            let o = git(&["diff", "-U0", BASE, "--", "crates/flyvis/src/reference.rs"]).unwrap();
            let diff = String::from_utf8_lossy(&o.stdout);
            let removed: Vec<&str> = diff.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).collect();
            assert!(removed.is_empty(), "reference.rs lines changed or removed since {BASE}:\n{}", removed.join("\n"));
        }
        _ => eprintln!("skipping the unchanged check: commit {BASE} is not available"),
    }

    // And they still pass: reference.rs's own tests and acceptance_reference.rs.
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-sequences").join("target-08");
    for args in [
        &["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "reference::tests::"][..],
        &["test", "-p", "semblance-flyvis", "--test", "acceptance_reference", "--locked", "--offline"][..],
    ] {
        let (passed, output) = cargo(ws, &target, args);
        assert!(passed, "{args:?} must pass:\n{output}");
        assert!(output.contains("test result: ok."), "{args:?} ran:\n{output}");
        assert!(!output.contains("running 0 tests"), "{args:?} ran no tests:\n{output}");
    }
}

// ---------------------------------------------------------------------------
// SEQUENCES-09: the real manifest
// ---------------------------------------------------------------------------

/// SEQUENCES-09: the real manifest's sequences are H6's twelve.
#[test]
fn sequences_09_the_real_manifest_has_h6s_twelve_sequences() {
    let Some(text) = read_fixture(MANIFEST) else { return };
    let m = Manifest::parse(&text).expect("manifest.json must parse");
    let s = parse_sequences(&text).expect("manifest.json's sequences must parse");

    let dt_ids: Vec<&str> = m.dts.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(dt_ids, ["1/200", "1/50"], "the manifest's dt ids");
    assert_eq!(s.len(), 12, "twelve sequences");
    assert_eq!(s.iter().map(|q| q.index).collect::<Vec<_>>(), (0..12).collect::<Vec<u32>>(), "indices 0-11");

    for q in &s {
        let i = q.index as usize;
        assert_eq!(q.frames.len(), dt_ids.len(), "sequence {i}: one layout per dt id, no others");
        let flash = i < 4;
        let (family, flyvis_index) = if flash { ("flashes", i) } else { ("moving_edge", i - 4) };
        assert_eq!((q.family.as_str(), q.flyvis_index as usize), (family, flyvis_index), "sequence {i}");

        let (n_frames, windows) = if flash { ([600, 150], [(200, 400), (50, 100)]) } else { ([525, 131], [(200, 325), (50, 81)]) };
        for (k, dt) in dt_ids.iter().enumerate() {
            let f = q.frames(dt).unwrap_or_else(|| panic!("sequence {i} has no frames for dt {dt}"));
            assert_eq!(f.n_frames, n_frames[k], "sequence {i} dt {dt}: n_frames");
            assert_eq!(f.stimulus_frames, windows[k], "sequence {i} dt {dt}: window");
            assert_eq!(f.snapshot_frames.len(), 6, "sequence {i} dt {dt}: six snapshots");
        }

        if flash {
            // OFF/ON x full-field/radius 6.
            assert_eq!(q.arg("intensity"), Some(if i < 2 { 0.0 } else { 1.0 }), "sequence {i}: intensity");
            assert_eq!(q.arg("radius"), Some(if i % 2 == 0 { -1.0 } else { 6.0 }), "sequence {i}: radius");
        } else {
            // Angles 0/30/90/150, OFF then ON, speed 13, height 80.
            let e = i - 4;
            assert_eq!(q.arg("angle"), Some([0.0, 30.0, 90.0, 150.0][e / 2]), "sequence {i}: angle");
            assert_eq!(q.arg("intensity"), Some((e % 2) as f64), "sequence {i}: OFF then ON");
            assert_eq!(q.arg("speed"), Some(13.0), "sequence {i}: speed");
            assert_eq!(q.arg("height"), Some(80.0), "sequence {i}: height");
        }
    }
}
