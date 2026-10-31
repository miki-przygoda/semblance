//! The fixture checks `validate` runs before any arithmetic: the files agree
//! with each other, with PROVENANCE.json, and with what the specs
//! registered. Every failure is a fixture error, never a recomputed bound.

use semblance_connectome::TypedNetwork;

use super::inputs::SETTLING_FILES;
use super::run::{centre_nodes, checked_run, file, HEXALS, SNAPSHOTS};
use super::{Fixtures, ParityError, TraceData, RUNS, SETTLING_F32_GAPS, TRACE_F32_GAPS};
use crate::pretrained::CELL_TYPES;
use crate::reference::{BlockKey, CentreNode, Dtype, Provenance};

fn mismatch(m: String) -> ParityError {
    ParityError::Mismatch(m)
}

fn check_gaps(p: &Provenance, table: &[(&str, &str, f64, f64); 4], trace: bool) -> Result<(), ParityError> {
    for &(model, dt, a, b) in table {
        let key = format!("{model} {dt}");
        let g = p
            .gaps
            .iter()
            .find(|g| g.key == key)
            .ok_or_else(|| mismatch(format!("{key}: PROVENANCE f32_f64_gap has no such entry")))?;
        let pairs = if trace {
            [("centre", g.centre, a), ("snapshots", g.snapshots, b)]
        } else {
            [("settled", g.settled, a), ("settling_centre", g.settling_centre, b)]
        };
        for (quantity, got, want) in pairs {
            if got.to_bits() != want.to_bits() {
                return Err(mismatch(format!("{model} {dt} {quantity}: PROVENANCE {got:e}, registered {want:e}")));
            }
        }
    }
    Ok(())
}

/// The settling gaps in PROVENANCE.json are the registered ones, bit for bit.
pub fn check_settling_gaps(p: &Provenance) -> Result<(), ParityError> {
    check_gaps(p, &SETTLING_F32_GAPS, false)
}

/// The trace gaps in PROVENANCE.json are the registered ones, bit for bit.
pub fn check_trace_gaps(p: &Provenance) -> Result<(), ParityError> {
    check_gaps(p, &TRACE_F32_GAPS, true)
}

/// PROVENANCE's four booleans read as registered: a clean export, threads
/// bit-equal, every checkpoint equal to ensemble.csv, and the forward path
/// equal to flyvis's `stimulus_response`.
pub(crate) fn check_booleans(p: &Provenance) -> Result<(), ParityError> {
    for (path, value, want) in [
        ("git.dirty", p.git_dirty, false),
        ("threads.bit_equal", p.threads_bit_equal, true),
        ("parameter_equality.all_50_checkpoints_equal_ensemble_csv", p.all_checkpoints_equal_ensemble, true),
        ("equivalence.stimulus_response_bit_equal", p.stimulus_response_bit_equal, true),
    ] {
        if value != want {
            return Err(mismatch(format!("PROVENANCE {path} is {value}: the reference is not the one registered")));
        }
    }
    Ok(())
}

/// The centre nodes are the network's, one per cell type, in `CELL_TYPES`
/// order. Each index is checked before it is used.
pub(crate) fn check_centre_nodes(centre: &[CentreNode], net: &TypedNetwork) -> Result<(), ParityError> {
    let n = net.num_nodes();
    if centre.len() != CELL_TYPES.len() {
        return Err(mismatch(format!("centre nodes: {} listed, expected {}", centre.len(), CELL_TYPES.len())));
    }
    for (k, c) in centre.iter().enumerate() {
        let node = c.node as usize;
        if node >= n {
            return Err(mismatch(format!("centre node {k}: node {node} is outside the network's {n} nodes")));
        }
        let t = &net.cell_types()[net.node_cell_type()[node] as usize];
        if *t != c.cell_type || c.cell_type != CELL_TYPES[k] {
            return Err(mismatch(format!(
                "centre node {k}: node {node} is {t}, manifest says {}, CELL_TYPES says {}",
                c.cell_type, CELL_TYPES[k]
            )));
        }
    }
    Ok(())
}

/// A block's shape, from the manifest alone.
fn shape(f: &Fixtures, name: &str, key: &BlockKey) -> Result<Vec<usize>, ParityError> {
    let info = file(f, name)?;
    info.blocks
        .iter()
        .find(|b| b.run == key.run && b.dt == key.dt && b.sequence == key.sequence)
        .map(|b| b.shape.clone())
        .ok_or_else(|| mismatch(format!("{name}: no block for run {:?}, dt {}, sequence {:?}", key.run, key.dt, key.sequence)))
}

/// Everything `validate` checks before any job runs.
pub(crate) fn check_fixtures(f: &Fixtures, t: &TraceData, p: &Provenance) -> Result<(), ParityError> {
    let n = f.net.num_nodes();
    centre_nodes(f)?;
    check_centre_nodes(&f.manifest.centre_nodes, &f.net)?;

    // the sequences: 12, in index order, six snapshot frames at every dt
    if t.sequences.len() != 12 {
        return Err(mismatch(format!("the manifest lists {} sequences, expected 12", t.sequences.len())));
    }
    for (i, q) in t.sequences.iter().enumerate() {
        if q.index as usize != i {
            return Err(mismatch(format!("manifest sequences must be listed in index order: entry {i} is sequence {}", q.index)));
        }
        for dt in &f.manifest.dts {
            let layout = q.frames(&dt.id).ok_or_else(|| mismatch(format!("sequence {i} has no frames at dt {}", dt.id)))?;
            if layout.snapshot_frames.len() != SNAPSHOTS {
                return Err(mismatch(format!(
                    "sequence {i}'s snapshot frames: expected {SNAPSHOTS}, found {}",
                    layout.snapshot_frames.len()
                )));
            }
        }
    }

    // every block the jobs read has the shape they expect
    for &(run, _, _, p) in &RUNS {
        let (info, dt, _) = checked_run(f, run)?;
        let sfx = match p {
            Dtype::F32 => "f32",
            Dtype::F64 => "f64",
        };
        let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
        let s = shape(f, &format!("settled.{sfx}.bin"), &key)?;
        if s != [n] {
            return Err(mismatch(format!("run {run}: settled shape: expected [{n}], found {s:?}")));
        }
        let s = shape(f, &format!("settling_centre.{sfx}.bin"), &key)?;
        if s != [dt.n_settle, CELL_TYPES.len()] {
            return Err(mismatch(format!(
                "run {run}: settling_centre shape: expected [{}, {}], found {s:?}",
                dt.n_settle,
                CELL_TYPES.len()
            )));
        }
        for q in &t.sequences {
            let seq = q.index;
            let frames = q.frames(&info.dt).map_or(0, |l| l.n_frames);
            let input = shape(f, "input.f64.bin", &BlockKey { run: None, dt: &info.dt, sequence: Some(seq) })?;
            if input != [frames, HEXALS] {
                return Err(mismatch(format!("input shape, sequence {seq}: expected [{frames}, {HEXALS}], found {input:?}")));
            }
            let key = BlockKey { run: Some(run), dt: &info.dt, sequence: Some(seq) };
            let c = shape(f, &format!("centre.{sfx}.bin"), &key)?;
            if c != [frames, CELL_TYPES.len()] {
                return Err(mismatch(format!(
                    "run {run}: centre shape, sequence {seq}: expected [{frames}, {}], found {c:?}",
                    CELL_TYPES.len()
                )));
            }
            let s = shape(f, &format!("snapshots.{sfx}.bin"), &key)?;
            if s != [SNAPSHOTS, n] {
                return Err(mismatch(format!("run {run}: snapshots shape, sequence {seq}: expected [{SNAPSHOTS}, {n}], found {s:?}")));
            }
        }
    }

    // each file is the size the manifest and PROVENANCE give
    let lengths = SETTLING_FILES
        .iter()
        .map(|&name| (name, f.settling.iter().find(|(n, _)| *n == name).map_or(0, |(_, b)| b.len())))
        .chain([
            ("input.f64.bin", t.input.len()),
            ("centre.f32.bin", t.centre_f32.len()),
            ("centre.f64.bin", t.centre_f64.len()),
            ("snapshots.f32.bin", t.snapshots_f32.len()),
            ("snapshots.f64.bin", t.snapshots_f64.len()),
        ]);
    let mut checked = Vec::new();
    for (name, len) in lengths {
        checked.push(name);
        let manifest = file(f, name)?.bytes;
        if len != manifest {
            return Err(mismatch(format!("{name}: {len} bytes, manifest says {manifest}")));
        }
        let size = p
            .sizes
            .iter()
            .find(|(k, _)| k == name)
            .map(|&(_, v)| v)
            .ok_or_else(|| mismatch(format!("{name}: PROVENANCE sizes has no entry")))?;
        if len as u64 != size {
            return Err(mismatch(format!("{name}: {len} bytes, PROVENANCE sizes says {size}")));
        }
    }
    // and PROVENANCE sizes no file the checks above did not compare, so the
    // record's count of size-checked files is the number actually checked
    if let Some((extra, _)) = p.sizes.iter().find(|(k, _)| !checked.contains(&k.as_str())) {
        return Err(mismatch(format!("{extra}: PROVENANCE sizes lists a file validate does not check")));
    }

    check_settling_gaps(p)?;
    check_trace_gaps(p)?;
    check_booleans(p)
}

/// A Provenance carrying the registered gaps and the registered booleans.
#[cfg(test)]
pub(crate) fn registered_provenance() -> Provenance {
    use crate::reference::GapEntry;
    {
        let gaps = SETTLING_F32_GAPS
            .iter()
            .zip(&TRACE_F32_GAPS)
            .map(|(s, t)| GapEntry {
                key: format!("{} {}", s.0, s.1),
                settled: s.2,
                settling_centre: s.3,
                centre: t.2,
                snapshots: t.3,
            })
            .collect();
        Provenance {
            exported: "2026-10-27".into(),
            export_id: "x".into(),
            generator_path: "tools/export_reference.py".into(),
            generator_sha256: "0".into(),
            git_head: "0".into(),
            git_dirty: false,
            versions: vec![],
            platform: "p".into(),
            machine: "m".into(),
            device: "cpu".into(),
            uv_lock_sha256: "0".into(),
            threads_default: 12,
            threads_checked: vec![1, 12],
            threads_bit_equal: true,
            threads_runs_checked: vec![0, 1],
            models: vec![],
            all_checkpoints_equal_ensemble: true,
            stimulus_response_bit_equal: true,
            equivalence_sequences: vec![0, 4],
            input_rounding: vec![],
            gaps,
            sizes: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered() -> Provenance {
        registered_provenance()
    }

    #[test]
    fn the_registered_gaps_pass() {
        let p = registered();
        assert_eq!(check_settling_gaps(&p), Ok(()));
        assert_eq!(check_trace_gaps(&p), Ok(()));
        assert_eq!(check_booleans(&p), Ok(()));
    }

    /// One ulp off in any of the 16 gaps is a Mismatch naming it.
    #[test]
    fn a_gap_one_ulp_off_is_a_mismatch_naming_it() {
        for i in 0..4 {
            for q in ["settled", "settling_centre", "centre", "snapshots"] {
                let mut p = registered();
                let g = &mut p.gaps[i];
                let v = match q {
                    "settled" => &mut g.settled,
                    "settling_centre" => &mut g.settling_centre,
                    "centre" => &mut g.centre,
                    _ => &mut g.snapshots,
                };
                *v = f64::from_bits(v.to_bits() + 1);
                let key = g.key.clone();
                let e = if matches!(q, "settled" | "settling_centre") { check_settling_gaps(&p) } else { check_trace_gaps(&p) };
                match e {
                    Err(ParityError::Mismatch(m)) => assert!(m.starts_with(&format!("{key} {q}: PROVENANCE ")), "{m}"),
                    other => panic!("{key} {q}: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn a_provenance_boolean_read_the_other_way_is_a_mismatch_naming_its_path() {
        type Flip = fn(&mut Provenance);
        let cases: [(&str, Flip); 4] = [
            ("git.dirty is true", |p| p.git_dirty = true),
            ("threads.bit_equal is false", |p| p.threads_bit_equal = false),
            (
                "parameter_equality.all_50_checkpoints_equal_ensemble_csv is false",
                |p| p.all_checkpoints_equal_ensemble = false,
            ),
            ("equivalence.stimulus_response_bit_equal is false", |p| p.stimulus_response_bit_equal = false),
        ];
        for (what, flip) in cases {
            let mut p = registered();
            flip(&mut p);
            assert_eq!(
                check_booleans(&p),
                Err(ParityError::Mismatch(format!("PROVENANCE {what}: the reference is not the one registered")))
            );
        }
    }

    /// A centre node past the network is refused before it indexes anything.
    #[test]
    fn a_centre_node_outside_the_network_is_a_mismatch_not_a_panic() {
        let net = TypedNetwork::new(semblance_connectome::TypedNetworkParts {
            cell_types: CELL_TYPES.iter().map(|t| t.to_string()).collect(),
            conn_types: vec![],
            conn_sign: vec![],
            node_cell_type: (0..65).collect(),
            node_u: vec![0; 65],
            node_v: vec![0; 65],
            edge_source: vec![],
            edge_target: vec![],
            edge_conn_type: vec![],
            edge_n_syn: vec![],
        })
        .expect("a valid network");
        let mut centre: Vec<CentreNode> =
            CELL_TYPES.iter().enumerate().map(|(k, t)| CentreNode { node: k as u32, cell_type: t.to_string() }).collect();
        assert_eq!(check_centre_nodes(&centre, &net), Ok(()));
        centre[7].node = 65;
        assert_eq!(
            check_centre_nodes(&centre, &net),
            Err(ParityError::Mismatch("centre node 7: node 65 is outside the network's 65 nodes".into()))
        );
        centre[7].node = 8;
        assert!(matches!(check_centre_nodes(&centre, &net), Err(ParityError::Mismatch(m)) if m.starts_with("centre node 7: node 8 is")));
    }
}
