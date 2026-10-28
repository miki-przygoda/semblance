# Graded dynamics and the settling gate: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement flyvis's graded dynamics in Rust (f32 and f64). Prove the
implementation reproduces flyvis's settling bit for bit given flyvis's own
synapse counts, and within pre-registered bounds given Rust's own `exp`.

**Architecture:** Three new library pieces:

- an ensemble loader and a manifest/binary reader in `semblance-flyvis`
- a graded Euler integrator in `semblance-sim`, generic over a sealed `Real`
  trait and depending only on `semblance-connectome`
- a generated table of torch's computed synapse counts, written by a new
  offline `tools/` script

A `#[cfg(test)]` gate module in `semblance-flyvis` ties them together over
task 1's eight reference runs.

**Tech Stack:** Rust 1.85+ (edition 2024), with no external crates. The
offline exporter is Python 3.12 with flyvis 1.2.0 and torch 2.14 (`tools/`,
uv).

**Spec:** `docs/superpowers/specs/2026-10-28-graded-settling-design.md`. The
spec is the authority. Section numbers below refer to it.

## Global Constraints

- **Zero external dependencies.** `Cargo.lock` lists exactly 8 packages, all
  `semblance*`. Adding `semblance-sim` as a dev-dependency of
  `semblance-flyvis` keeps it at 8.
- **Rust 1.85+, edition 2024.** CI also builds on the 1.85 MSRV job, so no
  API newer than 1.85.
- **Commit style:** plain imperative subject, no `feat:`/`fix:` prefixes.
- **Library code never panics on input.** Malformed input is a typed error.
- **Never relax a comparison.** The exact gate stays `==`. The libm bounds are
  f64 ≤ 1e-10, and f32 ≤ 2 × the gap from `PROVENANCE.json`'s
  `f32_f64_gap`. They are fixed now and never changed after a comparison runs.
- **`TOLERANCE` in the geometry gate is not touched.**
- **No fixture changes.** Nothing under `data/`, and no edits to `fixtures-1`
  or `fixtures-2`. `data/` and `docs/project-outline/` are never committed.
- **`tools/` stays offline.** No crate invokes Python. The generated `.rs` is
  data, like `pretrained.rs`.
- **Fixture tests skip** via `test_support` loaders, and become failures under
  `SEMBLANCE_REQUIRE_FIXTURES=1`.
- **Clippy is clean** under `cargo clippy --workspace --all-targets -- -D warnings`.
- **Branch `graded-settling`** (the spec commit `15b4b75` is already on it).
  Never `--force`. No merge without the owner's go-ahead. No publishing.

## Review Focus

These are the input classes and failure modes most likely to bite, most likely
first. Each one's test is added to the task that owns the code.

1. **Partial fixtures.** `data/flyvis-reference/` is present but
   `ensemble.csv` is missing, or the reverse. Every gate test must skip
   cleanly, or fail naming the missing path under
   `SEMBLANCE_REQUIRE_FIXTURES=1`, and never panic inside the `OnceLock`
   initialiser with an unrelated message. **Task 6:** `fixture()` loads every
   file through `test_support` before building anything.
2. **Asking for the wrong precision's block.** `settled.f32.bin` read with
   `read_f64` must be a `Dtype` error, not garbage. **Task 5:**
   `read_rejects_a_dtype_mismatch`.
3. **CSV line endings.** A file with no trailing newline must parse, and a
   CRLF file must be refused with its line number. **Task 1:**
   `accepts_a_file_without_a_trailing_newline`,
   `rejects_a_carriage_return`.
4. **A failed `step`.** A wrong-length input must leave the state untouched,
   so a caller that handles the error can continue. **Task 4:**
   `a_failed_step_leaves_the_state_alone`.
5. **A regenerated count table.** Running the exporter on another machine or
   torch build can change the table. The checks must catch a table written
   from the wrong source, and the exact gate must fail loudly rather than be
   loosened. **Task 3:** the ulp and difference-count checks; **Task 6:** the
   exact gate's report.

## Rulings made while planning

- **The exporter reads the checkpoint tensor directly** rather than
  instantiating a flyvis `Network` (spec section 3, step 1). flyvis's value
  is `raw_values.exp()` on that same [2355] float32 parameter
  (`initialization.py:199-202`). The default dtype does not change `exp` on
  an explicitly typed tensor. Building a network needs
  `export_reference.py`'s scratch-root machinery.
  - Cost if wrong: the exact gate fails, and that failure points straight
    here.
- **`max` is `if a > b { a } else { b }`**, as the spec says. It differs from
  `torch.max` only on signed zeros. A signed zero cannot change any sum here,
  because each sum starts from +0.
- **The shared real-network helper moves into `test_support`**
  (`real_spec_and_network`). Then `expand_checks`, `wiring` and
  `settling_checks` build it the same way.
- **No CI workflow change**, as the spec says. The gates run inside the
  existing `cargo test --workspace` step.

---

### Task 1: The ensemble loader (epic task 2)

**Files:**
- Create: `crates/flyvis/src/ensemble.rs`
- Modify: `crates/flyvis/src/lib.rs` (module and re-exports)
- Modify: `crates/flyvis/src/test_support.rs` (add `load_pretrained_fixture`)

**Interfaces:**
- Consumes: `crate::pretrained::{BIAS, TIME_CONST, SYN_STRENGTH, CELL_TYPES, CONN_TYPES}`
- Produces:
  - `semblance_flyvis::ensemble::{Ensemble, ModelParams, EnsembleError, EnsembleErrorKind, N_MODELS, ROWS_PER_MODEL}`
  - `Ensemble::parse(&str) -> Result<Ensemble, EnsembleError>`
  - `Ensemble::model(&self, &str) -> Option<&ModelParams>`
  - `Ensemble::ids(&self) -> impl Iterator<Item = &str>`
  - `ModelParams::pretrained() -> ModelParams`, with fields
    `bias: [f32; 65]`, `time_const: [f32; 65]`, `syn_strength: [f32; 604]`
  - `test_support::load_pretrained_fixture(name: &str) -> Option<String>`

- [ ] **Step 1: Add the fixture loader.** Append to
  `crates/flyvis/src/test_support.rs`:

```rust
/// Loads a file from `data/flyvis-pretrained/`, or `None` if it is absent.
/// See `tools/export_params.py`.
pub(crate) fn load_pretrained_fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../data/flyvis-pretrained/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(_) => {
            assert!(
                !fixtures_required(),
                "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing"
            );
            eprintln!(
                "skipping: {path} not found. Run ci/fetch-fixtures.sh, or \
                 `uv run --project tools tools/export_params.py`."
            );
            None
        }
    }
}
```

- [ ] **Step 2: Write the module with its failing tests.** Create
  `crates/flyvis/src/ensemble.rs`. The module doc, types and `parse` signature
  come first. The body is `todo!()` for now, so the tests compile and fail:

```rust
//! flyvis's trained ensemble: the free parameters of all 50 models, read from
//! `data/flyvis-pretrained/ensemble.csv` (written by `tools/export_params.py`).
//!
//! `pretrained.rs` compiles in model 000 only. This reads the same 734 values
//! for every model, positionally, in `pretrained.rs`'s orderings, and refuses
//! any row out of place: a misread row would attach a parameter to the wrong
//! cell type while everything downstream still runs.
//!
//! No file I/O -- callers hand in the text.

use crate::pretrained::{BIAS, CELL_TYPES, CONN_TYPES, SYN_STRENGTH, TIME_CONST};

/// Models in flyvis's pretrained ensemble.
pub const N_MODELS: usize = 50;
/// Rows per model: 65 biases, 65 time constants, 604 synapse strengths.
pub const ROWS_PER_MODEL: usize = 65 + 65 + 604;
const HEADER: &str = "model,kind,index,name,value";

/// One model's 734 free parameters, positional as in `pretrained.rs`.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelParams {
    pub bias: [f32; 65],
    pub time_const: [f32; 65],
    pub syn_strength: [f32; 604],
}

impl ModelParams {
    /// Model 000, from the compiled-in constants in `pretrained.rs`.
    pub fn pretrained() -> Self {
        ModelParams { bias: BIAS, time_const: TIME_CONST, syn_strength: SYN_STRENGTH }
    }
}

/// All 50 models, in file order (`000` to `049`).
#[derive(Clone, Debug, PartialEq)]
pub struct Ensemble {
    models: Vec<(String, ModelParams)>,
}

/// Why `ensemble.csv` was refused. `line` is 1-based.
#[derive(Clone, Debug, PartialEq)]
pub struct EnsembleError {
    pub line: usize,
    pub kind: EnsembleErrorKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EnsembleErrorKind {
    /// The file contains a `\r`; it is LF-only.
    CarriageReturn,
    /// Nothing but `#` lines.
    MissingHeader,
    BadHeader(String),
    /// A `#` line after the header.
    CommentAfterHeader,
    /// A row without exactly five fields.
    FieldCount(usize),
    /// `model` is not three ASCII digits.
    BadModel(String),
    ModelOutOfOrder { expected: String, found: String },
    BadKind { expected: &'static str, found: String },
    BadIndex { expected: usize, found: String },
    BadName { expected: String, found: String },
    /// Not a finite `f32`.
    BadValue(String),
    TooFewRows { expected: usize, found: usize },
    TooManyRows { expected: usize },
}

impl std::fmt::Display for EnsembleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use EnsembleErrorKind::*;
        write!(f, "ensemble.csv line {}: ", self.line)?;
        match &self.kind {
            CarriageReturn => write!(f, "contains a carriage return; the file is LF-only"),
            MissingHeader => write!(f, "no header line"),
            BadHeader(h) => write!(f, "header is {h:?}, expected {HEADER:?}"),
            CommentAfterHeader => write!(f, "a # line after the header"),
            FieldCount(n) => write!(f, "{n} fields, expected 5"),
            BadModel(m) => write!(f, "model {m:?} is not three digits"),
            ModelOutOfOrder { expected, found } => write!(f, "model {found}, expected {expected}"),
            BadKind { expected, found } => write!(f, "kind {found:?}, expected {expected}"),
            BadIndex { expected, found } => write!(f, "index {found:?}, expected {expected}"),
            BadName { expected, found } => write!(f, "name {found:?}, expected {expected:?}"),
            BadValue(v) => write!(f, "value {v:?} is not a finite f32"),
            TooFewRows { expected, found } => write!(f, "{found} rows, expected {expected}"),
            TooManyRows { expected } => write!(f, "more than {expected} rows"),
        }
    }
}

impl std::error::Error for EnsembleError {}

/// The (kind, index, name) that row `s` of every model must carry.
fn slot(s: usize) -> (&'static str, usize, String) {
    if s < 65 {
        ("bias", s, CELL_TYPES[s].to_string())
    } else if s < 130 {
        ("time_const", s - 65, CELL_TYPES[s - 65].to_string())
    } else {
        let (src, tar) = CONN_TYPES[s - 130];
        ("syn_strength", s - 130, format!("{src}>{tar}"))
    }
}

impl Ensemble {
    /// Parses the whole file. See the module doc for what is refused.
    pub fn parse(csv: &str) -> Result<Self, EnsembleError> {
        todo!()
    }

    /// One model's parameters, by its three-digit id (`"000"`..`"049"`).
    pub fn model(&self, id: &str) -> Option<&ModelParams> {
        self.models.iter().find(|(m, _)| m == id).map(|(_, p)| p)
    }

    /// The model ids, in file order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.models.iter().map(|(m, _)| m.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::load_pretrained_fixture;
    use EnsembleErrorKind::*;

    /// A valid file of `n_models` models, every one holding model 000's
    /// values. One `#` line and the header come first, so data row r is on
    /// line r + 3.
    fn csv(n_models: usize) -> String {
        let p = ModelParams::pretrained();
        let mut out = String::from("# synthetic\nmodel,kind,index,name,value\n");
        for m in 0..n_models {
            for s in 0..ROWS_PER_MODEL {
                let (kind, i, name) = slot(s);
                let v = match kind {
                    "bias" => p.bias[i],
                    "time_const" => p.time_const[i],
                    _ => p.syn_strength[i],
                };
                out.push_str(&format!("{m:03},{kind},{i},{name},{v}\n"));
            }
        }
        out
    }

    /// `text` with its 1-based `line` replaced by `new`.
    fn with_line(text: &str, line: usize, new: &str) -> String {
        let mut lines: Vec<&str> = text.split('\n').collect();
        lines[line - 1] = new;
        lines.join("\n")
    }

    fn error(text: &str) -> EnsembleError {
        Ensemble::parse(text).expect_err("must be refused")
    }

    fn bits(v: &[f32]) -> Vec<u32> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    #[test]
    fn parses_a_valid_file() {
        let e = Ensemble::parse(&csv(N_MODELS)).expect("valid");
        let ids: Vec<&str> = e.ids().collect();
        assert_eq!(ids.len(), N_MODELS);
        assert_eq!((ids[0], ids[49]), ("000", "049"));
        assert_eq!(e.model("049"), Some(&ModelParams::pretrained()));
        assert_eq!(e.model("050"), None);
    }

    #[test]
    fn accepts_a_file_without_a_trailing_newline() {
        let text = csv(N_MODELS);
        assert!(Ensemble::parse(text.trim_end_matches('\n')).is_ok());
    }

    #[test]
    fn rejects_a_carriage_return() {
        let text = csv(N_MODELS).replacen('\n', "\r\n", 1);
        assert_eq!(error(&text), EnsembleError { line: 1, kind: CarriageReturn });
    }

    #[test]
    fn rejects_a_file_with_no_header() {
        assert_eq!(error("# only a comment\n").kind, MissingHeader);
    }

    #[test]
    fn rejects_a_header_with_a_field_missing() {
        let text = with_line(&csv(N_MODELS), 2, "model,kind,index,name");
        assert_eq!(error(&text), EnsembleError { line: 2, kind: BadHeader("model,kind,index,name".into()) });
    }

    #[test]
    fn rejects_a_comment_after_the_header() {
        let text = csv(N_MODELS).replacen("value\n", "value\n# late\n", 1);
        assert_eq!(error(&text), EnsembleError { line: 3, kind: CommentAfterHeader });
    }

    #[test]
    fn rejects_a_shuffled_row() {
        let text = csv(N_MODELS);
        let lines: Vec<&str> = text.split('\n').collect();
        let swapped = with_line(&with_line(&text, 3, lines[3]), 4, lines[2]);
        assert_eq!(
            error(&swapped),
            EnsembleError { line: 3, kind: BadIndex { expected: 0, found: "1".into() } }
        );
    }

    #[test]
    fn rejects_a_wrong_name() {
        let text = csv(N_MODELS);
        let row = text.split('\n').nth(2).unwrap().replacen(",R1,", ",R2,", 1);
        assert_eq!(
            error(&with_line(&text, 3, &row)),
            EnsembleError { line: 3, kind: BadName { expected: "R1".into(), found: "R2".into() } }
        );
    }

    #[test]
    fn rejects_a_non_finite_value() {
        let text = with_line(&csv(N_MODELS), 3, "000,bias,0,R1,inf");
        assert_eq!(error(&text), EnsembleError { line: 3, kind: BadValue("inf".into()) });
    }

    #[test]
    fn rejects_a_short_row() {
        let text = with_line(&csv(N_MODELS), 3, "000,bias,0,R1");
        assert_eq!(error(&text), EnsembleError { line: 3, kind: FieldCount(4) });
    }

    #[test]
    fn rejects_a_duplicate_model() {
        let text = csv(N_MODELS);
        let line = 3 + ROWS_PER_MODEL; // first row of model 001
        let row = text.split('\n').nth(line - 1).unwrap().replacen("001,", "000,", 1);
        assert_eq!(
            error(&with_line(&text, line, &row)),
            EnsembleError { line, kind: ModelOutOfOrder { expected: "001".into(), found: "000".into() } }
        );
    }

    #[test]
    fn rejects_49_models() {
        assert!(matches!(error(&csv(49)).kind, TooFewRows { found, .. } if found == 49 * ROWS_PER_MODEL));
    }

    #[test]
    fn rejects_51_models() {
        assert!(matches!(error(&csv(51)).kind, TooManyRows { .. }));
    }

    /// The published file's model 000 is the compiled-in model 000, bit for bit.
    #[test]
    fn model_000_equals_the_compiled_in_parameters() {
        let Some(text) = load_pretrained_fixture("ensemble.csv") else { return };
        let e = Ensemble::parse(&text).expect("the published ensemble must parse");
        let ids: Vec<&str> = e.ids().collect();
        assert_eq!(ids, (0..N_MODELS).map(|m| format!("{m:03}")).collect::<Vec<_>>());
        let (got, want) = (e.model("000").unwrap(), ModelParams::pretrained());
        assert_eq!(bits(&got.bias), bits(&want.bias), "bias");
        assert_eq!(bits(&got.time_const), bits(&want.time_const), "time_const");
        assert_eq!(bits(&got.syn_strength), bits(&want.syn_strength), "syn_strength");
    }

    /// The exporter's reader rounds decimal -> f64 -> f32; Rust rounds
    /// decimal -> f32 once. They must agree on every value of every model,
    /// not just model 000.
    #[test]
    fn every_value_rounds_the_same_once_or_twice() {
        let Some(text) = load_pretrained_fixture("ensemble.csv") else { return };
        for line in text.lines().filter(|l| !l.starts_with('#')).skip(1) {
            let s = line.rsplit(',').next().unwrap();
            let once: f32 = s.parse().unwrap();
            let twice = s.parse::<f64>().unwrap() as f32;
            assert_eq!(once.to_bits(), twice.to_bits(), "{line}");
        }
    }
}
```

  Register the module in `crates/flyvis/src/lib.rs`. Add `pub mod ensemble;`
  directly above `pub mod expand;`, and this line after the existing
  `pub use expand::…` line:

```rust
pub use ensemble::{Ensemble, EnsembleError, EnsembleErrorKind, ModelParams};
```

- [ ] **Step 3: Run the tests and confirm they fail.**
  - Run: `cargo test -p semblance-flyvis ensemble`
  - Expected: every test that calls `parse` panics with
    `not yet implemented`. With `data/` absent, the two fixture tests skip
    instead.

- [ ] **Step 4: Implement `parse`.** Replace the `todo!()` body:

```rust
    pub fn parse(csv: &str) -> Result<Self, EnsembleError> {
        use EnsembleErrorKind::*;
        let err = |line: usize, kind: EnsembleErrorKind| EnsembleError { line, kind };

        let mut lines: Vec<&str> = csv.split('\n').collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        if let Some(i) = lines.iter().position(|l| l.contains('\r')) {
            return Err(err(i + 1, CarriageReturn));
        }
        let header = lines
            .iter()
            .position(|l| !l.starts_with('#'))
            .ok_or_else(|| err(lines.len(), MissingHeader))?;
        if lines[header] != HEADER {
            return Err(err(header + 1, BadHeader(lines[header].to_string())));
        }

        let rows = &lines[header + 1..];
        let expected = N_MODELS * ROWS_PER_MODEL;
        if rows.len() > expected {
            return Err(err(header + 2 + expected, TooManyRows { expected }));
        }
        let mut models = Vec::with_capacity(N_MODELS);
        let mut current = ModelParams { bias: [0.0; 65], time_const: [0.0; 65], syn_strength: [0.0; 604] };
        for (r, row) in rows.iter().enumerate() {
            let line = header + 2 + r;
            let (m, s) = (r / ROWS_PER_MODEL, r % ROWS_PER_MODEL);
            if row.starts_with('#') {
                return Err(err(line, CommentAfterHeader));
            }
            let f: Vec<&str> = row.split(',').collect();
            if f.len() != 5 {
                return Err(err(line, FieldCount(f.len())));
            }
            if f[0].len() != 3 || !f[0].bytes().all(|b| b.is_ascii_digit()) {
                return Err(err(line, BadModel(f[0].to_string())));
            }
            let model = format!("{m:03}");
            if f[0] != model {
                return Err(err(line, ModelOutOfOrder { expected: model, found: f[0].to_string() }));
            }
            let (kind, index, name) = slot(s);
            if f[1] != kind {
                return Err(err(line, BadKind { expected: kind, found: f[1].to_string() }));
            }
            if f[2] != index.to_string() {
                return Err(err(line, BadIndex { expected: index, found: f[2].to_string() }));
            }
            if f[3] != name {
                return Err(err(line, BadName { expected: name, found: f[3].to_string() }));
            }
            let value = f[4]
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| err(line, BadValue(f[4].to_string())))?;
            match kind {
                "bias" => current.bias[index] = value,
                "time_const" => current.time_const[index] = value,
                _ => current.syn_strength[index] = value,
            }
            if s == ROWS_PER_MODEL - 1 {
                models.push((model, current.clone()));
            }
        }
        if rows.len() < expected {
            return Err(err(lines.len(), TooFewRows { expected, found: rows.len() }));
        }
        Ok(Ensemble { models })
    }
```

- [ ] **Step 5: Run the tests and confirm they pass.**
  - Run: `cargo test -p semblance-flyvis ensemble -- --nocapture`
  - Expected: all pass. With `data/` present, neither fixture test prints
    `skipping`.

- [ ] **Step 6: Run clippy, then commit.**

```bash
cargo clippy -p semblance-flyvis --all-targets -- -D warnings
git add crates/flyvis/src/ensemble.rs crates/flyvis/src/lib.rs crates/flyvis/src/test_support.rs
git commit -m "Read flyvis's 50-model ensemble from ensemble.csv"
```

---

### Task 2: Wiring: synapse-count groups, input nodes, the sign check

**Files:**
- Create: `crates/flyvis/src/wiring.rs`
- Modify: `crates/flyvis/src/lib.rs`
- Modify: `crates/flyvis/src/test_support.rs` (add `real_spec_and_network`)
- Modify: `crates/flyvis/src/expand_checks.rs` (use the shared helper; group
  through `edge_syn_groups`; add `conn_sign_equals_the_trained_sign`)

**Interfaces:**
- Consumes:
  - `semblance_connectome::TypedNetwork` accessors
  - `crate::spec::ConnectomeSpec { input_units: Vec<String>, .. }`
  - `crate::pretrained::{SYN_COUNT, SIGN}`
- Produces:
  - `semblance_flyvis::wiring::edge_syn_groups(&TypedNetwork) -> Result<Vec<u32>, WiringError>`
  - `semblance_flyvis::wiring::input_nodes(&ConnectomeSpec, &TypedNetwork) -> Result<Vec<Vec<u32>>, WiringError>`
  - `WiringError::{GroupCount { expected, found }, UnknownInputUnit(String)}`
  - `test_support::real_spec_and_network() -> Option<(ConnectomeSpec, TypedNetwork)>`

- [ ] **Step 1: Add the shared real-network helper.** Append to
  `test_support.rs`:

```rust
/// The real spec and its expansion, or `None` if the spec has not been
/// fetched. Every gate that needs the real network builds it here.
pub(crate) fn real_spec_and_network(
) -> Option<(crate::spec::ConnectomeSpec, semblance_connectome::TypedNetwork)> {
    let raw = load_real_spec()?;
    let spec = crate::spec::ConnectomeSpec::from_json_str(&raw).expect("real spec must load");
    let net = crate::expand::expand(&spec, crate::expand::DEFAULT_EXTENT, crate::expand::DEFAULT_N_SYN_FILL)
        .expect("real spec must expand");
    Some((spec, net))
}
```

  In `expand_checks.rs`:
  - Replace `real_network`'s body with
    `real_spec_and_network().map(|(_, n)| n)`.
  - Import `crate::test_support::real_spec_and_network`.
  - Drop imports left unused (`expand`, `DEFAULT_*`, `ConnectomeSpec`,
    `load_real_spec`) if clippy flags them.

- [ ] **Step 2: Write `wiring.rs` with failing tests.** Create
  `crates/flyvis/src/wiring.rs`:

```rust
//! What the graded integrator needs from flyvis's build besides the network:
//! which synapse-count group each edge belongs to, and which nodes receive
//! the visual input.
//!
//! Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 2.

use std::collections::HashMap;

use semblance_connectome::TypedNetwork;

use crate::pretrained::SYN_COUNT;
use crate::spec::ConnectomeSpec;

#[derive(Clone, Debug, PartialEq)]
pub enum WiringError {
    /// The network's (connection type, du, dv) groups do not number
    /// `SYN_COUNT.len()`, so `SYN_COUNT` cannot be positional over them.
    GroupCount { expected: usize, found: usize },
    /// An input unit names no cell type of the network.
    UnknownInputUnit(String),
}

impl std::fmt::Display for WiringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WiringError::GroupCount { expected, found } =>
                write!(f, "{found} synapse-count groups, expected {expected}"),
            WiringError::UnknownInputUnit(u) => write!(f, "input unit {u:?} is not a cell type"),
        }
    }
}

impl std::error::Error for WiringError {}

/// Each edge's group, numbered by first appearance, and the group count.
fn group_edges(net: &TypedNetwork) -> (Vec<u32>, usize) {
    let mut index: HashMap<(u16, i16, i16), u32> = HashMap::new();
    let mut groups = Vec::with_capacity(net.num_edges());
    for e in 0..net.num_edges() {
        let s = net.edge_source()[e] as usize;
        let t = net.edge_target()[e] as usize;
        let key = (
            net.edge_conn_type()[e],
            net.node_u()[t] - net.node_u()[s],
            net.node_v()[t] - net.node_v()[s],
        );
        let next = index.len() as u32;
        groups.push(*index.entry(key).or_insert(next));
    }
    (groups, index.len())
}

/// For each edge, the index into `SYN_COUNT` (and the tables in
/// `syn_count_torch`) of its (connection type, du, dv) group. `SYN_COUNT` is
/// positional over those groups in first-appearance order over the built
/// edges.
pub fn edge_syn_groups(net: &TypedNetwork) -> Result<Vec<u32>, WiringError> {
    let (groups, found) = group_edges(net);
    if found != SYN_COUNT.len() {
        return Err(WiringError::GroupCount { expected: SYN_COUNT.len(), found });
    }
    Ok(groups)
}

/// The input nodes per input unit, in `spec.input_units` order, each in node
/// order: flyvis's [8, 721] `input_index` (`stimulus.py:128-130`).
pub fn input_nodes(spec: &ConnectomeSpec, net: &TypedNetwork) -> Result<Vec<Vec<u32>>, WiringError> {
    spec.input_units
        .iter()
        .map(|unit| {
            let t = net
                .cell_types()
                .iter()
                .position(|c| c == unit)
                .ok_or_else(|| WiringError::UnknownInputUnit(unit.clone()))?;
            Ok((0..net.num_nodes() as u32)
                .filter(|&i| net.node_cell_type()[i as usize] as usize == t)
                .collect())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::real_spec_and_network;
    use semblance_connectome::TypedNetworkParts;

    /// Types A and B on two columns; one connection type A -> B, realised at
    /// three different offsets.
    fn net() -> TypedNetwork {
        TypedNetwork::new(TypedNetworkParts {
            cell_types: vec!["A".into(), "B".into()],
            node_cell_type: vec![0, 0, 1, 1],
            node_u: vec![0, 1, 0, 1],
            node_v: vec![0, 0, 0, 0],
            conn_types: vec![(0, 1)],
            conn_sign: vec![1.0],
            edge_source: vec![0, 1, 0, 1],
            edge_target: vec![2, 3, 3, 2],
            edge_conn_type: vec![0, 0, 0, 0],
            edge_n_syn: vec![1.0; 4],
        })
        .unwrap()
    }

    fn spec(units: &str) -> ConnectomeSpec {
        let node = |n: &str| format!(
            r#"{{"name":"{n}","pattern":["stride",[1,1]],"activation":"relu","bias":0.0,
                "bias_fixed":false,"time_constant":0.05,"time_constant_fixed":false}}"#
        );
        ConnectomeSpec::from_json_str(&format!(
            r#"{{"nodes":[{},{}],"edges":[],"input_units":{units},"output_units":[]}}"#,
            node("A"),
            node("B")
        ))
        .unwrap()
    }

    #[test]
    fn groups_edges_by_type_and_offset_in_first_appearance_order() {
        // offsets (du, dv): (0,0), (0,0), (1,0), (-1,0)
        assert_eq!(group_edges(&net()), (vec![0, 0, 1, 2], 3));
    }

    #[test]
    fn refuses_a_network_whose_groups_do_not_fit_syn_count() {
        assert_eq!(
            edge_syn_groups(&net()),
            Err(WiringError::GroupCount { expected: 2355, found: 3 })
        );
    }

    #[test]
    fn input_nodes_keep_the_input_unit_order() {
        assert_eq!(input_nodes(&spec(r#"["B","A"]"#), &net()), Ok(vec![vec![2, 3], vec![0, 1]]));
    }

    #[test]
    fn an_input_unit_that_is_not_a_cell_type_is_an_error() {
        assert_eq!(
            input_nodes(&spec(r#"["C"]"#), &net()),
            Err(WiringError::UnknownInputUnit("C".into()))
        );
    }

    #[test]
    fn the_real_network_has_2355_groups_and_eight_input_types_of_721() {
        let Some((spec, n)) = real_spec_and_network() else { return };
        let groups = edge_syn_groups(&n).expect("2,355 groups");
        assert_eq!(groups.len(), 1_513_231);
        let inputs = input_nodes(&spec, &n).expect("input units are cell types");
        assert_eq!(inputs.len(), 8);
        for (k, nodes) in inputs.iter().enumerate() {
            assert_eq!(nodes.len(), 721, "input unit {k}");
            assert!(nodes.windows(2).all(|w| w[0] < w[1]), "input unit {k} ascending");
            let t = n.node_cell_type()[nodes[0] as usize] as usize;
            assert_eq!(n.cell_types()[t], spec.input_units[k]);
        }
    }
}
```

  In `lib.rs`, add `pub mod wiring;` after `pub mod stimuli;`'s block, and:

```rust
pub use wiring::{edge_syn_groups, input_nodes, WiringError};
```

- [ ] **Step 3: Run the new tests.**
  - Run: `cargo test -p semblance-flyvis wiring`
  - Expected: PASS. The code is written in full above, because it is moved
    logic. The real-network test proves it on the fixture.

- [ ] **Step 4: Route the existing count check through `edge_syn_groups`, and
  add the sign check.** In `expand_checks.rs`, replace the body of
  `trained_synapse_counts_match_the_expanded_network` after its
  `let Some(n) = …` line with:

```rust
    let groups = crate::wiring::edge_syn_groups(&n).expect("2,355 realised offset groups");
    // The first edge of each group, in group order.
    let mut first: Vec<Option<usize>> = vec![None; crate::pretrained::SYN_COUNT.len()];
    for (e, &g) in groups.iter().enumerate() {
        first[g as usize].get_or_insert(e);
    }
    for (g, e) in first.iter().enumerate() {
        let n_syn = n.edge_n_syn()[e.expect("every group has an edge")];
        let want = crate::pretrained::SYN_COUNT[g].exp();
        assert!(
            (want - n_syn).abs() <= 1e-6 * n_syn.max(1.0),
            "offset group {g}: exp(SYN_COUNT) = {want}, expansion has n_syn = {n_syn}"
        );
    }
```

  Update that test's doc comment. Its first paragraph stays. Change the
  sentence about the grouping to: "The grouping itself is
  `wiring::edge_syn_groups`, which the graded integrator uses; this test pins
  its order and values." Then add after
  `trained_parameter_ordering_matches_the_expanded_network`:

```rust
/// The sign the integrator multiplies is the spec's (`conn_sign`); the one
/// flyvis multiplies is the checkpoint's (`pretrained::SIGN`). They must be
/// the same 604 values, bit for bit, or the gate would compare two different
/// networks.
#[test]
fn conn_sign_equals_the_trained_sign() {
    let Some(n) = real_network() else { return };
    assert_eq!(n.conn_sign().len(), crate::pretrained::SIGN.len());
    for (j, (a, b)) in n.conn_sign().iter().zip(crate::pretrained::SIGN.iter()).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "sign of connection type {j}");
    }
}
```

- [ ] **Step 5: Run the flyvis suite.**
  - Run: `cargo test -p semblance-flyvis && cargo clippy -p semblance-flyvis --all-targets -- -D warnings`
  - Expected: all pass, clippy clean.

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/wiring.rs crates/flyvis/src/lib.rs crates/flyvis/src/test_support.rs crates/flyvis/src/expand_checks.rs
git commit -m "Add synapse-count groups and input nodes, and pin the sign to the checkpoint's"
```

---

### Task 3: torch's synapse counts, exported and compiled in

**Files:**
- Create: `tools/export_syn_count.py`
- Create (generated): `crates/flyvis/src/syn_count_torch.rs`
- Create: `crates/flyvis/src/syn_count_torch_checks.rs`
- Modify: `crates/flyvis/src/lib.rs`, `tools/README.md`

**Interfaces:**
- Consumes:
  - `tools/export_reference.py`'s `CHECKPOINT`, `PRETRAINED_RS`, `RESULTS`,
    `bits_equal` and `parse_pretrained`
  - the local checkpoints under `data/flyvis-pretrained/results/flow/0000/{000,049}/`
- Produces:
  - `semblance_flyvis::syn_count_torch::SYN_COUNT_TORCH_F32: [f32; 2355]`
  - `semblance_flyvis::syn_count_torch::SYN_COUNT_TORCH_F64: [f64; 2355]`

- [ ] **Step 1: Write the table checks first.** Create
  `crates/flyvis/src/syn_count_torch_checks.rs`:

```rust
//! The torch-computed synapse counts must be `exp` of `SYN_COUNT`, to within
//! the last bit or two -- and must differ from Rust's own `exp` on only the
//! few values where torch's and the C library's rounding disagree. A table
//! written from the wrong source (the logs themselves, f32 values widened
//! into the f64 table, another model's counts) fails one or the other.

use crate::pretrained::SYN_COUNT;
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};

/// At most this many entries may differ from std's `exp`. The 2026-10-28
/// measurement found about 105 of 2,355 in each precision.
const MAX_DIFFERING: usize = 200;

#[test]
fn the_f32_table_is_exp_of_syn_count_to_within_two_ulps() {
    let mut differ = 0;
    for (i, (&ln, &torch)) in SYN_COUNT.iter().zip(SYN_COUNT_TORCH_F32.iter()).enumerate() {
        let libm = ln.exp();
        let ulps = (torch.to_bits() as i64 - libm.to_bits() as i64).unsigned_abs();
        assert!(ulps <= 2, "entry {i}: torch {torch:e}, libm {libm:e}, {ulps} ulps apart");
        differ += usize::from(ulps != 0);
    }
    assert!(differ <= MAX_DIFFERING, "{differ} entries differ from f32::exp");
}

#[test]
fn the_f64_table_is_exp_of_syn_count_to_within_two_ulps() {
    let mut differ = 0;
    for (i, (&ln, &torch)) in SYN_COUNT.iter().zip(SYN_COUNT_TORCH_F64.iter()).enumerate() {
        let libm = (ln as f64).exp();
        let ulps = (torch.to_bits() as i128 - libm.to_bits() as i128).unsigned_abs();
        assert!(ulps <= 2, "entry {i}: torch {torch:e}, libm {libm:e}, {ulps} ulps apart");
        differ += usize::from(ulps != 0);
    }
    assert!(differ <= MAX_DIFFERING, "{differ} entries differ from f64::exp");
}
```

  (All counts are positive, so the bit distance is the ulp distance.)

- [ ] **Step 2: Write the exporter.** Create `tools/export_syn_count.py`:

```python
"""Export syn_count exactly as flyvis's PyTorch computes it, for 0.3.0's exact settling gate.

flyvis stores each synapse count as ln(count) and multiplies exp of it into
every weight, computed by torch in the run's precision: the parameter's value
is `raw_values.exp()` (flyvis/network/initialization.py:199-202), on the
[2355] float32 parameter in a float32 run and on its exact float64 widening
after `.double()`. torch's vectorised exp and the C library's disagree in the
last bit on about 105 of these 2,355 values, and that bit is the difference
between reproducing flyvis's settling exactly and reproducing it within a
bound. So this records torch's results once, as compiled-in constants, the
way export_params.py records the trained parameters.

The values are torch.exp of the checkpoint's own `edges_syn_count` tensor,
after asserting that the checkpoints of models 000 and 049 both hold exactly
pretrained.rs's SYN_COUNT. They are one machine's torch exp; the generated
file's header records which.

Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 3

Run by hand:  uv run --project tools tools/export_syn_count.py
"""

import datetime
import importlib.metadata
import math
import pathlib
import platform
import sys

import numpy as np
import torch

from export_reference import CHECKPOINT, PRETRAINED_RS, RESULTS, bits_equal, parse_pretrained

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "flyvis" / "src" / "syn_count_torch.rs"
MODELS = ("000", "049")
N = 2355
PER_LINE = 6


def fail(message: str):
    raise SystemExit(f"export_syn_count: {message}. Nothing was written.")


def raw_syn_count(model: str) -> torch.Tensor:
    """The checkpoint's syn_count parameter: ln(count), float32, [2355]."""
    path = RESULTS / model / CHECKPOINT
    t = torch.load(path, map_location="cpu", weights_only=True)["network"]["edges_syn_count"]
    if t.dtype != torch.float32 or tuple(t.shape) != (N,):
        fail(f"{path}: edges_syn_count is {t.dtype} {tuple(t.shape)}")
    return t.detach().contiguous()


def literal_f32(v: np.float32) -> str:
    """Shortest decimal that round-trips as f32; clippy's
    excessive_precision rejects anything longer."""
    s = str(np.float32(v))
    if not any(c in s for c in ".eE"):
        s += ".0"
    if np.float32(float(s)) != v:
        fail(f"{s} does not round-trip to {v!r}")
    return s


def literal_f64(v: np.float64) -> str:
    s = repr(float(v))
    if float(s) != float(v):
        fail(f"{s} does not round-trip to {v!r}")
    return s


def table(name: str, ty: str, literals: list, doc: str) -> str:
    rows = [", ".join(literals[i:i + PER_LINE]) for i in range(0, len(literals), PER_LINE)]
    body = "\n".join(f"    {r}," for r in rows)
    return f"/// {doc}\npub static {name}: [{ty}; {N}] = [\n{body}\n];\n"


def main() -> int:
    pre = parse_pretrained(PRETRAINED_RS.read_text())
    raw = {m: raw_syn_count(m) for m in MODELS}
    for m, t in raw.items():
        if not bits_equal(t, pre["SYN_COUNT"]):
            fail(f"checkpoint {m}'s syn_count differs from pretrained.rs's SYN_COUNT")

    t = raw["000"]
    c32 = torch.exp(t).numpy()           # the float32 run's raw_values.exp()
    c64 = torch.exp(t.double()).numpy()  # the float64 run's, after .double()
    if c32.dtype != np.float32 or c64.dtype != np.float64 or c32.shape != (N,) or c64.shape != (N,):
        fail(f"exp gave {c32.dtype} {c32.shape} and {c64.dtype} {c64.shape}")
    if not (np.isfinite(c32).all() and np.isfinite(c64).all() and (c32 > 0).all() and (c64 > 0).all()):
        fail("a count is not finite and positive")

    lns = t.numpy()
    d64 = sum(1 for ln, c in zip(lns, c64) if math.exp(float(ln)) != float(c))
    print(f"  torch.exp differs from math.exp on {d64} of {N} float64 counts")

    versions = (f"flyvis {importlib.metadata.version('flyvis')} · torch {torch.__version__} · "
                f"numpy {np.__version__} · {platform.machine()} · "
                f"CPU capability {torch.backends.cpu.get_cpu_capability()} · "
                f"exported {datetime.date.today().isoformat()}")
    text = f"""//! syn_count exactly as flyvis's PyTorch computes it, for the exact settling gate.
//!
//! GENERATED by `tools/export_syn_count.py`. Do not edit by hand.
//! {versions}
//!
//! flyvis stores each synapse count as ln(count) and multiplies `exp` of it
//! into every weight, computed by torch in the run's precision. torch's
//! vectorised `exp` and the C library's disagree in the last bit on some of
//! these 2,355 values, and that bit is the difference between reproducing
//! flyvis's settling exactly and reproducing it within a bound. These are
//! torch's results on the machine above, positional like `SYN_COUNT`.

// Measurements, not hand-written approximations of std constants; see the
// same allowance in pretrained.rs.
#![allow(clippy::approx_constant)]

{table("SYN_COUNT_TORCH_F32", "f32", [literal_f32(v) for v in c32],
       "`torch.exp` of `pretrained::SYN_COUNT`, in float32.")}
{table("SYN_COUNT_TORCH_F64", "f64", [literal_f64(v) for v in c64],
       "`torch.exp` of `pretrained::SYN_COUNT` widened to float64.")}"""
    OUT.write_text(text)
    print(f"  wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 3: Run the exporter.**
  - Run: `uv run --project tools tools/export_syn_count.py`
  - Expected output:

    ```
    torch.exp differs from math.exp on <about 108> of 2355 float64 counts
    wrote crates/flyvis/src/syn_count_torch.rs
    ```

  - If it fails, the error names the cause. Do not hand-edit the output.

- [ ] **Step 4: Register the modules and run the checks.** In `lib.rs`, add
  `pub mod syn_count_torch;`, then `#[cfg(test)] mod syn_count_torch_checks;`,
  after the `stimuli_checks` lines.
  - Run: `cargo test -p semblance-flyvis syn_count_torch && cargo clippy -p semblance-flyvis --all-targets -- -D warnings`
  - Expected: both checks pass, and clippy is clean.

- [ ] **Step 5: Document the script.** Add a row to `tools/README.md`'s
  script table:

```markdown
| `export_syn_count.py` | flyvis's 2,355 synapse counts exactly as torch exponentiates them, in float32 and float64 — the last bit of `exp` that separates the exact settling gate from the bounded one | `crates/flyvis/src/syn_count_torch.rs` |
```

- [ ] **Step 6: Commit.**

```bash
git add tools/export_syn_count.py tools/README.md crates/flyvis/src/syn_count_torch.rs crates/flyvis/src/syn_count_torch_checks.rs crates/flyvis/src/lib.rs
git commit -m "Export flyvis's synapse counts as torch computes them"
```

---

### Task 4: The graded integrator (epic task 3)

**Files:**
- Create: `crates/sim/src/graded.rs`
- Modify: `crates/sim/src/lib.rs` (add `pub mod graded;` and one doc line)

**Interfaces:**
- Consumes: `semblance_connectome::TypedNetwork`
- Produces:
  - `semblance_sim::graded::Real`, a sealed trait with `ZERO`, `ONE`,
    `from_f32`, `from_f64`, `to_f64` and `is_finite`
  - `semblance_sim::graded::GradedParams<'a, R> { bias, time_const, syn_strength: &'a [f32], edge_syn_count: &'a [R] }`
  - `Graded::<R>::new(&TypedNetwork, &GradedParams<R>, dt: f64) -> Result<Graded<R>, GradedError>`
  - `Graded::step(&mut self, &[R]) -> Result<(), GradedError>`
  - `Graded::state(&self) -> &[R]`, `Graded::reset(&mut self)`,
    `Graded::dt(&self) -> f64`
  - `GradedError::{Length { what: &'static str, expected, found }, NonFinite { what: &'static str, index }, BadDt(f64)}`

- [ ] **Step 1: Write the module skeleton and its failing tests.** Create
  `crates/sim/src/graded.rs`:

```rust
//! Graded (non-spiking) dynamics: flyvis's passive point neurons with
//! instantaneous graded-release synapses, integrated by explicit Euler.
//!
//! ```text
//! dV_i/dt = 1/max(tau_i, f32(dt)) * (-V_i + bias_i + sum_e w_e relu(V_src(e)) + x_i)
//! w_e     = sign * syn_count * max(syn_strength, 0)
//! ```
//!
//! A new path beside the spiking one, not an extension of it: none of this
//! crate's spiking traits is involved. It follows flyvis 1.2.0 operation for
//! operation -- the association of every sum, the reciprocal-then-multiply,
//! the time-constant floor at f32(dt) in both precisions, each node's inputs
//! summed in ascending edge order from zero -- because `semblance-flyvis`'s
//! settling gate holds it to bit-equality with flyvis. Do not reorder an
//! expression here, and do not use `mul_add`.
//!
//! Synapse counts arrive already in the run's precision. Where they come from
//! (flyvis's own `exp`, or Rust's) is the caller's choice.
//!
//! Spec: docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 4.

use std::fmt::Debug;
use std::ops::{Add, Div, Mul, Neg, Sub};

use semblance_connectome::TypedNetwork;

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// The precision a run computes in: f32 (flyvis's float32 run) or f64 (its
/// float64 run). Sealed.
pub trait Real:
    Copy
    + PartialOrd
    + Debug
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + sealed::Sealed
{
    const ZERO: Self;
    const ONE: Self;
    /// Exact.
    fn from_f32(x: f32) -> Self;
    /// An `as` cast: rounds to nearest for f32.
    fn from_f64(x: f64) -> Self;
    /// Exact.
    fn to_f64(self) -> f64;
    fn is_finite(self) -> bool;
}

impl Real for f32 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    fn from_f32(x: f32) -> Self { x }
    fn from_f64(x: f64) -> Self { x as f32 }
    fn to_f64(self) -> f64 { self as f64 }
    fn is_finite(self) -> bool { f32::is_finite(self) }
}

impl Real for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    fn from_f32(x: f32) -> Self { x as f64 }
    fn from_f64(x: f64) -> Self { x }
    fn to_f64(self) -> f64 { self }
    fn is_finite(self) -> bool { f64::is_finite(self) }
}

/// A model's parameters, as flyvis holds them: f32, one per cell type or
/// connection type, plus one synapse count per edge in the run's precision.
#[derive(Clone, Copy, Debug)]
pub struct GradedParams<'a, R: Real> {
    pub bias: &'a [f32],
    pub time_const: &'a [f32],
    pub syn_strength: &'a [f32],
    pub edge_syn_count: &'a [R],
}

#[derive(Clone, Debug, PartialEq)]
pub enum GradedError {
    /// A slice does not match the network.
    Length { what: &'static str, expected: usize, found: usize },
    /// A parameter or count is NaN or infinite.
    NonFinite { what: &'static str, index: usize },
    /// `dt` is not finite and positive.
    BadDt(f64),
}

impl std::fmt::Display for GradedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GradedError::Length { what, expected, found } =>
                write!(f, "{what} has {found} values, expected {expected}"),
            GradedError::NonFinite { what, index } => write!(f, "{what}[{index}] is not finite"),
            GradedError::BadDt(dt) => write!(f, "dt {dt} is not finite and positive"),
        }
    }
}

impl std::error::Error for GradedError {}

/// flyvis's network, precomputed for one model, one dt, one precision.
#[derive(Clone, Debug)]
pub struct Graded<R: Real> {
    dt: f64,
    step_dt: R,
    bias: Vec<R>,
    inv_tau: Vec<R>,
    /// Node i's incoming edges are `offsets[i]..offsets[i + 1]` of `src` and
    /// `weight`, in ascending edge index.
    offsets: Vec<usize>,
    src: Vec<u32>,
    weight: Vec<R>,
    state: Vec<R>,
    next: Vec<R>,
}

fn max<R: Real>(a: R, b: R) -> R {
    if a > b { a } else { b }
}

fn relu<R: Real>(x: R) -> R {
    if x > R::ZERO { x } else { R::ZERO }
}

impl<R: Real> Graded<R> {
    pub fn new(net: &TypedNetwork, params: &GradedParams<R>, dt: f64) -> Result<Self, GradedError> {
        todo!()
    }

    /// One explicit Euler step with `input` (one value per node) added to
    /// every node's velocity. A failed step changes nothing.
    pub fn step(&mut self, input: &[R]) -> Result<(), GradedError> {
        todo!()
    }

    /// Back to the initial state: every node at its bias.
    pub fn reset(&mut self) {
        self.state.copy_from_slice(&self.bias);
    }

    pub fn state(&self) -> &[R] {
        &self.state
    }

    pub fn dt(&self) -> f64 {
        self.dt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semblance_connectome::TypedNetworkParts;

    /// One node per entry of `node_types`, all at (0, 0): position plays no
    /// part in the dynamics. `conns` are (source type, target type, sign);
    /// `edges` are (source node, target node, connection type).
    fn network(node_types: &[u16], n_types: usize, conns: &[(u16, u16, f32)], edges: &[(u32, u32, u16)]) -> TypedNetwork {
        TypedNetwork::new(TypedNetworkParts {
            cell_types: (0..n_types).map(|i| format!("T{i}")).collect(),
            node_cell_type: node_types.to_vec(),
            node_u: vec![0; node_types.len()],
            node_v: vec![0; node_types.len()],
            conn_types: conns.iter().map(|&(a, b, _)| (a, b)).collect(),
            conn_sign: conns.iter().map(|&(_, _, s)| s).collect(),
            edge_source: edges.iter().map(|e| e.0).collect(),
            edge_target: edges.iter().map(|e| e.1).collect(),
            edge_conn_type: edges.iter().map(|e| e.2).collect(),
            edge_n_syn: vec![1.0; edges.len()],
        })
        .expect("test network is consistent")
    }

    fn graded<R: Real>(net: &TypedNetwork, bias: &[f32], tau: &[f32], strength: &[f32], counts: &[R], dt: f64) -> Graded<R> {
        let p = GradedParams { bias, time_const: tau, syn_strength: strength, edge_syn_count: counts };
        Graded::new(net, &p, dt).expect("valid parameters")
    }

    fn r<R: Real>(x: f32) -> R {
        R::from_f32(x)
    }

    /// A lone neuron starting at bias under constant input x approaches
    /// bias + x as bias + x(1 - (1 - dt/tau)^k). A round-off comparison
    /// against a formula, not a bound on the port.
    fn closed_form<R: Real>(tol: f64) {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<R>(&net, &[0.3], &[0.05], &[], &[], 0.01);
        for _ in 0..100 {
            g.step(&[r::<R>(0.2)]).unwrap();
        }
        let rate = R::from_f64(0.01).to_f64() / (0.05f32 as f64);
        let want = 0.3f32 as f64 + 0.2f32 as f64 * (1.0 - (1.0 - rate).powi(100));
        let got = g.state()[0].to_f64();
        assert!(((got - want) / want).abs() < tol, "{got} vs {want}");
    }

    #[test]
    fn a_lone_neuron_follows_the_closed_form_f64() { closed_form::<f64>(1e-12) }
    #[test]
    fn a_lone_neuron_follows_the_closed_form_f32() { closed_form::<f32>(1e-5) }

    fn relu_reads_the_source<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let zero = [R::ZERO, R::ZERO];
        // A negative source contributes nothing: the target stays at its bias.
        let mut g = graded::<R>(&net, &[-0.5, -0.2], &[0.1, 0.1], &[1.0], &[R::ONE], 0.01);
        g.step(&zero).unwrap();
        assert_eq!(g.state()[1], r::<R>(-0.2));
        // A positive source drives a target whose own state is negative.
        let mut g = graded::<R>(&net, &[0.5, -0.2], &[0.1, 0.1], &[1.0], &[R::ONE], 0.01);
        g.step(&zero).unwrap();
        let b = r::<R>(-0.2);
        let s = R::ZERO + R::ONE * r::<R>(0.5);
        let want = b + ((R::ONE / r::<R>(0.1)) * (((-b + b) + s) + R::ZERO)) * R::from_f64(0.01);
        assert_eq!(g.state()[1], want);
    }

    #[test]
    fn relu_reads_the_source_f32() { relu_reads_the_source::<f32>() }
    #[test]
    fn relu_reads_the_source_f64() { relu_reads_the_source::<f64>() }

    fn negative_strength_is_clamped<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<R>(&net, &[0.5, 0.25], &[0.1, 0.1], &[-2.0], &[R::ONE], 0.01);
        g.step(&[R::ZERO, R::ZERO]).unwrap();
        assert_eq!(g.state()[1], r::<R>(0.25), "a clamped weight leaves the target at rest");
    }

    #[test]
    fn negative_strength_is_clamped_f32() { negative_strength_is_clamped::<f32>() }
    #[test]
    fn negative_strength_is_clamped_f64() { negative_strength_is_clamped::<f64>() }

    #[test]
    fn the_floor_is_f32_dt_widened_in_f64() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f64>(&net, &[0.0], &[0.001], &[], &[], 0.005);
        g.step(&[1.0]).unwrap();
        let floor = 0.005f32 as f64; // 0.004999999888241291
        let want = 0.0 + ((1.0 / floor) * (((-0.0 + 0.0) + 0.0) + 1.0)) * 0.005;
        assert_eq!(g.state()[0], want);
        assert_ne!(g.state()[0], 1.0, "a floor at the f64 dt would give exactly 1");
    }

    #[test]
    fn above_the_floor_the_rate_is_one_over_tau() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f64>(&net, &[0.0], &[0.02], &[], &[], 0.005);
        g.step(&[1.0]).unwrap();
        assert_eq!(g.state()[0], ((1.0 / (0.02f32 as f64)) * 1.0) * 0.005);
    }

    #[test]
    fn the_floor_in_f32_is_f32_dt() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f32>(&net, &[0.0], &[0.001], &[], &[], 0.02);
        g.step(&[1.0]).unwrap();
        assert_eq!(g.state()[0], ((1.0f32 / 0.02f32) * 1.0) * 0.02f32);
    }

    #[test]
    fn input_is_a_current_not_a_clamp() {
        let net = network(&[0], 1, &[], &[]);
        let mut g = graded::<f64>(&net, &[0.0], &[0.1], &[], &[], 0.01);
        g.step(&[1.0]).unwrap();
        assert_eq!(g.state()[0], ((1.0 / (0.1f32 as f64)) * 1.0) * 0.01);
        assert!(g.state()[0] < 0.2, "one step moves the state a fraction of the way");
    }

    fn reads_the_old_state<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0), (1, 0, 1.0)], &[(0, 1, 0), (1, 0, 1)]);
        let mut g = graded::<R>(&net, &[1.0, 2.0], &[0.1, 0.1], &[1.0, 1.0], &[R::ONE, R::ONE], 0.01);
        g.step(&[R::ZERO, R::ZERO]).unwrap();
        let at = |b: f32, source: f32| {
            let b = r::<R>(b);
            let s = R::ZERO + R::ONE * r::<R>(source);
            b + ((R::ONE / r::<R>(0.1)) * (((-b + b) + s) + R::ZERO)) * R::from_f64(0.01)
        };
        assert_eq!(g.state()[0], at(1.0, 2.0));
        assert_eq!(g.state()[1], at(2.0, 1.0), "node 1 must see node 0's old state, 1.0");
    }

    #[test]
    fn every_node_reads_the_old_state_f32() { reads_the_old_state::<f32>() }
    #[test]
    fn every_node_reads_the_old_state_f64() { reads_the_old_state::<f64>() }

    fn weight_is_sign_times_count_times_strength<R: Real>() {
        let net = network(&[0, 1], 2, &[(0, 1, -1.0)], &[(0, 1, 0)]);
        let mut g = graded::<R>(&net, &[1.0, 0.0], &[0.1, 0.1], &[0.5], &[r::<R>(3.0)], 0.01);
        g.step(&[R::ZERO, R::ZERO]).unwrap();
        let w = r::<R>(-1.0) * r::<R>(3.0) * r::<R>(0.5);
        let s = R::ZERO + w * R::ONE;
        let want = R::ZERO + ((R::ONE / r::<R>(0.1)) * (((-R::ZERO + R::ZERO) + s) + R::ZERO)) * R::from_f64(0.01);
        assert_eq!(g.state()[1], want);
    }

    #[test]
    fn weight_is_sign_times_count_times_strength_f32() { weight_is_sign_times_count_times_strength::<f32>() }
    #[test]
    fn weight_is_sign_times_count_times_strength_f64() { weight_is_sign_times_count_times_strength::<f64>() }

    /// Node 3's inputs, in edge order: +1e8, then -1e8, then +1. In f32,
    /// (1e8 - 1e8) + 1 = 1, but (1e8 + 1) - 1e8 = 0. An edge into node 4
    /// sits between them, so a by-target index must still keep edge order.
    #[test]
    fn sums_each_nodes_inputs_in_ascending_edge_order() {
        let net = network(
            &[0, 1, 2, 3, 4],
            5,
            &[(0, 3, 1.0), (1, 3, -1.0), (2, 3, 1.0), (0, 4, 1.0)],
            &[(0, 3, 0), (0, 4, 3), (1, 3, 1), (2, 3, 2)],
        );
        let mut g = graded::<f32>(&net, &[1.0, 1.0, 1.0, 0.0, 0.0], &[0.1; 5], &[1e8, 1e8, 1.0, 1.0], &[1.0; 4], 0.01);
        g.step(&[0.0; 5]).unwrap();
        let want = 0.0 + ((1.0 / 0.1f32) * (((-0.0f32 + 0.0) + 1.0) + 0.0)) * 0.01f32;
        assert_eq!(g.state()[3], want);
    }

    #[test]
    fn reset_returns_every_node_to_its_bias() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<f64>(&net, &[0.5, -0.25], &[0.1, 0.1], &[1.0], &[1.0], 0.01);
        g.step(&[1.0, 1.0]).unwrap();
        g.reset();
        assert_eq!(g.state(), [0.5, -0.25]);
    }

    #[test]
    fn refuses_bad_parameters() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let new = |b: &[f32], t: &[f32], s: &[f32], c: &[f64], dt: f64| {
            Graded::<f64>::new(&net, &GradedParams { bias: b, time_const: t, syn_strength: s, edge_syn_count: c }, dt)
                .map(|_| ())
        };
        use GradedError::*;
        let (b, t, s, c) = (&[0.0, 0.0][..], &[0.1, 0.1][..], &[1.0][..], &[1.0][..]);
        assert_eq!(new(b, t, s, c, 0.01), Ok(()));
        assert_eq!(new(&[0.0], t, s, c, 0.01), Err(Length { what: "bias", expected: 2, found: 1 }));
        assert_eq!(new(b, &[0.1], s, c, 0.01), Err(Length { what: "time_const", expected: 2, found: 1 }));
        assert_eq!(new(b, t, &[1.0, 1.0], c, 0.01), Err(Length { what: "syn_strength", expected: 1, found: 2 }));
        assert_eq!(new(b, t, s, &[], 0.01), Err(Length { what: "edge_syn_count", expected: 1, found: 0 }));
        assert_eq!(new(&[f32::NAN, 0.0], t, s, c, 0.01), Err(NonFinite { what: "bias", index: 0 }));
        assert_eq!(new(b, &[0.1, f32::INFINITY], s, c, 0.01), Err(NonFinite { what: "time_const", index: 1 }));
        assert_eq!(new(b, t, &[f32::NAN], c, 0.01), Err(NonFinite { what: "syn_strength", index: 0 }));
        assert_eq!(new(b, t, s, &[f64::INFINITY], 0.01), Err(NonFinite { what: "edge_syn_count", index: 0 }));
        for dt in [0.0, -0.01, f64::NAN, f64::INFINITY] {
            assert!(matches!(new(b, t, s, c, dt), Err(BadDt(_))), "dt {dt}");
        }
    }

    #[test]
    fn a_failed_step_leaves_the_state_alone() {
        let net = network(&[0, 1], 2, &[(0, 1, 1.0)], &[(0, 1, 0)]);
        let mut g = graded::<f64>(&net, &[0.5, -0.25], &[0.1, 0.1], &[1.0], &[1.0], 0.01);
        g.step(&[0.0, 0.0]).unwrap();
        let before = g.state().to_vec();
        assert_eq!(g.step(&[0.0]), Err(GradedError::Length { what: "input", expected: 2, found: 1 }));
        assert_eq!(g.state(), &before[..]);
    }
}
```

  In `crates/sim/src/lib.rs`, add `pub mod graded;` directly above the
  `use semblance_connectome::…` line. Then add this bullet to the crate doc's
  "What is DONE here" list:

```rust
//! - [`graded`]: flyvis's graded (non-spiking) dynamics, in f32 and f64,
//!   beside the spiking path and touching none of its traits
```

- [ ] **Step 2: Run the tests and confirm they fail.**
  - Run: `cargo test -p semblance-sim graded`
  - Expected: every test panics with `not yet implemented`.

- [ ] **Step 3: Implement `new` and `step`.** Replace the two `todo!()` bodies:

```rust
    pub fn new(net: &TypedNetwork, params: &GradedParams<R>, dt: f64) -> Result<Self, GradedError> {
        if !(dt.is_finite() && dt > 0.0) {
            return Err(GradedError::BadDt(dt));
        }
        let n_types = net.cell_types().len();
        for (what, len, expected) in [
            ("bias", params.bias.len(), n_types),
            ("time_const", params.time_const.len(), n_types),
            ("syn_strength", params.syn_strength.len(), net.conn_types().len()),
            ("edge_syn_count", params.edge_syn_count.len(), net.num_edges()),
        ] {
            if len != expected {
                return Err(GradedError::Length { what, expected, found: len });
            }
        }
        for (what, values) in [
            ("bias", params.bias),
            ("time_const", params.time_const),
            ("syn_strength", params.syn_strength),
        ] {
            if let Some(index) = values.iter().position(|v| !v.is_finite()) {
                return Err(GradedError::NonFinite { what, index });
            }
        }
        if let Some(index) = params.edge_syn_count.iter().position(|v| !v.is_finite()) {
            return Err(GradedError::NonFinite { what: "edge_syn_count", index });
        }

        // flyvis floors at torch.tensor(dt).float() in both precisions
        // (dynamics.py:209), and takes the reciprocal before multiplying.
        let floor = R::from_f32(dt as f32);
        let type_inv_tau: Vec<R> =
            params.time_const.iter().map(|&t| R::ONE / max(R::from_f32(t), floor)).collect();
        let bias: Vec<R> = net.node_cell_type().iter().map(|&t| R::from_f32(params.bias[t as usize])).collect();
        let inv_tau: Vec<R> = net.node_cell_type().iter().map(|&t| type_inv_tau[t as usize]).collect();

        // By target, keeping ascending edge order within each node.
        let n = net.num_nodes();
        let mut offsets = vec![0usize; n + 1];
        for &t in net.edge_target() {
            offsets[t as usize + 1] += 1;
        }
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut cursor = offsets.clone();
        let mut src = vec![0u32; net.num_edges()];
        let mut weight = vec![R::ZERO; net.num_edges()];
        for e in 0..net.num_edges() {
            let c = net.edge_conn_type()[e] as usize;
            // sign * syn_count * syn_strength, left to right (dynamics.py:165-167),
            // with syn_strength clamped at zero first (network.py:489-498).
            let w = R::from_f32(net.conn_sign()[c])
                * params.edge_syn_count[e]
                * max(R::from_f32(params.syn_strength[c]), R::ZERO);
            let t = net.edge_target()[e] as usize;
            src[cursor[t]] = net.edge_source()[e];
            weight[cursor[t]] = w;
            cursor[t] += 1;
        }

        Ok(Graded {
            dt,
            step_dt: R::from_f64(dt),
            state: bias.clone(),
            next: vec![R::ZERO; n],
            bias,
            inv_tau,
            offsets,
            src,
            weight,
        })
    }

    pub fn step(&mut self, input: &[R]) -> Result<(), GradedError> {
        let n = self.state.len();
        if input.len() != n {
            return Err(GradedError::Length { what: "input", expected: n, found: input.len() });
        }
        let Graded { step_dt, bias, inv_tau, offsets, src, weight, state, next, .. } = self;
        for (i, out) in next.iter_mut().enumerate() {
            let (lo, hi) = (offsets[i], offsets[i + 1]);
            let mut s = R::ZERO;
            for (&j, &w) in src[lo..hi].iter().zip(&weight[lo..hi]) {
                s = s + w * relu(state[j as usize]);
            }
            let v = state[i];
            // ((-v + bias) + sum) + x, then times 1/tau, then v + vel * dt
            // (dynamics.py:207-218, network.py:406).
            let vel = inv_tau[i] * (((-v + bias[i]) + s) + input[i]);
            *out = v + vel * *step_dt;
        }
        std::mem::swap(state, next);
        Ok(())
    }
```

- [ ] **Step 4: Run the tests and confirm they pass.**
  - Run: `cargo test -p semblance-sim && cargo clippy -p semblance-sim --all-targets -- -D warnings`
  - Expected: all pass, and clippy is clean. If clippy flags the
    `for i in 0..n` prefix-sum loop as `needless_range_loop`, rewrite it as
    `for i in 1..=n { offsets[i] += offsets[i - 1]; }`. Do not add an
    `#[allow]`.
    If clippy flags an expected value in a test (for example `identity_op` on
    `0.0 + …`), bind the parts to named variables instead. Keep every
    operation and its order: those expressions mirror `step` on purpose.

- [ ] **Step 5: Commit.**

```bash
git add crates/sim/src/graded.rs crates/sim/src/lib.rs
git commit -m "Add flyvis's graded dynamics to semblance-sim, in f32 and f64"
```

---

### Task 5: The reference reader

**Files:**
- Create: `crates/flyvis/src/reference.rs`
- Modify: `crates/flyvis/src/lib.rs`
- Modify: `crates/flyvis/src/test_support.rs` (add `reference_dir`,
  `load_reference_fixture`, `load_reference_bytes`)

**Interfaces:**
- Consumes: `crate::json::{parse, Json}` and its accessors `get`,
  `as_array`, `as_str`, `as_f64`, `as_i64` and `is_null`
- Produces:
  - in `semblance_flyvis::reference`: `Manifest`, `DtInfo`, `RunInfo`,
    `CentreNode`, `FileInfo`, `Block`, `BlockKey`, `Array`, `Dtype` and
    `ReferenceError`
  - `Manifest::parse(&str)`
  - `Manifest::file(&self, &str) -> Option<&FileInfo>`
  - `FileInfo::read_f32` and `FileInfo::read_f64`, each taking
    `(&self, bytes: &[u8], key: &BlockKey)` and returning
    `Result<Array<T>, ReferenceError>`
  - in `test_support`: `reference_dir() -> String`,
    `load_reference_fixture(&str) -> Option<String>` and
    `load_reference_bytes(&str) -> Option<Vec<u8>>`

- [ ] **Step 1: Add the fixture loaders.** Append to `test_support.rs`:

```rust
/// `data/flyvis-reference/`, from `tools/export_reference.py`.
pub(crate) fn reference_dir() -> String {
    format!("{}/../../data/flyvis-reference", env!("CARGO_MANIFEST_DIR"))
}

fn reference_missing(path: &str) {
    assert!(!fixtures_required(), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
    eprintln!(
        "skipping: {path} not found. Run ci/fetch-fixtures.sh, or \
         `uv run --project tools tools/export_reference.py`."
    );
}

/// A text file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_fixture(name: &str) -> Option<String> {
    let path = format!("{}/{name}", reference_dir());
    std::fs::read_to_string(&path).map_err(|_| reference_missing(&path)).ok()
}

/// A binary file from `data/flyvis-reference/`, or `None` if it is absent.
pub(crate) fn load_reference_bytes(name: &str) -> Option<Vec<u8>> {
    let path = format!("{}/{name}", reference_dir());
    std::fs::read(&path).map_err(|_| reference_missing(&path)).ok()
}
```

- [ ] **Step 2: Write the module's types, signatures and failing tests.**
  Create `crates/flyvis/src/reference.rs`. Every function body below marked
  `todo!()` is filled in at Step 4:

```rust
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
        }
    }
}

impl std::error::Error for ReferenceError {}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, ReferenceError> { todo!() }

    pub fn file(&self, name: &str) -> Option<&FileInfo> {
        self.files.iter().find(|f| f.name == name)
    }
}

impl FileInfo {
    pub fn read_f32(&self, bytes: &[u8], key: &BlockKey) -> Result<Array<f32>, ReferenceError> { todo!() }
    pub fn read_f64(&self, bytes: &[u8], key: &BlockKey) -> Result<Array<f64>, ReferenceError> { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{load_reference_fixture, reference_dir};

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
}
```

  In `lib.rs`, add `pub mod reference;` after `pub mod pretrained;`'s block.

- [ ] **Step 3: Run the tests and confirm they fail.**
  - Run: `cargo test -p semblance-flyvis reference`
  - Expected: panics with `not yet implemented`.

- [ ] **Step 4: Implement parsing and reading.** Replace the `todo!()` bodies,
  and add these private helpers above `impl Manifest`:

```rust
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
        self.blocks
            .iter()
            .find(|b| b.run == key.run && b.dt == key.dt && b.sequence == key.sequence)
            .ok_or_else(|| ReferenceError::NoBlock { file: self.name.clone() })
    }
}
```

  Then the public bodies:

```rust
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
```

```rust
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
```

  `require` compares `schema_version` through `f64::to_string`, and
  `1.0_f64.to_string()` is `"1"`. The slicing cannot go out of range:
  `check_layout` proved every block lies inside `self.bytes`, and `block`
  proved `bytes.len() == self.bytes`.

- [ ] **Step 5: Run the tests and confirm they pass.**
  - Run: `cargo test -p semblance-flyvis reference && cargo clippy -p semblance-flyvis --all-targets -- -D warnings`
  - Expected: all pass, the real-manifest test included.

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/reference.rs crates/flyvis/src/lib.rs crates/flyvis/src/test_support.rs
git commit -m "Read flyvis's reference manifest and its binary blocks"
```

---

### Task 6: The settling gates (epic task 4)

**Files:**
- Create: `crates/flyvis/src/settling_checks.rs`
- Modify: `crates/flyvis/Cargo.toml` (dev-dependency)
- Modify: `crates/flyvis/src/lib.rs`
- Modify: `Cargo.toml` (workspace), only if the 120 s rule in Step 5 triggers

**Interfaces:**
- Consumes everything from Tasks 1–5:
  - `Ensemble`, `edge_syn_groups` and `input_nodes`
  - `SYN_COUNT_TORCH_F32` and `SYN_COUNT_TORCH_F64`
  - `Manifest`, `FileInfo::read_f32` and `FileInfo::read_f64`, `BlockKey` and
    `Dtype`
  - `semblance_sim::graded::{Graded, GradedParams, Real}`
  - `test_support::{real_spec_and_network, load_pretrained_fixture, load_reference_fixture, load_reference_bytes}`
- Produces: tests only

- [ ] **Step 1: Add the dev-dependency.** Append to `crates/flyvis/Cargo.toml`:

```toml

[dev-dependencies]
semblance-sim.workspace = true
```

  Run `cargo build -p semblance-flyvis --tests`, then
  `grep -c '^name = ' Cargo.lock`. Expected: `8`.

- [ ] **Step 2: Write the gate.** Create
  `crates/flyvis/src/settling_checks.rs`:

```rust
//! The settling gates: epic 0.3.0's stage 1.
//!
//! flyvis settles its network from the bias state for 1.0 s of uniform grey
//! before every stimulus. These tests run `semblance_sim::graded` through
//! the same settling for every run in task 1's reference set -- models 000
//! and 049, dt 1/200 and 0.02, float32 and float64 -- and compare the
//! settled state (45,669 values) and the 65 centre neurons at every settling
//! step against flyvis's.
//!
//! Two gates over the same eight runs:
//!
//!   exact  synapse counts as flyvis's torch computed them
//!          (`syn_count_torch.rs`). Every value must equal flyvis's.
//!   libm   synapse counts from Rust's own `exp`. Every value must lie within
//!          a bound fixed in the spec before any comparison ran.
//!
//! Neither is to be relaxed. A failure is a discrepancy to find in flyvis's
//! source, recorded as an amendment to the spec:
//! docs/superpowers/specs/2026-10-28-graded-settling-design.md, section 6.
//!
//! Each test prints its result (visible with `--nocapture`). Read the libm
//! ratios, do not just watch them pass. All skip without fixtures;
//! `SEMBLANCE_REQUIRE_FIXTURES=1` makes a skip a failure (see `CLAUDE.md`).

use std::sync::OnceLock;

use semblance_connectome::TypedNetwork;
use semblance_sim::graded::{Graded, GradedParams, Real};

use crate::ensemble::Ensemble;
use crate::json;
use crate::pretrained::{CELL_TYPES, SYN_COUNT};
use crate::reference::{Array, BlockKey, Dtype, FileInfo, Manifest, ReferenceError};
use crate::syn_count_torch::{SYN_COUNT_TORCH_F32, SYN_COUNT_TORCH_F64};
use crate::test_support::{load_pretrained_fixture, load_reference_bytes, load_reference_fixture, real_spec_and_network};
use crate::wiring::{edge_syn_groups, input_nodes};

/// flyvis's own float32-vs-float64 gaps, PROVENANCE.json `f32_f64_gap`:
/// (model, dt, settled, settling_centre). The libm gate's f32 bound is twice
/// each (two f32 computations can sit on opposite sides of the exact value).
/// `pre_registered_bounds_match_provenance` pins these to the file.
const F32_GAPS: [(&str, &str, f64, f64); 4] = [
    ("000", "1/200", 1.4114366875261908e-06, 1.347661143569212e-06),
    ("049", "1/200", 6.1578304686449314e-06, 1.9363140433270587e-06),
    ("000", "1/50", 1.276739533651039e-06, 2.6109457484935206e-06),
    ("049", "1/50", 6.4440782598373936e-06, 1.1488733433040466e-05),
];

/// The libm gate's float64 bound, for both quantities.
const F64_BOUND: f64 = 1e-10;

const SETTLING_FILES: [&str; 4] =
    ["settled.f32.bin", "settled.f64.bin", "settling_centre.f32.bin", "settling_centre.f64.bin"];

struct Fixture {
    net: TypedNetwork,
    groups: Vec<u32>,
    /// Every photoreceptor node, ascending.
    inputs: Vec<u32>,
    ensemble: Ensemble,
    manifest: Manifest,
    files: Vec<(&'static str, Vec<u8>)>,
}

static FIXTURE: OnceLock<Option<Fixture>> = OnceLock::new();

/// Everything the gates read, built once. Every file is loaded (or skipped)
/// through `test_support` before anything is built from it.
fn fixture() -> Option<&'static Fixture> {
    FIXTURE.get_or_init(load).as_ref()
}

fn load() -> Option<Fixture> {
    let ensemble = load_pretrained_fixture("ensemble.csv")?;
    let manifest = load_reference_fixture("manifest.json")?;
    let mut files = Vec::new();
    for name in SETTLING_FILES {
        files.push((name, load_reference_bytes(name)?));
    }
    let (spec, net) = real_spec_and_network()?;
    let groups = edge_syn_groups(&net).expect("2,355 synapse-count groups");
    let mut inputs: Vec<u32> = input_nodes(&spec, &net).expect("input units are cell types").into_iter().flatten().collect();
    inputs.sort_unstable();
    Some(Fixture {
        groups,
        inputs,
        ensemble: Ensemble::parse(&ensemble).expect("ensemble.csv must parse"),
        manifest: Manifest::parse(&manifest).expect("manifest.json must parse"),
        files,
        net,
    })
}

fn cell_type(net: &TypedNetwork, node: usize) -> &str {
    &net.cell_types()[net.node_cell_type()[node] as usize]
}

/// The two precisions, each with its count tables and reference reader.
trait Precision: Real {
    const DTYPE: Dtype;
    const SUFFIX: &'static str;
    fn torch_count(group: usize) -> Self;
    fn libm_count(group: usize) -> Self;
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError>;
}

impl Precision for f32 {
    const DTYPE: Dtype = Dtype::F32;
    const SUFFIX: &'static str = "f32";
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F32[g] }
    fn libm_count(g: usize) -> Self { SYN_COUNT[g].exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f32(bytes, key)
    }
}

impl Precision for f64 {
    const DTYPE: Dtype = Dtype::F64;
    const SUFFIX: &'static str = "f64";
    fn torch_count(g: usize) -> Self { SYN_COUNT_TORCH_F64[g] }
    fn libm_count(g: usize) -> Self { (SYN_COUNT[g] as f64).exp() }
    fn read(file: &FileInfo, bytes: &[u8], key: &BlockKey) -> Result<Array<Self>, ReferenceError> {
        file.read_f64(bytes, key)
    }
}

/// Where the per-edge synapse counts come from.
#[derive(Clone, Copy)]
enum Counts {
    /// flyvis's own: torch's `exp`, from `syn_count_torch.rs`.
    Torch,
    /// Rust's `exp` of `SYN_COUNT`.
    Libm,
    /// The spec's integer counts, never exponentiated: a negative control.
    IntegerNSyn,
}

/// One settling run, Rust's and flyvis's values widened to f64.
struct Settled {
    rust: Vec<f64>,
    flyvis: Vec<f64>,
    rust_centre: Vec<f64>,
    flyvis_centre: Vec<f64>,
}

/// Settles `model`'s network at reference run `run`'s dt and precision, and
/// reads that run's reference.
fn settle<R: Precision>(f: &Fixture, run: u32, model: &str, counts: Counts) -> Settled {
    let info = &f.manifest.runs[run as usize];
    assert_eq!(info.index, run, "manifest runs must be listed in index order");
    assert_eq!(info.precision, R::DTYPE, "run {run}'s precision");
    let dt = f.manifest.dts.iter().find(|d| d.id == info.dt).expect("the run's dt is listed");
    let want_dt = match dt.id.as_str() {
        "1/200" => 1.0 / 200.0,
        "1/50" => 0.02,
        other => panic!("unexpected dt id {other}"),
    };
    assert_eq!(dt.dt, want_dt, "dt {}", dt.id);
    assert_eq!(dt.n_settle, (f.manifest.t_pre / dt.dt) as usize, "settling steps at dt {}", dt.id);

    let p = f.ensemble.model(model).expect("the model is in the ensemble");
    let count: Vec<R> = f
        .groups
        .iter()
        .enumerate()
        .map(|(e, &g)| match counts {
            Counts::Torch => R::torch_count(g as usize),
            Counts::Libm => R::libm_count(g as usize),
            Counts::IntegerNSyn => R::from_f32(f.net.edge_n_syn()[e]),
        })
        .collect();
    let params = GradedParams { bias: &p.bias, time_const: &p.time_const, syn_strength: &p.syn_strength, edge_syn_count: &count };
    let mut g = Graded::<R>::new(&f.net, &params, dt.dt).expect("the real network and parameters are valid");

    let mut input = vec![R::ZERO; f.net.num_nodes()];
    for &i in &f.inputs {
        input[i as usize] = R::from_f64(f.manifest.pre_stim_value);
    }
    let centre: Vec<usize> = f.manifest.centre_nodes.iter().map(|c| c.node as usize).collect();
    let mut rust_centre = Vec::with_capacity(dt.n_settle * centre.len());
    for step in 0..dt.n_settle {
        g.step(&input).expect("one input value per node");
        if let Some(i) = g.state().iter().position(|v| !v.is_finite()) {
            panic!("run {run}: node {i} ({}) is not finite after settle step {step}", cell_type(&f.net, i));
        }
        // settle step `step` is the state after step + 1 Euler steps
        rust_centre.extend(centre.iter().map(|&i| g.state()[i].to_f64()));
    }

    let key = BlockKey { run: Some(run), dt: &info.dt, sequence: None };
    let settled = read::<R>(f, &format!("settled.{}.bin", R::SUFFIX), &key);
    assert_eq!(settled.shape, [f.net.num_nodes()], "settled shape");
    let flyvis_centre = read::<R>(f, &format!("settling_centre.{}.bin", R::SUFFIX), &key);
    assert_eq!(flyvis_centre.shape, [dt.n_settle, centre.len()], "settling_centre shape");
    Settled {
        rust: g.state().iter().map(|v| v.to_f64()).collect(),
        flyvis: settled.data.iter().map(|v| v.to_f64()).collect(),
        rust_centre,
        flyvis_centre: flyvis_centre.data.iter().map(|v| v.to_f64()).collect(),
    }
}

fn read<R: Precision>(f: &Fixture, name: &str, key: &BlockKey) -> Array<R> {
    let file = f.manifest.file(name).unwrap_or_else(|| panic!("{name} is not in the manifest"));
    let bytes = &f.files.iter().find(|(n, _)| *n == name).expect("loaded by fixture()").1;
    R::read(file, bytes, key).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The largest absolute error, where, and how many values are not equal.
struct Diff {
    max: f64,
    at: usize,
    differ: usize,
}

fn diff(rust: &[f64], flyvis: &[f64]) -> Diff {
    assert_eq!(rust.len(), flyvis.len());
    let mut d = Diff { max: 0.0, at: 0, differ: 0 };
    for (i, (a, b)) in rust.iter().zip(flyvis).enumerate() {
        let e = (a - b).abs();
        d.differ += usize::from(a != b);
        if e > d.max {
            d.max = e;
            d.at = i;
        }
    }
    d
}

fn exact<R: Precision>(run: u32) {
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let s = settle::<R>(f, run, &info.model, Counts::Torch);
    let (settled, centre) = (diff(&s.rust, &s.flyvis), diff(&s.rust_centre, &s.flyvis_centre));
    let n_centre = f.manifest.centre_nodes.len();
    eprintln!(
        "exact run {run} ({}, dt {}, {}): settled {} of {} differ; settling centre {} of {} differ",
        info.model, info.dt, R::SUFFIX, settled.differ, s.rust.len(), centre.differ, s.rust_centre.len()
    );
    if settled.differ + centre.differ > 0 {
        let first = s
            .rust_centre
            .iter()
            .zip(&s.flyvis_centre)
            .position(|(a, b)| a != b)
            .map(|i| format!("settle step {} ({})", i / n_centre, f.manifest.centre_nodes[i % n_centre].cell_type));
        panic!(
            "run {run} is not bit-equal to flyvis: {} settled and {} settling-centre values differ; \
             first difference at {}; largest settled error {:e} at node {} ({})",
            settled.differ,
            centre.differ,
            first.unwrap_or_else(|| "the settled state only".into()),
            settled.max,
            settled.at,
            cell_type(&f.net, settled.at)
        );
    }
}

fn bounds(model: &str, dt: &str, precision: Dtype) -> (f64, f64) {
    match precision {
        Dtype::F64 => (F64_BOUND, F64_BOUND),
        Dtype::F32 => {
            let &(_, _, s, c) = F32_GAPS.iter().find(|g| g.0 == model && g.1 == dt).expect("a gap for every f32 run");
            (2.0 * s, 2.0 * c)
        }
    }
}

fn libm<R: Precision>(run: u32) {
    let Some(f) = fixture() else { return };
    let info = &f.manifest.runs[run as usize];
    let (bound_settled, bound_centre) = bounds(&info.model, &info.dt, info.precision);
    let s = settle::<R>(f, run, &info.model, Counts::Libm);
    let (settled, centre) = (diff(&s.rust, &s.flyvis), diff(&s.rust_centre, &s.flyvis_centre));
    let n_centre = f.manifest.centre_nodes.len();

    let mut per_type = vec![0.0f64; f.net.cell_types().len()];
    for (i, (a, b)) in s.rust.iter().zip(&s.flyvis).enumerate() {
        let t = f.net.node_cell_type()[i] as usize;
        per_type[t] = per_type[t].max((a - b).abs());
    }
    let mut worst: Vec<usize> = (0..per_type.len()).collect();
    worst.sort_by(|&a, &b| per_type[b].total_cmp(&per_type[a]));
    let worst: Vec<String> = worst[..3].iter().map(|&t| format!("{} {:.2e}", f.net.cell_types()[t], per_type[t])).collect();

    eprintln!(
        "libm run {run} ({}, dt {}, {}): settled max {:.3e} at node {} ({}), {:.3} of bound; \
         settling centre max {:.3e} at step {} ({}), {:.3} of bound; worst types {}; settled bit-equal {} of {}",
        info.model, info.dt, R::SUFFIX,
        settled.max, settled.at, cell_type(&f.net, settled.at), settled.max / bound_settled,
        centre.max, centre.at / n_centre, f.manifest.centre_nodes[centre.at % n_centre].cell_type, centre.max / bound_centre,
        worst.join(", "), s.rust.len() - settled.differ, s.rust.len()
    );
    assert!(
        settled.max <= bound_settled && centre.max <= bound_centre,
        "run {run}: Rust's exp exceeds the pre-registered bound: settled {:e} (bound {:e}), \
         settling centre {:e} (bound {:e})",
        settled.max, bound_settled, centre.max, bound_centre
    );
}

macro_rules! settling_runs {
    ($($exact:ident, $libm:ident, $run:expr, $t:ty;)*) => {
        $(
            #[test]
            fn $exact() { exact::<$t>($run) }
            #[test]
            fn $libm() { libm::<$t>($run) }
        )*
    };
}

settling_runs! {
    settling_exact_run_0_000_1_200_f32, settling_libm_run_0_000_1_200_f32, 0, f32;
    settling_exact_run_1_000_1_200_f64, settling_libm_run_1_000_1_200_f64, 1, f64;
    settling_exact_run_2_000_1_50_f32, settling_libm_run_2_000_1_50_f32, 2, f32;
    settling_exact_run_3_000_1_50_f64, settling_libm_run_3_000_1_50_f64, 3, f64;
    settling_exact_run_4_049_1_200_f32, settling_libm_run_4_049_1_200_f32, 4, f32;
    settling_exact_run_5_049_1_200_f64, settling_libm_run_5_049_1_200_f64, 5, f64;
    settling_exact_run_6_049_1_50_f32, settling_libm_run_6_049_1_50_f32, 6, f32;
    settling_exact_run_7_049_1_50_f64, settling_libm_run_7_049_1_50_f64, 7, f64;
}

/// The manifest's runs are exactly the eight the tests above are named for,
/// and its centre nodes are the expanded network's, one per cell type.
#[test]
fn reference_runs_are_the_eight_expected() {
    let Some(f) = fixture() else { return };
    let want = [
        (0u32, "000", "1/200", Dtype::F32), (1, "000", "1/200", Dtype::F64),
        (2, "000", "1/50", Dtype::F32), (3, "000", "1/50", Dtype::F64),
        (4, "049", "1/200", Dtype::F32), (5, "049", "1/200", Dtype::F64),
        (6, "049", "1/50", Dtype::F32), (7, "049", "1/50", Dtype::F64),
    ];
    let got: Vec<_> = f.manifest.runs.iter().map(|r| (r.index, r.model.as_str(), r.dt.as_str(), r.precision)).collect();
    assert_eq!(got, want);
    assert_eq!(f.manifest.centre_nodes.len(), CELL_TYPES.len());
    for (k, c) in f.manifest.centre_nodes.iter().enumerate() {
        assert_eq!(cell_type(&f.net, c.node as usize), c.cell_type, "centre node {k}");
        assert_eq!(c.cell_type, CELL_TYPES[k], "centre node {k} in CELL_TYPES order");
    }
}

/// The gaps above are PROVENANCE.json's, bit for bit.
#[test]
fn pre_registered_bounds_match_provenance() {
    let Some(text) = load_reference_fixture("PROVENANCE.json") else { return };
    let root = json::parse(&text).expect("PROVENANCE.json must parse");
    let gaps = root.get("f32_f64_gap").expect("PROVENANCE.json has f32_f64_gap");
    for &(model, dt, settled, centre) in &F32_GAPS {
        let key = format!("{model} {dt}");
        let g = gaps.get(&key).unwrap_or_else(|| panic!("no gap for {key}"));
        for (name, want) in [("settled", settled), ("settling_centre", centre)] {
            let got = g.get(name).and_then(|v| v.as_f64()).unwrap_or_else(|| panic!("{key} {name}"));
            assert_eq!(got.to_bits(), want.to_bits(), "{key} {name}: PROVENANCE {got:e}, gate {want:e}");
        }
    }
}

/// A control: the wrong model's parameters, against run 3's reference
/// (model 000, dt 0.02, f64), must miss by far more than any bound. A gate
/// that compared nothing to nothing would pass this.
#[test]
fn the_gate_tells_models_apart() {
    let Some(f) = fixture() else { return };
    let s = settle::<f64>(f, 3, "049", Counts::Torch);
    let d = diff(&s.rust, &s.flyvis);
    assert!(d.max > 1e-3, "model 049 against model 000's reference: max {:e}", d.max);
}

/// A control at the scale the libm bound exists to resolve: the spec's
/// integer counts in place of exp(ln count) must exceed the f64 bound.
/// Measured 4.8e-7 to 6.7e-6 across runs during the spec's review.
#[test]
fn the_gate_resolves_integer_counts() {
    let Some(f) = fixture() else { return };
    let s = settle::<f64>(f, 3, "000", Counts::IntegerNSyn);
    let d = diff(&s.rust, &s.flyvis);
    assert!(d.max > F64_BOUND, "integer counts: settled max {:e}", d.max);
}
```

  In `lib.rs`, add `#[cfg(test)] mod settling_checks;` after
  `pub mod reference;`.

- [ ] **Step 3: Run the gates.**
  - Run:
    `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test -p semblance-flyvis settling -- --nocapture 2>&1 | tee /tmp/settling.log; grep -E '^(exact|libm) run|test result' /tmp/settling.log`
  - Expected:
    - 20 tests pass: 16 runs, 2 supporting, 2 controls
    - eight `exact run …: settled 0 of 45669 differ; settling centre 0 of …
      differ` lines
    - eight `libm run …` lines with every ratio ≤ 1
  - **If any test fails, stop.** Do not change a bound, a constant or `==`.
    Diagnose with the epic's ladder: magnitude, which cell types, which dt,
    whether it grows with step count. Read the flyvis source cited in the
    spec's G-table, and report.

- [ ] **Step 4: Run the whole workspace.**
  - Run: `SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo run --quiet -- roundtrip`
  - Expected:
    - 0 failed
    - clippy clean
    - both roundtrip distances `0.000000`

- [ ] **Step 5: Measure the time and apply the 120 s rule.**
  - Run: `time cargo test -p semblance-flyvis settling`
  - Record the wall-clock time.
  - **If it is over 120 s,** add to the workspace `Cargo.toml`:

```toml
# The settling gates run 1.5M-edge Euler steps in debug builds; the generic
# integrator is monomorphised in semblance-flyvis's tests, so both crates
# need it. Spec section "Testing", the 120 s rule.
[profile.dev.package.semblance-sim]
opt-level = 3

[profile.dev.package.semblance-flyvis]
opt-level = 3
```

    Re-run Step 4 and the timing, and record both times for the PR
    description.
  - **If it is under 120 s,** change nothing.

- [ ] **Step 6: Commit.**

```bash
git add crates/flyvis/src/settling_checks.rs crates/flyvis/src/lib.rs crates/flyvis/Cargo.toml Cargo.lock
# and Cargo.toml if Step 5 changed it
git commit -m "Gate the graded integrator on flyvis's settling, exactly and with Rust's exp"
```

---

### Task 7: Documentation, counts, and the PR

**Files:**
- Modify: `CLAUDE.md`, `README.md`

**Interfaces:**
- Consumes: the measured results of Task 6

- [ ] **Step 1: Measure the test and skip counts.** The skip count is the
  failure count with `data/` absent and every fixture required. Use
  `--no-fail-fast`, so every test binary runs:

```bash
cargo test --workspace 2>&1 | grep 'test result' | awk '{p+=$4; f+=$6} END {print "passed", p, "failed", f}'
mv data data.measure-off
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace --no-fail-fast 2>&1 | grep 'test result' | awk '{f+=$6} END {print "fixture-gated", f}'
mv data.measure-off data
ls data   # confirm it is back
```

  Call the totals `TOTAL` and `SKIPS`.

- [ ] **Step 2: Update `CLAUDE.md`.**
  - `# 204 tests, 0 failures` → `# TOTAL tests, 0 failures`.
  - `26 of those tests skip when …` → `SKIPS of those tests skip when …`.
    Extend the list of directories with `data/flyvis-reference/` and
    `data/flyvis-pretrained/ensemble.csv`. Keep the sentence's provenance
    clauses.
  - Replace "`data/flyvis-pretrained/ensemble.csv`; no test reads either
    yet." with "`data/flyvis-pretrained/ensemble.csv`, which the settling
    gates read."
  - Replace the paragraph starting **`crates/sim` is on the way out.** with:

```markdown
**The spiking path in `crates/sim` is on the way out.** `Lif`, `Engine` and
the traits around them model spiking neurons, and their signatures encode that
assumption (`step` returns `&[bool]`, `observe` takes `&[bool]`, `Behaviour`
counts spikes). The fly visual system is largely non-spiking — graded
membrane potential, not spikes — which `semblance_sim::graded` implements,
beside the spiking path and touching none of its traits. Replacing the
spiking trait surface is planned work, so do not invest in extending `Lif`.
```

  - After the paragraph ending "…that tolerated difference turned out to be
    what failed the geometry gate at bar height 80.", insert:

```markdown
**The settling gates are exact where exactness exists.**
`crates/flyvis/src/settling_checks.rs` settles the graded integrator for all
eight reference runs and runs two gates over them. The exact gate uses
`crates/flyvis/src/syn_count_torch.rs` (generated by
`tools/export_syn_count.py`: torch's own `exp` of every synapse count). It
requires every settled value, and every centre-neuron value at every settling
step, to equal flyvis's. The libm gate uses Rust's `exp`, and holds each run to
a bound fixed in the spec before it ran: 1e-10 in f64, and twice flyvis's own
f32/f64 gap in f32. torch's `exp` and the C library's differ in the last bit
on about 105 of the 2,355 counts. That is the only reason the second gate has
a bound. If the exact gate fails, find the discrepancy in flyvis's source; do
not relax it. Both gates print their results; read the libm ratios.
```

  - In the **Zero external dependencies** paragraph about `tools/`, after "It
    exists because flyvis's trained parameters ship as PyTorch checkpoints.",
    add: "`crates/flyvis/src/pretrained.rs` and `syn_count_torch.rs` are its
    output, compiled in: regenerate them only by rerunning their scripts."

- [ ] **Step 3: Update `README.md`.**
  - `**Without fixture data, 26 flyvis parity tests skip and still report
    green.**` → `SKIPS`.
  - Replace the `sim` row of the crate table with:

```markdown
| `sim` | graded dynamics (`semblance_sim::graded`): flyvis's passive point neurons, explicit Euler, f32 and f64, reproducing flyvis's settling bit for bit. Also an LIF spiking path, a placeholder to be replaced | sensory encoding + motor decoding |
```

  - In the `flyvis` row:
    - Add to the built column, before "all gated against flyvis's own
      output": "the 50-model trained ensemble, flyvis's reference traces,".
    - Replace "graded dynamics that reproduce flyvis's traces (planned for
      0.3.0)" with "the input mapping and full-trace gate under stimulus
      (0.3.0, in progress)".

- [ ] **Step 4: Verify everything once more.**

```bash
cargo build --workspace
SEMBLANCE_REQUIRE_FIXTURES=1 cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --quiet -- roundtrip
grep -c '^name = ' Cargo.lock   # 8
git status --short              # nothing under data/ or docs/project-outline/
```

- [ ] **Step 5: Commit, push, and open the PR.**

```bash
git add CLAUDE.md README.md
git commit -m "Document the graded integrator and the settling gates"
git push -u origin graded-settling
```

  Open the PR with `gh pr create --base main --head graded-settling`. Title:
  "Graded dynamics, gated exactly on flyvis's settling". The body contains:
  - what landed (Tasks 1–6, one line each)
  - the 16 `exact run` / `libm run` lines from `/tmp/settling.log`
  - the controls' results
  - the gate timing, and whether the 120 s rule triggered
  - `TOTAL` and `SKIPS`
  - "Merge waits for the owner."
  - the closing line `🤖 Generated with [Claude Code](https://claude.com/claude-code)`

- [ ] **Step 6: Wait for CI, then stop.** CI runs on the self-hosted runner,
  which must be running (`~/actions-runner/semblance/start.sh`).
  - Watch: `gh pr checks --watch`.
  - Report the result to the owner. **Do not merge.**
  - Record the achieved results in the epic's local task files (tasks 2, 3
    and 4) and in its README. These are not committed.
