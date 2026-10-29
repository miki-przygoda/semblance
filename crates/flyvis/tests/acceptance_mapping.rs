//! Acceptance tests for behaviour group `mapping` of epic stimulus-traces:
//! mapping a hexal frame onto the photoreceptors.
//!
//! One test per behaviour id (MAPPING-01 .. MAPPING-08), each driving the
//! crate's public API (`semblance_flyvis::scatter_hexals`, `input_nodes`,
//! `expand`).
//!
//! MAPPING-07 reads the real fixtures (`data/flyvis-supplementary/` and
//! `data/flyvis-stimuli/hex_coords.csv`). MAPPING-08 runs the crate's own
//! mapping gate, `wiring::tests::input_mapping_matches_flyviss_lattice`, in a
//! copy of the workspace's sources under `CARGO_TARGET_TMPDIR` whose `data/`
//! links the real fixtures, with `hex_coords.csv` transposed or reflected so
//! that the network's mapping no longer matches it. The real `data/` is never
//! written. Both skip without the fixtures unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`, which turns a skip into a failure.

use std::path::{Path, PathBuf};
use std::process::Command;

use semblance_connectome::{TypedNetwork, TypedNetworkParts};
use semblance_flyvis::{
    expand, input_nodes, scatter_hexals, ConnectomeSpec, WiringError, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL,
};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
const HEX_COORDS: &str = "flyvis-stimuli/hex_coords.csv";

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

/// hex_coords.csv's rows as (u, v), checking the header and the index column.
fn hex_rows(csv: &str) -> Vec<(i32, i32)> {
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("index,u,v"), "hex_coords.csv header");
    lines
        .enumerate()
        .map(|(h, line)| {
            let f: Vec<i32> = line.split(',').map(|x| x.trim().parse().expect("an integer")).collect();
            assert_eq!((f.len(), f[0]), (3, h as i32), "hex_coords.csv row {h}");
            (f[1], f[2])
        })
        .collect()
}

// ---------------------------------------------------------------------------
// A hand-built network with two input types
// ---------------------------------------------------------------------------

/// Three cell types on three columns: R1 (nodes 0..3), L1 (3..6), R2 (6..9),
/// interleaved by column so a type's nodes are not contiguous. R1 and R2
/// are the input types; L1 is not.
fn small_net() -> TypedNetwork {
    TypedNetwork::new(TypedNetworkParts {
        cell_types: vec!["R1".into(), "L1".into(), "R2".into()],
        node_cell_type: vec![0, 1, 2, 0, 1, 2, 0, 1, 2],
        node_u: vec![0, 0, 0, 1, 1, 1, -1, -1, -1],
        node_v: vec![0, 0, 0, 0, 0, 0, 1, 1, 1],
        conn_types: vec![(0, 1)],
        conn_sign: vec![1.0],
        edge_source: vec![0],
        edge_target: vec![1],
        edge_conn_type: vec![0],
        edge_n_syn: vec![1.0],
    })
    .unwrap()
}

fn small_spec(units: &str) -> ConnectomeSpec {
    let node = |n: &str| {
        format!(
            r#"{{"name":"{n}","pattern":["stride",[1,1]],"activation":"relu","bias":0.0,
                "bias_fixed":false,"time_constant":0.05,"time_constant_fixed":false}}"#
        )
    };
    ConnectomeSpec::from_json_str(&format!(
        r#"{{"nodes":[{},{},{}],"edges":[],"input_units":{units},"output_units":[]}}"#,
        node("R1"),
        node("L1"),
        node("R2")
    ))
    .unwrap()
}

// ---------------------------------------------------------------------------
// MAPPING-01 .. MAPPING-06
// ---------------------------------------------------------------------------

/// MAPPING-01: out[nodes[k][h]] == frame[h] for every input type k and hexal h.
#[test]
fn mapping_01_every_input_type_receives_the_frame_at_its_hth_node() {
    let nodes: Vec<Vec<u32>> = vec![vec![5, 0, 9, 2], vec![1, 7, 3, 11], vec![8, 4, 10, 6]];
    let frame = [1.5f64, -2.25, 0.0, 42.0];
    let mut out = vec![f64::NAN; 12];
    assert_eq!(scatter_hexals(&nodes, &frame, &mut out), Ok(()));
    for (k, list) in nodes.iter().enumerate() {
        for (h, &i) in list.iter().enumerate() {
            assert_eq!(out[i as usize].to_bits(), frame[h].to_bits(), "type {k}, hexal {h}, node {i}");
        }
    }
    // Generic over the element type: the same holds for f32.
    let frame32 = [0.25f32, 0.5, 0.75, 1.0];
    let mut out32 = vec![0f32; 12];
    assert_eq!(scatter_hexals(&nodes, &frame32, &mut out32), Ok(()));
    for list in &nodes {
        for (h, &i) in list.iter().enumerate() {
            assert_eq!(out32[i as usize], frame32[h]);
        }
    }
}

/// MAPPING-02: nodes not in `nodes` keep their values, so a buffer zeroed
/// once stays zero outside the input nodes across repeated frames.
#[test]
fn mapping_02_non_input_nodes_keep_their_values_across_frames() {
    let nodes: Vec<Vec<u32>> = vec![vec![1, 4], vec![6, 3]];
    let inputs = [1usize, 3, 4, 6];
    let mut out: Vec<i64> = (0..8).map(|i| 100 + i).collect();
    assert_eq!(scatter_hexals(&nodes, &[-1i64, -2], &mut out), Ok(()));
    for (i, &x) in out.iter().enumerate() {
        if !inputs.contains(&i) {
            assert_eq!(x, 100 + i as i64, "non-input node {i} was overwritten");
        }
    }

    // Zeroed once, then many frames: zero everywhere outside the inputs.
    let mut buf = vec![0i64; 8];
    for t in 1..=20i64 {
        assert_eq!(scatter_hexals(&nodes, &[t, -t], &mut buf), Ok(()));
        for (i, &x) in buf.iter().enumerate() {
            if inputs.contains(&i) {
                assert_ne!(x, 0, "frame {t}: input node {i} was not written");
            } else {
                assert_eq!(x, 0, "frame {t}: non-input node {i} is not zero");
            }
        }
        assert_eq!((buf[1], buf[4], buf[6], buf[3]), (t, -t, t, -t), "frame {t} overwrote the inputs");
    }
}

/// MAPPING-03: on a hand-built two-input-type network, exactly the listed
/// nodes of both types are written; everything else is untouched.
#[test]
fn mapping_03_a_two_type_network_writes_exactly_both_types_nodes() {
    let net = small_net();
    let nodes = input_nodes(&small_spec(r#"["R2","R1"]"#), &net).expect("R1 and R2 are cell types");
    assert_eq!(nodes, vec![vec![2, 5, 8], vec![0, 3, 6]], "input nodes per type, in input-unit order");

    let sentinel = -7.0f32;
    let frame = [10.0f32, 20.0, 30.0];
    let mut out = vec![sentinel; net.num_nodes()];
    assert_eq!(scatter_hexals(&nodes, &frame, &mut out), Ok(()));
    for (i, &x) in out.iter().enumerate() {
        let t = &net.cell_types()[net.node_cell_type()[i] as usize];
        let column = i / 3;
        if t == "L1" {
            assert_eq!(x, sentinel, "node {i} ({t}) is not an input and was written");
        } else {
            assert_eq!(x, frame[column], "node {i} ({t}) should hold hexal {column}");
        }
    }
    let written = out.iter().filter(|&&x| x != sentinel).count();
    assert_eq!(written, 6, "exactly the 2 x 3 listed nodes are written");
}

/// MAPPING-04: a node list whose length differs from the frame's is
/// FrameLength { expected: list length, found: frame length }.
#[test]
fn mapping_04_a_frame_of_the_wrong_length_is_a_frame_length_error() {
    let mut out = [0u8; 10];
    assert_eq!(
        scatter_hexals(&[vec![0, 1, 2]], &[1u8, 2], &mut out),
        Err(WiringError::FrameLength { expected: 3, found: 2 })
    );
    assert_eq!(
        scatter_hexals(&[vec![0, 1]], &[1u8, 2, 3, 4], &mut out),
        Err(WiringError::FrameLength { expected: 2, found: 4 })
    );
    // A later type with a mismatched list is caught too.
    assert_eq!(
        scatter_hexals(&[vec![0, 1], vec![2, 3, 4]], &[1u8, 2], &mut out),
        Err(WiringError::FrameLength { expected: 3, found: 2 })
    );
}

/// MAPPING-05: a node past the end of `out` is NodeOutOfRange { node, len }.
#[test]
fn mapping_05_a_node_past_the_buffer_is_a_node_out_of_range_error() {
    let mut out = [0u8; 6];
    assert_eq!(
        scatter_hexals(&[vec![0, 6]], &[1u8, 2], &mut out),
        Err(WiringError::NodeOutOfRange { node: 6, len: 6 })
    );
    assert_eq!(
        scatter_hexals(&[vec![0, 1], vec![2, 1000]], &[1u8, 2], &mut out),
        Err(WiringError::NodeOutOfRange { node: 1000, len: 6 })
    );
    let mut empty: [u8; 0] = [];
    assert_eq!(
        scatter_hexals(&[vec![0]], &[1u8], &mut empty),
        Err(WiringError::NodeOutOfRange { node: 0, len: 0 })
    );
}

/// MAPPING-06: any erroring call leaves `out` exactly as it was, even when
/// earlier types were valid and could have been written.
#[test]
fn mapping_06_an_erroring_call_leaves_out_unchanged() {
    let before: Vec<i32> = (0..8).map(|i| 1000 + i).collect();
    let frame = [1i32, 2, 3];
    type Case<'a> = (&'a str, Vec<Vec<u32>>, &'a [i32]);
    let cases: Vec<Case> = vec![
        ("first type too short", vec![vec![0, 1], vec![2, 3, 4]], &frame),
        ("last type too short", vec![vec![0, 1, 2], vec![3, 4, 5], vec![6, 7]], &frame),
        ("frame too long", vec![vec![0, 1, 2]], &[1, 2, 3, 4]),
        ("last node out of range", vec![vec![0, 1, 2], vec![3, 4, 8]], &frame),
        ("first node out of range", vec![vec![9, 1, 2]], &frame),
        ("valid type then bad length", vec![vec![0, 1, 2], vec![3, 4, 5, 6]], &frame),
    ];
    for (what, nodes, frame) in cases {
        let mut out = before.clone();
        assert!(scatter_hexals(&nodes, frame, &mut out).is_err(), "{what}: must be an error");
        assert_eq!(out, before, "{what}: out changed on error");
    }
}

// ---------------------------------------------------------------------------
// MAPPING-07: the real fixture
// ---------------------------------------------------------------------------

/// MAPPING-07: on the real network, a frame lit only at hexal h lights
/// exactly 8 nodes, one in each input type, all at hex_coords.csv row h.
#[test]
fn mapping_07_a_single_lit_hexal_lands_on_eight_input_nodes_at_its_lattice_coords() {
    let Some(csv) = read_fixture(HEX_COORDS) else { return };
    let Some(raw) = read_fixture(SPEC) else { return };
    let rows = hex_rows(&csv);
    assert_eq!(rows.len(), 721, "hex_coords.csv has 721 hexals");

    let spec = ConnectomeSpec::from_json_str(&raw).expect("the real spec loads");
    let net = expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).expect("the real spec expands");
    let nodes = input_nodes(&spec, &net).expect("input units are cell types");
    assert_eq!(nodes.len(), 8, "eight input types");

    let mut frame = vec![0u8; 721];
    let mut out = vec![0u8; net.num_nodes()];
    for (h, &(u, v)) in rows.iter().enumerate() {
        frame.fill(0);
        frame[h] = 1;
        out.fill(0);
        scatter_hexals(&nodes, &frame, &mut out).expect("721 hexals onto 8 x 721 input nodes");
        let lit: Vec<usize> = (0..out.len()).filter(|&i| out[i] != 0).collect();
        assert_eq!(lit.len(), 8, "hexal {h} lit {} nodes", lit.len());
        assert!(lit.iter().all(|&i| out[i] == 1), "hexal {h}: lit nodes hold 1");

        let mut types: Vec<&str> =
            lit.iter().map(|&i| net.cell_types()[net.node_cell_type()[i] as usize].as_str()).collect();
        types.sort_unstable();
        let n = types.len();
        types.dedup();
        assert_eq!(types.len(), n, "hexal {h}: two lit nodes share a type: {types:?}");
        let mut want: Vec<&str> = spec.input_units.iter().map(String::as_str).collect();
        want.sort_unstable();
        assert_eq!(types, want, "hexal {h}: one lit node per input type");

        for &i in &lit {
            assert_eq!(
                (i32::from(net.node_u()[i]), i32::from(net.node_v()[i])),
                (u, v),
                "hexal {h} lands on node {i} ({}), not at hex_coords.csv row {h}",
                net.cell_types()[net.node_cell_type()[i] as usize]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// MAPPING-08: the mapping gate against a transposed or reflected mapping
// ---------------------------------------------------------------------------

const GATE: &str = "wiring::tests::input_mapping_matches_flyviss_lattice";

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

/// A copy of the workspace's sources whose data/ links the real fixtures,
/// with `hex_coords.csv` replaced by `hex_coords`.
struct Shadow {
    root: PathBuf,
    target: PathBuf,
}

impl Shadow {
    fn new() -> Shadow {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("acceptance-mapping");
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

    fn data(&self, hex_coords: &str) {
        let data = self.root.join("data");
        if data.symlink_metadata().is_ok() {
            std::fs::remove_dir_all(&data).unwrap();
        }
        std::fs::create_dir_all(data.join("flyvis-stimuli")).unwrap();
        let real = Path::new(WORKSPACE).join("data");
        link(&real.join("flyvis-supplementary"), &data.join("flyvis-supplementary"));
        for entry in std::fs::read_dir(real.join("flyvis-stimuli")).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name() != "hex_coords.csv" {
                link(&entry.path(), &data.join("flyvis-stimuli").join(entry.file_name()));
            }
        }
        std::fs::write(data.join("flyvis-stimuli/hex_coords.csv"), hex_coords).unwrap();
    }

    /// Runs the mapping gate alone; returns (passed, output).
    fn gate(&self) -> (bool, String) {
        let out = Command::new(option_env!("CARGO").unwrap_or("cargo"))
            .current_dir(&self.root)
            .env("CARGO_TARGET_DIR", &self.target)
            .env("SEMBLANCE_REQUIRE_FIXTURES", "1")
            .args(["test", "-p", "semblance-flyvis", "--lib", "--locked", "--offline", "--", "--exact", GATE])
            .args(["--nocapture", "--test-threads=1"])
            .output()
            .expect("cargo runs");
        let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(output.contains("running 1 test"), "the gate must run exactly once:\n{output}");
        (out.status.success(), output)
    }
}

fn rewrite(rows: &[(i32, i32)], f: impl Fn(i32, i32) -> (i32, i32)) -> String {
    let mut s = String::from("index,u,v\n");
    for (h, &(u, v)) in rows.iter().enumerate() {
        let (a, b) = f(u, v);
        s.push_str(&format!("{h},{a},{b}\n"));
    }
    s
}

/// MAPPING-08: with the mapping transposed or reflected relative to the
/// lattice, the mapping gate fails -- and it is the mapping gate itself, a
/// test that runs no dynamics, that fails.
#[test]
fn mapping_08_the_mapping_gate_fails_on_a_transposed_or_reflected_mapping() {
    let Some(csv) = read_fixture(HEX_COORDS) else { return };
    if read_fixture(SPEC).is_none() {
        return;
    }
    let rows = hex_rows(&csv);
    let shadow = Shadow::new();

    // Control: the unaltered lattice passes the gate in the shadow.
    shadow.data(&csv);
    let (passed, output) = shadow.gate();
    assert!(passed, "control: the gate must pass on the real hex_coords.csv:\n{output}");

    for (what, altered) in [
        ("transposed (u, v) -> (v, u)", rewrite(&rows, |u, v| (v, u))),
        ("reflected (u, v) -> (-u, -v)", rewrite(&rows, |u, v| (-u, -v))),
    ] {
        shadow.data(&altered);
        let (passed, output) = shadow.gate();
        assert!(!passed, "{what}: the mapping gate must fail:\n{output}");
        assert!(output.contains(&format!("{GATE} ... FAILED")), "{what}: the failure is the mapping gate's:\n{output}");
        assert!(output.contains("hexal 0 lands on node"), "{what}: fails at the first hexal's coordinates:\n{output}");
        assert!(!output.contains("settling_checks") && !output.contains("trace_checks"), "{what}: no dynamics ran:\n{output}");
    }
}
