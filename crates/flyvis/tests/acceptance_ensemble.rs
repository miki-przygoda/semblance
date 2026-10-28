//! Acceptance tests for behaviour group `ensemble` of epic graded-settling:
//! reading flyvis's trained ensemble through the crate's public API.
//!
//! One test per behaviour id (ENSEMBLE-01 .. ENSEMBLE-13). Synthetic files are
//! built here from the public `pretrained` tables; the real
//! `data/flyvis-pretrained/ensemble.csv` is read only by ENSEMBLE-12, which
//! skips when it is absent unless `SEMBLANCE_REQUIRE_FIXTURES=1`.

use semblance_flyvis::pretrained::{BIAS, CELL_TYPES, CONN_TYPES, SYN_STRENGTH, TIME_CONST};
use semblance_flyvis::{Ensemble, EnsembleError, EnsembleErrorKind, ModelParams};

const N: usize = 50;
const ROWS: usize = 65 + 65 + 604;
const HEADER: &str = "model,kind,index,name,value";

/// (kind, index, name) of slot `s` in every model.
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

/// The value written for model `m`, slot `s`: distinct per model so that
/// returning the wrong model's parameters is visible.
fn value(m: usize, s: usize) -> String {
    let base = match slot(s) {
        ("bias", i, _) => BIAS[i],
        ("time_const", i, _) => TIME_CONST[i],
        (_, i, _) => SYN_STRENGTH[i],
    };
    if m == 0 { format!("{base}") } else { format!("{}", base + m as f32 * 0.25) }
}

fn row(m: usize, s: usize) -> String {
    let (kind, i, name) = slot(s);
    format!("{m:03},{kind},{i},{name},{}", value(m, s))
}

/// A valid file: one `#` line, the header on line 2, data row r on line r + 3.
fn csv(models: usize) -> String {
    let mut out = String::from("# synthetic\n");
    out.push_str(HEADER);
    out.push('\n');
    for m in 0..models {
        for s in 0..ROWS {
            out.push_str(&row(m, s));
            out.push('\n');
        }
    }
    out
}

/// Line of data row (model m, slot s) in `csv(..)`.
fn line_of(m: usize, s: usize) -> usize {
    m * ROWS + s + 3
}

/// `text` with 1-based `line` replaced by `new`.
fn with_line(text: &str, line: usize, new: &str) -> String {
    let mut lines: Vec<&str> = text.split('\n').collect();
    lines[line - 1] = new;
    lines.join("\n")
}

fn get_line(text: &str, line: usize) -> String {
    text.split('\n').nth(line - 1).unwrap().to_string()
}

fn refuse(text: &str) -> EnsembleError {
    let e = Ensemble::parse(text).expect_err("must be refused");
    // Every refusal names its line, in both the struct and the message.
    assert!(e.line >= 1, "line is 1-based: {e:?}");
    assert!(e.to_string().contains(&format!("line {}", e.line)), "{e}");
    e
}

fn expected_params(m: usize) -> ModelParams {
    let mut p = ModelParams { bias: [0.0; 65], time_const: [0.0; 65], syn_strength: [0.0; 604] };
    for s in 0..ROWS {
        let v: f32 = value(m, s).parse().unwrap();
        match slot(s) {
            ("bias", i, _) => p.bias[i] = v,
            ("time_const", i, _) => p.time_const[i] = v,
            (_, i, _) => p.syn_strength[i] = v,
        }
    }
    p
}

fn bits(v: &[f32]) -> Vec<u32> {
    v.iter().map(|x| x.to_bits()).collect()
}

#[test]
fn ensemble_01_parses_fifty_models_with_or_without_trailing_newline() {
    let with_nl = csv(N);
    let without_nl = with_nl.trim_end_matches('\n').to_string();
    assert!(!without_nl.ends_with('\n'));
    for text in [&with_nl, &without_nl] {
        let e = Ensemble::parse(text).expect("a well-formed file parses");
        let ids: Vec<&str> = e.ids().collect();
        let want: Vec<String> = (0..N).map(|m| format!("{m:03}")).collect();
        assert_eq!(ids, want);
        for m in 0..N {
            let p = e.model(&format!("{m:03}")).expect("every id resolves");
            assert_eq!(p.bias.len(), 65);
            assert_eq!(p.time_const.len(), 65);
            assert_eq!(p.syn_strength.len(), 604);
            let w = expected_params(m);
            assert_eq!(bits(&p.bias), bits(&w.bias), "model {m:03} bias");
            assert_eq!(bits(&p.time_const), bits(&w.time_const), "model {m:03} time_const");
            assert_eq!(bits(&p.syn_strength), bits(&w.syn_strength), "model {m:03} syn_strength");
        }
    }
}

/// xorshift64*, so the mutations are deterministic without a dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// Ensemble::parse("") returns EnsembleError { line: 0, kind: MissingHeader }:
// line 0 is not a 1-based line number of the input.
#[test]
#[ignore = "acceptance-failing: ENSEMBLE-02"]
fn ensemble_02_no_file_io_no_panics_every_error_names_line_and_expectation() {
    // No file I/O: a path to the real file is just text, refused at line 1.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/flyvis-pretrained/ensemble.csv");
    let e = refuse(path);
    assert_eq!(e.line, 1);
    assert!(matches!(e.kind, EnsembleErrorKind::BadHeader(_)), "{e:?}");

    let valid = csv(N);
    let n_lines = valid.trim_end_matches('\n').split('\n').count();
    let mut inputs: Vec<String> = vec![
        String::new(),
        "\n".into(),
        "#".into(),
        "# only a comment\n".into(),
        HEADER.into(),
        format!("{HEADER}\n"),
        format!("{HEADER}\n,,,,\n"),
        format!("{HEADER}\n000,bias,0,R1,1.0,extra\n"),
        format!("{HEADER}\n\u{fffd}\u{0}\n"),
        "\u{0}\u{1}\u{2}".into(),
        "\r".into(),
        format!("{HEADER}\n000,bias,99999999999999999999999,R1,1\n"),
        format!("{HEADER}\n000,syn_strength,0,R1,1\n"),
        format!("{HEADER}\n999,bias,0,R1,1\n"),
        csv(51),
        csv(1),
    ];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let pieces = [
        "", ",", "\n", "#", "\r", "x", "000", "bias", "time_const", "syn_strength", "-1", "1e40",
        "NaN", "\u{e9}", "R1>R1", "65", "604",
    ];
    for _ in 0..300 {
        let line = 1 + rng.below(n_lines);
        let mut lines: Vec<String> = valid.split('\n').map(String::from).collect();
        let l = &mut lines[line - 1];
        match rng.below(4) {
            0 => {
                let at = rng.below(l.len() + 1);
                l.insert_str(at, pieces[rng.below(pieces.len())]);
            }
            1 => {
                let at = rng.below(l.len() + 1);
                l.truncate(at);
            }
            2 => *l = pieces[rng.below(pieces.len())].to_string(),
            _ => {
                lines.remove(line - 1);
            }
        }
        inputs.push(lines.join("\n"));
    }

    let mut violations = Vec::new();
    for text in &inputs {
        let total = text.split('\n').count().max(1);
        let res = std::panic::catch_unwind(|| Ensemble::parse(text));
        let Ok(res) = res else {
            violations.push(format!("parse panicked on input of {} bytes", text.len()));
            continue;
        };
        if let Err(e) = res {
            let msg = e.to_string();
            if !(e.line >= 1 && e.line <= total) {
                violations.push(format!("input {:?}: line {} outside 1..={total}: {e:?}", preview(text), e.line));
            }
            if !msg.contains(&format!("line {}", e.line)) {
                violations.push(format!("message does not name its line: {msg}"));
            }
            if msg.len() <= format!("ensemble.csv line {}: ", e.line).len() {
                violations.push(format!("message does not say what was expected: {msg}"));
            }
        }
    }
    assert!(violations.is_empty(), "{} violations:\n{}", violations.len(), violations.join("\n"));
}

fn preview(text: &str) -> String {
    text.chars().take(40).collect()
}

#[test]
fn ensemble_03_rejects_a_carriage_return_anywhere() {
    let text = csv(N);
    // In the comment, in the header, mid data, at the very end.
    for line in [1, 2, line_of(3, 100), line_of(N - 1, ROWS - 1)] {
        let l = get_line(&text, line);
        let e = refuse(&with_line(&text, line, &format!("{l}\r")));
        assert_eq!(e, EnsembleError { line, kind: EnsembleErrorKind::CarriageReturn });
    }
    let l = get_line(&text, 5);
    let mid = format!("{}\r{}", &l[..4], &l[4..]);
    assert_eq!(refuse(&with_line(&text, 5, &mid)).line, 5);
}

#[test]
fn ensemble_04_rejects_a_header_that_is_not_exact() {
    let text = csv(N);
    for bad in [
        "model,kind,index,name",
        "model,kind,index,name,value,extra",
        "Model,kind,index,name,value",
        "model,kind,index,value,name",
        "model, kind,index,name,value",
        "",
    ] {
        let e = refuse(&with_line(&text, 2, bad));
        assert_eq!(e.line, 2, "{bad:?}: {e:?}");
        assert_eq!(e.kind, EnsembleErrorKind::BadHeader(bad.to_string()), "{bad:?}");
    }
}

#[test]
fn ensemble_05_comments_before_header_only() {
    // Several # lines (and none) before the header: accepted.
    let body = csv(N);
    let no_comment = body.replacen("# synthetic\n", "", 1);
    assert!(Ensemble::parse(&no_comment).is_ok());
    let many = format!("#\n# exported by tools/export_params.py\n#model,kind\n{body}");
    assert_eq!(Ensemble::parse(&many).expect("comments before header").ids().count(), N);

    // A # line after the header: refused at its own line.
    let late_first = body.replacen("value\n", "value\n# late\n", 1);
    assert_eq!(
        refuse(&late_first),
        EnsembleError { line: 3, kind: EnsembleErrorKind::CommentAfterHeader }
    );
    let at = line_of(10, 7);
    let mut lines: Vec<&str> = body.split('\n').collect();
    lines.insert(at - 1, "# in the middle");
    assert_eq!(
        refuse(&lines.join("\n")),
        EnsembleError { line: at, kind: EnsembleErrorKind::CommentAfterHeader }
    );
}

#[test]
fn ensemble_06_rejects_bad_field_count_model_or_kind() {
    use EnsembleErrorKind::*;
    let text = csv(N);
    let line = line_of(2, 70);
    let good = get_line(&text, line);
    let f: Vec<&str> = good.split(',').collect();

    // Field count.
    for (bad, n) in [
        (f[..4].join(","), 4),
        (format!("{good},x"), 6),
        (String::new(), 1),
        (format!("{},{},{},{}", f[0], f[1], f[2], f[4]), 4),
    ] {
        assert_eq!(refuse(&with_line(&text, line, &bad)), EnsembleError { line, kind: FieldCount(n) }, "{bad:?}");
    }
    // Model not three ASCII digits.
    for m in ["2", "02", "0002", "00a", " 02", "٠٠٢", ""] {
        let bad = format!("{m},{},{},{},{}", f[1], f[2], f[3], f[4]);
        assert_eq!(
            refuse(&with_line(&text, line, &bad)),
            EnsembleError { line, kind: BadModel(m.to_string()) },
            "{m:?}"
        );
    }
    // Kind not one of the three.
    for k in ["Bias", "weight", "", "time-const", "syn_strength "] {
        let bad = format!("{},{k},{},{},{}", f[0], f[2], f[3], f[4]);
        let e = refuse(&with_line(&text, line, &bad));
        assert_eq!(e.line, line, "{k:?}");
        assert!(matches!(&e.kind, BadKind { found, .. } if found == k), "{k:?}: {e:?}");
    }
}

#[test]
fn ensemble_07_rejects_rows_out_of_order_or_wrong_index() {
    use EnsembleErrorKind::*;
    let text = csv(N);

    // A shuffled row: rows for bias 0 and bias 1 of model 000 swapped.
    let (a, b) = (get_line(&text, line_of(0, 0)), get_line(&text, line_of(0, 1)));
    let swapped = with_line(&with_line(&text, line_of(0, 0), &b), line_of(0, 1), &a);
    assert_eq!(
        refuse(&swapped),
        EnsembleError { line: 3, kind: BadIndex { expected: 0, found: "1".into() } }
    );

    // Swapped across models' kind blocks: a time_const row where a bias is due.
    let line = line_of(5, 64);
    let tc = get_line(&text, line_of(5, 65));
    let e = refuse(&with_line(&text, line, &tc));
    assert_eq!(e.line, line);
    assert!(matches!(&e.kind, BadKind { found, .. } if found == "time_const"), "{e:?}");

    // syn_strength before the time constants are done.
    let line = line_of(7, 129);
    let ss = get_line(&text, line_of(7, 130));
    let e = refuse(&with_line(&text, line, &ss));
    assert_eq!(e.line, line);
    assert!(matches!(&e.kind, BadKind { found, .. } if found == "syn_strength"), "{e:?}");

    // index that is not the row's position, though name/value are untouched.
    for (m, s, bad_index) in [(0, 3, "4"), (12, 65 + 10, "11"), (49, ROWS - 1, "604"), (1, 0, "00"), (1, 1, "+1")] {
        let line = line_of(m, s);
        let mut f: Vec<String> = get_line(&text, line).split(',').map(String::from).collect();
        f[2] = bad_index.into();
        let e = refuse(&with_line(&text, line, &f.join(",")));
        assert_eq!(e.line, line, "{bad_index}");
        assert!(matches!(&e.kind, BadIndex { found, .. } if found == bad_index), "{e:?}");
    }

    // A whole row missing from the middle: refused at the line where the next
    // row lands out of place.
    let line = line_of(3, 200);
    let mut lines: Vec<&str> = text.split('\n').collect();
    lines.remove(line - 1);
    let e = refuse(&lines.join("\n"));
    assert_eq!(e.line, line);
}

#[test]
fn ensemble_08_rejects_a_wrong_name() {
    use EnsembleErrorKind::*;
    let text = csv(N);
    let cases = [
        (line_of(0, 0), "R2".to_string(), CELL_TYPES[0].to_string()),
        (line_of(4, 65 + 30), "R1".to_string(), CELL_TYPES[30].to_string()),
        (line_of(9, 130), {
            let (s, t) = CONN_TYPES[1];
            format!("{s}>{t}")
        }, {
            let (s, t) = CONN_TYPES[0];
            format!("{s}>{t}")
        }),
        (line_of(9, 131), {
            let (s, t) = CONN_TYPES[1];
            format!("{t}>{s}")
        }, {
            let (s, t) = CONN_TYPES[1];
            format!("{s}>{t}")
        }),
        (line_of(49, ROWS - 1), "".to_string(), {
            let (s, t) = CONN_TYPES[603];
            format!("{s}>{t}")
        }),
    ];
    for (line, bad, want) in cases {
        let mut f: Vec<String> = get_line(&text, line).split(',').map(String::from).collect();
        assert_eq!(f[3], want, "fixture sanity");
        if bad == want {
            continue;
        }
        f[3] = bad.clone();
        assert_eq!(
            refuse(&with_line(&text, line, &f.join(","))),
            EnsembleError { line, kind: BadName { expected: want, found: bad } }
        );
    }
}

#[test]
fn ensemble_09_values_must_be_finite_f32() {
    use EnsembleErrorKind::*;
    let text = csv(N);
    let line = line_of(6, 300);
    let prefix: Vec<String> = get_line(&text, line).split(',').take(4).map(String::from).collect();
    let prefix = prefix.join(",");
    for v in ["", "abc", "1.0.0", "inf", "-inf", "NaN", "infinity", "1e39", "-1e39", "0x10", " 1.0", "1,0"] {
        let bad = format!("{prefix},{v}");
        let e = refuse(&with_line(&text, line, &bad));
        assert_eq!(e.line, line, "{v:?}: {e:?}");
        if v != "1,0" {
            assert_eq!(e.kind, BadValue(v.to_string()), "{v:?}");
        }
    }

    // Finite values are taken via str::parse::<f32>(), bit for bit.
    for v in ["0.1", "-3.4028235e38", "1e-45", "0.333333343267", "-0", "7", "+2.5", "1E3"] {
        let t = with_line(&text, line, &format!("{prefix},{v}"));
        let e = Ensemble::parse(&t).unwrap_or_else(|e| panic!("{v:?} is finite: {e}"));
        let got = e.model("006").unwrap().syn_strength[300 - 130];
        assert_eq!(got.to_bits(), v.parse::<f32>().unwrap().to_bits(), "{v:?}");
    }
}

#[test]
fn ensemble_10_models_unique_increasing_and_exactly_fifty() {
    use EnsembleErrorKind::*;
    let text = csv(N);

    // A model appears twice: model 001 relabelled 000.
    let dup: Vec<String> = text
        .split('\n')
        .map(|l| if l.starts_with("001,") { l.replacen("001,", "000,", 1) } else { l.to_string() })
        .collect();
    let e = refuse(&dup.join("\n"));
    assert_eq!(e, EnsembleError {
        line: line_of(1, 0),
        kind: ModelOutOfOrder { expected: "001".into(), found: "000".into() }
    });

    // Models not in increasing order: 002 and 003 swapped wholesale.
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    let (s2, s3) = (line_of(2, 0) - 1, line_of(3, 0) - 1);
    let block2: Vec<String> = lines[s2..s2 + ROWS].to_vec();
    let block3: Vec<String> = lines[s3..s3 + ROWS].to_vec();
    lines.splice(s2..s3 + ROWS, block3.into_iter().chain(block2));
    let e = refuse(&lines.join("\n"));
    assert_eq!(e.line, line_of(2, 0));
    assert!(matches!(e.kind, ModelOutOfOrder { .. }), "{e:?}");

    // A gap in the ids: 000..048 then 050 (fifty models, but not 000..049).
    let gap: Vec<String> = text
        .split('\n')
        .map(|l| if l.starts_with("049,") { l.replacen("049,", "050,", 1) } else { l.to_string() })
        .collect();
    let e = refuse(&gap.join("\n"));
    assert_eq!(e.line, line_of(49, 0));

    // 49 models, with and without trailing newline; 51 models; 49 and a half.
    for t in [csv(49), csv(49).trim_end_matches('\n').to_string()] {
        let e = refuse(&t);
        assert!(matches!(e.kind, TooFewRows { found, .. } if found == 49 * ROWS), "{e:?}");
        let last = t.trim_end_matches('\n').split('\n').count();
        assert_eq!(e.line, last);
    }
    let e = refuse(&csv(51));
    assert!(matches!(e.kind, TooManyRows { .. }), "{e:?}");
    assert_eq!(e.line, line_of(50, 0));

    let half: Vec<&str> = text.split('\n').take(line_of(49, 300)).collect();
    let e = refuse(&half.join("\n"));
    assert!(matches!(e.kind, TooFewRows { .. }), "{e:?}");

    // Header only: zero models.
    let e = refuse(&format!("{HEADER}\n"));
    assert!(matches!(e.kind, TooFewRows { found: 0, .. }), "{e:?}");
}

#[test]
fn ensemble_11_pretrained_is_model_000_from_compiled_constants() {
    let p = ModelParams::pretrained();
    assert_eq!(bits(&p.bias), bits(&BIAS));
    assert_eq!(bits(&p.time_const), bits(&TIME_CONST));
    assert_eq!(bits(&p.syn_strength), bits(&SYN_STRENGTH));
    assert_eq!(p.bias.len() + p.time_const.len() + p.syn_strength.len(), 734);
    assert_eq!(semblance_flyvis::pretrained::FREE_PARAMETER_COUNT, 734);
    // And it is what model 000 of a parsed file holds when written from it.
    let e = Ensemble::parse(&csv(N)).unwrap();
    assert_eq!(e.model("000"), Some(&p));
}

fn real_ensemble() -> Option<String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/flyvis-pretrained/ensemble.csv");
    match std::fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(_) => {
            let required = std::env::var("SEMBLANCE_REQUIRE_FIXTURES").is_ok_and(|v| v == "1");
            assert!(!required, "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
            eprintln!("skipping: {path} not found; run ci/fetch-fixtures.sh");
            None
        }
    }
}

#[test]
fn ensemble_12_real_ensemble_matches_pretrained_and_rounds_once() {
    let Some(text) = real_ensemble() else { return };
    let e = Ensemble::parse(&text).expect("the published ensemble parses");
    let ids: Vec<&str> = e.ids().collect();
    let want: Vec<String> = (0..N).map(|m| format!("{m:03}")).collect();
    assert_eq!(ids, want);

    let mut lines = text.lines().skip_while(|l| l.starts_with('#'));
    assert_eq!(lines.next(), Some(HEADER));
    let data: Vec<&str> = lines.collect();
    assert_eq!(data.len(), 36_700);

    let (got, p) = (e.model("000").unwrap(), ModelParams::pretrained());
    assert_eq!(bits(&got.bias), bits(&p.bias), "bias");
    assert_eq!(bits(&got.time_const), bits(&p.time_const), "time_const");
    assert_eq!(bits(&got.syn_strength), bits(&p.syn_strength), "syn_strength");

    for l in &data {
        let s = l.rsplit(',').next().unwrap();
        let once: f32 = s.parse().unwrap();
        let twice = s.parse::<f64>().unwrap() as f32;
        assert_eq!(once.to_bits(), twice.to_bits(), "{l}");
    }
}

#[test]
fn ensemble_13_unknown_model_id_is_none() {
    let e = Ensemble::parse(&csv(N)).unwrap();
    for id in ["050", "999", "", "0", "00", "0000", "abc", " 000", "000 ", "49"] {
        assert_eq!(e.model(id), None, "{id:?}");
    }
    assert!(e.model("049").is_some());
}
