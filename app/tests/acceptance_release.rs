//! Acceptance tests for the 0.3.0 release: packaging and release metadata
//! (behaviour group `release`, RELEASE-01 to RELEASE-11).
//!
//! These drive cargo itself (`cargo metadata`, `cargo package --workspace`,
//! `cargo publish --workspace --dry-run`) and read the shipped documents and
//! manifests as a reader would. Packaging and the dry run build into this
//! test target's own temp dir, so they never touch the outer build. They need
//! network access to the crates.io index, as `cargo package` always does.
//!
//! `cargo package` is run with `--allow-dirty` so that working on this
//! workspace does not fail the suite; on the clean tree the behaviour names
//! it is a no-op.
//!
//! RELEASE-12 (the tag, no GitHub Release, publishing and yanking only on
//! the owner's go-ahead) is a post-merge step of the head and the owner and
//! has no test here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

const LIBRARIES: [&str; 7] = [
    "semblance-rng",
    "semblance-connectome",
    "semblance-codec",
    "semblance-sim",
    "semblance-evo",
    "semblance-fidelity",
    "semblance-flyvis",
];

/// Package name -> directory, relative to the workspace root.
const PACKAGE_DIRS: [(&str, &str); 8] = [
    ("semblance-rng", "crates/rng"),
    ("semblance-connectome", "crates/connectome"),
    ("semblance-codec", "crates/codec"),
    ("semblance-sim", "crates/sim"),
    ("semblance-evo", "crates/evo"),
    ("semblance-fidelity", "crates/fidelity"),
    ("semblance-flyvis", "crates/flyvis"),
    ("semblance", "app"),
];

fn workspace() -> PathBuf {
    PathBuf::from(WORKSPACE)
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(workspace().join(rel)).unwrap_or_else(|e| panic!("reading {rel}: {e}"))
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn cargo() -> Command {
    let mut cmd = Command::new(option_env!("CARGO").unwrap_or("cargo"));
    cmd.current_dir(workspace());
    cmd
}

fn run(mut cmd: Command, what: &str) -> Output {
    let out = cmd.output().unwrap_or_else(|e| panic!("{what} did not start: {e}"));
    assert!(
        out.status.success(),
        "{what} failed ({:?}):\nstdout:\n{}\nstderr:\n{}",
        out.status,
        text(&out.stdout),
        text(&out.stderr)
    );
    out
}

fn git(args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(workspace())
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} did not start: {e}"))
}

// ---------------------------------------------------------------------------
// A minimal JSON reader, for `cargo metadata` (the workspace takes no
// dependencies, not even for tests).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Json {
    Null,
    Bool,
    Num,
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v).unwrap_or(&Json::Null),
            _ => &Json::Null,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(v) => v,
            _ => &[],
        }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) {
        self.ws();
        assert_eq!(self.b.get(self.i), Some(&c), "JSON: expected {:?} at {}", c as char, self.i);
        self.i += 1;
    }
    fn value(&mut self) -> Json {
        self.ws();
        match self.b[self.i] {
            b'{' => {
                self.i += 1;
                let mut kv = Vec::new();
                self.ws();
                if self.b[self.i] == b'}' {
                    self.i += 1;
                    return Json::Obj(kv);
                }
                loop {
                    self.ws();
                    let k = self.string();
                    self.eat(b':');
                    let v = self.value();
                    kv.push((k, v));
                    self.ws();
                    let c = self.b[self.i];
                    self.i += 1;
                    if c == b'}' {
                        return Json::Obj(kv);
                    }
                    assert_eq!(c, b',', "JSON: expected ',' or '}}'");
                }
            }
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                self.ws();
                if self.b[self.i] == b']' {
                    self.i += 1;
                    return Json::Arr(v);
                }
                loop {
                    v.push(self.value());
                    self.ws();
                    let c = self.b[self.i];
                    self.i += 1;
                    if c == b']' {
                        return Json::Arr(v);
                    }
                    assert_eq!(c, b',', "JSON: expected ',' or ']'");
                }
            }
            b'"' => Json::Str(self.string()),
            b't' => {
                self.i += 4;
                Json::Bool
            }
            b'f' => {
                self.i += 5;
                Json::Bool
            }
            b'n' => {
                self.i += 4;
                Json::Null
            }
            _ => {
                let s = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.i += 1;
                }
                {
                assert!(self.i > s, "JSON: unexpected byte at {s}");
                Json::Num
            }
            }
        }
    }
    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out = Vec::new();
        loop {
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).unwrap(),
                b'\\' => {
                    let e = self.b[self.i];
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = std::str::from_utf8(&self.b[self.i..self.i + 4]).unwrap();
                            self.i += 4;
                            let cp = u32::from_str_radix(hex, 16).unwrap();
                            let ch = char::from_u32(cp).unwrap_or('\u{fffd}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }
}

fn parse_json(s: &str) -> Json {
    let mut p = Parser { b: s.as_bytes(), i: 0 };
    p.value()
}

/// `cargo metadata --no-deps` for the workspace, once.
fn metadata() -> &'static Json {
    static M: OnceLock<Json> = OnceLock::new();
    M.get_or_init(|| {
        let mut cmd = cargo();
        cmd.args(["metadata", "--format-version", "1", "--no-deps", "--locked"]);
        let out = run(cmd, "cargo metadata");
        parse_json(&text(&out.stdout))
    })
}

fn package(name: &str) -> &'static Json {
    metadata()
        .get("packages")
        .arr()
        .iter()
        .find(|p| p.get("name").str() == Some(name))
        .unwrap_or_else(|| panic!("cargo metadata has no package {name}"))
}

/// `(dependency name, kind)` for a package; kind is `None` for a normal one.
fn dependencies(name: &str) -> Vec<(String, Option<String>)> {
    package(name)
        .get("dependencies")
        .arr()
        .iter()
        .map(|d| (d.get("name").str().unwrap().to_string(), d.get("kind").str().map(str::to_string)))
        .collect()
}

// ---------------------------------------------------------------------------
// Manifest text
// ---------------------------------------------------------------------------

/// The `key = value` lines of one `[section]` of a TOML file.
fn toml_section(src: &str, section: &str) -> Vec<(String, String)> {
    let header = format!("[{section}]");
    let mut inside = false;
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == header;
            continue;
        }
        if inside && !t.is_empty() && !t.starts_with('#') {
            if let Some((k, v)) = t.split_once('=') {
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Packaging, shared by RELEASE-03 and RELEASE-06
// ---------------------------------------------------------------------------

fn package_target() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("acceptance-release-package")
}

/// Runs `cargo package --workspace` once and extracts every `.crate`.
/// Returns package name -> (extracted root, every entry path in the archive).
fn packaged() -> &'static BTreeMap<String, (PathBuf, Vec<String>)> {
    static P: OnceLock<BTreeMap<String, (PathBuf, Vec<String>)>> = OnceLock::new();
    P.get_or_init(|| {
        let target = package_target();
        let pkg_dir = target.join("package");
        let _ = std::fs::remove_dir_all(&pkg_dir);
        let mut cmd = cargo();
        cmd.args(["package", "--workspace", "--locked", "--allow-dirty"]).env("CARGO_TARGET_DIR", &target);
        run(cmd, "cargo package --workspace");

        let extract = target.join("extracted");
        let _ = std::fs::remove_dir_all(&extract);
        std::fs::create_dir_all(&extract).unwrap();

        let mut out = BTreeMap::new();
        for (name, _) in PACKAGE_DIRS {
            let file = pkg_dir.join(format!("{name}-0.3.0.crate"));
            assert!(file.is_file(), "cargo package did not produce {}", file.display());
            let mut list = Command::new("tar");
            list.arg("tzf").arg(&file);
            let entries: Vec<String> =
                text(&run(list, "tar tzf").stdout).lines().map(str::to_string).collect();
            let mut x = Command::new("tar");
            x.arg("xzf").arg(&file).arg("-C").arg(&extract);
            run(x, "tar xzf");
            out.insert(name.to_string(), (extract.join(format!("{name}-0.3.0")), entries));
        }
        out
    })
}

/// `cargo publish --workspace --dry-run`'s stderr, once.
fn publish_dry_run() -> &'static str {
    static D: OnceLock<String> = OnceLock::new();
    D.get_or_init(|| {
        let mut cmd = cargo();
        cmd.args(["publish", "--workspace", "--dry-run", "--locked", "--allow-dirty"])
            .env("CARGO_TARGET_DIR", PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("acceptance-release-publish"));
        let out = run(cmd, "cargo publish --workspace --dry-run");
        text(&out.stderr)
    })
}

/// Package names in the order the dry run uploads them.
fn publish_order() -> Vec<String> {
    publish_dry_run()
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("Uploading "))
        .map(|rest| rest.split_whitespace().next().unwrap().to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// RELEASE-01
// ---------------------------------------------------------------------------

#[test]
fn release_01_the_workspace_and_lockfile_are_at_0_3_0_with_only_the_eight_semblance_packages() {
    let manifest = read("Cargo.toml");

    let pkg = toml_section(&manifest, "workspace.package");
    let version = pkg.iter().find(|(k, _)| k == "version").map(|(_, v)| v.as_str());
    assert_eq!(version, Some("\"0.3.0\""), "[workspace.package] version");

    let deps = toml_section(&manifest, "workspace.dependencies");
    let names: Vec<&str> = deps.iter().map(|(k, _)| k.as_str()).collect();
    let mut want: Vec<&str> = LIBRARIES.to_vec();
    want.sort();
    let mut got = names.clone();
    got.sort();
    assert_eq!(got, want, "[workspace.dependencies] pins every library and nothing else");
    for (k, v) in &deps {
        assert!(v.contains("version = \"0.3.0\""), "{k} is pinned at {v}");
        assert!(!v.contains("0.2.0"), "{k} is pinned at {v}");
    }

    // cargo's own view: every workspace member is 0.3.0.
    for (name, _) in PACKAGE_DIRS {
        assert_eq!(package(name).get("version").str(), Some("0.3.0"), "{name}'s version");
    }

    // Cargo.lock: exactly the eight, at 0.3.0, none from a registry.
    let lock = read("Cargo.lock");
    let mut locked: Vec<(String, String)> = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{key} = ")))
                .map(|v| v.trim_matches('"').to_string())
        };
        assert!(field("source").is_none(), "Cargo.lock has an external package:\n{block}");
        assert!(field("checksum").is_none(), "Cargo.lock has a registry package:\n{block}");
        locked.push((field("name").expect("a name"), field("version").expect("a version")));
    }
    let mut expect: Vec<(String, String)> =
        PACKAGE_DIRS.iter().map(|(n, _)| (n.to_string(), "0.3.0".to_string())).collect();
    expect.sort();
    locked.sort();
    assert_eq!(locked, expect, "Cargo.lock's packages");
}

// ---------------------------------------------------------------------------
// RELEASE-02
// ---------------------------------------------------------------------------

#[test]
fn release_02_flyvis_depends_on_sim_app_depends_on_flyvis_and_publishing_order_is_valid() {
    let flyvis = dependencies("semblance-flyvis");
    assert!(
        flyvis.iter().any(|(n, k)| n == "semblance-sim" && k.is_none()),
        "semblance-sim is a normal dependency of semblance-flyvis: {flyvis:?}"
    );
    let app = dependencies("semblance");
    assert!(
        app.iter().any(|(n, k)| n == "semblance-flyvis" && k.is_none()),
        "app depends on semblance-flyvis: {app:?}"
    );

    let order = publish_order();
    assert_eq!(order.len(), 8, "the dry run uploads all eight packages: {order:?}");
    let at = |n: &str| order.iter().position(|o| o == n).unwrap_or_else(|| panic!("{n} not uploaded: {order:?}"));
    assert!(at("semblance-flyvis") > at("semblance-sim"), "flyvis publishes after sim: {order:?}");
    assert_eq!(order.last().map(String::as_str), Some("semblance"), "semblance publishes last: {order:?}");
    // Every normal dependency is uploaded before its dependent.
    for (name, _) in PACKAGE_DIRS {
        for (dep, kind) in dependencies(name) {
            if kind.is_none() && dep.starts_with("semblance") {
                assert!(at(&dep) < at(name), "{dep} must publish before {name}: {order:?}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RELEASE-03
// ---------------------------------------------------------------------------

#[test]
fn release_03_every_package_carries_license_and_notice_and_the_publish_dry_run_passes() {
    let licence = read("LICENSE");
    let notice = read("NOTICE");

    // The per-package files are tracked symlinks to the root ones.
    for (name, dir) in PACKAGE_DIRS {
        let up = if dir == "app" { "../" } else { "../../" };
        for f in ["LICENSE", "NOTICE"] {
            let rel = format!("{dir}/{f}");
            let path = workspace().join(&rel);
            let meta = std::fs::symlink_metadata(&path).unwrap_or_else(|e| panic!("{rel}: {e}"));
            assert!(meta.file_type().is_symlink(), "{rel} is a symlink ({name})");
            assert_eq!(std::fs::read_link(&path).unwrap(), Path::new(&format!("{up}{f}")), "{rel}'s target");
            let staged = git(&["ls-files", "-s", "--", &rel]);
            let line = text(&staged.stdout);
            assert!(line.starts_with("120000 "), "{rel} is tracked as a symlink: {line:?}");
        }
    }

    // Every .crate holds LICENSE and NOTICE as real files with the root text.
    let mut entries = 0;
    for (name, (root, list)) in packaged() {
        for f in ["LICENSE", "NOTICE"] {
            let entry = format!("{name}-0.3.0/{f}");
            let n = list.iter().filter(|e| **e == entry).count();
            assert_eq!(n, 1, "{name}.crate holds {f} once: {list:?}");
            entries += n;
            let path = root.join(f);
            let meta = std::fs::symlink_metadata(&path).unwrap();
            assert!(meta.file_type().is_file(), "{name}'s {f} is dereferenced into a file");
            let body = std::fs::read_to_string(&path).unwrap();
            let want = if f == "LICENSE" { &licence } else { &notice };
            assert_eq!(&body, want, "{name}'s {f} is the root {f}");
        }
    }
    let total: usize = packaged()
        .values()
        .flat_map(|(_, l)| l.iter())
        .filter(|e| e.ends_with("/LICENSE") || e.ends_with("/NOTICE"))
        .count();
    assert_eq!(entries, 16);
    assert_eq!(total, 16, "LICENSE/NOTICE entries across all eight .crate files");

    let dry = publish_dry_run();
    assert_eq!(publish_order().len(), 8, "the dry run reaches every package:\n{dry}");
}

// ---------------------------------------------------------------------------
// RELEASE-04
// ---------------------------------------------------------------------------

#[test]
fn release_04_semblance_flyvis_is_licensed_apache_2_0_and_mit() {
    assert_eq!(package("semblance-flyvis").get("license").str(), Some("Apache-2.0 AND MIT"));
    let manifest = read("crates/flyvis/Cargo.toml");
    let pkg = toml_section(&manifest, "package");
    let licence = pkg.iter().find(|(k, _)| k.starts_with("license")).cloned();
    assert_eq!(licence, Some(("license".to_string(), "\"Apache-2.0 AND MIT\"".to_string())));
}

// ---------------------------------------------------------------------------
// RELEASE-05
// ---------------------------------------------------------------------------

/// Collapses runs of whitespace so wrapped prose compares as one line.
fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn release_05_notice_lists_syn_count_torch_and_credits_the_ported_code() {
    let notice = read("NOTICE");
    let f = flat(&notice);
    assert!(f.contains("crates/flyvis/src/syn_count_torch.rs"), "NOTICE lists syn_count_torch.rs");
    let at = f.find("crates/flyvis/src/syn_count_torch.rs").unwrap();
    let entry = &f[at..(at + 200).min(f.len())];
    assert!(entry.contains("torch's `exp` of `SYN_COUNT`"), "{entry}");
    assert!(entry.contains("f32 and f64"), "{entry}");
    assert!(entry.contains("tools/export_syn_count.py"), "{entry}");

    let ported = f.find("Parts of semblance-flyvis").expect("NOTICE says parts of semblance-flyvis are ports");
    let para = &f[ported..(ported + 700).min(f.len())];
    assert!(para.contains("ports of flyvis's algorithms"), "{para}");
    assert!(para.contains("connectome expansion"), "{para}");
    assert!(para.contains("hex-eye fp16 mask construction"), "{para}");
    assert!(para.contains("ground truth"), "{para}");
    assert!(para.contains("MIT licence"), "{para}");
    assert!(para.contains("attribution"), "{para}");
    assert!(para.contains("changes no licence"), "{para}");
    // The flyvis MIT text is carried in full.
    assert!(f.contains("MIT License"));
    assert!(f.contains("Permission is hereby granted, free of charge"));
}

// ---------------------------------------------------------------------------
// RELEASE-06
// ---------------------------------------------------------------------------

#[test]
fn release_06_flyvis_sim_and_app_packages_carry_no_tests_and_rng_keeps_its_vectors() {
    let pkgs = packaged();
    for name in ["semblance-flyvis", "semblance-sim", "semblance"] {
        let (_, list) = &pkgs[name];
        let tests: Vec<&String> = list.iter().filter(|e| e.starts_with(&format!("{name}-0.3.0/tests/"))).collect();
        assert!(tests.is_empty(), "{name}.crate carries tests/: {tests:?}");
        let dir = PACKAGE_DIRS.iter().find(|(n, _)| *n == name).unwrap().1;
        assert!(
            workspace().join(dir).join("tests").is_dir(),
            "{dir}/tests exists in the workspace, so the exclusion is what keeps it out"
        );
    }
    let (_, rng) = &pkgs["semblance-rng"];
    assert!(
        rng.iter().any(|e| e == "semblance-rng-0.3.0/tests/vectors.rs"),
        "semblance-rng keeps tests/vectors.rs: {rng:?}"
    );
}

// ---------------------------------------------------------------------------
// RELEASE-07
// ---------------------------------------------------------------------------

#[test]
fn release_07_the_descriptions_and_categories_are_the_specs() {
    let desc = |n: &str| package(n).get("description").str().unwrap_or_else(|| panic!("{n} has a description")).to_string();

    assert_eq!(
        desc("semblance-sim"),
        "Neuron dynamics: flyvis's graded (non-spiking) integrator in f32 and f64, plus a legacy LIF \
         spiking baseline and the engine that couples a connectome to input and readout."
    );
    assert!(!desc("semblance-sim").contains("LIF baseline"));
    assert_eq!(
        desc("semblance-flyvis"),
        "flyvis (Lappalainen et al. 2024) in Rust: reads the published connectome spec, expands it \
         bit-exactly into the realised network, carries flyvis's trained parameters and ground truth \
         (MIT data; see NOTICE), and checks parity with flyvis's own traces. No external dependencies."
    );
    assert_eq!(
        desc("semblance-codec"),
        "The compress/uncompress round-trip: a Codec trait, a lossless WeightCodec baseline, and a \
         ReducedCodec whose encode and decode are not implemented yet: calling them panics (todo!())."
    );
    assert_eq!(
        desc("semblance"),
        "CLI that wires the pipeline together: simulate, round-trip, evolve, and validate parity with \
         flyvis. Std-only argument parsing."
    );

    let manifest = read("Cargo.toml");
    let pkg = toml_section(&manifest, "workspace.package");
    let cats = pkg.iter().find(|(k, _)| k == "categories").map(|(_, v)| v.as_str());
    assert_eq!(cats, Some("[\"science::neuroscience\", \"simulation\"]"));
    for (name, _) in PACKAGE_DIRS {
        let got: Vec<&str> = package(name).get("categories").arr().iter().filter_map(Json::str).collect();
        assert_eq!(got, ["science::neuroscience", "simulation"], "{name}'s categories");
    }
}

// ---------------------------------------------------------------------------
// RELEASE-08
// ---------------------------------------------------------------------------

#[test]
fn release_08_citation_is_version_0_3_0_dated_the_tag() {
    let cff = read("CITATION.cff");
    let field = |k: &str| {
        cff.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}: ")))
            .map(|v| v.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| panic!("CITATION.cff has no top-level {k}"))
    };
    assert_eq!(field("version"), "0.3.0");
    let date = field("date-released");
    let b = date.as_bytes();
    assert!(
        b.len() == 10 && b[4] == b'-' && b[7] == b'-' && date.chars().filter(char::is_ascii_digit).count() == 8,
        "date-released is YYYY-MM-DD: {date}"
    );

    // The tag date: checkable once the head has tagged v0.3.0.
    let tag = git(&["for-each-ref", "refs/tags/v0.3.0", "--format=%(objecttype) %(taggerdate:short)"]);
    let tag = text(&tag.stdout);
    let tag = tag.trim();
    if tag.is_empty() {
        eprintln!("skipping: no v0.3.0 tag in this clone, so date-released is not compared with it");
    } else {
        let (kind, tag_date) = tag.split_once(' ').unwrap_or((tag, ""));
        assert_eq!(kind, "tag", "v0.3.0 is an annotated tag");
        assert_eq!(date, tag_date, "date-released is the tag date");
    }
}

// ---------------------------------------------------------------------------
// RELEASE-09
// ---------------------------------------------------------------------------

/// The `//!` crate docs of a source file, as one flattened string.
fn crate_docs(rel: &str) -> String {
    let src = read(rel);
    let lines: Vec<&str> = src.lines().filter_map(|l| l.strip_prefix("//!")).collect();
    lines.join("\n")
}

#[test]
fn release_09_flyvis_crate_docs_have_licensing_and_parity_paragraphs_and_generated_files_are_untouched() {
    let docs = crate_docs("crates/flyvis/src/lib.rs");
    let lic_at = docs.find("# Licensing").expect("a Licensing section");
    let lic = flat(&docs[lic_at..]);
    let lic = &lic[..lic.find(" # ").unwrap_or(lic.len())];
    for item in ["[`pretrained`]", "[`syn_count_torch`]", "[`groundtruth`]"] {
        assert!(lic.contains(item), "Licensing names {item}: {lic}");
    }
    assert!(lic.contains("flyvis's data"), "{lic}");
    assert!(lic.contains("MIT licence"), "{lic}");
    assert!(lic.contains("port flyvis's MIT-licensed code"), "{lic}");
    assert!(lic.contains("NOTICE"), "{lic}");

    let f = flat(&docs);
    let par = f.find("[`parity`]").expect("a paragraph on parity");
    let para = &f[par..(par + 600).min(f.len())];
    assert!(para.contains("`semblance validate`"), "{para}");

    // The generated files are byte for byte what their scripts wrote (as of
    // the last regeneration, before this epic).
    for (rel, blob) in [
        ("crates/flyvis/src/pretrained.rs", "6e33c164e02e17f3c116e9b1396dd18adf94a9be"),
        ("crates/flyvis/src/syn_count_torch.rs", "09fe3006f9c4c2953b073c0a1dd15e0b452f64c6"),
    ] {
        let out = git(&["hash-object", "--", rel]);
        assert!(out.status.success(), "git hash-object {rel}: {}", text(&out.stderr));
        assert_eq!(text(&out.stdout).trim(), blob, "{rel} was edited");
    }
}

// ---------------------------------------------------------------------------
// Counts, shared by RELEASE-10 and RELEASE-11
// ---------------------------------------------------------------------------

/// The integer directly before `marker` in `s`.
fn number_before(s: &str, marker: &str) -> Option<u32> {
    let at = s.find(marker)?;
    let head = s[..at].trim_end();
    let digits: String = head.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
    digits.parse().ok()
}

fn claude_counts() -> (u32, u32) {
    let c = read("CLAUDE.md");
    let tests = number_before(&c, " tests, 0 failures").expect("CLAUDE.md gives the test count");
    let skips = number_before(&c, " of those tests skip").expect("CLAUDE.md gives the skip count");
    (tests, skips)
}

// ---------------------------------------------------------------------------
// RELEASE-10
// ---------------------------------------------------------------------------

#[test]
fn release_10_readme_has_the_headline_how_to_check_the_record_and_absolute_links() {
    let readme = read("README.md");
    let f = flat(&readme);

    // Headline.
    assert!(f.contains("0.3.0 reproduces flyvis 1.2.0's CPU reference"), "headline");
    assert!(f.contains("bit for bit"), "headline");
    assert!(f.contains("`syn_count_torch.rs`"), "headline names syn_count_torch.rs");
    assert!(f.contains("within pre-registered bounds"), "headline");
    assert!(f.contains("its own `exp`"), "headline: Rust's own exp");

    // How to check.
    assert!(f.contains("`ci/fetch-fixtures.sh`"));
    assert!(f.contains("`cargo run --release -- validate`"));
    assert!(f.contains("exits 0 if every gate holds, 1 if a gate failed, and 2 if"), "exit codes 0/1/2");

    // The record, linked absolutely, generated on hosted CI.
    let record = "(https://github.com/miki-przygoda/semblance/blob/main/docs/results/0.3.0-flyvis-parity.md)";
    let at = f.find(record).expect("an absolute link to the record");
    let around = &f[at.saturating_sub(200)..at];
    assert!(around.contains("hosted CI"), "the record is noted as generated on hosted CI: {around}");
    assert!(!readme.contains("](docs/results/"), "no relative link to the record");

    // Quickstart.
    let quick_at = readme.find("## Quickstart").expect("a Quickstart section");
    let quick = &readme[quick_at..];
    let quick = &quick[..quick[3..].find("\n## ").map(|i| i + 3).unwrap_or(quick.len())];
    assert!(quick.lines().any(|l| l.trim_start().starts_with("cargo run") && l.contains("-- validate")), "{quick}");

    // Crate table.
    let row = |c: &str| {
        readme
            .lines()
            .find(|l| l.starts_with(&format!("| `{c}` |")))
            .unwrap_or_else(|| panic!("README has a `{c}` row"))
            .to_string()
    };
    assert!(!row("flyvis").contains("in progress"), "{}", row("flyvis"));
    assert!(!row("flyvis").contains("(0.3.0"), "{}", row("flyvis"));
    assert!(row("app").contains("validate"), "{}", row("app"));

    // Counts: re-measured, the same as CLAUDE.md's, no SEQUENCES-08 sentence.
    let (_, skips) = claude_counts();
    let readme_skips = number_before(&f, " flyvis parity tests skip").expect("README gives the skip count");
    assert_eq!(readme_skips, skips, "README and CLAUDE.md agree on the skip count");
    assert_ne!(readme_skips, 123, "the skip count is re-measured, not 0.2.0's");
    assert!(!readme.contains("SEQUENCES-08"), "the SEQUENCES-08 sentence is gone");

    // LICENSE and NOTICE, linked absolutely.
    assert!(f.contains("[LICENSE](https://github.com/miki-przygoda/semblance/blob/main/LICENSE)"));
    assert!(f.contains("[NOTICE](https://github.com/miki-przygoda/semblance/blob/main/NOTICE)"));
    assert!(!readme.contains("](LICENSE)") && !readme.contains("](NOTICE)"), "no relative licence links");
}

// ---------------------------------------------------------------------------
// RELEASE-11
// ---------------------------------------------------------------------------

#[test]
fn release_11_claude_md_is_at_0_3_0_and_describes_validate_the_record_and_the_yanks() {
    let c = read("CLAUDE.md");
    let f = flat(&c);

    assert!(f.contains("Current: **v0.3.0**, crate versions `0.3.0`"), "the version line");

    let (tests, skips) = claude_counts();
    assert_ne!((tests, skips), (429, 123), "the counts are re-measured, not 0.2.0's");
    assert!(f.contains(&format!("\"{tests} passed, 0 failed\"")), "the passed count matches {tests}");
    assert!(f.contains(&format!("every one of the {skips} fails")), "the require-fixtures count matches {skips}");

    // validate's exit codes.
    assert!(f.contains("cargo run --release -- validate"));
    assert!(f.contains("Exit 0:") && f.contains("Exit 1:") && f.contains("Exit 2:"), "validate's exit codes");

    // The record.
    assert!(f.contains("docs/results/0.3.0-flyvis-parity.md"));
    assert!(f.contains("`parity-record` artifact"), "regenerated from CI's parity-record artifact");
    assert!(f.contains("never by hand or on a workstation"));
    assert!(f.contains("SEMBLANCE_REQUIRE_LIBM_RECORD"));

    // The tools/ boundary.
    assert!(f.contains("it never runs `tools/` or git"), "validate never runs tools/ or git");

    // The yank sentence, before the yanks.
    assert!(f.contains("`semblance-codec` 0.3.0 still contains both `todo!()`s; its description says so."));
    assert!(
        f.contains(
            "The six 0.1.0 releases (semblance, -codec, -connectome, -evo, -fidelity, -sim) are to be yanked \
             once all eight 0.3.0 crates are live"
        ) || f.contains("were yanked on"),
        "the yank sentence"
    );
    assert!(f.contains("yanking codec alone would strand evo, fidelity and semblance 0.1.0"));
    assert!(f.contains("Publishing and yanking wait for the owner's go-ahead") || f.contains("were yanked on"));
    assert!(f.contains("A real `ReducedCodec` later needs no yank of its own."));
    assert!(!f.contains("When a real implementation ships, `cargo yank` that version"), "the old yank sentence is gone");
}
