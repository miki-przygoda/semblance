# `semblance-flyvis` — reading the published flyvis artifacts

**Date:** 2026-09-17
**Status:** approved, not yet implemented
**Epic:** 0.2.0 flyvis import, tasks 3 (build items) — `docs/project-outline/0.2.0-flyvis-import/`

## Why

The project's reference model is flyvis ([Lappalainen et al., *Nature* 2024](https://www.nature.com/articles/s41586-024-07939-3)).
Its connectome is distributed as a 188 KB JSON file of *generative* rules —
65 cell types and 605 connection types, tiled over a hexagonal lattice to
produce 45,669 neurons and ~1.5M synapses. That file is the compressed
representation this project exists to study, so reading it faithfully is the
foundation for everything after.

Investigation during the epic established that the whole pipeline is portable:
the tiling and edge construction are ordinary arithmetic, reproducible to an
exact neuron count without running flyvis. So there is **no Python side** and no
interchange format. Rust reads the published artifact directly.

That collides with one constraint: parsing JSON in Rust normally means
`serde_json`, and this workspace has zero external dependencies — a property
that cost the whole of v0.1.1 to establish and that `CLAUDE.md` states
explicitly. A conversion step was rejected because it reintroduces a toolchain
somewhere and produces a derived file that can drift. So: a minimal JSON parser,
written in-crate.

The deciding argument is provenance, not effort. "We parse the file Nature
published" is a stronger sentence than "we parse a file we converted", and for a
project whose bar is defensibility that removes a step a reviewer would
otherwise take on trust.

## Scope

**In:**

1. A new crate, `semblance-flyvis`.
2. A minimal JSON parser as a **private** module.
3. Typed structs for the flyvis connectome spec, and a loader that builds them
   from the JSON.
4. The flyvis ground truth, transcribed into Rust constants.

**Out — deliberately, and not to be added by an implementer:**

- **Tiling the lattice.** Turning the spec into 45,669 neurons is task 5/6 of
  the epic. This spec stops at a faithful in-memory representation of the file.
- **Dynamics.** `PPNeuronIGRSynapses` is 0.3.0.
- **Stimuli.** Task 7.
- Any change to `connectome`, `sim`, `codec`, `fidelity`, `evo` or `app`.
- Publishing the JSON parser as its own crate. The world does not need another
  one, and an eighth crates.io package called `semblance-json` would be poor
  citizenship.

## Crate structure

A new crate rather than an extension of `connectome`:

```
crates/flyvis/
├── Cargo.toml          name = "semblance-flyvis", zero dependencies
└── src/
    ├── lib.rs          re-exports; crate docs
    ├── json.rs         pub(crate) — the parser
    ├── spec.rs         pub  — typed connectome spec + loader
    └── groundtruth.rs  pub  — transcribed constants
```

`connectome` is a graph type; it should not carry a JSON parser or
flyvis-specific literals. Keeping the import in its own crate also means the
dependency arrow points one way — `semblance-flyvis` depends on
`semblance-connectome`, never the reverse.

This crate has **no dependencies at all** for the work in this spec (it gains
`semblance-connectome` in task 5, when it starts producing graphs).

## The JSON parser (`json.rs`, private)

### Required subset, measured from the actual file

| | |
|---|---|
| size | 192,611 bytes |
| non-ASCII bytes | 0 |
| backslash escapes | 0 |
| exponent notation | none |
| max nesting depth | 6 |
| numbers | 5,506 integers, 2,319 floats |

Implement **full escape handling (including `\uXXXX`) and exponent notation
anyway**. They are a few lines each, and a parser that silently mis-reads a file
dropped in later is worse than one slightly larger than today's input demands.

Do **not** implement: streaming, zero-copy/borrowed values, a serde-style
derive, or arbitrary-precision numbers. The file is 188 KB and is read once.

### Types

```rust
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}
```

`Object` is a `Vec` of pairs, not a map: it preserves document order, avoids
pulling in hashing, and the largest object here has single-digit keys, so lookup
cost is irrelevant.

`Number` is a single `f64`. JSON does not distinguish integers from floats, and
`f64` represents every integer in this file exactly (well within 2⁵³). Integer
access goes through an accessor that rejects non-integral values rather than
truncating silently.

### API

```rust
pub(crate) fn parse(input: &str) -> Result<Json, JsonError>;

impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Json>;
    pub(crate) fn as_array(&self) -> Option<&[Json]>;
    pub(crate) fn as_str(&self) -> Option<&str>;
    pub(crate) fn as_f64(&self) -> Option<f64>;
    pub(crate) fn as_i64(&self) -> Option<i64>;   // None if not integral
    pub(crate) fn as_bool(&self) -> Option<bool>;
    pub(crate) fn is_null(&self) -> bool;
}
```

### Errors

```rust
pub(crate) struct JsonError {
    pub(crate) kind: JsonErrorKind,
    pub(crate) offset: usize,   // byte offset into the input
}

pub(crate) enum JsonErrorKind {
    UnexpectedChar(char),
    UnexpectedEnd,
    InvalidNumber,
    InvalidEscape,
    InvalidUnicodeEscape,
    TrailingData,
    DepthLimitExceeded,
}
```

Malformed input **must** return an error, never panic and never silently
truncate. The byte offset is required — diagnosing a 188 KB file without one is
miserable.

`DepthLimitExceeded` guards against stack overflow from adversarial nesting in a
recursive-descent parser. Limit: 64. The real file uses 6.

## The connectome spec (`spec.rs`, public)

Mirrors the file's structure exactly. No interpretation, no tiling, no
normalisation — that belongs to later tasks, and keeping this layer literal is
what makes it checkable against the source.

```rust
pub enum Pattern {
    Stride { u: i32, v: i32 },
    Tile(i32),
    Single,
}

pub struct NodeSpec {
    pub name: String,
    pub pattern: Pattern,
    pub activation: String,
    pub bias: f32,
    pub bias_fixed: bool,
    pub time_constant: Option<f32>,
    pub time_constant_fixed: bool,
}

pub struct Offset {
    pub du: i32,
    pub dv: i32,
    pub n_syn: f32,
}

pub struct EdgeSpec {
    pub src: String,
    pub tar: String,
    pub offsets: Vec<Offset>,
    pub alpha: i32,
    pub alpha_fixed: bool,
    pub alpha_references: Vec<String>,
    pub time_constant: Option<f32>,
    pub time_constant_fixed: bool,
    pub lambda_mult: f32,
    pub edge_type: String,
}

pub struct ConnectomeSpec {
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
    pub input_units: Vec<String>,
    pub output_units: Vec<String>,
}

impl ConnectomeSpec {
    pub fn from_json_str(input: &str) -> Result<Self, SpecError>;
}
```

Notes on faithfulness:

- `pattern` in the file is a two-element array: `["stride", [u, v]]`,
  `["tile", n]`, or `["single", ...]`. All 65 nodes in the released file use
  `stride`; support all three, because flyvis's own loader does.
- `time_constant` is genuinely `null` for some entries — hence `Option<f32>`.
- `offsets` entries are `[[du, dv], n_syn]`. `n_syn` is a float, not an integer:
  flyvis's hull-filling introduces fractional counts.
- `alpha` is the **synapse sign** (`+1`/`-1`), fixed from literature, with
  `alpha_references` citing the source. It is not a weight.
- `receptors` exists in the file and is always empty. Ignore it; do not model it.

`SpecError` distinguishes a JSON-level failure from a schema-level one (missing
key, wrong type, unknown pattern name), and carries the path to the offending
element — e.g. `edges[417].offsets[3]`.

## The ground truth (`groundtruth.rs`, public)

Transcribed from `flyvis/utils/groundtruth_utils.py`, **not parsed at runtime**.
It is ~700 literals; making it a compiled constant removes file I/O, removes a
parser, removes drift, and puts it in the test surface.

```rust
pub enum Polarity { On, Off, Unknown }

pub const POLARITY: &[(&str, Polarity)];          // 65 entries, 32 known
pub const PREFERRED_DIRECTIONS: &[(&str, u16)];   // 8 entries, degrees
pub const TUNING_CURVES: &[(&str, [f32; 12])];    // 8 types x 12 points
pub const MOTION_TUNING: &[&str];                 // 8
pub const NO_MOTION_TUNING: &[&str];              // 10
pub const KNOWN_DSI_TYPES: &[&str];               // 18 = the two above
pub const INSUFFICIENT_DATA: &[&str];             // 9
pub const NOISY_DATA: &[&str];                    // 4
pub const ON_PATHWAY: &[&str];                    // 12
pub const OFF_PATHWAY: &[&str];                   // 12
```

Upstream spells one of these `unsufficient_data`. Use the correct spelling here
and record the upstream name in the doc comment, so the mapping stays findable.

`TUNING_CURVES` are 12-point direction tuning curves for T4a–d and T5a–d, from
Maisak et al. 2013 Fig. 3g,h. `PREFERRED_DIRECTIONS` are exact degrees:
T4a/T5a 180, T4b/T5b 0, T4c/T5c 90, T4d/T5d 270.

### Provenance, recorded in the module docs

| | |
|---|---|
| source | `TuragaLab/flyvis`, `flyvis/utils/groundtruth_utils.py` |
| transcribed at | `9a516686d158aa568e2035bf65ec15646fb020a8` (2024-11-08) |
| repo HEAD when transcribed | `92b3845cc426dd309a1a0e1b3890156c42e14021` (2026-08-06) |
| release | v1.2.0 |

The file has been untouched upstream since November 2024, so drift is unlikely —
but record the SHA so a future divergence is detectable rather than silent.

## Testing

The parser is **self-verifying at the spec level**, because the epic's
investigation established the right answers independently. These are the
acceptance tests, not smoke tests:

| Assertion | Value |
|---|---|
| `spec.nodes.len()` | 65 |
| `spec.edges.len()` | 605 |
| distinct `(src, tar)` pairs | 605 |
| total offset entries across all edges | 2,140 |
| summed `n_syn` across all offsets | 8,087 |
| `input_units.len()` | 8 |
| `output_units.len()` | 34 |
| nodes with `pattern == Stride` | 65 |
| distinct `activation` values | `{"relu"}` |
| distinct `edge_type` values | `{"chem"}` |
| edges with ≥3 offsets | 243 |
| edges with <3 offsets | 362 |
| node names containing `CT1` | `CT1(Lo1)`, `CT1(M10)` |

If a stride, an offset or a number is mis-read, these move. That is a far
stronger check than "does it parse valid JSON".

Ground-truth tests:

| Assertion | Value |
|---|---|
| `POLARITY.len()` | 65 |
| entries with `On` | 18 |
| entries with `Off` | 14 |
| entries with `Unknown` | 33 |
| `PREFERRED_DIRECTIONS.len()` | 8 |
| `TUNING_CURVES.len()` | 8 |
| `KNOWN_DSI_TYPES.len()` | 18 |
| every name in every table | appears in `POLARITY` |

That last one is the important one — it catches a typo'd cell-type name in
transcription, which is otherwise invisible and would silently drop a type from
the metric later.

Parser unit tests cover the usual hostile inputs: unterminated string, trailing
comma, bare identifier, `01` (leading zero), lone surrogate, depth bomb, empty
input, trailing data after a valid document. Each must produce the right
`JsonErrorKind` and a sensible offset.

**The real file must be in the test path.** Tests read
`data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json`. That
directory is gitignored, so tests must **skip with a clear message when it is
absent**, not fail — a fresh clone has no data. Document the download URL in the
skip message.

## Constraints

- Rust 1.85+, edition 2024.
- **Zero external dependencies.** `Cargo.lock` stays at 7 packages for this
  spec's work; it becomes 8 when the crate itself is added, all `semblance*`.
- Commit style: plain imperative subject, no `feat:`/`fix:`/`chore:` prefixes.
- The new crate inherits workspace metadata and is published like the others.

## Risks

**A parser is not research.** It buys no scientific insight and it is the kind
of task that expands to fill available time. The subset is small and the
acceptance tests are known — if it starts growing features nothing in this spec
asks for, that is the signal to stop.

**Transcription is error-prone in a way that is invisible.** A wrong digit in a
tuning curve or a misspelled cell type will not fail to compile and will not
fail a count test. The cross-check that every name in every table appears in
`POLARITY` is the main defence; a second pair of eyes on the numbers is the
other.

**`n_syn` as float may surprise later work.** Hull-filled entries carry
fractional synapse counts. Anything downstream that assumes integer synapse
counts will be wrong, and this is the layer where that assumption would be
easiest to make by accident.
