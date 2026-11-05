//! flyvis's own index values and the input it fed, as exported by
//! `tools/export_indices.py` into `data/flyvis-indices/` (fixtures-3): the
//! reference gate A holds Rust's index code to.
//!
//! Every f32 array has a leading model axis of 50 (`"000"` to `"049"`) and
//! every f64 array one of 2 (`"000"`, `"049"`): the manifest's `models` lists
//! them per precision. Each array records its own dtype: at f32 flyvis
//! returns float64 for `dsi`, `pd`, `tuning_r` and the `*_to_known` values,
//! so those are `<f8` inside `.f32.bin` files. [`IndexFixture::array`]
//! widens either to f64.
//!
//! The input is stored once per distinct frame (`frames.f64.bin`) and once
//! per stimulus frame as a row index (`frame_index.u32.bin`), flashes then
//! edges; [`IndexFixture::battery`] plays it back through
//! [`parity::Battery`](crate::parity::Battery).
//!
//! Files are not hashed here -- `ci/fetch-fixtures.sh` verifies every
//! fixture against `ci/fixtures.sha256` -- so this checks structure: every
//! array sized to its shape and its file, every frame index inside
//! `frames`, every block contiguous and sized to its stimulus, every window
//! inside its stimulus.
//!
//! Spec: docs/superpowers/specs/2026-11-04-flyvis-index-export-design.md,
//! section 5, as amended by the plan
//! (docs/superpowers/plans/2026-11-04-flyvis-index-export.md, amendments of
//! 2026-11-04).

use std::path::Path;

use crate::json::{self, Json};
use crate::parity::{read_bytes, read_text, Battery, ParityError};
use crate::reference::{array, get, number, string, Dtype, ReferenceError};

const DIR: &str = "flyvis-indices";
const MANIFEST: &str = "flyvis-indices/manifest.json";
const FRAMES: &str = "frames.f64.bin";
const FRAME_INDEX: &str = "frame_index.u32.bin";
const HEXALS: usize = 721;

/// One stimulus of a battery, as the manifest lists it. Flashes carry
/// `radius`; edges carry `angle`, `speed` and `width`.
#[derive(Clone, Debug, PartialEq)]
pub struct StimulusEntry {
    /// Its position in the battery, and in flyvis's dataset.
    pub index: usize,
    pub label: String,
    pub n_frames: usize,
    /// `[start, end]` in frames at dt 1/200, both inclusive: the FRI window
    /// for flashes, the peak window for edges.
    pub window: (usize, usize),
    pub intensity: u8,
    pub radius: Option<i32>,
    pub angle: Option<u16>,
    pub speed: Option<f64>,
    pub width: Option<u16>,
}

/// An index array, widened to f64, with its shape (C order; the first axis
/// is the precision's models).
#[derive(Clone, Debug, PartialEq)]
pub struct IndexArray {
    pub shape: Vec<usize>,
    pub data: Vec<f64>,
}

/// Which of the two exported batteries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryKind {
    Flashes,
    Edges,
}

impl BatteryKind {
    fn name(self) -> &'static str {
        match self {
            BatteryKind::Flashes => "flashes",
            BatteryKind::Edges => "edges",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ArrayEntry {
    name: String,
    precision: Dtype,
    dtype: Dtype,
    file: String,
    shape: Vec<usize>,
    bytes: usize,
}

/// The manifest, parsed and checked on its own.
struct Layout {
    cell_types: Vec<String>,
    models_f32: Vec<String>,
    models_f64: Vec<String>,
    flashes: Vec<StimulusEntry>,
    edges: Vec<StimulusEntry>,
    rows: usize,
    frames_bytes: usize,
    /// Per stimulus, flashes then edges: its offset into `frame_index`.
    starts: Vec<usize>,
    index_entries: usize,
    arrays: Vec<ArrayEntry>,
}

/// The exported set, checked and held in memory.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexFixture {
    cell_types: Vec<String>,
    models_f32: Vec<String>,
    models_f64: Vec<String>,
    flashes: Vec<StimulusEntry>,
    edges: Vec<StimulusEntry>,
    /// `rows × 721`, each distinct input frame once.
    frames: Vec<f64>,
    /// Every frame of every stimulus, flashes then edges: a row of `frames`.
    index: Vec<u32>,
    /// Per stimulus, flashes then edges: its offset into `index`.
    starts: Vec<usize>,
    arrays: Vec<(ArrayEntry, Vec<u8>)>,
}

fn malformed(file: &str, detail: impl std::fmt::Display) -> ParityError {
    ParityError::Malformed { rel: format!("{DIR}/{file}"), detail: detail.to_string() }
}

fn bad_manifest(detail: impl std::fmt::Display) -> ParityError {
    malformed("manifest.json", detail)
}

fn manifest_error(e: ReferenceError) -> ParityError {
    bad_manifest(e)
}

/// An integer field that fits in `T`.
fn int<T: TryFrom<i64>>(obj: &Json, key: &str, at: &str) -> Result<T, ReferenceError> {
    get(obj, key, at)?
        .as_i64()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}")))
}

fn strings(obj: &Json, key: &str, at: &str) -> Result<Vec<String>, ReferenceError> {
    array(obj, key, at)?
        .iter()
        .enumerate()
        .map(|(i, v)| v.as_str().map(str::to_string).ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}[{i}]"))))
        .collect()
}

fn counts(obj: &Json, key: &str, at: &str) -> Result<Vec<usize>, ReferenceError> {
    array(obj, key, at)?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_i64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}[{i}]")))
        })
        .collect()
}

fn precision(s: &str, at: &str) -> Result<Dtype, ParityError> {
    match s {
        "f32" => Ok(Dtype::F32),
        "f64" => Ok(Dtype::F64),
        other => Err(bad_manifest(format!("{at}: precision {other:?}, expected \"f32\" or \"f64\""))),
    }
}

fn suffix(p: Dtype) -> &'static str {
    match p {
        Dtype::F32 => "f32",
        Dtype::F64 => "f64",
    }
}

fn entry(e: &Json, kind: BatteryKind, at: &str) -> Result<StimulusEntry, ReferenceError> {
    let window = counts(e, "window", at)?;
    let &[start, end] = &window[..] else { return Err(ReferenceError::WrongType(format!("{at}.window"))) };
    let flash = kind == BatteryKind::Flashes;
    Ok(StimulusEntry {
        index: int(e, "index", at)?,
        label: string(e, "label", at)?,
        n_frames: int(e, "n_frames", at)?,
        window: (start, end),
        intensity: int(e, "intensity", at)?,
        radius: if flash { Some(int(e, "radius", at)?) } else { None },
        angle: if flash { None } else { Some(int(e, "angle", at)?) },
        speed: if flash { None } else { Some(number(e, "speed", at)?) },
        width: if flash { None } else { Some(int(e, "width", at)?) },
    })
}

impl Layout {
    fn parse(text: &str) -> Result<Layout, ParityError> {
        let root = json::parse(text).map_err(|e| bad_manifest(format!("not valid JSON (byte {})", e.offset)))?;
        let r = &root;
        let at = "manifest";
        let format: i64 = int(r, "format", at).map_err(manifest_error)?;
        if format != 1 {
            return Err(bad_manifest(format!("unsupported format {format}")));
        }
        let models = get(r, "models", at).map_err(manifest_error)?;
        let models_f32 = strings(models, "f32", "manifest.models").map_err(manifest_error)?;
        let models_f64 = strings(models, "f64", "manifest.models").map_err(manifest_error)?;
        let cell_types = strings(r, "cell_types", at).map_err(manifest_error)?;

        let batteries = get(r, "batteries", at).map_err(manifest_error)?;
        let mut tables = Vec::new();
        for kind in [BatteryKind::Flashes, BatteryKind::Edges] {
            let at = format!("manifest.batteries.{}", kind.name());
            let list = array(batteries, kind.name(), "manifest.batteries")
                .map_err(manifest_error)?
                .iter()
                .enumerate()
                .map(|(k, e)| entry(e, kind, &format!("{at}[{k}]")))
                .collect::<Result<Vec<_>, _>>()
                .map_err(manifest_error)?;
            for (k, s) in list.iter().enumerate() {
                if s.index != k {
                    return Err(bad_manifest(format!("{at}[{k}] has index {}", s.index)));
                }
                if !(s.window.0 <= s.window.1 && s.window.1 < s.n_frames) {
                    return Err(bad_manifest(format!(
                        "{at}[{k}]: window [{}, {}] is not inside its {} frames",
                        s.window.0, s.window.1, s.n_frames
                    )));
                }
            }
            tables.push(list);
        }
        let edges = tables.pop().expect("two batteries");
        let flashes = tables.pop().expect("two batteries");

        let frames = get(r, "frames", at).map_err(manifest_error)?;
        let at_f = "manifest.frames";
        let rows: usize = int(frames, "rows", at_f).map_err(manifest_error)?;
        let hexals: usize = int(frames, "hexals", at_f).map_err(manifest_error)?;
        let frames_bytes: usize = int(frames, "bytes", at_f).map_err(manifest_error)?;
        for (key, want) in [("file", FRAMES), ("dtype", "<f8")] {
            let found = string(frames, key, at_f).map_err(manifest_error)?;
            if found != want {
                return Err(bad_manifest(format!("{at_f}.{key} is {found:?}, expected {want:?}")));
            }
        }
        if hexals != HEXALS {
            return Err(bad_manifest(format!("{at_f}.hexals is {hexals}, expected {HEXALS}")));
        }
        if rows.checked_mul(HEXALS * 8) != Some(frames_bytes) {
            return Err(bad_manifest(format!("{at_f}: {frames_bytes} bytes for {rows} rows of {HEXALS} f64")));
        }

        let fi = get(r, "frame_index", at).map_err(manifest_error)?;
        let at_i = "manifest.frame_index";
        for (key, want) in [("file", FRAME_INDEX), ("dtype", "<u4")] {
            let found = string(fi, key, at_i).map_err(manifest_error)?;
            if found != want {
                return Err(bad_manifest(format!("{at_i}.{key} is {found:?}, expected {want:?}")));
            }
        }
        let blocks = array(fi, "blocks", at_i).map_err(manifest_error)?;
        let stimuli: Vec<(BatteryKind, &StimulusEntry)> = flashes
            .iter()
            .map(|e| (BatteryKind::Flashes, e))
            .chain(edges.iter().map(|e| (BatteryKind::Edges, e)))
            .collect();
        if blocks.len() != stimuli.len() {
            return Err(bad_manifest(format!("{at_i}: {} blocks for {} stimuli", blocks.len(), stimuli.len())));
        }
        let mut starts = Vec::with_capacity(stimuli.len());
        let mut offset = 0usize;
        for (k, (b, (kind, s))) in blocks.iter().zip(&stimuli).enumerate() {
            let at = format!("{at_i}.blocks[{k}]");
            let battery = string(b, "battery", &at).map_err(manifest_error)?;
            let stimulus: usize = int(b, "stimulus", &at).map_err(manifest_error)?;
            let off: usize = int(b, "offset", &at).map_err(manifest_error)?;
            let count: usize = int(b, "count", &at).map_err(manifest_error)?;
            if battery != kind.name() || stimulus != s.index {
                return Err(bad_manifest(format!(
                    "{at} is {battery} {stimulus}, expected {} {}: one block per stimulus, flashes then edges",
                    kind.name(),
                    s.index
                )));
            }
            if off != offset {
                return Err(bad_manifest(format!("{at}: offset {off}, expected {offset}")));
            }
            if count != s.n_frames {
                return Err(bad_manifest(format!("{at}: count {count}, the stimulus has {} frames", s.n_frames)));
            }
            starts.push(offset);
            offset += count;
        }

        let mut arrays: Vec<ArrayEntry> = Vec::new();
        for (k, a) in array(r, "arrays", at).map_err(manifest_error)?.iter().enumerate() {
            let at = format!("manifest.arrays[{k}]");
            let name = string(a, "name", &at).map_err(manifest_error)?;
            let p = precision(&string(a, "precision", &at).map_err(manifest_error)?, &at)?;
            let file = string(a, "file", &at).map_err(manifest_error)?;
            let dtype = match string(a, "dtype", &at).map_err(manifest_error)?.as_str() {
                "<f4" => Dtype::F32,
                "<f8" => Dtype::F64,
                other => return Err(bad_manifest(format!("{at}: dtype {other:?}, expected \"<f4\" or \"<f8\""))),
            };
            let shape = counts(a, "shape", &at).map_err(manifest_error)?;
            let bytes: usize = int(a, "bytes", &at).map_err(manifest_error)?;
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(bad_manifest(format!("{at}: array name {name:?}")));
            }
            let want_file = format!("{name}.{}.bin", suffix(p));
            if file != want_file {
                return Err(bad_manifest(format!("{at}: file {file:?}, expected {want_file:?}")));
            }
            let want_bytes = shape.iter().try_fold(dtype.size(), |n, &d| n.checked_mul(d));
            if want_bytes != Some(bytes) {
                return Err(bad_manifest(format!("{at} ({file}): {bytes} bytes, its shape {shape:?} gives {want_bytes:?}")));
            }
            let n_models = match p {
                Dtype::F32 => models_f32.len(),
                Dtype::F64 => models_f64.len(),
            };
            if shape.first() != Some(&n_models) {
                return Err(bad_manifest(format!(
                    "{at} ({file}): shape {shape:?}, the leading axis must be the {n_models} models at {}",
                    suffix(p)
                )));
            }
            if arrays.iter().any(|x| x.name == name && x.precision == p) {
                return Err(bad_manifest(format!("{at}: {file} is listed twice")));
            }
            arrays.push(ArrayEntry { name, precision: p, dtype, file, shape, bytes });
        }

        Ok(Layout {
            cell_types,
            models_f32,
            models_f64,
            flashes,
            edges,
            rows,
            frames_bytes,
            starts,
            index_entries: offset,
            arrays,
        })
    }

    /// Every data file the manifest names.
    fn files(&self) -> Vec<&str> {
        [FRAMES, FRAME_INDEX].into_iter().chain(self.arrays.iter().map(|a| a.file.as_str())).collect()
    }

    fn build(self, files: Vec<(String, Vec<u8>)>) -> Result<IndexFixture, ParityError> {
        let names = self.files();
        let mut by_name: Vec<(String, Option<Vec<u8>>)> = Vec::with_capacity(files.len());
        for (name, data) in files {
            if !names.contains(&name.as_str()) {
                return Err(ParityError::Mismatch(format!("{DIR}/{name}: not named by the manifest")));
            }
            if by_name.iter().any(|(n, _)| *n == name) {
                return Err(ParityError::Mismatch(format!("{DIR}/{name}: given twice")));
            }
            by_name.push((name, Some(data)));
        }
        let mut take = |name: &str| -> Result<Vec<u8>, ParityError> {
            by_name
                .iter_mut()
                .find(|(n, _)| n == name)
                .and_then(|(_, d)| d.take())
                .ok_or_else(|| ParityError::Mismatch(format!("{DIR}/{name}: named by the manifest but not given")))
        };

        let frames = take(FRAMES)?;
        if frames.len() != self.frames_bytes {
            return Err(malformed(FRAMES, format!("{} bytes, the manifest says {}", frames.len(), self.frames_bytes)));
        }
        let frames: Vec<f64> =
            frames.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().expect("8 bytes"))).collect();

        let index = take(FRAME_INDEX)?;
        if index.len() != self.index_entries * 4 {
            return Err(malformed(
                FRAME_INDEX,
                format!("{} bytes, its blocks need {} entries of 4", index.len(), self.index_entries),
            ));
        }
        let index: Vec<u32> =
            index.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().expect("4 bytes"))).collect();
        if let Some((k, &row)) = index.iter().enumerate().find(|&(_, &row)| row as usize >= self.rows) {
            return Err(malformed(FRAME_INDEX, format!("entry {k} is row {row}; frames has {} rows", self.rows)));
        }

        let mut arrays = Vec::with_capacity(self.arrays.len());
        for a in self.arrays {
            let data = take(&a.file)?;
            if data.len() != a.bytes {
                return Err(malformed(&a.file, format!("{} bytes, the manifest says {}", data.len(), a.bytes)));
            }
            arrays.push((a, data));
        }

        Ok(IndexFixture {
            cell_types: self.cell_types,
            models_f32: self.models_f32,
            models_f64: self.models_f64,
            flashes: self.flashes,
            edges: self.edges,
            frames,
            index,
            starts: self.starts,
            arrays,
        })
    }
}

impl IndexFixture {
    /// Reads `<data>/flyvis-indices/manifest.json` and every file it names.
    /// A missing file is `ParityError::Missing`, with the way to get it.
    pub fn load(data: &Path) -> Result<IndexFixture, ParityError> {
        let layout = Layout::parse(&read_text(data, MANIFEST)?)?;
        let files = layout
            .files()
            .into_iter()
            .map(|name| Ok((name.to_string(), read_bytes(data, &format!("{DIR}/{name}"))?)))
            .collect::<Result<Vec<_>, ParityError>>()?;
        layout.build(files)
    }

    /// The set from the manifest's text and its data files' bytes, by file
    /// name (`"frames.f64.bin"`, …): exactly the files the manifest names.
    pub fn from_bytes(manifest_json: &str, files: Vec<(String, Vec<u8>)>) -> Result<IndexFixture, ParityError> {
        Layout::parse(manifest_json)?.build(files)
    }

    /// flyvis's 65 cell types, in the order of every array's neuron axis.
    pub fn cell_types(&self) -> &[String] {
        &self.cell_types
    }

    /// The models along every array's leading axis at `precision`.
    pub fn models(&self, precision: Dtype) -> &[String] {
        match precision {
            Dtype::F32 => &self.models_f32,
            Dtype::F64 => &self.models_f64,
        }
    }

    pub fn flashes(&self) -> &[StimulusEntry] {
        &self.flashes
    }

    pub fn edges(&self) -> &[StimulusEntry] {
        &self.edges
    }

    fn entries(&self, kind: BatteryKind) -> &[StimulusEntry] {
        match kind {
            BatteryKind::Flashes => &self.flashes,
            BatteryKind::Edges => &self.edges,
        }
    }

    /// The index array `name` computed at `precision`, widened to f64 from
    /// whichever dtype it was stored in.
    pub fn array(&self, name: &str, precision: Dtype) -> Result<IndexArray, ParityError> {
        let (a, bytes) = self
            .arrays
            .iter()
            .find(|(a, _)| a.name == name && a.precision == precision)
            .ok_or_else(|| ParityError::Mismatch(format!("{DIR}: no array {name}.{}", suffix(precision))))?;
        let data = match a.dtype {
            Dtype::F32 => {
                bytes.chunks_exact(4).map(|c| f64::from(f32::from_le_bytes(c.try_into().expect("4 bytes")))).collect()
            }
            Dtype::F64 => bytes.chunks_exact(8).map(|c| f64::from_le_bytes(c.try_into().expect("8 bytes"))).collect(),
        };
        Ok(IndexArray { shape: a.shape.clone(), data })
    }

    /// Stimulus `s` of battery `kind`: its `n_frames × 721` input values,
    /// as flyvis fed them.
    pub fn expand(&self, kind: BatteryKind, s: usize) -> Result<Vec<f64>, ParityError> {
        let mut out = Vec::new();
        self.expand_into(kind, s, &mut out)?;
        Ok(out)
    }

    fn expand_into(&self, kind: BatteryKind, s: usize, out: &mut Vec<f64>) -> Result<(), ParityError> {
        let entries = self.entries(kind);
        let e = entries.get(s).ok_or_else(|| {
            ParityError::Usage(format!("{DIR}: no {} stimulus {s}; there are {}", kind.name(), entries.len()))
        })?;
        let start = self.starts[if kind == BatteryKind::Flashes { s } else { self.flashes.len() + s }];
        out.clear();
        out.reserve(e.n_frames * HEXALS);
        for &row in &self.index[start..start + e.n_frames] {
            let row = row as usize;
            out.extend_from_slice(&self.frames[row * HEXALS..(row + 1) * HEXALS]);
        }
        Ok(())
    }

    /// Battery `kind`'s stimuli in index order, labelled as the manifest
    /// labels them.
    pub fn battery(&self, kind: BatteryKind) -> IndexBattery<'_> {
        IndexBattery { x: self, kind, next: 0 }
    }
}

/// One exported battery's input, for `parity::respond`.
pub struct IndexBattery<'a> {
    x: &'a IndexFixture,
    kind: BatteryKind,
    next: usize,
}

impl Battery for IndexBattery<'_> {
    fn len(&self) -> usize {
        self.x.entries(self.kind).len()
    }

    fn next_stimulus(&mut self, out: &mut Vec<f64>) -> Result<Option<String>, ParityError> {
        let Some(e) = self.x.entries(self.kind).get(self.next) else { return Ok(None) };
        self.x.expand_into(self.kind, self.next, out)?;
        self.next += 1;
        Ok(Some(e.label.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{or_skip, read_fixture_bytes, read_fixture_text, require_present};

    /// One flash of 2 frames, one edge of 3, two distinct frames, one f32
    /// array of shape [1, 2] (one model at f32).
    fn tiny() -> (String, Vec<(String, Vec<u8>)>) {
        let manifest = r#"{"format":1,"flyvis_version":"1.2.0","dt":0.005,"t_pre":1.0,"pre_stim_value":0.5,
          "models":{"f32":["000"],"f64":["000"]},"cell_types":["R1","T4a"],"speeds":[2.4],
          "batteries":{"flashes":[{"index":0,"label":"flash r6 i1","baseline":0.5,"intensity":1,"radius":6,"n_frames":2,"window":[0,1]}],
                       "edges":[{"index":0,"label":"edge a0 i0 s2.4","angle":0,"intensity":0,"speed":2.4,"width":80,"n_frames":3,"window":[0,2]}]},
          "frames":{"file":"frames.f64.bin","dtype":"<f8","rows":2,"hexals":721,"bytes":11536},
          "frame_index":{"file":"frame_index.u32.bin","dtype":"<u4","blocks":[
             {"battery":"flashes","stimulus":0,"offset":0,"count":2},{"battery":"edges","stimulus":0,"offset":2,"count":3}]},
          "arrays":[{"name":"fri","precision":"f32","file":"fri.f32.bin","dtype":"<f4","shape":[1,2],"bytes":8},
                    {"name":"dsi","precision":"f64","file":"dsi.f64.bin","dtype":"<f8","shape":[1],"bytes":8}],
          "sha256":{}}"#;
        let mut frames = vec![0.5f64; 721];
        frames.extend(vec![1.0f64; 721]);
        let frames: Vec<u8> = frames.iter().flat_map(|v| v.to_le_bytes()).collect();
        let index: Vec<u8> = [0u32, 1, 0, 0, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let fri: Vec<u8> = [0.25f32, -0.5].iter().flat_map(|v| v.to_le_bytes()).collect();
        let dsi: Vec<u8> = 0.125f64.to_le_bytes().to_vec();
        (
            manifest.to_string(),
            vec![
                ("frames.f64.bin".into(), frames),
                ("frame_index.u32.bin".into(), index),
                ("fri.f32.bin".into(), fri),
                ("dsi.f64.bin".into(), dsi),
            ],
        )
    }

    fn is_malformed(r: Result<IndexFixture, ParityError>, file: &str) {
        match r {
            Err(ParityError::Malformed { rel, .. }) => assert_eq!(rel, format!("{DIR}/{file}")),
            Err(e) => panic!("expected Malformed for {file}, got {e}"),
            Ok(_) => panic!("expected Malformed for {file}, got a fixture"),
        }
    }

    #[test]
    fn a_tiny_fixture_reads_back() {
        let (m, files) = tiny();
        let x = IndexFixture::from_bytes(&m, files).unwrap();
        assert_eq!(x.cell_types(), ["R1", "T4a"]);
        assert_eq!((x.models(Dtype::F32), x.models(Dtype::F64)), (&["000".to_string()][..], &["000".to_string()][..]));
        assert_eq!(x.array("fri", Dtype::F32).unwrap(), IndexArray { shape: vec![1, 2], data: vec![0.25, -0.5] });
        assert_eq!(x.array("dsi", Dtype::F64).unwrap(), IndexArray { shape: vec![1], data: vec![0.125] });
        assert_eq!(
            x.flashes(),
            [StimulusEntry {
                index: 0,
                label: "flash r6 i1".into(),
                n_frames: 2,
                window: (0, 1),
                intensity: 1,
                radius: Some(6),
                angle: None,
                speed: None,
                width: None,
            }]
        );
        assert_eq!((x.edges()[0].angle, x.edges()[0].speed, x.edges()[0].width, x.edges()[0].radius), (Some(0), Some(2.4), Some(80), None));

        let mut b = x.battery(BatteryKind::Edges);
        assert_eq!(b.len(), 1);
        let mut out = vec![9.0; 5];
        assert_eq!(b.next_stimulus(&mut out).unwrap().as_deref(), Some("edge a0 i0 s2.4"));
        assert_eq!(out.len(), 3 * 721);
        assert_eq!((out[0], out[721], out[2 * 721]), (0.5, 0.5, 1.0));
        assert_eq!(b.next_stimulus(&mut out).unwrap(), None);

        let mut b = x.battery(BatteryKind::Flashes);
        assert_eq!(b.len(), 1);
        assert_eq!(b.next_stimulus(&mut out).unwrap().as_deref(), Some("flash r6 i1"));
        assert_eq!((out.len(), out[0], out[721]), (2 * 721, 0.5, 1.0));
        assert_eq!(x.expand(BatteryKind::Flashes, 0).unwrap(), out);
        assert!(matches!(x.expand(BatteryKind::Flashes, 1), Err(ParityError::Usage(_))));
    }

    #[test]
    fn a_shape_that_disagrees_with_its_bytes_is_malformed() {
        let (m, files) = tiny();
        let m = m.replace(r#""shape":[1,2],"bytes":8"#, r#""shape":[1,3],"bytes":8"#);
        is_malformed(IndexFixture::from_bytes(&m, files), "manifest.json");
    }

    #[test]
    fn a_file_shorter_than_its_entry_is_malformed() {
        let (m, mut files) = tiny();
        files[2].1.pop();
        is_malformed(IndexFixture::from_bytes(&m, files), "fri.f32.bin");
    }

    #[test]
    fn a_leading_axis_other_than_the_model_count_is_malformed() {
        let (m, files) = tiny();
        let m = m.replace(r#""shape":[1,2],"bytes":8"#, r#""shape":[2,1],"bytes":8"#);
        is_malformed(IndexFixture::from_bytes(&m, files), "manifest.json");
    }

    #[test]
    fn a_frame_index_past_the_last_row_is_malformed() {
        let (m, mut files) = tiny();
        files[1].1 = [0u32, 1, 0, 0, 2].iter().flat_map(|v| v.to_le_bytes()).collect();
        is_malformed(IndexFixture::from_bytes(&m, files), "frame_index.u32.bin");
    }

    #[test]
    fn a_block_count_other_than_its_stimulus_frames_is_malformed() {
        let (m, files) = tiny();
        let m = m.replace(r#""offset":2,"count":3"#, r#""offset":2,"count":2"#);
        is_malformed(IndexFixture::from_bytes(&m, files), "manifest.json");
    }

    #[test]
    fn a_window_past_its_stimulus_is_malformed() {
        let (m, files) = tiny();
        let m = m.replace(r#""n_frames":3,"window":[0,2]"#, r#""n_frames":3,"window":[0,3]"#);
        is_malformed(IndexFixture::from_bytes(&m, files), "manifest.json");
    }

    #[test]
    fn an_unsupported_format_or_bad_json_is_malformed() {
        let (m, files) = tiny();
        is_malformed(IndexFixture::from_bytes(&m.replace(r#""format":1"#, r#""format":2"#), files.clone()), "manifest.json");
        is_malformed(IndexFixture::from_bytes("{", files), "manifest.json");
    }

    #[test]
    fn a_missing_or_unlisted_file_is_a_mismatch() {
        let (m, mut files) = tiny();
        let dsi = files.pop().unwrap();
        assert!(matches!(IndexFixture::from_bytes(&m, files.clone()), Err(ParityError::Mismatch(_))));
        files.push(dsi);
        files.push(("extra.bin".into(), vec![]));
        assert!(matches!(IndexFixture::from_bytes(&m, files), Err(ParityError::Mismatch(_))));
    }

    #[test]
    fn a_missing_array_is_a_mismatch() {
        let (m, files) = tiny();
        let x = IndexFixture::from_bytes(&m, files).unwrap();
        assert!(matches!(x.array("dsi", Dtype::F32), Err(ParityError::Mismatch(_))));
        assert!(matches!(x.array("fri", Dtype::F64), Err(ParityError::Mismatch(_))));
    }

    #[test]
    fn load_reports_a_missing_set_as_missing_with_its_remedy() {
        let dir = crate::test_support::scratch_dir("indices-ref-missing");
        match IndexFixture::load(&dir) {
            Err(e @ ParityError::Missing { .. }) => {
                let ParityError::Missing { rel, .. } = &e else { unreachable!() };
                assert_eq!(rel, "flyvis-indices/manifest.json");
                assert!(e.to_string().contains("export_indices.py"), "{e}");
            }
            other => panic!("expected Missing, got {:?}", other.err()),
        }
    }

    /// The staged set, or `None` after a skip notice. Under
    /// `SEMBLANCE_REQUIRE_FIXTURES=1` an absent manifest fails instead; a
    /// present manifest with anything wrong behind it always fails.
    fn real() -> Option<IndexFixture> {
        or_skip(require_present(&[MANIFEST]))?;
        Some(IndexFixture::load(&crate::test_support::data_dir()).unwrap_or_else(|e| panic!("{e}")))
    }

    #[test]
    fn the_real_fixture_has_the_tabled_batteries() {
        let Some(x) = real() else { return };
        assert_eq!((x.flashes().len(), x.edges().len(), x.cell_types().len()), (4, 144, 65));
        let all: Vec<String> = (0..50).map(|i| format!("{i:03}")).collect();
        assert_eq!(x.models(Dtype::F32), &all[..]);
        assert_eq!(x.models(Dtype::F64), ["000", "049"]);
        for e in x.flashes() {
            assert_eq!((e.n_frames, e.window), (600, (200, 400)), "{}", e.label);
        }
        let ends = [(2.4, 878, 1078), (4.8, 539, 739), (9.7, 367, 567), (13.0, 325, 525), (19.0, 285, 485), (25.0, 265, 465)];
        for e in x.edges() {
            let &(_, end, n) = ends.iter().find(|(s, _, _)| Some(*s) == e.speed).expect("a tabled speed");
            assert_eq!((e.n_frames, e.window), (n, (200, end)), "{}", e.label);
        }
        for (name, shape) in [
            ("fri", vec![65]),
            ("fri_peaks", vec![65, 2]),
            ("edge_peaks", vec![144, 65]),
            ("dsi", vec![2, 65]),
            ("pd", vec![2, 65]),
            ("tuning_r", vec![8, 2, 6]),
            ("fri_to_known", vec![]),
            ("dsi_to_known", vec![]),
            ("pd_to_known", vec![8]),
        ] {
            for (p, n) in [(Dtype::F32, 50), (Dtype::F64, 2)] {
                let a = x.array(name, p).unwrap();
                let want: Vec<usize> = std::iter::once(n).chain(shape.iter().copied()).collect();
                assert_eq!(a.shape, want, "{name} at {p:?}");
                assert_eq!(a.data.len(), want.iter().product::<usize>(), "{name} at {p:?}");
            }
        }
    }

    /// The flashes and the speed-13 edges the reference holds expand to
    /// fixtures-2's input.f64.bin, frame for frame.
    #[test]
    fn the_shared_stimuli_expand_to_fixtures_2s_input() {
        let Some(x) = real() else { return };
        let Some(text) = or_skip(read_fixture_text("flyvis-reference/manifest.json")) else { return };
        let Some(input) = or_skip(read_fixture_bytes("flyvis-reference/input.f64.bin")) else { return };
        let manifest = crate::reference::Manifest::parse(&text).unwrap();
        let sequences = crate::reference::parse_sequences(&text).unwrap();
        let file = manifest.file("input.f64.bin").unwrap();
        assert_eq!(sequences.len(), 12);
        for q in &sequences {
            let key = crate::reference::BlockKey { run: None, dt: "1/200", sequence: Some(q.index) };
            let want = file.read_f64(&input, &key).unwrap();
            let intensity = q.arg("intensity");
            let (kind, s) = if q.family == "flashes" {
                let s = x.flashes().iter().position(|e| {
                    Some(f64::from(e.intensity)) == intensity && e.radius.map(f64::from) == q.arg("radius")
                });
                (BatteryKind::Flashes, s)
            } else {
                let s = x.edges().iter().position(|e| {
                    e.angle.map(f64::from) == q.arg("angle") && Some(f64::from(e.intensity)) == intensity && e.speed == q.arg("speed")
                });
                (BatteryKind::Edges, s)
            };
            let s = s.unwrap_or_else(|| panic!("sequence {} ({}) is not in the battery", q.index, q.family));
            let got = x.expand(kind, s).unwrap();
            assert_eq!(got.len(), want.data.len(), "sequence {}", q.index);
            assert!(got.iter().zip(&want.data).all(|(a, b)| a.to_bits() == b.to_bits()), "sequence {}", q.index);
        }
    }
}
