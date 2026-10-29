//! The parity record, docs/results/0.3.0-flyvis-parity.md, is a hand-written
//! preamble followed by what `semblance validate --markdown` printed on
//! hosted CI. This test regenerates that output and compares it with the
//! committed file (spec
//! docs/superpowers/specs/2026-10-29-flyvis-parity-validate-design.md,
//! section 7, R9 and R22).
//!
//! Never compared: the host-specific section. Compared only on a host whose
//! libm fingerprint is the record's: the libm section. Under
//! `SEMBLANCE_REQUIRE_LIBM_RECORD=1` (CI, the recording host) a different
//! fingerprint is a failure; elsewhere it replaces the libm section with one
//! placeholder line in both texts and prints why, visible under
//! `--nocapture`.
//!
//! The record is regenerated only from CI's `parity-record` artifact, never
//! by editing numbers here or on a workstation. This test spawns the binary
//! under test and nothing else; it reads no git history.

use std::collections::HashSet;
use std::process::Command;

const RECORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/results/0.3.0-flyvis-parity.md");
const BEGIN: &str = "<!-- BEGIN semblance validate --markdown -->";
const END: &str = "<!-- END semblance validate --markdown -->";
const HOST_BEGIN: &str = "<!-- BEGIN host-specific -->";
const HOST_END: &str = "<!-- END host-specific -->";
const LIBM_BEGIN: &str = "<!-- BEGIN libm fingerprint=";
const LIBM_END: &str = "<!-- END libm -->";
const PLACEHOLDER: &str = "<!-- libm sections not compared -->";

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1")
}

/// The byte offset of `marker`'s one occurrence in `text`.
fn once(text: &str, marker: &str, what: &str) -> Result<usize, String> {
    let mut found = text.match_indices(marker).map(|(i, _)| i);
    match (found.next(), found.next()) {
        (Some(i), None) => Ok(i),
        (None, _) => Err(format!("{what}: no {marker}")),
        (Some(_), Some(_)) => Err(format!("{what}: {marker} more than once")),
    }
}

/// (the preamble, the generated block from BEGIN to END inclusive).
fn split<'a>(text: &'a str, what: &str) -> Result<(&'a str, &'a str), String> {
    let (b, e) = (once(text, BEGIN, what)?, once(text, END, what)?);
    if e < b {
        return Err(format!("{what}: {END} comes before {BEGIN}"));
    }
    Ok((&text[..b], &text[b..e + END.len()]))
}

/// `f32:{n}:{16 hex digits},f64:{n}:{16 hex digits}`.
fn parse_fingerprint(s: &str) -> Result<(), String> {
    let part = |p: &str, name: &str| -> bool {
        let mut f = p.splitn(3, ':');
        f.next() == Some(name)
            && f.next().is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            && f.next().is_some_and(|h| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()))
    };
    match s.split_once(',') {
        Some((a, b)) if part(a, "f32") && part(b, "f64") => Ok(()),
        _ => Err(format!("the libm fingerprint {s:?} does not parse")),
    }
}

/// Checks a generated block's markers and returns its libm fingerprint:
/// one host-specific section, one libm section whose fingerprint parses and
/// equals the host section's `libm fingerprint` row.
fn check_block(block: &str, what: &str) -> Result<String, String> {
    let (hb, he) = (once(block, HOST_BEGIN, what)?, once(block, HOST_END, what)?);
    let (lb, le) = (once(block, LIBM_BEGIN, what)?, once(block, LIBM_END, what)?);
    if he < hb || le < lb {
        return Err(format!("{what}: a section ends before it begins"));
    }
    let start = lb + LIBM_BEGIN.len();
    let len = block[start..].find(" -->").ok_or_else(|| format!("{what}: the libm marker does not close"))?;
    let fingerprint = &block[start..start + len];
    parse_fingerprint(fingerprint).map_err(|e| format!("{what}: {e}"))?;
    let host = block[hb..he]
        .lines()
        .find_map(|l| l.strip_prefix("| libm fingerprint | ").and_then(|r| r.strip_suffix(" |")))
        .ok_or_else(|| format!("{what}: the host section has no libm fingerprint row"))?;
    if host != fingerprint {
        return Err(format!("{what}: the libm marker's fingerprint {fingerprint} differs from the host section's {host}"));
    }
    Ok(fingerprint.to_string())
}

/// `block` without its host-specific section, markers and line included.
fn remove_host(block: &str) -> String {
    match (block.find(HOST_BEGIN), block.find(HOST_END)) {
        (Some(b), Some(e)) if b < e => {
            let end = e + HOST_END.len() + usize::from(block[e + HOST_END.len()..].starts_with('\n'));
            format!("{}{}", &block[..b], &block[end..])
        }
        _ => block.to_string(),
    }
}

/// `block` with its libm section, markers included, replaced by one line.
fn replace_libm(block: &str) -> String {
    match (block.find(LIBM_BEGIN), block.find(LIBM_END)) {
        (Some(b), Some(e)) if b < e => format!("{}{PLACEHOLDER}{}", &block[..b], &block[e + LIBM_END.len()..]),
        _ => block.to_string(),
    }
}

#[derive(Debug, PartialEq)]
enum Libm {
    Compare,
    Skip,
    Fail(String),
}

/// What to do with the libm sections: a pure function of the two
/// fingerprints and `SEMBLANCE_REQUIRE_LIBM_RECORD`.
fn libm_decision(here: &str, record: &str, require: bool) -> Libm {
    if here == record {
        Libm::Compare
    } else if require {
        Libm::Fail(format!(
            "SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint {here} differs from the record's {record}"
        ))
    } else {
        Libm::Skip
    }
}

/// R22's numeral tokens: maximal runs of `0-9 . e + -` that start with a
/// digit and contain a `.` or an `e` or three or more digits. Trailing `.`,
/// `+` and `-` are punctuation, not part of the number. Tokens with two or
/// more `.` (versions) are exempt.
fn numerals(text: &str) -> Vec<String> {
    let in_class = |c: char| c.is_ascii_digit() || matches!(c, '.' | 'e' | '+' | '-');
    let mut out = Vec::new();
    let mut run = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if in_class(c) {
            run.push(c);
            continue;
        }
        let token = run.trim_end_matches(['.', '+', '-']);
        if token.starts_with(|c: char| c.is_ascii_digit())
            && (token.contains(['.', 'e']) || token.bytes().filter(u8::is_ascii_digit).count() >= 3)
            && token.matches('.').count() < 2
        {
            out.push(token.to_string());
        }
        run.clear();
    }
    out
}

/// Every numeral the preamble quotes, outside its `Generated:` line, occurs
/// in the generated block (R22).
fn check_preamble(preamble: &str, block: &str) -> Result<(), String> {
    let known: HashSet<String> = numerals(block).into_iter().collect();
    let missing: Vec<String> = preamble
        .lines()
        .filter(|l| !l.contains("Generated:"))
        .flat_map(numerals)
        .filter(|t| !known.contains(t))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("the preamble quotes numbers the generated block does not print: {}", missing.join(", ")))
    }
}

/// The first line where `a` and `b` differ, both sides shown.
fn first_difference(a: &str, b: &str) -> Option<String> {
    let (mut la, mut lb) = (a.lines(), b.lines());
    for n in 1.. {
        match (la.next(), lb.next()) {
            (None, None) => return None,
            (x, y) if x == y => continue,
            (x, y) => {
                return Some(format!(
                    "line {n} of the generated block differs:\n  committed: {}\n  validate:  {}",
                    x.unwrap_or("(end)"),
                    y.unwrap_or("(end)")
                ))
            }
        }
    }
    None
}

#[test]
fn the_committed_record_is_what_validate_prints() {
    // 1. The committed file, on every host, fixtures or not.
    let committed = std::fs::read_to_string(RECORD).unwrap_or_else(|e| panic!("{RECORD}: {e}"));
    let (preamble, block) = split(&committed, "the committed record").unwrap_or_else(|e| panic!("{e}"));
    let recorded = check_block(block, "the committed record").unwrap_or_else(|e| panic!("{e}"));
    check_preamble(preamble, block).unwrap_or_else(|e| panic!("{e}"));

    // 2. The fixtures, or a skip notice naming the first absent one.
    let data = format!("{}/../data", env!("CARGO_MANIFEST_DIR"));
    for rel in semblance_flyvis::parity::required_files() {
        let path = format!("{data}/{rel}");
        if std::fs::metadata(&path).is_err() {
            assert!(!flag("SEMBLANCE_REQUIRE_FIXTURES"), "SEMBLANCE_REQUIRE_FIXTURES=1 but {path} is missing");
            eprintln!("skipping: {path} not found. {}", semblance_flyvis::parity::how_to_get(rel));
            return;
        }
    }

    // 3. Run validate, as CI's record step does.
    let out = Command::new(env!("CARGO_BIN_EXE_semblance"))
        .args(["validate", "--markdown", "--data", &data])
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "validate must pass:\n{stdout}{}", String::from_utf8_lossy(&out.stderr));

    // 4. Its block, held to the same marker checks.
    let (_, fresh) = split(&stdout, "validate's output").unwrap_or_else(|e| panic!("{e}"));
    let here = check_block(fresh, "validate's output").unwrap_or_else(|e| panic!("{e}"));

    // 5. The host-specific sections are never compared.
    let (mut a, mut b) = (remove_host(block), remove_host(fresh));

    // 6. The libm sections, only on the record's libm.
    match libm_decision(&here, &recorded, flag("SEMBLANCE_REQUIRE_LIBM_RECORD")) {
        Libm::Compare => {}
        Libm::Fail(m) => panic!("{m}"),
        Libm::Skip => {
            a = replace_libm(&a);
            b = replace_libm(&b);
            eprintln!("record: libm sections not compared: this host's libm fingerprint {here} differs from the record's {recorded}");
        }
    }

    // 7. Byte for byte.
    if let Some(d) = first_difference(&a, &b) {
        panic!("{d}");
    }
    assert_eq!(a, b, "the generated blocks differ in their line endings");
}

// ---------------------------------------------------------------------------
// The text functions, on synthetic blocks
// ---------------------------------------------------------------------------

const FP: &str = "f32:104:0123456789abcdef,f64:108:fedcba9876543210";

fn block(fp_marker: &str, fp_host: &str, libm_body: &str, rest: &str) -> String {
    format!(
        "{BEGIN}\n## Reference\n| a | 1.5e-3 |\n\n{HOST_BEGIN}\n| threads | 4 |\n| libm fingerprint | {fp_host} |\n{HOST_END}\n\n\
         {LIBM_BEGIN}{fp_marker} -->\n{libm_body}\n{LIBM_END}\n\n{rest}\n{END}"
    )
}

#[test]
fn placeholder_replacement_hides_only_the_libm_section() {
    let x = block(FP, FP, "| 4 | 8.605e-1 |", "| control | 2.5e0 |");
    let y = block(FP, FP, "| 4 | 8.601e-1 |", "| control | 2.5e0 |");
    assert_ne!(remove_host(&x), remove_host(&y));
    assert_eq!(replace_libm(&remove_host(&x)), replace_libm(&remove_host(&y)));
    assert!(replace_libm(&x).contains(PLACEHOLDER) && !replace_libm(&x).contains("8.605e-1"));
    // outside the libm section a difference still shows
    let z = block(FP, FP, "| 4 | 8.605e-1 |", "| control | 2.4e0 |");
    let d = first_difference(&replace_libm(&x), &replace_libm(&z)).expect("a difference");
    assert!(d.contains("| control | 2.5e0 |") && d.contains("| control | 2.4e0 |"), "{d}");
    // the host section, whatever it holds, is removed
    assert_eq!(remove_host(&x), remove_host(&x.replace("| threads | 4 |", "| threads | 16 |")));
    assert!(!remove_host(&x).contains("threads"));
}

#[test]
fn a_missing_duplicated_or_unparsable_marker_fails() {
    let good = block(FP, FP, "x", "y");
    assert_eq!(check_block(&good, "t"), Ok(FP.to_string()));
    assert!(split(&format!("preamble\n{good}\n"), "t").is_ok());
    assert!(split("preamble only\n", "t").is_err(), "no block");
    assert!(split(&format!("{good}\n{good}"), "t").is_err(), "a block twice");
    assert!(check_block(&good.replace(HOST_END, ""), "t").is_err(), "no host end");
    assert!(check_block(&good.replace(LIBM_END, &format!("{LIBM_END}\n{LIBM_END}")), "t").is_err(), "libm end twice");
    for bad in ["f32:104:0123,f64:108:fedcba9876543210", "f64:108:fedcba9876543210,f32:104:0123456789abcdef", "x"] {
        assert!(check_block(&block(bad, bad, "x", "y"), "t").is_err(), "{bad}");
    }
}

#[test]
fn a_host_fingerprint_that_disagrees_with_the_libm_marker_fails() {
    let other = "f32:104:0123456789abcdef,f64:109:fedcba9876543210";
    let e = check_block(&block(FP, other, "x", "y"), "the committed record").unwrap_err();
    assert!(e.contains("differs from the host section's"), "{e}");
}

#[test]
fn the_preamble_may_quote_only_numbers_the_block_prints() {
    let b = block(FP, FP, "| 4 | 8.605e-1 |", "| run 2 | 7.439e2 | 45669 |");
    assert_eq!(check_preamble("The worst ratio is 8.605e-1, and 7.439e2 in run 2.\n", &b), Ok(()));
    assert_eq!(check_preamble("45669 values, f32 and f64, 12 sequences, 65 types.\n", &b), Ok(()));
    let e = check_preamble("The worst ratio is about 0.86.\n", &b).unwrap_err();
    assert!(e.ends_with("0.86"), "{e}");
    assert!(check_preamble("744 times the bound\n", &b).is_err(), "three digits");
    assert_eq!(check_preamble("semblance 0.3.0 reproduces flyvis 1.2.0.\n", &b), Ok(()), "versions are exempt");
    assert_eq!(check_preamble("**Generated:** Rust 1.97, run 123456, 2026-10-30.\n", &b), Ok(()), "the Generated: line");
    assert_eq!(numerals("by 3.773e-3. Then 1e-10, f32, 1/200 and fixtures-2"), ["3.773e-3", "1e-10", "200"]);
}

#[test]
fn require_libm_record_turns_a_fingerprint_mismatch_into_a_failure() {
    let other = "f32:0:0000000000000000,f64:0:0000000000000000";
    assert_eq!(libm_decision(FP, FP, true), Libm::Compare);
    assert_eq!(libm_decision(FP, FP, false), Libm::Compare);
    assert_eq!(libm_decision(other, FP, false), Libm::Skip);
    assert_eq!(
        libm_decision(other, FP, true),
        Libm::Fail(format!(
            "SEMBLANCE_REQUIRE_LIBM_RECORD=1 but this host's libm fingerprint {other} differs from the record's {FP}"
        ))
    );
}
