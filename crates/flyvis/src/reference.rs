//! Reads epic 0.3.0 task 1's reference set: `data/flyvis-reference/manifest.json`
//! and the raw little-endian `.bin` files it describes, written by
//! `tools/export_reference.py`.
//!
//! No file I/O: callers hand in the manifest text and a file's bytes. Files
//! are not hashed here -- `ci/fetch-fixtures.sh` verifies every fixture
//! against `ci/fixtures.sha256` -- so this checks structure: every block
//! inside its file, contiguous, sized to its shape, uniquely keyed. Keys the
//! reader does not use are ignored. Duplicate JSON keys are not detected
//! (`json.rs` returns the first); the file is hash-pinned.
//!
//! Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 5.
//! The manifest's `sequences`, read by [`parse_sequences`]:
//! docs/superpowers/specs/2026-10-28-stimulus-traces-design.md, section 2.

use crate::json::{self, Json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dtype {
    F32,
    F64,
}

impl Dtype {
    pub fn size(self) -> usize {
        match self {
            Dtype::F32 => 4,
            Dtype::F64 => 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DtInfo { pub id: String, pub dt: f64, pub floor_dt_f32: f64, pub n_settle: usize }

#[derive(Clone, Debug, PartialEq)]
pub struct RunInfo { pub index: u32, pub model: String, pub dt: String, pub precision: Dtype }

#[derive(Clone, Debug, PartialEq)]
pub struct CentreNode { pub node: u32, pub cell_type: String }

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub run: Option<u32>,
    pub dt: String,
    pub sequence: Option<u32>,
    pub offset: usize,
    pub bytes: usize,
    pub shape: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileInfo { pub name: String, pub dtype: Dtype, pub axes: Vec<String>, pub bytes: usize, pub blocks: Vec<Block> }

/// Selects one block. All three fields must match, `None` included.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockKey<'a> { pub run: Option<u32>, pub dt: &'a str, pub sequence: Option<u32> }

/// A block's values with its shape (C order).
#[derive(Clone, Debug, PartialEq)]
pub struct Array<T> { pub shape: Vec<usize>, pub data: Vec<T> }

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub n_nodes: usize,
    pub t_pre: f64,
    pub pre_stim_value: f64,
    pub dts: Vec<DtInfo>,
    pub runs: Vec<RunInfo>,
    pub centre_nodes: Vec<CentreNode>,
    pub files: Vec<FileInfo>,
}

/// One reference stimulus sequence, from the manifest's `sequences`. Field
/// names are the manifest's own.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceInfo {
    /// `index`: the sequence's key in every per-sequence block.
    pub index: u32,
    /// `family`: `"flashes"` or `"moving_edge"`.
    pub family: String,
    /// `flyvis_index`: its index within flyvis's own stimulus dataset.
    pub flyvis_index: u32,
    /// `args`: every numeric entry, in file order. Non-numeric entries
    /// (`offsets`, `post_pad_mode`) are skipped.
    pub args: Vec<(String, f64)>,
    /// `frames`: one entry per dt id, in file order.
    pub frames: Vec<SequenceFrames>,
}

/// A sequence's frame layout at one dt.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceFrames {
    /// The key: `"1/200"` or `"1/50"`.
    pub dt: String,
    pub n_frames: usize,
    /// `[start, end)`: end is exclusive.
    pub stimulus_frames: (usize, usize),
    /// The frames `snapshots.*.bin` holds the full state at, ascending.
    pub snapshot_frames: Vec<usize>,
}

impl SequenceInfo {
    /// The numeric argument `name`, if the manifest gives one.
    pub fn arg(&self, name: &str) -> Option<f64> {
        self.args.iter().find(|(k, _)| k == name).map(|&(_, v)| v)
    }

    /// The frame layout at dt id `dt`.
    pub fn frames(&self, dt: &str) -> Option<&SequenceFrames> {
        self.frames.iter().find(|f| f.dt == dt)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReferenceError {
    Json { offset: usize },
    Missing(String),
    WrongType(String),
    Unsupported { field: String, found: String },
    /// A block's `bytes` is not product(shape) x dtype size.
    BlockShape { file: String, block: usize },
    /// A block's shape has a different rank from the file's axes.
    BlockAxes { file: String, block: usize },
    /// A block does not start where the previous one ended.
    BlockLayout { file: String, block: usize },
    BlockTotal { file: String, expected: usize, found: usize },
    DuplicateBlock { file: String, block: usize },
    ByteLength { file: String, expected: usize, found: usize },
    Dtype { file: String, expected: Dtype, found: Dtype },
    NoBlock { file: String },
    /// A sequence's frame indices are out of order or past its `n_frames`.
    FrameRange(String),
    /// PROVENANCE.json lacks this field, or has it with the wrong type.
    /// Named by its JSON path (`git.dirty`, `models[1].id`).
    ProvenanceMissing(String),
}

impl std::fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ReferenceError::*;
        match self {
            Json { offset } => write!(f, "manifest is not valid JSON (byte {offset})"),
            Missing(p) => write!(f, "manifest has no {p}"),
            WrongType(p) => write!(f, "manifest {p} has the wrong type"),
            Unsupported { field, found } => write!(f, "unsupported {field}: {found}"),
            BlockShape { file, block } => write!(f, "{file} block {block}: bytes do not match its shape"),
            BlockAxes { file, block } => write!(f, "{file} block {block}: shape rank differs from the axes"),
            BlockLayout { file, block } => write!(f, "{file} block {block}: overlaps or leaves a gap"),
            BlockTotal { file, expected, found } => write!(f, "{file}: blocks cover {found} bytes, file has {expected}"),
            DuplicateBlock { file, block } => write!(f, "{file} block {block}: key already used"),
            ByteLength { file, expected, found } => write!(f, "{file}: {found} bytes, manifest says {expected}"),
            Dtype { file, expected, found } => write!(f, "{file} is {found:?}, asked for {expected:?}"),
            NoBlock { file } => write!(f, "{file}: no block has that key"),
            FrameRange(p) => write!(f, "manifest {p} is out of order or past n_frames"),
            ProvenanceMissing(p) => write!(f, "PROVENANCE.json has no {p}"),
        }
    }
}

impl std::error::Error for ReferenceError {}

fn get<'a>(obj: &'a Json, key: &str, at: &str) -> Result<&'a Json, ReferenceError> {
    obj.get(key).ok_or_else(|| ReferenceError::Missing(format!("{at}.{key}")))
}

fn string(obj: &Json, key: &str, at: &str) -> Result<String, ReferenceError> {
    get(obj, key, at)?.as_str().map(str::to_string).ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}")))
}

fn number(obj: &Json, key: &str, at: &str) -> Result<f64, ReferenceError> {
    get(obj, key, at)?.as_f64().ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}")))
}

fn count(obj: &Json, key: &str, at: &str) -> Result<usize, ReferenceError> {
    get(obj, key, at)?
        .as_i64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}")))
}

fn index(value: &Json, at: &str) -> Result<u32, ReferenceError> {
    value.as_i64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| ReferenceError::WrongType(at.to_string()))
}

/// `null` is `None`; otherwise a non-negative integer that fits in u32.
fn optional_index(obj: &Json, key: &str, at: &str) -> Result<Option<u32>, ReferenceError> {
    let v = get(obj, key, at)?;
    if v.is_null() { Ok(None) } else { index(v, &format!("{at}.{key}")).map(Some) }
}

fn array<'a>(obj: &'a Json, key: &str, at: &str) -> Result<&'a [Json], ReferenceError> {
    get(obj, key, at)?.as_array().ok_or_else(|| ReferenceError::WrongType(format!("{at}.{key}")))
}

fn require(obj: &Json, key: &str, want: &str) -> Result<(), ReferenceError> {
    let found = match get(obj, key, "manifest")? {
        Json::Number(n) => n.to_string(),
        other => other.as_str().map(str::to_string).ok_or_else(|| ReferenceError::WrongType(format!("manifest.{key}")))?,
    };
    if found == want { Ok(()) } else { Err(ReferenceError::Unsupported { field: key.to_string(), found }) }
}

/// An object's entries, in file order.
fn object<'a>(obj: &'a Json, key: &str, at: &str) -> Result<&'a [(String, Json)], ReferenceError> {
    match get(obj, key, at)? {
        Json::Object(pairs) => Ok(pairs),
        _ => Err(ReferenceError::WrongType(format!("{at}.{key}"))),
    }
}

/// A non-negative integer: a frame index or count.
fn frame_index(value: &Json, at: &str) -> Result<usize, ReferenceError> {
    value.as_i64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| ReferenceError::WrongType(at.to_string()))
}

fn parse_frames(dt: &str, f: &Json, at: &str) -> Result<SequenceFrames, ReferenceError> {
    let n_frames = count(f, "n_frames", at)?;
    let window_at = format!("{at}.stimulus_frames");
    let [start, end] = array(f, "stimulus_frames", at)? else {
        return Err(ReferenceError::WrongType(window_at));
    };
    let stimulus_frames = (frame_index(start, &window_at)?, frame_index(end, &window_at)?);
    if !(stimulus_frames.0 <= stimulus_frames.1 && stimulus_frames.1 <= n_frames) {
        return Err(ReferenceError::FrameRange(window_at));
    }
    let snapshots_at = format!("{at}.snapshot_frames");
    let snapshot_frames = array(f, "snapshot_frames", at)?
        .iter()
        .map(|v| frame_index(v, &snapshots_at))
        .collect::<Result<Vec<_>, _>>()?;
    let ascending = snapshot_frames.windows(2).all(|w| w[0] < w[1]);
    if !ascending || snapshot_frames.last().is_some_and(|&last| last >= n_frames) {
        return Err(ReferenceError::FrameRange(snapshots_at));
    }
    Ok(SequenceFrames { dt: dt.to_string(), n_frames, stimulus_frames, snapshot_frames })
}

/// Reads the manifest's `sequences`. Separate from [`Manifest::parse`], which
/// does not require them.
///
/// Checks structure only: every field present and typed, each
/// `stimulus_frames` with start <= end <= `n_frames`, each `snapshot_frames`
/// strictly ascending and inside `0..n_frames`. How many sequences there are,
/// and which dts each covers, are the caller's to check.
pub fn parse_sequences(json: &str) -> Result<Vec<SequenceInfo>, ReferenceError> {
    let root = json::parse(json).map_err(|e| ReferenceError::Json { offset: e.offset })?;
    array(&root, "sequences", "manifest")?
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let at = format!("sequences[{i}]");
            let args = object(s, "args", &at)?
                .iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .collect();
            let frames = object(s, "frames", &at)?
                .iter()
                .map(|(dt, f)| parse_frames(dt, f, &format!("{at}.frames.{dt}")))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SequenceInfo {
                index: index(get(s, "index", &at)?, &format!("{at}.index"))?,
                family: string(s, "family", &at)?,
                flyvis_index: index(get(s, "flyvis_index", &at)?, &format!("{at}.flyvis_index"))?,
                args,
                frames,
            })
        })
        .collect()
}

fn parse_file(f: &Json, at: &str) -> Result<FileInfo, ReferenceError> {
    let name = string(f, "name", at)?;
    let dtype = match string(f, "dtype", at)?.as_str() {
        "<f4" => Dtype::F32,
        "<f8" => Dtype::F64,
        other => return Err(ReferenceError::Unsupported { field: format!("{at}.dtype"), found: other.to_string() }),
    };
    let axes = array(f, "axes", at)?
        .iter()
        .map(|a| a.as_str().map(str::to_string).ok_or_else(|| ReferenceError::WrongType(format!("{at}.axes"))))
        .collect::<Result<Vec<_>, _>>()?;
    let blocks = array(f, "blocks", at)?
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let at = format!("{at}.blocks[{i}]");
            Ok(Block {
                run: optional_index(b, "run", &at)?,
                dt: string(b, "dt", &at)?,
                sequence: optional_index(b, "sequence", &at)?,
                offset: count(b, "offset", &at)?,
                bytes: count(b, "bytes", &at)?,
                shape: array(b, "shape", &at)?
                    .iter()
                    .map(|d| d.as_i64().and_then(|n| usize::try_from(n).ok()).ok_or_else(|| ReferenceError::WrongType(format!("{at}.shape"))))
                    .collect::<Result<Vec<_>, _>>()?,
            })
        })
        .collect::<Result<Vec<_>, ReferenceError>>()?;
    let file = FileInfo { name, dtype, axes, bytes: count(f, "bytes", at)?, blocks };
    check_layout(&file)?;
    Ok(file)
}

/// Every block sized to its shape, of the file's rank, uniquely keyed, and
/// laid end to end from offset 0 to the file's last byte.
fn check_layout(f: &FileInfo) -> Result<(), ReferenceError> {
    let err_at = |block| (f.name.clone(), block);
    let mut end = 0usize;
    for (i, b) in f.blocks.iter().enumerate() {
        let (file, block) = err_at(i);
        if b.shape.len() != f.axes.len() {
            return Err(ReferenceError::BlockAxes { file, block });
        }
        let size = b.shape.iter().try_fold(f.dtype.size(), |acc, &d| acc.checked_mul(d));
        if size != Some(b.bytes) {
            return Err(ReferenceError::BlockShape { file, block });
        }
        if b.offset != end {
            return Err(ReferenceError::BlockLayout { file, block });
        }
        if f.blocks[..i].iter().any(|p| p.run == b.run && p.dt == b.dt && p.sequence == b.sequence) {
            return Err(ReferenceError::DuplicateBlock { file, block });
        }
        end = b.offset.checked_add(b.bytes).ok_or(ReferenceError::BlockLayout { file: f.name.clone(), block: i })?;
    }
    if end != f.bytes {
        return Err(ReferenceError::BlockTotal { file: f.name.clone(), expected: f.bytes, found: end });
    }
    Ok(())
}

impl FileInfo {
    fn block(&self, bytes: &[u8], key: &BlockKey, want: Dtype) -> Result<&Block, ReferenceError> {
        if self.dtype != want {
            return Err(ReferenceError::Dtype { file: self.name.clone(), expected: want, found: self.dtype });
        }
        if bytes.len() != self.bytes {
            return Err(ReferenceError::ByteLength { file: self.name.clone(), expected: self.bytes, found: bytes.len() });
        }
        // The fields are public, so a FileInfo need not have come from
        // `Manifest::parse`; recheck the layout before slicing by it.
        check_layout(self)?;
        self.blocks
            .iter()
            .find(|b| b.run == key.run && b.dt == key.dt && b.sequence == key.sequence)
            .ok_or_else(|| ReferenceError::NoBlock { file: self.name.clone() })
    }
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, ReferenceError> {
        let root = json::parse(text).map_err(|e| ReferenceError::Json { offset: e.offset })?;
        let r = &root;
        let at = "manifest";
        require(r, "schema_version", "1")?;
        require(r, "byte_order", "little")?;
        require(r, "array_order", "C")?;
        let dts = array(r, "dts", at)?
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let at = format!("dts[{i}]");
                Ok(DtInfo {
                    id: string(d, "id", &at)?,
                    dt: number(d, "dt", &at)?,
                    floor_dt_f32: number(d, "floor_dt_f32", &at)?,
                    n_settle: count(d, "n_settle", &at)?,
                })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        let runs = array(r, "runs", at)?
            .iter()
            .enumerate()
            .map(|(i, x)| {
                let at = format!("runs[{i}]");
                let precision = match string(x, "precision", &at)?.as_str() {
                    "f32" => Dtype::F32,
                    "f64" => Dtype::F64,
                    other => return Err(ReferenceError::Unsupported { field: format!("{at}.precision"), found: other.to_string() }),
                };
                Ok(RunInfo {
                    index: index(get(x, "index", &at)?, &format!("{at}.index"))?,
                    model: string(x, "model", &at)?,
                    dt: string(x, "dt", &at)?,
                    precision,
                })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        let centre_nodes = array(r, "centre_nodes", at)?
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let at = format!("centre_nodes[{i}]");
                Ok(CentreNode { node: index(get(c, "node", &at)?, &format!("{at}.node"))?, cell_type: string(c, "cell_type", &at)? })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        let files = array(r, "files", at)?
            .iter()
            .enumerate()
            .map(|(i, f)| parse_file(f, &format!("files[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Manifest {
            n_nodes: count(r, "n_nodes", at)?,
            t_pre: number(r, "t_pre", at)?,
            pre_stim_value: number(r, "pre_stim_value", at)?,
            dts,
            runs,
            centre_nodes,
            files,
        })
    }

    pub fn file(&self, name: &str) -> Option<&FileInfo> {
        self.files.iter().find(|f| f.name == name)
    }
}

impl FileInfo {
    pub fn read_f32(&self, bytes: &[u8], key: &BlockKey) -> Result<Array<f32>, ReferenceError> {
        let b = self.block(bytes, key, Dtype::F32)?;
        let data = bytes[b.offset..b.offset + b.bytes]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(Array { shape: b.shape.clone(), data })
    }

    pub fn read_f64(&self, bytes: &[u8], key: &BlockKey) -> Result<Array<f64>, ReferenceError> {
        let b = self.block(bytes, key, Dtype::F64)?;
        let data = bytes[b.offset..b.offset + b.bytes]
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
            .collect();
        Ok(Array { shape: b.shape.clone(), data })
    }
}

/// What `PROVENANCE.json` records about the export the reference came from:
/// the fields `semblance validate` checks or prints. Each field names the
/// JSON path it is read from.
#[derive(Clone, Debug, PartialEq)]
pub struct Provenance {
    /// `exported`
    pub exported: String,
    /// `export_id`
    pub export_id: String,
    /// `generator.path`
    pub generator_path: String,
    /// `generator.sha256`
    pub generator_sha256: String,
    /// `git.head`
    pub git_head: String,
    /// `git.dirty`
    pub git_dirty: bool,
    /// `versions`, in file order
    pub versions: Vec<(String, String)>,
    /// `machine.platform`
    pub platform: String,
    /// `machine.machine`
    pub machine: String,
    /// `device`
    pub device: String,
    /// `uv_lock_sha256`
    pub uv_lock_sha256: String,
    /// `threads.default`
    pub threads_default: u32,
    /// `threads.determinism_check`
    pub threads_checked: Vec<u32>,
    /// `threads.bit_equal`
    pub threads_bit_equal: bool,
    /// `threads.runs_checked`
    pub threads_runs_checked: Vec<u32>,
    /// `models[]`
    pub models: Vec<ProvenanceModel>,
    /// `parameter_equality.all_50_checkpoints_equal_ensemble_csv`
    pub all_checkpoints_equal_ensemble: bool,
    /// `equivalence.stimulus_response_bit_equal`
    pub stimulus_response_bit_equal: bool,
    /// `equivalence.sequences`
    pub equivalence_sequences: Vec<u32>,
    /// `input_rounding`, per dt id, in file order
    pub input_rounding: Vec<(String, f64)>,
    /// `f32_f64_gap`, per "{model} {dt}" key, in file order
    pub gaps: Vec<GapEntry>,
    /// `sizes`, per file name, in file order
    pub sizes: Vec<(String, u64)>,
}

/// One of PROVENANCE.json's `models`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProvenanceModel {
    pub id: String,
    pub name: String,
    pub rank: String,
    pub validation_loss: f64,
    pub chkpt_00000_sha256: String,
}

/// One `f32_f64_gap` entry: flyvis's own float32-vs-float64 gaps for one
/// (model, dt).
#[derive(Clone, Debug, PartialEq)]
pub struct GapEntry {
    /// "{model} {dt}", e.g. "000 1/200".
    pub key: String,
    pub settled: f64,
    pub settling_centre: f64,
    pub centre: f64,
    pub snapshots: f64,
}

fn path(at: &str, key: &str) -> String {
    if at.is_empty() { key.to_string() } else { format!("{at}.{key}") }
}

fn field<'a>(obj: &'a Json, at: &str, key: &str) -> Result<&'a Json, ReferenceError> {
    obj.get(key).ok_or_else(|| ReferenceError::ProvenanceMissing(path(at, key)))
}

fn typed<T>(obj: &Json, at: &str, key: &str, f: impl Fn(&Json) -> Option<T>) -> Result<T, ReferenceError> {
    f(field(obj, at, key)?).ok_or_else(|| ReferenceError::ProvenanceMissing(path(at, key)))
}

fn text(obj: &Json, at: &str, key: &str) -> Result<String, ReferenceError> {
    typed(obj, at, key, |v| v.as_str().map(str::to_string))
}

fn flag(obj: &Json, at: &str, key: &str) -> Result<bool, ReferenceError> {
    typed(obj, at, key, Json::as_bool)
}

fn small(v: &Json) -> Option<u32> {
    v.as_i64().and_then(|n| u32::try_from(n).ok())
}

fn smalls(obj: &Json, at: &str, key: &str) -> Result<Vec<u32>, ReferenceError> {
    typed(obj, at, key, |v| v.as_array()?.iter().map(small).collect())
}

/// An object's entries, in file order, each converted by `f`.
fn entries<T>(obj: &Json, at: &str, key: &str, f: impl Fn(&Json) -> Option<T>) -> Result<Vec<(String, T)>, ReferenceError> {
    let here = path(at, key);
    let Json::Object(pairs) = field(obj, at, key)? else {
        return Err(ReferenceError::ProvenanceMissing(here));
    };
    pairs
        .iter()
        .map(|(k, v)| f(v).map(|x| (k.clone(), x)).ok_or_else(|| ReferenceError::ProvenanceMissing(path(&here, k))))
        .collect()
}

impl Provenance {
    /// Parses the fields above, strictly; every other field is ignored. A
    /// missing or mistyped field is `ProvenanceMissing`, naming its path.
    pub fn parse(json: &str) -> Result<Provenance, ReferenceError> {
        let root = json::parse(json).map_err(|e| ReferenceError::Json { offset: e.offset })?;
        let r = &root;
        let generator = field(r, "", "generator")?;
        let git = field(r, "", "git")?;
        let machine = field(r, "", "machine")?;
        let threads = field(r, "", "threads")?;
        let models = typed(r, "", "models", |v| v.as_array().map(<[Json]>::to_vec))?
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let at = format!("models[{i}]");
                Ok(ProvenanceModel {
                    id: text(m, &at, "id")?,
                    name: text(m, &at, "name")?,
                    rank: text(m, &at, "rank")?,
                    validation_loss: typed(m, &at, "validation_loss", Json::as_f64)?,
                    chkpt_00000_sha256: text(m, &at, "chkpt_00000_sha256")?,
                })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        let parameters = field(r, "", "parameter_equality")?;
        let equivalence = field(r, "", "equivalence")?;
        let Json::Object(gap_pairs) = field(r, "", "f32_f64_gap")? else {
            return Err(ReferenceError::ProvenanceMissing("f32_f64_gap".into()));
        };
        let gaps = gap_pairs
            .iter()
            .map(|(key, g)| {
                let at = format!("f32_f64_gap.{key}");
                let n = |q: &str| typed(g, &at, q, Json::as_f64);
                Ok(GapEntry {
                    key: key.clone(),
                    settled: n("settled")?,
                    settling_centre: n("settling_centre")?,
                    centre: n("centre")?,
                    snapshots: n("snapshots")?,
                })
            })
            .collect::<Result<Vec<_>, ReferenceError>>()?;
        Ok(Provenance {
            exported: text(r, "", "exported")?,
            export_id: text(r, "", "export_id")?,
            generator_path: text(generator, "generator", "path")?,
            generator_sha256: text(generator, "generator", "sha256")?,
            git_head: text(git, "git", "head")?,
            git_dirty: flag(git, "git", "dirty")?,
            versions: entries(r, "", "versions", |v| v.as_str().map(str::to_string))?,
            platform: text(machine, "machine", "platform")?,
            machine: text(machine, "machine", "machine")?,
            device: text(r, "", "device")?,
            uv_lock_sha256: text(r, "", "uv_lock_sha256")?,
            threads_default: typed(threads, "threads", "default", small)?,
            threads_checked: smalls(threads, "threads", "determinism_check")?,
            threads_bit_equal: flag(threads, "threads", "bit_equal")?,
            threads_runs_checked: smalls(threads, "threads", "runs_checked")?,
            models,
            all_checkpoints_equal_ensemble: flag(parameters, "parameter_equality", "all_50_checkpoints_equal_ensemble_csv")?,
            stimulus_response_bit_equal: flag(equivalence, "equivalence", "stimulus_response_bit_equal")?,
            equivalence_sequences: smalls(equivalence, "equivalence", "sequences")?,
            input_rounding: entries(r, "", "input_rounding", Json::as_f64)?,
            gaps,
            sizes: entries(r, "", "sizes", |v| v.as_i64().and_then(|n| u64::try_from(n).ok()))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{load_reference_bytes, load_reference_fixture, reference_dir};

    const BLOCK_0: &str = r#"{"run":0,"dt":"1/50","sequence":null,"offset":0,"bytes":12,"shape":[3]}"#;
    const BLOCK_1: &str = r#"{"run":null,"dt":"1/50","sequence":4,"offset":12,"bytes":8,"shape":[2]}"#;

    fn manifest(file_bytes: usize, blocks: &[&str]) -> String {
        format!(
            r#"{{"schema_version":1,"byte_order":"little","array_order":"C","n_nodes":3,
              "t_pre":1.0,"pre_stim_value":0.5,
              "dts":[{{"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}}],
              "runs":[{{"index":0,"model":"000","dt":"1/50","precision":"f32"}}],
              "centre_nodes":[{{"node":2,"cell_type":"R1"}}],
              "files":[{{"name":"x.f32.bin","dtype":"<f4","axes":["node"],"bytes":{file_bytes},
                         "sha256":"ignored","blocks":[{}]}}]}}"#,
            blocks.join(",")
        )
    }

    fn good() -> String {
        manifest(20, &[BLOCK_0, BLOCK_1])
    }

    fn bytes() -> Vec<u8> {
        [1.0f32, 2.0, 3.0, 4.0, 5.0].iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    fn file_error(text: &str) -> ReferenceError {
        Manifest::parse(text).expect_err("must be refused")
    }

    const SEQUENCES: &str = r#"{"sequences":[
        {"index":0,"family":"flashes","flyvis_index":0,
         "args":{"baseline":0.5,"intensity":0.0,"radius":-1},
         "frames":{"1/200":{"n_frames":600,"stimulus_frames":[200,400],"snapshot_frames":[0,119,239,359,479,599]},
                   "1/50":{"n_frames":150,"stimulus_frames":[50,100],"snapshot_frames":[0,29,59,89,119,149]}}},
        {"index":1,"family":"moving_edge","flyvis_index":4,
         "args":{"angle":90,"intensity":0.0,"speed":13.0,"offsets":[-10,11],"post_pad_mode":"continue"},
         "frames":{"1/50":{"n_frames":131,"stimulus_frames":[50,81],"snapshot_frames":[0,26,52,78,104,130]}}}]}"#;

    fn sequence_error(text: &str) -> ReferenceError {
        parse_sequences(text).expect_err("must be refused")
    }

    #[test]
    fn parses_the_sequences_of_a_synthetic_manifest() {
        let s = parse_sequences(SEQUENCES).expect("valid");
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].index, s[0].family.as_str(), s[0].flyvis_index), (0, "flashes", 0));
        assert_eq!(
            s[0].args,
            vec![("baseline".to_string(), 0.5), ("intensity".to_string(), 0.0), ("radius".to_string(), -1.0)]
        );
        assert_eq!(
            s[0].frames("1/200"),
            Some(&SequenceFrames {
                dt: "1/200".into(),
                n_frames: 600,
                stimulus_frames: (200, 400),
                snapshot_frames: vec![0, 119, 239, 359, 479, 599],
            })
        );
        assert_eq!(s[0].frames.iter().map(|f| f.dt.as_str()).collect::<Vec<_>>(), ["1/200", "1/50"]);
        assert_eq!((s[1].arg("angle"), s[1].arg("speed"), s[1].arg("radius")), (Some(90.0), Some(13.0), None));
        assert_eq!(s[1].arg("offsets"), None, "a non-numeric arg is skipped");
        assert_eq!(s[1].frames("1/200"), None);
        assert_eq!(s[1].frames("1/50").map(|f| f.stimulus_frames), Some((50, 81)));
    }

    #[test]
    fn sequences_refuse_frames_out_of_order_or_past_n_frames() {
        let at = |p: &str| ReferenceError::FrameRange(p.to_string());
        let snap = "sequences[1].frames.1/50.snapshot_frames";
        let window = "sequences[1].frames.1/50.stimulus_frames";
        assert_eq!(sequence_error(&SEQUENCES.replace("104,130]", "104,131]")), at(snap), "a snapshot at n_frames");
        assert_eq!(sequence_error(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,52,26,78,104,130]")), at(snap));
        assert_eq!(sequence_error(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,26,26,78,104,130]")), at(snap));
        assert_eq!(sequence_error(&SEQUENCES.replace("[50,81]", "[50,132]")), at(window), "a window past n_frames");
        assert_eq!(sequence_error(&SEQUENCES.replace("[50,81]", "[82,81]")), at(window), "a window that ends first");
        // the window may be empty, and may end at n_frames exactly
        assert!(parse_sequences(&SEQUENCES.replace("[50,81]", "[131,131]")).is_ok());
    }

    #[test]
    fn sequences_refuse_a_missing_or_mistyped_field() {
        use ReferenceError::{Missing, WrongType};
        assert_eq!(sequence_error(r#"{"runs":[]}"#), Missing("manifest.sequences".into()));
        assert_eq!(
            sequence_error(&SEQUENCES.replacen(r#""flyvis_index":0,"#, "", 1)),
            Missing("sequences[0].flyvis_index".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replacen(r#""n_frames":600"#, r#""n_frames":"600""#, 1)),
            WrongType("sequences[0].frames.1/200.n_frames".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replace("[50,81]", "[50]")),
            WrongType("sequences[1].frames.1/50.stimulus_frames".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,-26,52,78,104,130]")),
            WrongType("sequences[1].frames.1/50.snapshot_frames".into())
        );
        assert_eq!(
            sequence_error(&SEQUENCES.replacen(r#""args":{"baseline":0.5,"intensity":0.0,"radius":-1}"#, r#""args":[]"#, 1)),
            WrongType("sequences[0].args".into())
        );
        assert_eq!(sequence_error(&SEQUENCES.replacen(r#""family":"flashes""#, r#""family":1"#, 1)), WrongType("sequences[0].family".into()));
    }

    /// The reference set's content, which the parser leaves to its caller:
    /// a frame layout for every dt id, with six snapshot frames each.
    fn check_content(sequences: &[SequenceInfo], dt_ids: &[&str]) -> Result<(), String> {
        for s in sequences {
            for &dt in dt_ids {
                let f = s.frames(dt).ok_or_else(|| format!("sequence {} has no frames for dt {dt}", s.index))?;
                if f.snapshot_frames.len() != 6 {
                    return Err(format!("sequence {} dt {dt}: {} snapshot frames", s.index, f.snapshot_frames.len()));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn the_content_check_refuses_a_missing_dt_entry_and_five_snapshots() {
        let s = parse_sequences(SEQUENCES).unwrap();
        assert_eq!(check_content(&s[..1], &["1/200", "1/50"]), Ok(()));
        assert_eq!(check_content(&s, &["1/200", "1/50"]), Err("sequence 1 has no frames for dt 1/200".into()));
        let five = parse_sequences(&SEQUENCES.replace("[0,26,52,78,104,130]", "[0,26,52,78,130]")).unwrap();
        assert_eq!(check_content(&five[1..], &["1/50"]), Err("sequence 1 dt 1/50: 5 snapshot frames".into()));
    }

    /// The published manifest's sequences are the twelve the trace gates
    /// play: flashes 0-3, then moving edges 4-11 at angles 0/30/90/150, OFF
    /// then ON, speed 13, height 80 (spec, fact H6).
    #[test]
    fn the_real_manifest_lists_the_twelve_reference_sequences() {
        let Some(text) = load_reference_fixture("manifest.json") else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        let s = parse_sequences(&text).expect("manifest.json's sequences must parse");
        let dt_ids: Vec<&str> = m.dts.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(dt_ids, ["1/200", "1/50"]);
        check_content(&s, &dt_ids).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.len(), 12);
        for (i, q) in s.iter().enumerate() {
            assert_eq!(q.index, i as u32, "sequences are listed in index order");
            assert_eq!(q.frames.len(), 2, "sequence {i}: one layout per dt, no others");
            let (family, flyvis_index, n_frames, window): (&str, usize, [usize; 2], [(usize, usize); 2]) = if i < 4 {
                ("flashes", i, [600, 150], [(200, 400), (50, 100)])
            } else {
                ("moving_edge", i - 4, [525, 131], [(200, 325), (50, 81)])
            };
            assert_eq!((q.family.as_str(), q.flyvis_index as usize), (family, flyvis_index), "sequence {i}");
            for (k, dt) in dt_ids.iter().enumerate() {
                let f = q.frames(dt).unwrap();
                assert_eq!((f.n_frames, f.stimulus_frames), (n_frames[k], window[k]), "sequence {i} dt {dt}");
                // trunc(linspace(0, n - 1, 6))
                let want: Vec<usize> = (0..6).map(|j| (j as f64 * (n_frames[k] - 1) as f64 / 5.0) as usize).collect();
                assert_eq!(f.snapshot_frames, want, "sequence {i} dt {dt}");
            }
            if i < 4 {
                assert_eq!(q.arg("intensity"), Some(if i < 2 { 0.0 } else { 1.0 }), "sequence {i}");
                assert_eq!(q.arg("radius"), Some(if i % 2 == 0 { -1.0 } else { 6.0 }), "sequence {i}");
            } else {
                let e = i - 4;
                assert_eq!(q.arg("angle"), Some([0.0, 30.0, 90.0, 150.0][e / 2]), "sequence {i}");
                assert_eq!(q.arg("intensity"), Some((e % 2) as f64), "sequence {i}");
                assert_eq!((q.arg("speed"), q.arg("height"), q.arg("width")), (Some(13.0), Some(80.0), Some(80.0)), "sequence {i}");
            }
        }
    }

    #[test]
    fn parses_a_valid_manifest() {
        let m = Manifest::parse(&good()).expect("valid");
        assert_eq!(m.n_nodes, 3);
        assert_eq!(m.dts[0], DtInfo { id: "1/50".into(), dt: 0.02, floor_dt_f32: 0.019999999552965164, n_settle: 50 });
        assert_eq!(m.runs[0], RunInfo { index: 0, model: "000".into(), dt: "1/50".into(), precision: Dtype::F32 });
        assert_eq!(m.centre_nodes[0], CentreNode { node: 2, cell_type: "R1".into() });
        let f = m.file("x.f32.bin").unwrap();
        assert_eq!((f.dtype, f.bytes, f.blocks.len()), (Dtype::F32, 20, 2));
        assert_eq!(f.blocks[1].run, None);
        assert_eq!(f.blocks[1].sequence, Some(4));
    }

    #[test]
    fn refuses_an_overlapping_block() {
        let b1 = BLOCK_1.replace(r#""offset":12"#, r#""offset":8"#);
        assert_eq!(file_error(&manifest(20, &[BLOCK_0, &b1])), ReferenceError::BlockLayout { file: "x.f32.bin".into(), block: 1 });
    }

    #[test]
    fn refuses_a_gap() {
        let b1 = BLOCK_1.replace(r#""offset":12"#, r#""offset":16"#);
        assert_eq!(file_error(&manifest(24, &[BLOCK_0, &b1])), ReferenceError::BlockLayout { file: "x.f32.bin".into(), block: 1 });
    }

    #[test]
    fn refuses_bytes_that_do_not_match_the_shape() {
        let b0 = BLOCK_0.replace("[3]", "[4]");
        assert_eq!(file_error(&manifest(20, &[&b0, BLOCK_1])), ReferenceError::BlockShape { file: "x.f32.bin".into(), block: 0 });
    }

    #[test]
    fn refuses_a_shape_whose_rank_differs_from_the_axes() {
        let b0 = BLOCK_0.replace("[3]", "[3,1]");
        assert_eq!(file_error(&manifest(20, &[&b0, BLOCK_1])), ReferenceError::BlockAxes { file: "x.f32.bin".into(), block: 0 });
    }

    #[test]
    fn refuses_two_blocks_with_the_same_key() {
        let b1 = BLOCK_1.replace(r#""run":null"#, r#""run":0"#).replace(r#""sequence":4"#, r#""sequence":null"#);
        assert_eq!(file_error(&manifest(20, &[BLOCK_0, &b1])), ReferenceError::DuplicateBlock { file: "x.f32.bin".into(), block: 1 });
    }

    #[test]
    fn refuses_blocks_that_do_not_fill_the_file() {
        assert_eq!(
            file_error(&manifest(24, &[BLOCK_0, BLOCK_1])),
            ReferenceError::BlockTotal { file: "x.f32.bin".into(), expected: 24, found: 20 }
        );
    }

    #[test]
    fn refuses_schema_version_2_and_big_endian() {
        assert!(matches!(file_error(&good().replace(r#""schema_version":1"#, r#""schema_version":2"#)), ReferenceError::Unsupported { .. }));
        assert!(matches!(file_error(&good().replace(r#""little""#, r#""big""#)), ReferenceError::Unsupported { .. }));
    }

    #[test]
    fn reads_a_block_with_its_shape() {
        let m = Manifest::parse(&good()).unwrap();
        let f = m.file("x.f32.bin").unwrap();
        let a = f.read_f32(&bytes(), &BlockKey { run: Some(0), dt: "1/50", sequence: None }).unwrap();
        assert_eq!(a, Array { shape: vec![3], data: vec![1.0, 2.0, 3.0] });
    }

    #[test]
    fn selects_a_block_with_no_run_by_dt_and_sequence() {
        let m = Manifest::parse(&good()).unwrap();
        let f = m.file("x.f32.bin").unwrap();
        let a = f.read_f32(&bytes(), &BlockKey { run: None, dt: "1/50", sequence: Some(4) }).unwrap();
        assert_eq!(a.data, vec![4.0, 5.0]);
    }

    #[test]
    fn read_rejects_a_dtype_mismatch() {
        let m = Manifest::parse(&good()).unwrap();
        let key = BlockKey { run: Some(0), dt: "1/50", sequence: None };
        assert_eq!(
            m.file("x.f32.bin").unwrap().read_f64(&bytes(), &key),
            Err(ReferenceError::Dtype { file: "x.f32.bin".into(), expected: Dtype::F64, found: Dtype::F32 })
        );
    }

    #[test]
    fn read_rejects_a_short_slice_and_a_key_that_matches_no_block() {
        let m = Manifest::parse(&good()).unwrap();
        let f = m.file("x.f32.bin").unwrap();
        let key = BlockKey { run: Some(0), dt: "1/50", sequence: None };
        assert_eq!(f.read_f32(&bytes()[..16], &key), Err(ReferenceError::ByteLength { file: "x.f32.bin".into(), expected: 20, found: 16 }));
        let other = BlockKey { run: Some(1), dt: "1/50", sequence: None };
        assert_eq!(f.read_f32(&bytes(), &other), Err(ReferenceError::NoBlock { file: "x.f32.bin".into() }));
    }

    /// The published manifest parses, and every file it names has the
    /// length it states.
    #[test]
    fn the_real_manifest_parses_and_every_file_has_its_stated_length() {
        let Some(text) = load_reference_fixture("manifest.json") else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        assert_eq!(m.runs.len(), 8);
        assert_eq!(m.files.len(), 9);
        for f in &m.files {
            let path = format!("{}/{}", reference_dir(), f.name);
            let len = std::fs::metadata(&path).unwrap_or_else(|e| panic!("{path}: {e}")).len();
            assert_eq!(len as usize, f.bytes, "{}", f.name);
        }
    }

    /// Every block of the two `settled` files reads at its stated shape, in
    /// the precision the file declares, from the real bytes.
    #[test]
    fn a_hand_built_file_info_is_checked_before_reading() {
        let block = |offset, bytes, shape: Vec<usize>| Block { run: Some(0), dt: "a".into(), sequence: None, offset, bytes, shape };
        let file = |bytes, dtype, b| FileInfo { name: "x.bin".into(), dtype, axes: vec!["n".into()], bytes, blocks: vec![b] };
        let key = BlockKey { run: Some(0), dt: "a", sequence: None };
        let overrun = file(4, Dtype::F32, block(0, 8, vec![2]));
        assert_eq!(overrun.read_f32(&[0u8; 4], &key), Err(ReferenceError::BlockTotal { file: "x.bin".into(), expected: 4, found: 8 }));
        let ragged = file(6, Dtype::F32, block(0, 6, vec![1]));
        assert_eq!(ragged.read_f32(&[0u8; 6], &key), Err(ReferenceError::BlockShape { file: "x.bin".into(), block: 0 }));
        let overrun64 = file(8, Dtype::F64, block(0, 16, vec![2]));
        assert_eq!(overrun64.read_f64(&[0u8; 8], &key), Err(ReferenceError::BlockTotal { file: "x.bin".into(), expected: 8, found: 16 }));
    }

    #[test]
    fn every_real_settled_block_reads_at_its_shape() {
        let Some(text) = load_reference_fixture("manifest.json") else { return };
        let m = Manifest::parse(&text).expect("manifest.json must parse");
        for name in ["settled.f32.bin", "settled.f64.bin"] {
            let f = m.file(name).unwrap_or_else(|| panic!("manifest has no {name}"));
            let Some(bytes) = load_reference_bytes(name) else { return };
            assert!(!f.blocks.is_empty(), "{name} has no blocks");
            for b in &f.blocks {
                let key = BlockKey { run: b.run, dt: &b.dt, sequence: b.sequence };
                let (shape, len) = match f.dtype {
                    Dtype::F32 => f.read_f32(&bytes, &key).map(|a| (a.shape, a.data.len())),
                    Dtype::F64 => f.read_f64(&bytes, &key).map(|a| (a.shape, a.data.len())),
                }
                .unwrap_or_else(|e| panic!("{e}"));
                assert_eq!(shape, b.shape, "{name}");
                assert_eq!(len, shape.iter().product::<usize>(), "{name}");
            }
        }
    }

    /// A minimal PROVENANCE.json with every field `Provenance::parse` reads.
    const PROVENANCE: &str = r#"{"exported":"2026-10-27","export_id":"x-1",
        "generator":{"path":"tools/export_reference.py","sha256":"ab"},
        "git":{"head":"cd","dirty":false},
        "versions":{"python":"3.12.11","flyvis":"1.2.0"},
        "machine":{"platform":"p-1","machine":"arm64","torch_config":"ignored"},
        "uv_lock_sha256":"ef","device":"cpu",
        "threads":{"default":12,"determinism_check":[1,12],"bit_equal":true,"runs_checked":[0,1]},
        "models":[{"id":"000","name":"flow/0000/000","validation_loss":5.25,"rank":"best","chkpt_00000_sha256":"01"}],
        "parameter_equality":{"all_50_checkpoints_equal_ensemble_csv":true},
        "equivalence":{"stimulus_response_bit_equal":true,"sequences":[0,4]},
        "input_rounding":{"1/200":2.9801242251714655e-08,"1/50":1e-8},
        "f32_f64_gap":{"000 1/200":{"settled":1.5,"settling_centre":2.5,"centre":3.5,"snapshots":4.5}},
        "sizes":{"settled.f32.bin":730704},
        "wall_time_s":248.9}"#;

    #[test]
    fn provenance_parses_a_minimal_document() {
        let p = Provenance::parse(PROVENANCE).expect("valid");
        assert_eq!((p.exported.as_str(), p.export_id.as_str()), ("2026-10-27", "x-1"));
        assert_eq!((p.generator_path.as_str(), p.generator_sha256.as_str()), ("tools/export_reference.py", "ab"));
        assert_eq!((p.git_head.as_str(), p.git_dirty), ("cd", false));
        assert_eq!(p.versions, [("python".to_string(), "3.12.11".to_string()), ("flyvis".into(), "1.2.0".into())]);
        assert_eq!((p.platform.as_str(), p.machine.as_str(), p.device.as_str()), ("p-1", "arm64", "cpu"));
        assert_eq!(p.uv_lock_sha256, "ef");
        assert_eq!((p.threads_default, p.threads_bit_equal), (12, true));
        assert_eq!((p.threads_checked, p.threads_runs_checked), (vec![1, 12], vec![0, 1]));
        assert_eq!(
            p.models,
            [ProvenanceModel {
                id: "000".into(),
                name: "flow/0000/000".into(),
                rank: "best".into(),
                validation_loss: 5.25,
                chkpt_00000_sha256: "01".into(),
            }]
        );
        assert!(p.all_checkpoints_equal_ensemble && p.stimulus_response_bit_equal);
        assert_eq!(p.equivalence_sequences, [0, 4]);
        assert_eq!(p.input_rounding, [("1/200".to_string(), 2.9801242251714655e-08), ("1/50".into(), 1e-8)]);
        assert_eq!(
            p.gaps,
            [GapEntry { key: "000 1/200".into(), settled: 1.5, settling_centre: 2.5, centre: 3.5, snapshots: 4.5 }]
        );
        assert_eq!(p.sizes, [("settled.f32.bin".to_string(), 730704)]);
    }

    /// Every field read, missing or of the wrong type, is an error naming
    /// its JSON path.
    #[test]
    fn provenance_refuses_a_missing_or_mistyped_field_naming_its_path() {
        let cases: [(&str, &str, &str); 30] = [
            (r#""exported":"2026-10-27","#, "", "exported"),
            (r#""exported":"2026-10-27""#, r#""exported":1"#, "exported"),
            (r#""export_id":"x-1","#, "", "export_id"),
            (r#""path":"tools/export_reference.py","#, "", "generator.path"),
            (r#""sha256":"ab""#, r#""sha256":null"#, "generator.sha256"),
            (r#""head":"cd","#, "", "git.head"),
            (r#""dirty":false"#, r#""dirty":"no""#, "git.dirty"),
            (r#""versions":{"python":"3.12.11","#, r#""versions":{"python":3,"#, "versions.python"),
            (r#""versions":{"python":"3.12.11","flyvis":"1.2.0"},"#, r#""versions":[],"#, "versions"),
            (r#""platform":"p-1","#, "", "machine.platform"),
            (r#""machine":"arm64","#, "", "machine.machine"),
            (r#""device":"cpu","#, "", "device"),
            (r#""uv_lock_sha256":"ef","#, "", "uv_lock_sha256"),
            (r#""default":12,"#, r#""default":-1,"#, "threads.default"),
            (r#""determinism_check":[1,12]"#, r#""determinism_check":[1,"12"]"#, "threads.determinism_check"),
            (r#""bit_equal":true"#, r#""bit_equal":1"#, "threads.bit_equal"),
            (r#","runs_checked":[0,1]"#, "", "threads.runs_checked"),
            (r#""models":["#, r#""models":"none","x":["#, "models"),
            (r#""id":"000","#, "", "models[0].id"),
            (r#""name":"flow/0000/000","#, "", "models[0].name"),
            (r#""validation_loss":5.25"#, r#""validation_loss":"5.25""#, "models[0].validation_loss"),
            (r#""rank":"best","#, "", "models[0].rank"),
            (r#","chkpt_00000_sha256":"01""#, "", "models[0].chkpt_00000_sha256"),
            (
                r#""all_50_checkpoints_equal_ensemble_csv":true"#,
                r#""x":true"#,
                "parameter_equality.all_50_checkpoints_equal_ensemble_csv",
            ),
            (r#""stimulus_response_bit_equal":true,"#, "", "equivalence.stimulus_response_bit_equal"),
            (r#","sequences":[0,4]"#, "", "equivalence.sequences"),
            (r#""1/50":1e-8"#, r#""1/50":"1e-8""#, "input_rounding.1/50"),
            (r#""settled":1.5,"#, "", "f32_f64_gap.000 1/200.settled"),
            (r#""snapshots":4.5"#, r#""snapshots":null"#, "f32_f64_gap.000 1/200.snapshots"),
            (r#""settled.f32.bin":730704"#, r#""settled.f32.bin":-4"#, "sizes.settled.f32.bin"),
        ];
        for (from, to, want) in cases {
            let doc = PROVENANCE.replacen(from, to, 1);
            assert_ne!(doc, PROVENANCE, "{from} must occur in the document");
            let e = Provenance::parse(&doc).expect_err(want);
            assert_eq!(e, ReferenceError::ProvenanceMissing(want.to_string()), "{from} -> {to}");
            assert_eq!(e.to_string(), format!("PROVENANCE.json has no {want}"));
        }
    }

    /// The real PROVENANCE.json parses, and holds what the registry expects.
    #[test]
    fn the_real_provenance_parses() {
        let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
        let p = Provenance::parse(&text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["000", "049"]);
        assert_eq!(p.gaps.len(), 4);
        assert_eq!(p.sizes.len(), 9);
        assert!(!p.git_dirty && p.threads_bit_equal && p.all_checkpoints_equal_ensemble && p.stimulus_response_bit_equal);
    }
}
