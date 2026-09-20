//! Typed view of the published flyvis connectome spec.
//!
//! Mirrors the file's schema literally. No tiling, no normalisation, no
//! interpretation -- keeping this layer faithful is what makes it checkable
//! against the source.

use crate::json::{Json, JsonError, parse};

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

/// Distinguishes a JSON-level failure (the input was not even valid JSON)
/// from a schema-level one (valid JSON that does not match the connectome
/// spec: a missing key, a wrong type, an unrecognised pattern name).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpecErrorKind {
    /// The input failed to parse as JSON, at this byte offset.
    Json { offset: usize },
    /// The input parsed as JSON but violates the connectome spec's schema.
    Schema,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpecError {
    pub kind: SpecErrorKind,
    /// Path to the offending element, e.g. `edges[417].offsets[3]`.
    pub path: String,
    pub reason: String,
}

impl SpecError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        SpecError { kind: SpecErrorKind::Schema, path: path.into(), reason: reason.into() }
    }
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for SpecError {}

impl From<JsonError> for SpecError {
    fn from(err: JsonError) -> Self {
        SpecError {
            kind: SpecErrorKind::Json { offset: err.offset },
            path: "<root>".to_string(),
            reason: format!("{:?} at byte offset {}", err.kind, err.offset),
        }
    }
}

/// Builds the dotted/indexed path of a child field, e.g. `child_path("edges[3]",
/// "offsets")` -> `"edges[3].offsets"`, or just `"offsets"` when `parent` is
/// empty (the document root).
fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() { key.to_string() } else { format!("{parent}.{key}") }
}

fn field<'a>(obj: &'a Json, key: &str, obj_path: &str) -> Result<&'a Json, SpecError> {
    obj.get(key).ok_or_else(|| SpecError::new(child_path(obj_path, key), "missing field"))
}

fn require_object<'a>(v: &'a Json, path: &str) -> Result<&'a Json, SpecError> {
    match v {
        Json::Object(_) => Ok(v),
        _ => Err(SpecError::new(path, "expected an object")),
    }
}

fn require_array<'a>(v: &'a Json, path: &str) -> Result<&'a [Json], SpecError> {
    v.as_array().ok_or_else(|| SpecError::new(path, "expected an array"))
}

fn require_str<'a>(v: &'a Json, path: &str) -> Result<&'a str, SpecError> {
    v.as_str().ok_or_else(|| SpecError::new(path, "expected a string"))
}

fn require_bool(v: &Json, path: &str) -> Result<bool, SpecError> {
    v.as_bool().ok_or_else(|| SpecError::new(path, "expected a bool"))
}

fn require_f64(v: &Json, path: &str) -> Result<f64, SpecError> {
    v.as_f64().ok_or_else(|| SpecError::new(path, "expected a number"))
}

/// Requires a finite, integral number (per `Json::as_i64`) that fits in `i32`.
fn require_i32(v: &Json, path: &str) -> Result<i32, SpecError> {
    let n = v.as_i64().ok_or_else(|| SpecError::new(path, "expected an integral number"))?;
    i32::try_from(n).map_err(|_| SpecError::new(path, "integer out of i32 range"))
}

/// `null` maps to `None`; anything else must be a number.
fn require_optional_f32(v: &Json, path: &str) -> Result<Option<f32>, SpecError> {
    if v.is_null() { Ok(None) } else { Ok(Some(require_f64(v, path)? as f32)) }
}

fn parse_string_array(v: &Json, path: &str) -> Result<Vec<String>, SpecError> {
    let arr = require_array(v, path)?;
    arr.iter()
        .enumerate()
        .map(|(i, s)| require_str(s, &format!("{path}[{i}]")).map(str::to_string))
        .collect()
}

/// `pattern` is a two-element array: `["stride", [u, v]]`, `["tile", n]`, or
/// `["single", ...]`. An unrecognised first element is a `SpecError`, not a
/// silent default -- flyvis's own loader supports all three even though the
/// published file only ever uses `stride`.
fn parse_pattern(v: &Json, path: &str) -> Result<Pattern, SpecError> {
    let arr = require_array(v, path)?;
    if arr.len() != 2 {
        return Err(SpecError::new(
            path,
            format!("expected a 2-element pattern array, found {}", arr.len()),
        ));
    }
    let name = require_str(&arr[0], &format!("{path}[0]"))?;
    let arg_path = format!("{path}[1]");
    match name {
        "stride" => {
            let uv = require_array(&arr[1], &arg_path)?;
            if uv.len() != 2 {
                return Err(SpecError::new(
                    &arg_path,
                    format!("expected [u, v], found {} elements", uv.len()),
                ));
            }
            let u = require_i32(&uv[0], &format!("{arg_path}[0]"))?;
            let v = require_i32(&uv[1], &format!("{arg_path}[1]"))?;
            Ok(Pattern::Stride { u, v })
        }
        "tile" => Ok(Pattern::Tile(require_i32(&arr[1], &arg_path)?)),
        "single" => Ok(Pattern::Single),
        other => {
            Err(SpecError::new(format!("{path}[0]"), format!("unrecognised pattern name {other:?}")))
        }
    }
}

fn parse_node(v: &Json, path: &str) -> Result<NodeSpec, SpecError> {
    require_object(v, path)?;
    let name = require_str(field(v, "name", path)?, &child_path(path, "name"))?.to_string();
    let pattern = parse_pattern(field(v, "pattern", path)?, &child_path(path, "pattern"))?;
    let activation =
        require_str(field(v, "activation", path)?, &child_path(path, "activation"))?.to_string();
    let bias = require_f64(field(v, "bias", path)?, &child_path(path, "bias"))? as f32;
    let bias_fixed = require_bool(field(v, "bias_fixed", path)?, &child_path(path, "bias_fixed"))?;
    let time_constant = require_optional_f32(
        field(v, "time_constant", path)?,
        &child_path(path, "time_constant"),
    )?;
    let time_constant_fixed = require_bool(
        field(v, "time_constant_fixed", path)?,
        &child_path(path, "time_constant_fixed"),
    )?;
    Ok(NodeSpec {
        name,
        pattern,
        activation,
        bias,
        bias_fixed,
        time_constant,
        time_constant_fixed,
    })
}

/// One `offsets` entry: `[[du, dv], n_syn]`. `du`/`dv` must be integral;
/// `n_syn` is a float and is not rounded.
fn parse_offset(v: &Json, path: &str) -> Result<Offset, SpecError> {
    let arr = require_array(v, path)?;
    if arr.len() != 2 {
        return Err(SpecError::new(
            path,
            format!("expected [[du, dv], n_syn], found {} elements", arr.len()),
        ));
    }
    let duv_path = format!("{path}[0]");
    let duv = require_array(&arr[0], &duv_path)?;
    if duv.len() != 2 {
        return Err(SpecError::new(
            &duv_path,
            format!("expected [du, dv], found {} elements", duv.len()),
        ));
    }
    let du = require_i32(&duv[0], &format!("{duv_path}[0]"))?;
    let dv = require_i32(&duv[1], &format!("{duv_path}[1]"))?;
    let n_syn = require_f64(&arr[1], &format!("{path}[1]"))? as f32;
    Ok(Offset { du, dv, n_syn })
}

fn parse_edge(v: &Json, path: &str) -> Result<EdgeSpec, SpecError> {
    require_object(v, path)?;
    let src = require_str(field(v, "src", path)?, &child_path(path, "src"))?.to_string();
    let tar = require_str(field(v, "tar", path)?, &child_path(path, "tar"))?.to_string();

    let offsets_path = child_path(path, "offsets");
    let offsets_json = require_array(field(v, "offsets", path)?, &offsets_path)?;
    let offsets = offsets_json
        .iter()
        .enumerate()
        .map(|(i, o)| parse_offset(o, &format!("{offsets_path}[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;

    let alpha = require_i32(field(v, "alpha", path)?, &child_path(path, "alpha"))?;
    let alpha_fixed = require_bool(field(v, "alpha_fixed", path)?, &child_path(path, "alpha_fixed"))?;
    let alpha_references = parse_string_array(
        field(v, "alpha_references", path)?,
        &child_path(path, "alpha_references"),
    )?;
    let time_constant = require_optional_f32(
        field(v, "time_constant", path)?,
        &child_path(path, "time_constant"),
    )?;
    let time_constant_fixed = require_bool(
        field(v, "time_constant_fixed", path)?,
        &child_path(path, "time_constant_fixed"),
    )?;
    let lambda_mult =
        require_f64(field(v, "lambda_mult", path)?, &child_path(path, "lambda_mult"))? as f32;
    let edge_type =
        require_str(field(v, "edge_type", path)?, &child_path(path, "edge_type"))?.to_string();

    Ok(EdgeSpec {
        src,
        tar,
        offsets,
        alpha,
        alpha_fixed,
        alpha_references,
        time_constant,
        time_constant_fixed,
        lambda_mult,
        edge_type,
    })
}

impl ConnectomeSpec {
    pub fn from_json_str(input: &str) -> Result<Self, SpecError> {
        let root = parse(input)?;
        require_object(&root, "<root>")?;

        let nodes_json = require_array(field(&root, "nodes", "")?, "nodes")?;
        let nodes = nodes_json
            .iter()
            .enumerate()
            .map(|(i, n)| parse_node(n, &format!("nodes[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;

        let edges_json = require_array(field(&root, "edges", "")?, "edges")?;
        let edges = edges_json
            .iter()
            .enumerate()
            .map(|(i, e)| parse_edge(e, &format!("edges[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;

        // `receptors` is always empty in the published file; ignore it
        // entirely rather than modelling it.
        let input_units = parse_string_array(field(&root, "input_units", "")?, "input_units")?;
        let output_units = parse_string_array(field(&root, "output_units", "")?, "output_units")?;

        Ok(ConnectomeSpec { nodes, edges, input_units, output_units })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::load_real_spec;

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
        assert!((n_syn - 8087.2522).abs() < 0.01, "summed synapse counts: {n_syn}");

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
    fn parses_tile_and_single_patterns() {
        let tile = parse_pattern(&parse(r#"["tile", 3]"#).unwrap(), "pattern").unwrap();
        assert_eq!(tile, Pattern::Tile(3));

        let single = parse_pattern(&parse(r#"["single", null]"#).unwrap(), "pattern").unwrap();
        assert_eq!(single, Pattern::Single);
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
    fn spec_error_kind_distinguishes_json_from_schema_failures() {
        let json_err = ConnectomeSpec::from_json_str("not json").unwrap_err();
        assert!(matches!(json_err.kind, SpecErrorKind::Json { .. }), "{json_err:?}");

        let schema_err = ConnectomeSpec::from_json_str("{}").unwrap_err();
        assert_eq!(schema_err.kind, SpecErrorKind::Schema);

        let wrong_type_root = ConnectomeSpec::from_json_str("[]").unwrap_err();
        assert_eq!(wrong_type_root.kind, SpecErrorKind::Schema);
    }

    #[test]
    fn spec_error_implements_display_and_std_error() {
        let err = ConnectomeSpec::from_json_str("{}").unwrap_err();
        let rendered = err.to_string();
        assert!(rendered.contains(&err.path));
        assert!(rendered.contains(&err.reason));

        fn assert_is_std_error<E: std::error::Error>(_: &E) {}
        assert_is_std_error(&err);

        // Must be usable behind `?` as a boxed std error, e.g. from `main()`.
        fn returns_boxed_error() -> Result<(), Box<dyn std::error::Error>> {
            ConnectomeSpec::from_json_str("{}")?;
            Ok(())
        }
        assert!(returns_boxed_error().is_err());
    }

    #[test]
    fn loads_a_minimal_handwritten_spec() {
        let src = r#"{
          "nodes": [{"name":"R1","pattern":["stride",[1,1]],"activation":"relu",
                     "bias":3.5,"bias_fixed":false,"time_constant":0.05,"time_constant_fixed":false}],
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
        assert_eq!(s.nodes[0].time_constant, Some(0.05));
        assert_eq!(s.edges[0].offsets.len(), 1);
        assert_eq!(s.edges[0].offsets[0].du, 0);
        assert_eq!(s.edges[0].offsets[0].dv, 0);
        assert_eq!(s.edges[0].offsets[0].n_syn, 40.0);
        assert_eq!(s.edges[0].alpha, -1);
        assert_eq!(s.edges[0].alpha_references, vec!["Hardie1989".to_string()]);
    }
}
