//! What the parity checks read: the fixture (spec, ensemble, manifest, the
//! four settling files) and the trace files. Reading is separate from
//! parsing, so the test gates keep their own reads and skip notices while
//! `validate` reads from a data directory.

use std::path::Path;

use semblance_connectome::TypedNetwork;

use super::{ParityError, RUNS};
use crate::ensemble::Ensemble;
use crate::expand::{expand, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL};
use crate::reference::{parse_sequences, Dtype, Manifest, SequenceInfo};
use crate::spec::ConnectomeSpec;
use crate::wiring::{edge_syn_groups, input_nodes};

pub(crate) const SPEC: &str = "flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json";
pub(crate) const ENSEMBLE: &str = "flyvis-pretrained/ensemble.csv";
pub(crate) const MANIFEST: &str = "flyvis-reference/manifest.json";
pub(crate) const PROVENANCE: &str = "flyvis-reference/PROVENANCE.json";

/// The settling files, by their names in the manifest.
pub(crate) const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

/// The 13 files under the data directory that `validate` reads.
const REQUIRED: [&str; 13] = [
    SPEC,
    ENSEMBLE,
    MANIFEST,
    PROVENANCE,
    "flyvis-reference/settled.f32.bin",
    "flyvis-reference/settled.f64.bin",
    "flyvis-reference/settling_centre.f32.bin",
    "flyvis-reference/settling_centre.f64.bin",
    "flyvis-reference/input.f64.bin",
    "flyvis-reference/centre.f32.bin",
    "flyvis-reference/centre.f64.bin",
    "flyvis-reference/snapshots.f32.bin",
    "flyvis-reference/snapshots.f64.bin",
];

/// The 13 paths, relative to the data directory, that `validate` needs.
pub fn required_files() -> &'static [&'static str] {
    &REQUIRED
}

/// How to produce a file under `data/`, by its directory.
pub fn how_to_get(rel: &str) -> &'static str {
    match rel.split('/').next() {
        Some("flyvis-supplementary") => {
            "Download \
             https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
             and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/, \
             or run ci/fetch-fixtures.sh"
        }
        Some("flyvis-build") => {
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-stimuli") => {
            "Run `uv run --project tools tools/export_stimuli.py`, \
             `uv run --project tools tools/export_moving_edges.py` and, \
             for the `_h80` files, `uv run --project tools tools/export_reference.py` \
             to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        }
        Some("flyvis-pretrained") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        }
        Some("flyvis-reference") => {
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        }
        _ => "Run ci/fetch-fixtures.sh.",
    }
}

/// The bytes of `data/<rel>`: `Missing` if absent, `Unreadable` otherwise.
pub(crate) fn read_bytes(data: &Path, rel: &str) -> Result<Vec<u8>, ParityError> {
    let path = data.join(rel);
    std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => ParityError::Missing { rel: rel.to_string(), path },
        kind => ParityError::Unreadable { rel: rel.to_string(), path, kind },
    })
}

/// The text of `data/<rel>`.
pub(crate) fn read_text(data: &Path, rel: &str) -> Result<String, ParityError> {
    String::from_utf8(read_bytes(data, rel)?)
        .map_err(|e| ParityError::Malformed { rel: rel.to_string(), detail: format!("not UTF-8 ({e})") })
}

fn malformed(rel: &str, detail: impl std::fmt::Display) -> ParityError {
    ParityError::Malformed { rel: rel.to_string(), detail: detail.to_string() }
}

/// Everything the settling gates read, parsed and built: the expanded
/// network, its synapse-count groups and input nodes, the ensemble, the
/// manifest, and the four settling files' bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Fixtures {
    pub(crate) net: TypedNetwork,
    pub(crate) groups: Vec<u32>,
    /// The photoreceptor nodes per input type, in `spec.input_units` order,
    /// each in node order: flyvis's [8, 721] `input_index`.
    pub(crate) input_nodes: Vec<Vec<u32>>,
    pub(crate) ensemble: Ensemble,
    pub(crate) manifest: Manifest,
    pub(crate) settling: Vec<(&'static str, Vec<u8>)>,
}

impl Fixtures {
    /// Reads, in the gates' order: ensemble.csv, manifest.json, the four
    /// settling files, the spec. Then [`Fixtures::from_parts`].
    pub fn load(data: &Path) -> Result<Fixtures, ParityError> {
        let ensemble = read_text(data, ENSEMBLE)?;
        let manifest = read_text(data, MANIFEST)?;
        let mut settling = Vec::new();
        for name in SETTLING_FILES {
            settling.push((name, read_bytes(data, &format!("flyvis-reference/{name}"))?));
        }
        let spec = read_text(data, SPEC)?;
        Fixtures::from_parts(&spec, &ensemble, &manifest, settling)
    }

    /// Pure: no I/O. What the test fixture and `load` both call.
    ///
    /// Refuses, as `Malformed`: a file that does not parse, a spec that does
    /// not expand to a network with flyvis's synapse-count groups and input
    /// units, a manifest whose runs are not the eight registered ones, and a
    /// settling file that is missing from `settling` or the manifest or is
    /// not the length the manifest gives.
    pub fn from_parts(
        spec_json: &str,
        ensemble_csv: &str,
        manifest_json: &str,
        settling: Vec<(&'static str, Vec<u8>)>,
    ) -> Result<Fixtures, ParityError> {
        let ensemble = Ensemble::parse(ensemble_csv).map_err(|e| malformed(ENSEMBLE, e))?;
        let manifest = Manifest::parse(manifest_json).map_err(|e| malformed(MANIFEST, e))?;
        let runs: Vec<(u32, &str, &str, Dtype)> =
            manifest.runs.iter().map(|r| (r.index, r.model.as_str(), r.dt.as_str(), r.precision)).collect();
        if runs != RUNS {
            return Err(malformed(MANIFEST, format!("the manifest's runs are not the eight registered: {runs:?}")));
        }
        for name in SETTLING_FILES {
            let rel = format!("flyvis-reference/{name}");
            let info = manifest.file(name).ok_or_else(|| malformed(MANIFEST, format!("{name} is not in the manifest")))?;
            let bytes = &settling.iter().find(|(n, _)| *n == name).ok_or_else(|| malformed(&rel, "not loaded"))?.1;
            if bytes.len() != info.bytes {
                return Err(malformed(&rel, format!("{name}: {} bytes, manifest says {}", bytes.len(), info.bytes)));
            }
        }
        let spec = ConnectomeSpec::from_json_str(spec_json).map_err(|e| malformed(SPEC, e))?;
        let net = expand(&spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL).map_err(|e| malformed(SPEC, e))?;
        let groups = edge_syn_groups(&net).map_err(|e| malformed(SPEC, e))?;
        let input_nodes = input_nodes(&spec, &net).map_err(|e| malformed(SPEC, e))?;
        Ok(Fixtures { net, groups, input_nodes, ensemble, manifest, settling })
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn network(&self) -> &TypedNetwork {
        &self.net
    }
}

/// The trace files, all of them: what `validate` reads.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceData {
    pub(crate) sequences: Vec<SequenceInfo>,
    pub(crate) input: Vec<u8>,
    pub(crate) centre_f32: Vec<u8>,
    pub(crate) centre_f64: Vec<u8>,
    pub(crate) snapshots_f32: Vec<u8>,
    pub(crate) snapshots_f64: Vec<u8>,
}

impl TraceData {
    /// Reads the manifest's sequences, then input, centre and snapshots.
    pub fn load(data: &Path) -> Result<TraceData, ParityError> {
        let manifest = read_text(data, MANIFEST)?;
        let sequences = parse_sequences(&manifest).map_err(|e| malformed(MANIFEST, e))?;
        let file = |name: &str| read_bytes(data, &format!("flyvis-reference/{name}"));
        Ok(TraceData {
            sequences,
            input: file("input.f64.bin")?,
            centre_f32: file("centre.f32.bin")?,
            centre_f64: file("centre.f64.bin")?,
            snapshots_f32: file("snapshots.f32.bin")?,
            snapshots_f64: file("snapshots.f64.bin")?,
        })
    }

    /// `view(F32)` borrows centre.f32.bin and snapshots.f32.bin, `view(F64)`
    /// the f64 pair; both borrow input.f64.bin. `precision` is set to `p`.
    pub fn view(&self, p: Dtype) -> TraceInputs<'_> {
        let (centre, snapshots) = match p {
            Dtype::F32 => (&self.centre_f32, &self.snapshots_f32),
            Dtype::F64 => (&self.centre_f64, &self.snapshots_f64),
        };
        TraceInputs { precision: p, sequences: &self.sequences, input: &self.input, centre, snapshots }
    }
}

/// One precision's trace inputs, borrowed. The test gates build this from
/// their lazily loaded files, read in the order sequences, input, centre,
/// snapshots.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceInputs<'a> {
    pub precision: Dtype,
    pub sequences: &'a [SequenceInfo],
    pub input: &'a [u8],
    pub centre: &'a [u8],
    pub snapshots: &'a [u8],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C8 depends on these: the gates' skip notices print them.
    #[test]
    fn how_to_get_names_the_way_to_each_directory() {
        assert_eq!(
            how_to_get("flyvis-supplementary/x.json"),
            "Download https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/\
             41586_2024_7939_MOESM3_ESM.zip and extract Lappalainen_et_al_connectome_avg_filters.json into \
             data/flyvis-supplementary/, or run ci/fetch-fixtures.sh"
        );
        assert_eq!(
            how_to_get("flyvis-build/nodes.csv"),
            "Run `uv run --project tools tools/export_build.py` to generate it, or run ci/fetch-fixtures.sh."
        );
        assert_eq!(
            how_to_get("flyvis-stimuli/flashes.bin"),
            "Run `uv run --project tools tools/export_stimuli.py`, `uv run --project tools \
             tools/export_moving_edges.py` and, for the `_h80` files, `uv run --project tools \
             tools/export_reference.py` to generate the stimulus fixtures, or run ci/fetch-fixtures.sh."
        );
        assert_eq!(
            how_to_get("flyvis-pretrained/ensemble.csv"),
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_params.py`."
        );
        assert_eq!(
            how_to_get("flyvis-reference/manifest.json"),
            "Run ci/fetch-fixtures.sh, or `uv run --project tools tools/export_reference.py`."
        );
        assert_eq!(how_to_get("elsewhere/x"), "Run ci/fetch-fixtures.sh.");
    }

    #[test]
    fn the_required_files_are_the_thirteen_the_gates_read() {
        let r = required_files();
        assert_eq!(r.len(), 13);
        for (i, a) in r.iter().enumerate() {
            assert!(!r[..i].contains(a), "{a} listed twice");
        }
        for name in SETTLING_FILES {
            assert!(r.contains(&format!("flyvis-reference/{name}").as_str()), "{name}");
        }
    }

    #[test]
    fn a_missing_file_is_missing_and_names_its_path() {
        let dir = std::env::temp_dir().join(format!("semblance-parity-{}-missing", std::process::id()));
        match read_bytes(&dir, "flyvis-reference/manifest.json") {
            Err(ParityError::Missing { rel, path }) => {
                assert_eq!(rel, "flyvis-reference/manifest.json");
                assert_eq!(path, dir.join("flyvis-reference/manifest.json"));
            }
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    const MANIFEST_RUNS: &str = r#""runs":[
        {"index":0,"model":"000","dt":"1/200","precision":"f32"},{"index":1,"model":"000","dt":"1/200","precision":"f64"},
        {"index":2,"model":"000","dt":"1/50","precision":"f32"},{"index":3,"model":"000","dt":"1/50","precision":"f64"},
        {"index":4,"model":"049","dt":"1/200","precision":"f32"},{"index":5,"model":"049","dt":"1/200","precision":"f64"},
        {"index":6,"model":"049","dt":"1/50","precision":"f32"},{"index":7,"model":"049","dt":"1/50","precision":"f64"}]"#;

    /// A manifest that parses, with the eight runs and four empty settling
    /// files of 4 or 8 bytes each.
    fn manifest(runs: &str) -> String {
        let file = |name: &str, dtype: &str, bytes: usize| {
            format!(
                r#"{{"name":"{name}","dtype":"{dtype}","axes":["n"],"bytes":{bytes},
                     "blocks":[{{"run":0,"dt":"1/50","sequence":null,"offset":0,"bytes":{bytes},"shape":[1]}}]}}"#
            )
        };
        format!(
            r#"{{"schema_version":1,"byte_order":"little","array_order":"C","n_nodes":1,"t_pre":1.0,"pre_stim_value":0.5,
              "dts":[{{"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}}],
              {runs},"centre_nodes":[],"files":[{},{},{},{}]}}"#,
            file("settled.f32.bin", "<f4", 4),
            file("settled.f64.bin", "<f8", 8),
            file("settling_centre.f32.bin", "<f4", 4),
            file("settling_centre.f64.bin", "<f8", 8)
        )
    }

    type Settling = Vec<(&'static str, Vec<u8>)>;

    fn settling(short: Option<&str>) -> Settling {
        SETTLING_FILES
            .iter()
            .map(|&n| {
                let len = if n.contains("f32") { 4 } else { 8 };
                (n, vec![0u8; if Some(n) == short { len - 1 } else { len }])
            })
            .collect()
    }

    /// A valid ensemble: 50 models, each with model 000's parameters.
    pub(crate) fn synthetic_ensemble() -> String {
        use crate::ensemble::{ModelParams, N_MODELS};
        use crate::pretrained::{CELL_TYPES, CONN_TYPES};
        let p = ModelParams::pretrained();
        let mut out = String::from("model,kind,index,name,value\n");
        for m in 0..N_MODELS {
            for (i, t) in CELL_TYPES.iter().enumerate() {
                out.push_str(&format!("{m:03},bias,{i},{t},{}\n", p.bias[i]));
            }
            for (i, t) in CELL_TYPES.iter().enumerate() {
                out.push_str(&format!("{m:03},time_const,{i},{t},{}\n", p.time_const[i]));
            }
            for (i, (s, t)) in CONN_TYPES.iter().enumerate() {
                out.push_str(&format!("{m:03},syn_strength,{i},{s}>{t},{}\n", p.syn_strength[i]));
            }
        }
        out
    }

    /// Malformed input is an error naming the file, never a panic.
    #[test]
    fn from_parts_refuses_malformed_input_without_panicking() {
        let ensemble = synthetic_ensemble();
        assert!(Ensemble::parse(&ensemble).is_ok(), "the synthetic ensemble is valid");
        let one_run = MANIFEST_RUNS[..MANIFEST_RUNS.find("},").unwrap() + 1].to_string() + "]";
        let cases: [(&str, &str, String, Settling, &str); 4] = [
            ("an empty spec", &ensemble, manifest(MANIFEST_RUNS), settling(None), SPEC),
            ("a truncated ensemble", &ensemble[..ensemble.len() / 2], manifest(MANIFEST_RUNS), settling(None), ENSEMBLE),
            ("a manifest with one run", &ensemble, manifest(&one_run), settling(None), MANIFEST),
            (
                "a settling file one byte short",
                &ensemble,
                manifest(MANIFEST_RUNS),
                settling(Some("settled.f64.bin")),
                "flyvis-reference/settled.f64.bin",
            ),
        ];
        for (what, ensemble_csv, manifest_json, files, rel) in cases {
            match Fixtures::from_parts("", ensemble_csv, &manifest_json, files) {
                Err(ParityError::Malformed { rel: got, detail }) => assert_eq!(got, rel, "{what}: {detail}"),
                other => panic!("{what}: expected Malformed naming {rel}, got {:?}", other.map(|_| ())),
            }
        }
    }
}
