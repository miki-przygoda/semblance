# semblance-flyvis Ingest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `semblance-flyvis` crate that reads the published flyvis connectome JSON into typed Rust structs, and carries flyvis's curated ground truth as compiled constants — with zero external dependencies.

**Architecture:** Four layers, bottom-up. A private minimal JSON parser (`json.rs`) produces a `Json` value tree. A public `spec.rs` walks that tree into typed structs mirroring the file's schema exactly, with no interpretation. A public `groundtruth.rs` holds ~700 transcribed literals. `lib.rs` re-exports. Nothing tiles the lattice — that is a later epic task.

**Tech Stack:** Rust, edition 2024, no external crates.

**Spec:** `docs/superpowers/specs/2026-09-17-flyvis-ingest-design.md`

## Global Constraints

- Rust **1.85** or newer; edition **2024** (inherited via `.workspace = true`).
- **No external dependencies.** `crates/flyvis/Cargo.toml` has an empty `[dependencies]`. After this plan `Cargo.lock` has **8** packages, all `semblance*`.
- The JSON parser is **`pub(crate)`** — never exported, never published as its own crate.
- Malformed JSON must **return an error, never panic and never silently truncate**. Errors carry a byte offset.
- Recursion depth limit: **64**.
- Tests that need `data/flyvis-supplementary/` must **skip with a clear message when it is absent**, never fail — that directory is gitignored and a fresh clone lacks it.
- Commit style: plain imperative subject, NO `feat:`/`fix:`/`chore:` prefixes.
- Run all verification from the workspace root, `/Users/miki_przygoda/Projects/GitHub-Projects/semblance`.

## File Structure

| File | Responsibility |
|---|---|
| `crates/flyvis/Cargo.toml` | package manifest, metadata inherited, zero deps |
| `crates/flyvis/src/lib.rs` | crate docs, module wiring, re-exports |
| `crates/flyvis/src/json.rs` | `pub(crate)` — `Json`, `parse`, accessors, errors |
| `crates/flyvis/src/spec.rs` | `pub` — `ConnectomeSpec` and friends, loader |
| `crates/flyvis/src/groundtruth.rs` | `pub` — transcribed constants |
| `Cargo.toml` | add `crates/flyvis` member + workspace dependency entry |

## Reference data

The real file lives at
`data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json`
(gitignored). If absent, download:

```
https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip
```

and extract `Lappalainen_et_al_connectome_avg_filters.json` from it.

These values are established and are the acceptance criteria throughout:

| | |
|---|---|
| nodes | 65 |
| edges | 605 |
| distinct `(src, tar)` | 605 |
| total offset entries | 2,140 |
| summed `n_syn` | 8,087 |
| `input_units` | 8 |
| `output_units` | 34 |
| edges with ≥3 offsets | 243 |
| edges with <3 offsets | 362 |
| activations present | `{"relu"}` |
| edge types present | `{"chem"}` |
| CT1 node names | `CT1(Lo1)`, `CT1(M10)` |

---

### Task 1: Crate skeleton and the JSON core

**Files:**
- Create: `crates/flyvis/Cargo.toml`, `crates/flyvis/src/lib.rs`, `crates/flyvis/src/json.rs`
- Modify: `Cargo.toml` (members, workspace dependencies)

**Interfaces:**
- Consumes: nothing.
- Produces: `pub(crate) enum Json`, `pub(crate) fn parse(&str) -> Result<Json, JsonError>`, and the accessors `get`/`as_array`/`as_str`/`as_f64`/`as_i64`/`as_bool`/`is_null`. Tasks 2 and 3 build on these exact names.

- [ ] **Step 1: Register the crate**

In the root `Cargo.toml`, add `"crates/flyvis",` to `members` (after `crates/rng`), and add to `[workspace.dependencies]`:

```toml
semblance-flyvis = { path = "crates/flyvis", version = "0.1.0" }
```

- [ ] **Step 2: Create the manifest**

`crates/flyvis/Cargo.toml`:

```toml
[package]
name = "semblance-flyvis"
version.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true
authors.workspace = true
repository.workspace = true
homepage.workspace = true
keywords.workspace = true
categories.workspace = true
readme = "../../README.md"
description = "Reads the published flyvis connectome spec and carries its curated ground truth, with no external dependencies."

[lib]
name = "semblance_flyvis"
path = "src/lib.rs"

[dependencies]
```

The empty `[dependencies]` is deliberate — it documents that this crate depends on nothing.

- [ ] **Step 3: Write the failing tests**

Create `crates/flyvis/src/json.rs` containing ONLY this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars() {
        assert!(matches!(parse("null").unwrap(), Json::Null));
        assert!(matches!(parse("true").unwrap(), Json::Bool(true)));
        assert!(matches!(parse("false").unwrap(), Json::Bool(false)));
        assert_eq!(parse("42").unwrap().as_i64(), Some(42));
        assert_eq!(parse("-1").unwrap().as_i64(), Some(-1));
        assert_eq!(parse("3.5").unwrap().as_f64(), Some(3.5));
        assert_eq!(parse("\"hi\"").unwrap().as_str(), Some("hi"));
    }

    #[test]
    fn as_i64_rejects_non_integral() {
        assert_eq!(parse("3.5").unwrap().as_i64(), None);
        assert_eq!(parse("3.0").unwrap().as_i64(), Some(3));
    }

    #[test]
    fn parses_nested_structures() {
        let j = parse(r#"{"a": [1, 2, {"b": null}], "c": true}"#).unwrap();
        assert_eq!(j.get("a").and_then(|x| x.as_array()).map(|a| a.len()), Some(3));
        assert_eq!(j.get("a").unwrap().as_array().unwrap()[0].as_i64(), Some(1));
        assert!(j.get("a").unwrap().as_array().unwrap()[2].get("b").unwrap().is_null());
        assert_eq!(j.get("c").and_then(|x| x.as_bool()), Some(true));
        assert!(j.get("missing").is_none());
    }

    #[test]
    fn ignores_insignificant_whitespace() {
        let j = parse(" {\n\t\"k\" :\r [ 1 , 2 ] }\n ").unwrap();
        assert_eq!(j.get("k").unwrap().as_array().unwrap().len(), 2);
    }

    #[test]
    fn preserves_object_key_order() {
        let j = parse(r#"{"z": 1, "a": 2, "m": 3}"#).unwrap();
        match j {
            Json::Object(pairs) => {
                let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
                assert_eq!(keys, vec!["z", "a", "m"]);
            }
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn empty_containers() {
        assert_eq!(parse("[]").unwrap().as_array().unwrap().len(), 0);
        assert!(matches!(parse("{}").unwrap(), Json::Object(ref p) if p.is_empty()));
    }
}
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis`
Expected: FAIL to compile — `cannot find type 'Json' in this scope`.

- [ ] **Step 5: Implement the parser core**

Write the implementation ABOVE the test module in `crates/flyvis/src/json.rs`. It must provide:

```rust
//! Minimal JSON reader for the published flyvis connectome spec.
//!
//! Deliberately small: the file is 188 KB, read once, and this crate carries no
//! external dependencies. Not a general-purpose library — no streaming, no
//! borrowed values, no derive.

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JsonError {
    pub(crate) kind: JsonErrorKind,
    pub(crate) offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JsonErrorKind {
    UnexpectedChar(char),
    UnexpectedEnd,
    InvalidNumber,
    InvalidEscape,
    InvalidUnicodeEscape,
    TrailingData,
    DepthLimitExceeded,
}

pub(crate) fn parse(input: &str) -> Result<Json, JsonError> { /* ... */ }
```

Implementation requirements for this task:

- Recursive descent over `&[u8]` or `char_indices`, tracking a byte offset.
- Whitespace between tokens: space, tab, CR, LF.
- `Object` is a `Vec<(String, Json)>` preserving document order — do not sort, do not deduplicate.
- Numbers: optional `-`, integer part, optional fractional part. Parse into `f64`.
- Strings: content between quotes. **Escapes are task 2** — for now, a backslash may return `JsonErrorKind::InvalidEscape`.
- After the top-level value, only whitespace may follow; anything else is `TrailingData`.
- Accessors exactly as named in the Interfaces block. `as_i64` returns `None` unless the value is finite and `x.fract() == 0.0` and within `i64` range.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis`
Expected: PASS, 6 tests.

- [ ] **Step 7: Verify it reads the real file**

Add this test to the module:

```rust
    /// The real file is gitignored; skip rather than fail when it is absent.
    fn load_real_spec() -> Option<String> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"
        );
        match std::fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(_) => {
                eprintln!(
                    "skipping: {path} not found. Download \
                     https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
                     and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/"
                );
                None
            }
        }
    }

    #[test]
    fn parses_the_real_connectome_spec() {
        let Some(raw) = load_real_spec() else { return };
        let j = parse(&raw).expect("real spec must parse");
        assert_eq!(j.get("nodes").unwrap().as_array().unwrap().len(), 65);
        assert_eq!(j.get("edges").unwrap().as_array().unwrap().len(), 605);
        assert_eq!(j.get("input_units").unwrap().as_array().unwrap().len(), 8);
        assert_eq!(j.get("output_units").unwrap().as_array().unwrap().len(), 34);
    }
```

Run: `cargo test -p semblance-flyvis`
Expected: PASS, 7 tests. If the data is present the counts must match exactly; if absent the test prints the skip message and passes.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock crates/flyvis
git commit -m "Add semblance-flyvis with a minimal JSON reader

Parses the published flyvis connectome spec without pulling in
serde_json, which would cost the workspace's zero-dependency property.
Escapes, exponents and the depth limit follow."
```

---

### Task 2: Parser robustness

**Files:**
- Modify: `crates/flyvis/src/json.rs`

**Interfaces:**
- Consumes: everything from task 1.
- Produces: no new public names. `parse` gains escape, exponent and depth handling, and every `JsonErrorKind` becomes reachable.

- [ ] **Step 1: Write the failing tests**

Add to the test module in `crates/flyvis/src/json.rs`:

```rust
    #[test]
    fn parses_string_escapes() {
        assert_eq!(parse(r#""a\"b""#).unwrap().as_str(), Some("a\"b"));
        assert_eq!(parse(r#""a\\b""#).unwrap().as_str(), Some("a\\b"));
        assert_eq!(parse(r#""a\/b""#).unwrap().as_str(), Some("a/b"));
        assert_eq!(parse(r#""a\nb""#).unwrap().as_str(), Some("a\nb"));
        assert_eq!(parse(r#""a\tb""#).unwrap().as_str(), Some("a\tb"));
        assert_eq!(parse(r#""a\rb""#).unwrap().as_str(), Some("a\rb"));
        assert_eq!(parse(r#""a\bb""#).unwrap().as_str(), Some("a\u{8}b"));
        assert_eq!(parse(r#""a\fb""#).unwrap().as_str(), Some("a\u{c}b"));
    }

    #[test]
    fn parses_unicode_escapes() {
        // NOTE: the inputs below must contain literal backslash-u sequences.
        assert_eq!(parse(r#""\u0041""#).unwrap().as_str(), Some("A"));
        assert_eq!(parse(r#""\u00e9""#).unwrap().as_str(), Some("é"));
        // a well-formed surrogate PAIR combines into one char (U+1F600)
        assert_eq!(parse(r#""\ud83d\ude00""#).unwrap().as_str(), Some("😀"));
        // escapes and literal characters must agree
        assert_eq!(parse(r#""\u0041""#).unwrap(), parse(r#""A""#).unwrap());
    }

    #[test]
    fn rejects_malformed_surrogate_pairs() {
        use JsonErrorKind::*;
        // lone high surrogate
        assert_eq!(parse(r#""\ud83d""#).unwrap_err().kind, InvalidUnicodeEscape);
        // lone low surrogate
        assert_eq!(parse(r#""\ude00""#).unwrap_err().kind, InvalidUnicodeEscape);
        // high surrogate followed by a non-surrogate escape
        assert_eq!(parse(r#""\ud83d\u0041""#).unwrap_err().kind, InvalidUnicodeEscape);
        // high surrogate followed by an ordinary character
        assert_eq!(parse(r#""\ud83dA""#).unwrap_err().kind, InvalidUnicodeEscape);
    }

    #[test]
    fn parses_exponents() {
        assert_eq!(parse("1e3").unwrap().as_f64(), Some(1000.0));
        assert_eq!(parse("1E3").unwrap().as_f64(), Some(1000.0));
        assert_eq!(parse("1.5e-2").unwrap().as_f64(), Some(0.015));
        assert_eq!(parse("2e+2").unwrap().as_f64(), Some(200.0));
    }

    #[test]
    fn rejects_malformed_input_with_offsets() {
        use JsonErrorKind::*;
        let cases: &[(&str, JsonErrorKind)] = &[
            ("", UnexpectedEnd),
            ("  ", UnexpectedEnd),
            (r#""unterminated"#, UnexpectedEnd),
            ("[1, 2,]", UnexpectedChar(']')),
            ("{\"a\": 1,}", UnexpectedChar('}')),
            ("{\"a\" 1}", UnexpectedChar('1')),
            ("nul", UnexpectedEnd),
            ("tru", UnexpectedEnd),
            ("[1] extra", TrailingData),
            ("01", TrailingData),
            (r#""bad \x escape""#, InvalidEscape),
            (r#""\uZZZZ""#, InvalidUnicodeEscape),
            (r#""\ud83d""#, InvalidUnicodeEscape),
            ("-", InvalidNumber),
            ("1.", InvalidNumber),
            ("1e", InvalidNumber),
        ];
        for (input, want) in cases {
            let err = parse(input).unwrap_err();
            assert_eq!(&err.kind, want, "input {input:?} produced {err:?}");
            assert!(err.offset <= input.len(), "offset out of range for {input:?}");
        }
    }

    #[test]
    fn rejects_deep_nesting_without_overflowing_the_stack() {
        let deep = format!("{}1{}", "[".repeat(200), "]".repeat(200));
        let err = parse(&deep).unwrap_err();
        assert_eq!(err.kind, JsonErrorKind::DepthLimitExceeded);
    }

    #[test]
    fn accepts_nesting_up_to_the_limit() {
        let ok = format!("{}1{}", "[".repeat(64), "]".repeat(64));
        assert!(parse(&ok).is_ok());
    }
```

Note on `"01"`: the top-level number `0` parses, then `1` is trailing data — that is correct JSON behaviour and the expected error is `TrailingData`, not `InvalidNumber`.

Note on `"\ud83d"` alone: a high surrogate with no following low surrogate is invalid.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis`
Expected: FAIL. The escape, unicode and exponent tests fail; several malformed cases produce the wrong `JsonErrorKind`.

- [ ] **Step 3: Implement**

Extend `crates/flyvis/src/json.rs`:

- **String escapes:** `\" \\ \/ \b \f \n \r \t` and `\uXXXX`. For `\uXXXX` in the high-surrogate range `D800..=DBFF`, require an immediately following `\uXXXX` low surrogate in `DC00..=DFFF` and combine them; anything else is `InvalidUnicodeEscape`. A lone low surrogate is also `InvalidUnicodeEscape`. Unknown escape characters are `InvalidEscape`.
- **Exponents:** after the optional fractional part, accept `e`/`E`, optional `+`/`-`, and one or more digits. A missing exponent digit is `InvalidNumber`.
- **Number validation:** a `-` with no digits, a `.` with no following digit, and an `e` with no following digits are all `InvalidNumber`.
- **Depth limit:** track nesting depth. The limit is **64 nesting levels**: a document nested 64 deep parses, and the 65th level returns `DepthLimitExceeded`. (Concretely, `"[".repeat(64)` is fine; `"[".repeat(65)` is not.) This guards a recursive-descent parser against stack overflow on adversarial input; the real file uses depth 6.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis`
Expected: PASS. (Test count drifts if earlier fix rounds added tests -- assert on green, not on a total.)

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src/json.rs
git commit -m "Complete the JSON reader: escapes, exponents, depth limit

The flyvis spec uses none of these -- it is pure ASCII with no escapes
and no exponent notation -- but a reader that silently mis-parses a file
someone drops in later is worse than one slightly larger than today's
input demands."
```

---

### Task 3: The typed connectome spec

**Files:**
- Create: `crates/flyvis/src/spec.rs`
- Modify: `crates/flyvis/src/lib.rs`

**Interfaces:**
- Consumes: `json::{parse, Json, JsonError}` from tasks 1–2.
- Produces: `pub` `Pattern`, `NodeSpec`, `Offset`, `EdgeSpec`, `ConnectomeSpec`, `SpecError`, and `ConnectomeSpec::from_json_str`.

- [ ] **Step 1: Write the failing tests**

Create the test module at the bottom of `crates/flyvis/src/spec.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn load_real_spec() -> Option<String> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../data/flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"
        );
        match std::fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(_) => {
                eprintln!(
                    "skipping: {path} not found. Download \
                     https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip \
                     and extract Lappalainen_et_al_connectome_avg_filters.json into data/flyvis-supplementary/"
                );
                None
            }
        }
    }

    #[test]
    fn real_spec_matches_the_published_figures() {
        let Some(raw) = load_real_spec() else { return };
        let s = ConnectomeSpec::from_json_str(&raw).expect("real spec must load");

        assert_eq!(s.nodes.len(), 65, "cell types");
        assert_eq!(s.edges.len(), 605, "connection types");
        assert_eq!(s.input_units.len(), 8);
        assert_eq!(s.output_units.len(), 34);

        let pairs: std::collections::BTreeSet<_> =
            s.edges.iter().map(|e| (e.src.as_str(), e.tar.as_str())).collect();
        assert_eq!(pairs.len(), 605, "distinct (src, tar) pairs");

        let offsets: usize = s.edges.iter().map(|e| e.offsets.len()).sum();
        assert_eq!(offsets, 2140, "total offset entries");

        let n_syn: f64 = s.edges.iter().flat_map(|e| &e.offsets).map(|o| o.n_syn as f64).sum();
        assert_eq!(n_syn.round() as i64, 8087, "summed synapse counts");

        assert_eq!(s.edges.iter().filter(|e| e.offsets.len() >= 3).count(), 243);
        assert_eq!(s.edges.iter().filter(|e| e.offsets.len() < 3).count(), 362);
    }

    #[test]
    fn real_spec_field_values_are_faithful() {
        let Some(raw) = load_real_spec() else { return };
        let s = ConnectomeSpec::from_json_str(&raw).unwrap();

        assert!(s.nodes.iter().all(|n| matches!(n.pattern, Pattern::Stride { .. })));
        assert!(s.nodes.iter().all(|n| n.activation == "relu"));
        assert!(s.edges.iter().all(|e| e.edge_type == "chem"));

        let ct1: Vec<&str> = s.nodes.iter().map(|n| n.name.as_str())
            .filter(|n| n.contains("CT1")).collect();
        assert_eq!(ct1, vec!["CT1(Lo1)", "CT1(M10)"]);

        // alpha is the synapse SIGN, not a weight
        assert!(s.edges.iter().all(|e| e.alpha == 1 || e.alpha == -1));

        // some time constants are genuinely null
        assert!(s.nodes.iter().any(|n| n.time_constant.is_none()));
    }

    #[test]
    fn rejects_schema_violations() {
        assert!(ConnectomeSpec::from_json_str("{}").is_err(), "missing nodes/edges");
        assert!(ConnectomeSpec::from_json_str(r#"{"nodes": 1, "edges": []}"#).is_err());
        assert!(ConnectomeSpec::from_json_str("not json").is_err());
        let bad_pattern = r#"{"nodes":[{"name":"X","pattern":["spiral",[1,1]],"activation":"relu",
            "bias":0.0,"bias_fixed":false,"time_constant":null,"time_constant_fixed":false}],
            "edges":[],"input_units":[],"output_units":[]}"#;
        assert!(ConnectomeSpec::from_json_str(bad_pattern).is_err(), "unknown pattern name");
    }

    #[test]
    fn loads_a_minimal_handwritten_spec() {
        let src = r#"{
          "nodes": [{"name":"R1","pattern":["stride",[1,1]],"activation":"relu",
                     "bias":3.5,"bias_fixed":false,"time_constant":null,"time_constant_fixed":false}],
          "edges": [{"src":"R1","tar":"L1","offsets":[[[0,0],40]],"alpha":-1,"alpha_fixed":true,
                     "alpha_references":["Hardie1989"],"time_constant":null,
                     "time_constant_fixed":false,"lambda_mult":5.86,"edge_type":"chem"}],
          "input_units": ["R1"],
          "output_units": ["L1"]
        }"#;
        let s = ConnectomeSpec::from_json_str(src).unwrap();
        assert_eq!(s.nodes[0].name, "R1");
        assert!(matches!(s.nodes[0].pattern, Pattern::Stride { u: 1, v: 1 }));
        assert_eq!(s.nodes[0].bias, 3.5);
        assert_eq!(s.nodes[0].time_constant, None);
        assert_eq!(s.edges[0].offsets.len(), 1);
        assert_eq!(s.edges[0].offsets[0].du, 0);
        assert_eq!(s.edges[0].offsets[0].dv, 0);
        assert_eq!(s.edges[0].offsets[0].n_syn, 40.0);
        assert_eq!(s.edges[0].alpha, -1);
        assert_eq!(s.edges[0].alpha_references, vec!["Hardie1989".to_string()]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p semblance-flyvis spec`
Expected: FAIL to compile — `ConnectomeSpec` not found.

- [ ] **Step 3: Implement the types and loader**

Write above the test module in `crates/flyvis/src/spec.rs`:

```rust
//! Typed view of the published flyvis connectome spec.
//!
//! Mirrors the file's schema literally. No tiling, no normalisation, no
//! interpretation -- keeping this layer faithful is what makes it checkable
//! against the source.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pattern {
    Stride { u: i32, v: i32 },
    Tile(i32),
    Single,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeSpec {
    pub name: String,
    pub pattern: Pattern,
    pub activation: String,
    pub bias: f32,
    pub bias_fixed: bool,
    pub time_constant: Option<f32>,
    pub time_constant_fixed: bool,
}

/// One entry of a connection type's filter: a hex-lattice offset and the
/// synapse count at it.
///
/// `n_syn` is a float, not an integer: flyvis's hull-filling introduces
/// fractional counts for edges reconstructed across gaps in the EM data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    pub du: i32,
    pub dv: i32,
    pub n_syn: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EdgeSpec {
    pub src: String,
    pub tar: String,
    pub offsets: Vec<Offset>,
    /// The synapse SIGN (+1 or -1), fixed from literature. Not a weight.
    pub alpha: i32,
    pub alpha_fixed: bool,
    pub alpha_references: Vec<String>,
    pub time_constant: Option<f32>,
    pub time_constant_fixed: bool,
    pub lambda_mult: f32,
    pub edge_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectomeSpec {
    pub nodes: Vec<NodeSpec>,
    pub edges: Vec<EdgeSpec>,
    pub input_units: Vec<String>,
    pub output_units: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecError {
    /// Path to the offending element, e.g. `edges[417].offsets[3]`.
    pub path: String,
    pub reason: String,
}

impl ConnectomeSpec {
    pub fn from_json_str(input: &str) -> Result<Self, SpecError> { /* ... */ }
}
```

Requirements:

- `pattern` in the file is a two-element array: `["stride", [u, v]]`, `["tile", n]`, `["single", ...]`. Map to the `Pattern` variants; an unrecognised name is a `SpecError`.
- `time_constant` may be JSON `null` — map to `None`, not to `0.0`.
- `offsets` entries are `[[du, dv], n_syn]`. `du`/`dv` must be integral; `n_syn` may be fractional.
- The file has a `receptors` key which is always empty. **Ignore it; do not model it.**
- Every missing key, wrong type, or non-integral integer is a `SpecError` whose `path` names the element.
- A `JsonError` from the parser converts into a `SpecError` with the byte offset in `reason`.

Also wire the modules in `crates/flyvis/src/lib.rs`:

```rust
//! Reads the published flyvis connectome spec and carries its curated ground
//! truth.
//!
//! flyvis (Lappalainen et al., Nature 2024) distributes its connectome as
//! generative rules -- 65 cell types and 605 connection types tiled over a
//! hexagonal lattice -- rather than as an edge list. This crate reads that file
//! faithfully; expanding it into a graph belongs elsewhere.

mod json;
pub mod groundtruth;
pub mod spec;

pub use spec::{ConnectomeSpec, EdgeSpec, NodeSpec, Offset, Pattern, SpecError};
```

`groundtruth` does not exist until task 4; for this task declare only `mod json;` and `pub mod spec;` plus the `pub use`, and add `pub mod groundtruth;` in task 4.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p semblance-flyvis`
Expected: PASS, 17 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src
git commit -m "Add the typed flyvis connectome spec

Mirrors the published schema literally -- 65 cell types, 605 connection
types, hex offsets with fractional synapse counts. Tiling the lattice is
deliberately not here.

Verified against the published figures: 2,140 offset entries summing to
8,087 synapses, 243 edge types wide enough for hull filling."
```

---

### Task 4: The ground truth

**Files:**
- Create: `crates/flyvis/src/groundtruth.rs`
- Modify: `crates/flyvis/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub enum Polarity` and the ten `pub const` tables named below.

**Do not hand-transcribe.** ~700 literals copied by hand will contain an error that compiles fine and passes every count test. Generate the file mechanically, then verify.

- [ ] **Step 1: Fetch the upstream source**

```bash
gh api "repos/TuragaLab/flyvis/contents/flyvis/utils/groundtruth_utils.py?ref=9a516686d158aa568e2035bf65ec15646fb020a8" \
  --jq '.content' | base64 -d > /tmp/groundtruth_utils.py
wc -l /tmp/groundtruth_utils.py   # expect 630
```

That SHA is the last commit to touch the file (2024-11-08); pinning it makes the transcription reproducible.

- [ ] **Step 2: Generate `groundtruth.rs`**

```bash
python3 - <<'PY' > crates/flyvis/src/groundtruth.rs
import ast
tree = ast.parse(open('/tmp/groundtruth_utils.py').read())
g = {}
for n in tree.body:
    if isinstance(n, ast.Assign) and isinstance(n.targets[0], ast.Name):
        name = n.targets[0].id
        try:
            g[name] = ast.literal_eval(n.value); continue
        except Exception:
            pass
        if isinstance(n.value, ast.Call) and getattr(n.value.func, 'id', None) == 'dict':
            g[name] = {kw.arg: ast.literal_eval(kw.value) for kw in n.value.keywords}

P = {1: 'Polarity::On', -1: 'Polarity::Off', 0: 'Polarity::Unknown'}
out = []
w = out.append
w('//! flyvis\'s curated ground truth, transcribed from published literature.')
w('//!')
w('//! Source: `TuragaLab/flyvis`, `flyvis/utils/groundtruth_utils.py`')
w('//! Transcribed at commit `9a516686d158aa568e2035bf65ec15646fb020a8` (2024-11-08).')
w('//! Repo HEAD at transcription: `92b3845cc426dd309a1a0e1b3890156c42e14021`, release v1.2.0.')
w('//!')
w('//! Generated mechanically from that file, not hand-copied. Upstream spells')
w('//! `INSUFFICIENT_DATA` as `unsufficient_data`; the spelling is corrected here.')
w('')
w('#[derive(Clone, Copy, Debug, PartialEq, Eq)]')
w('pub enum Polarity {')
w('    /// Depolarizes to light increment.')
w('    On,')
w('    /// Depolarizes to light decrement.')
w('    Off,')
w('    /// No established contrast selectivity.')
w('    Unknown,')
w('}')
w('')
w('/// Contrast selectivity per cell type. 65 entries; 32 have known polarity.')
w('pub const POLARITY: &[(&str, Polarity)] = &[')
for k, v in g['polarity'].items():
    w(f'    ("{k}", {P[v]}),')
w('];')
w('')
w('/// Preferred direction in degrees, for the direction-selective T4/T5 subtypes.')
w('pub const PREFERRED_DIRECTIONS: &[(&str, u16)] = &[')
for k, v in g['preferred_directions'].items():
    w(f'    ("{k}", {v}),')
w('];')
w('')
w('/// 12-point direction tuning curves, from Maisak et al. 2013 Fig. 3g,h.')
w('pub const TUNING_CURVES: &[(&str, [f32; 12])] = &[')
for k, v in g['tuning_curves'].items():
    assert len(v) == 12, f'{k} has {len(v)} points, expected 12'
    vals = ', '.join(f'{x}' for x in v)
    w(f'    ("{k}", [{vals}]),')
w('];')
w('')
for const, key, doc in [
    ('MOTION_TUNING',     'motion_tuning',     'Cell types with established motion tuning.'),
    ('NO_MOTION_TUNING',  'no_motion_tuning',  'Cell types established as NOT motion tuned.'),
    ('ON_PATHWAY',        'on_pathway',        'Cell types assigned to the ON pathway.'),
    ('OFF_PATHWAY',       'off_pathway',       'Cell types assigned to the OFF pathway.'),
    ('INSUFFICIENT_DATA', 'unsufficient_data', 'Cell types with insufficient published data; exclude from metrics.'),
    ('NOISY_DATA',        'noisy_data',        'Cell types whose published measurements are noisy; treat with care.'),
]:
    w(f'/// {doc}')
    w(f'pub const {const}: &[&str] = &[')
    for x in g[key]:
        w(f'    "{x}",')
    w('];')
    w('')
w('/// Cell types with an established direction-selectivity answer, either way.')
w('pub const KNOWN_DSI_TYPES: &[&str] = &[')
for x in [*g['no_motion_tuning'], *g['motion_tuning']]:
    w(f'    "{x}",')
w('];')
print('\n'.join(out))
PY
wc -l crates/flyvis/src/groundtruth.rs
```

- [ ] **Step 3: Write the verification tests**

Append to `crates/flyvis/src/groundtruth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sizes_match_upstream() {
        assert_eq!(POLARITY.len(), 65);
        assert_eq!(PREFERRED_DIRECTIONS.len(), 8);
        assert_eq!(TUNING_CURVES.len(), 8);
        assert_eq!(MOTION_TUNING.len(), 8);
        assert_eq!(NO_MOTION_TUNING.len(), 10);
        assert_eq!(KNOWN_DSI_TYPES.len(), 18);
        assert_eq!(INSUFFICIENT_DATA.len(), 9);
        assert_eq!(NOISY_DATA.len(), 4);
        assert_eq!(ON_PATHWAY.len(), 12);
        assert_eq!(OFF_PATHWAY.len(), 12);
    }

    #[test]
    fn polarity_distribution_matches_the_paper() {
        let on = POLARITY.iter().filter(|(_, p)| *p == Polarity::On).count();
        let off = POLARITY.iter().filter(|(_, p)| *p == Polarity::Off).count();
        let unknown = POLARITY.iter().filter(|(_, p)| *p == Polarity::Unknown).count();
        assert_eq!(on, 18);
        assert_eq!(off, 14);
        assert_eq!(unknown, 33);
        assert_eq!(on + off, 32, "the paper's 32 contrast-selective cell types");
    }

    /// Catches a typo'd cell type, which no count test would notice and which
    /// would silently drop a type from the metric later.
    #[test]
    fn every_named_cell_type_exists_in_polarity() {
        let known: std::collections::BTreeSet<&str> =
            POLARITY.iter().map(|(n, _)| *n).collect();
        let mut named: Vec<&str> = Vec::new();
        named.extend(PREFERRED_DIRECTIONS.iter().map(|(n, _)| *n));
        named.extend(TUNING_CURVES.iter().map(|(n, _)| *n));
        named.extend(MOTION_TUNING.iter().copied());
        named.extend(NO_MOTION_TUNING.iter().copied());
        named.extend(KNOWN_DSI_TYPES.iter().copied());
        named.extend(INSUFFICIENT_DATA.iter().copied());
        named.extend(NOISY_DATA.iter().copied());
        named.extend(ON_PATHWAY.iter().copied());
        named.extend(OFF_PATHWAY.iter().copied());
        for n in named {
            assert!(known.contains(n), "{n} is not a cell type in POLARITY");
        }
    }

    #[test]
    fn polarity_names_are_unique() {
        let set: std::collections::BTreeSet<&str> = POLARITY.iter().map(|(n, _)| *n).collect();
        assert_eq!(set.len(), POLARITY.len(), "duplicate cell type in POLARITY");
    }

    #[test]
    fn t4_t5_preferred_directions_are_cardinal() {
        for (name, deg) in PREFERRED_DIRECTIONS {
            assert!(matches!(deg, 0 | 90 | 180 | 270), "{name} has direction {deg}");
        }
        assert_eq!(PREFERRED_DIRECTIONS.len(), 8);
    }

    #[test]
    fn tuning_curves_are_normalised_twelve_point() {
        for (name, curve) in TUNING_CURVES {
            assert_eq!(curve.len(), 12, "{name}");
            assert!(curve.iter().all(|v| (0.0..=1.0).contains(v)), "{name} out of [0,1]");
        }
    }
}
```

- [ ] **Step 4: Wire the module and run**

Add `pub mod groundtruth;` to `crates/flyvis/src/lib.rs`.

Run: `cargo test -p semblance-flyvis`
Expected: PASS, 23 tests.

Run: `cargo fmt -p semblance-flyvis` — the generated file will not be idiomatically formatted.

- [ ] **Step 5: Commit**

```bash
git add crates/flyvis/src
git commit -m "Add flyvis's curated ground truth as compiled constants

Generated mechanically from groundtruth_utils.py at upstream commit
9a51668, not hand-copied -- ~700 literals transcribed by hand would
contain an error that compiles fine and passes every count test.

32 cell types have established contrast selectivity, matching the
paper. A cross-check asserts every cell type named in any table exists
in POLARITY, which is what catches a typo that no count would."
```

---

## Verification checklist

Run from the workspace root after task 4:

- [ ] `cargo build --workspace` — clean
- [ ] `cargo test --workspace` — 26 pre-existing + 23 new = **49 tests, 0 failures**
- [ ] `cargo clippy --workspace --all-targets` — no NEW warnings beyond the 5 pre-existing (4 `needless_range_loop` in `sim`, 1 manual char comparison in `connectome`)
- [ ] `grep -c '^name = ' Cargo.lock` — **8**, all `semblance*`
- [ ] `cargo run --quiet -- roundtrip` — both distances still exactly `0.000000`
- [ ] `cargo publish --dry-run -p semblance-flyvis` — passes (needs a clean tree)
- [ ] Temporarily move `data/flyvis-supplementary/` aside and re-run `cargo test --workspace` — data-dependent tests must **skip with the download message, not fail**. Move it back afterwards.

## Out of scope

Deliberately NOT part of this plan, and not to be added by an implementer:

- **Tiling the lattice.** Turning the spec into 45,669 neurons is task 5/6 of the epic, and doing it here would blur the boundary that makes the spec layer checkable.
- Dynamics (`PPNeuronIGRSynapses`) — that is 0.3.0.
- Stimulus generation — epic task 7.
- Any change to `connectome`, `sim`, `codec`, `fidelity`, `evo`, or `app`.
- Publishing the JSON parser as its own crate.
- Version bumps or crates.io publishing.
- The 5 pre-existing clippy warnings.
