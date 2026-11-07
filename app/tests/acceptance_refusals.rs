//! Acceptance tests for behaviour group `refusals` of epic fidelity-measures,
//! the part the `semblance` binary shows: REFUSALS-05. REFUSALS-01 .. 04 and
//! 06 .. 09 are in `crates/fidelity/tests/acceptance_refusals.rs`.
//!
//! The test spawns the built binary and reads its stdout, stderr and exit
//! code.

use std::process::Command;

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Run {
    fn all(&self) -> String {
        format!("exit {:?}\n--- stdout\n{}--- stderr\n{}", self.code, self.stdout, self.stderr)
    }
}

fn semblance(args: &[&str]) -> Run {
    let o = Command::new(env!("CARGO_BIN_EXE_semblance")).args(args).output().expect("the binary runs");
    Run {
        code: o.status.code(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

/// The value printed after `label` on its line, as written.
fn value_after<'a>(out: &'a str, label: &str) -> Option<&'a str> {
    out.lines().find_map(|l| l.trim_start().strip_prefix(label)).and_then(|rest| rest.split_whitespace().next())
}

/// REFUSALS-05: `roundtrip` and `evolve` handle the now-fallible legacy
/// functions, and `roundtrip` still prints 0.000000 for both distances.
#[test]
fn refusals_05_roundtrip_and_evolve_handle_the_fallible_distances() {
    let r = semblance(&["roundtrip"]);
    assert_eq!(r.code, Some(0), "{}", r.all());
    assert_eq!(value_after(&r.stdout, "structural distance:"), Some("0.000000"), "{}", r.all());
    assert_eq!(value_after(&r.stdout, "behavioural distance:"), Some("0.000000"), "{}", r.all());
    assert!(!r.stderr.contains("panicked"), "{}", r.all());

    let e = semblance(&["evolve"]);
    assert_eq!(e.code, Some(0), "{}", e.all());
    assert!(!e.stderr.contains("panicked"), "{}", e.all());
    let generations: Vec<&str> = e
        .stdout
        .lines()
        .filter(|l| {
            let mut w = l.split_whitespace();
            matches!((w.next().map(str::parse::<u32>), w.next().map(str::parse::<f32>)), (Some(Ok(_)), Some(Ok(_))))
        })
        .collect();
    assert_eq!(generations.len(), 20, "{}", e.all());
}
