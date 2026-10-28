//! Acceptance tests for behaviour group `reference` of epic graded-settling:
//! reading the reference manifest and its binaries through the crate's
//! public `semblance_flyvis::reference` API.
//!
//! One test per behaviour id (REFERENCE-01 .. REFERENCE-10). Manifests and
//! file bytes are built here; the real `data/flyvis-reference/` is read only
//! by REFERENCE-09, which skips when it is absent unless
//! `SEMBLANCE_REQUIRE_FIXTURES=1`.

use semblance_flyvis::reference::{
    Array, BlockKey, CentreNode, DtInfo, Dtype, Manifest, ReferenceError, RunInfo,
};

/// Header fields of a synthetic manifest, including every key the reader
/// is meant to ignore.
const HEADER: &str = r#""schema_version":1,
  "generator":"tools/export_reference.py",
  "flyvis_version":"1.2.0",
  "byte_order":"little",
  "array_order":"C",
  "n_nodes":7,
  "n_hexals":3,
  "t_pre":1.0,
  "pre_stim_value":0.5,
  "batch_size":1,
  "dts":[{"id":"1/200","dt":0.005,"floor_dt_f32":0.004999999888241291,"n_settle":200},
         {"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}],
  "models":[{"id":"000","name":"flow/0000/000","validation_loss":5.1,"rank":"best"}],
  "runs":[{"index":0,"model":"000","dt":"1/200","precision":"f32"},
          {"index":1,"model":"000","dt":"1/50","precision":"f64"}],
  "frame_semantics":{"frame":"frame i: ...","nested":[1,{"x":null}]},
  "centre_nodes":[{"node":2,"cell_type":"R1"},{"node":5,"cell_type":"T4a"}],
  "transients_visible_on":"centre",
  "sequences":[{"index":0,"family":"flashes","frames":{"1/50":{"n_frames":3}}}],"#;

/// f32 file: run 0 settled [3]; run 1, sequence 4 [2,2]; run null (input),
/// sequence 4 [1,2]. 12 + 16 + 8 = 36 bytes.
const F32_BLOCKS: [&str; 3] = [
    r#"{"run":0,"dt":"1/200","sequence":null,"offset":0,"bytes":12,"shape":[3,1]}"#,
    r#"{"run":1,"dt":"1/50","sequence":4,"offset":12,"bytes":16,"shape":[2,2]}"#,
    r#"{"run":null,"dt":"1/50","sequence":4,"offset":28,"bytes":8,"shape":[1,2]}"#,
];

/// f64 file: run null, dt 1/50, sequences 0 and 1, each [2]. 16 + 16 bytes.
const F64_BLOCKS: [&str; 2] = [
    r#"{"run":null,"dt":"1/50","sequence":0,"offset":0,"bytes":16,"shape":[2]}"#,
    r#"{"run":null,"dt":"1/50","sequence":1,"offset":16,"bytes":16,"shape":[2]}"#,
];

fn f32_file(bytes: usize, blocks: &[&str]) -> String {
    format!(
        r#"{{"name":"a.f32.bin","dtype":"<f4","axes":["x","y"],"bytes":{bytes},"sha256":"0000","blocks":[{}]}}"#,
        blocks.join(",")
    )
}

fn f64_file(bytes: usize, blocks: &[&str]) -> String {
    format!(
        r#"{{"name":"b.f64.bin","dtype":"<f8","axes":["seq"],"bytes":{bytes},"sha256":"not-a-real-hash","blocks":[{}]}}"#,
        blocks.join(",")
    )
}

fn manifest_with(files: &[String]) -> String {
    format!("{{{HEADER}\n  \"files\":[{}]}}", files.join(","))
}

fn good() -> String {
    manifest_with(&[f32_file(36, &F32_BLOCKS), f64_file(32, &F64_BLOCKS)])
}

/// The same manifest as `good()` with every key the reader ignores removed.
fn minimal() -> String {
    format!(
        r#"{{"schema_version":1,"byte_order":"little","array_order":"C","n_nodes":7,"t_pre":1.0,"pre_stim_value":0.5,
          "dts":[{{"id":"1/200","dt":0.005,"floor_dt_f32":0.004999999888241291,"n_settle":200}},
                 {{"id":"1/50","dt":0.02,"floor_dt_f32":0.019999999552965164,"n_settle":50}}],
          "runs":[{{"index":0,"model":"000","dt":"1/200","precision":"f32"}},
                  {{"index":1,"model":"000","dt":"1/50","precision":"f64"}}],
          "centre_nodes":[{{"node":2,"cell_type":"R1"}},{{"node":5,"cell_type":"T4a"}}],
          "files":[{},{}]}}"#,
        f32_file(36, &F32_BLOCKS).replace(r#""sha256":"0000","#, ""),
        f64_file(32, &F64_BLOCKS).replace(r#""sha256":"not-a-real-hash","#, ""),
    )
}

/// A manifest whose f32 file has these blocks and this total size.
fn with_f32(bytes: usize, blocks: &[&str]) -> String {
    manifest_with(&[f32_file(bytes, blocks), f64_file(32, &F64_BLOCKS)])
}

fn f32_bytes() -> Vec<u8> {
    (1..=9).flat_map(|i| (i as f32).to_le_bytes()).collect()
}

fn f64_bytes() -> Vec<u8> {
    [10.5f64, -2.25, 1e-300, 7.0].iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn refuse(text: &str) -> ReferenceError {
    match Manifest::parse(text) {
        Ok(_) => panic!("manifest must be refused:\n{text}"),
        Err(e) => e,
    }
}

#[test]
fn reference_01_parses_a_valid_manifest_ignoring_unread_keys() {
    let m = Manifest::parse(&good()).expect("valid manifest");
    assert_eq!(m.n_nodes, 7);
    assert_eq!(m.t_pre, 1.0);
    assert_eq!(m.pre_stim_value, 0.5);
    assert_eq!(
        m.dts,
        vec![
            DtInfo { id: "1/200".into(), dt: 0.005, floor_dt_f32: 0.004999999888241291, n_settle: 200 },
            DtInfo { id: "1/50".into(), dt: 0.02, floor_dt_f32: 0.019999999552965164, n_settle: 50 },
        ]
    );
    assert_eq!(
        m.runs,
        vec![
            RunInfo { index: 0, model: "000".into(), dt: "1/200".into(), precision: Dtype::F32 },
            RunInfo { index: 1, model: "000".into(), dt: "1/50".into(), precision: Dtype::F64 },
        ]
    );
    assert_eq!(
        m.centre_nodes,
        vec![CentreNode { node: 2, cell_type: "R1".into() }, CentreNode { node: 5, cell_type: "T4a".into() }]
    );
    assert_eq!(m.files.len(), 2);
    let a = &m.files[0];
    assert_eq!((a.name.as_str(), a.dtype, a.bytes), ("a.f32.bin", Dtype::F32, 36));
    assert_eq!(a.axes, vec!["x".to_string(), "y".to_string()]);
    assert_eq!(a.blocks.len(), 3);
    assert_eq!((a.blocks[0].run, a.blocks[0].dt.as_str(), a.blocks[0].sequence), (Some(0), "1/200", None));
    assert_eq!((a.blocks[1].offset, a.blocks[1].bytes, a.blocks[1].shape.clone()), (12, 16, vec![2, 2]));
    assert_eq!((a.blocks[2].run, a.blocks[2].sequence), (None, Some(4)));
    let b = &m.files[1];
    assert_eq!((b.name.as_str(), b.dtype, b.bytes, b.blocks.len()), ("b.f64.bin", Dtype::F64, 32, 2));

    // Removing every ignored key changes nothing.
    let stripped = minimal();
    assert_eq!(Manifest::parse(&stripped).expect("stripped manifest"), m);
}

#[test]
fn reference_02_rejects_unsupported_schema_byte_order_or_array_order() {
    let g = good();
    let cases = [
        g.replace(r#""schema_version":1"#, r#""schema_version":2"#),
        g.replace(r#""schema_version":1"#, r#""schema_version":0"#),
        g.replace(r#""byte_order":"little""#, r#""byte_order":"big""#),
        g.replace(r#""array_order":"C""#, r#""array_order":"F""#),
    ];
    for (text, field) in cases.iter().zip(["schema_version", "schema_version", "byte_order", "array_order"]) {
        assert_ne!(text, &g);
        match refuse(text) {
            ReferenceError::Unsupported { field: f, .. } => assert_eq!(f, field),
            other => panic!("{field}: expected Unsupported, got {other:?}"),
        }
    }
}

#[test]
fn reference_03_rejects_overlap_gap_or_wrong_total() {
    let [b0, b1, b2] = F32_BLOCKS;
    let file = "a.f32.bin".to_string();

    // Overlap: block 1 starts inside block 0.
    let overlap = b1.replace(r#""offset":12"#, r#""offset":8"#);
    assert!(matches!(refuse(&with_f32(36, &[b0, &overlap, b2])), ReferenceError::BlockLayout { file: f, block: 1 } if f == file));

    // Gap: block 2 starts 4 bytes after block 1 ends.
    let gap = b2.replace(r#""offset":28"#, r#""offset":32"#);
    assert!(matches!(refuse(&with_f32(40, &[b0, b1, &gap])), ReferenceError::BlockLayout { file: f, block: 2 } if f == file));

    // First block not at offset 0.
    let late = b0.replace(r#""offset":0"#, r#""offset":4"#);
    assert!(matches!(refuse(&with_f32(36, &[&late, b1, b2])), ReferenceError::BlockLayout { file: f, block: 0 } if f == file));

    // Blocks cover 36 bytes; the file claims more, or fewer.
    assert_eq!(refuse(&with_f32(40, &F32_BLOCKS)), ReferenceError::BlockTotal { file: file.clone(), expected: 40, found: 36 });
    assert_eq!(refuse(&with_f32(32, &F32_BLOCKS)), ReferenceError::BlockTotal { file: file.clone(), expected: 32, found: 36 });

    // A file with no blocks but nonzero size.
    assert_eq!(refuse(&with_f32(36, &[])), ReferenceError::BlockTotal { file, expected: 36, found: 0 });
}

#[test]
fn reference_04_rejects_bytes_not_matching_shape_or_rank_not_matching_axes() {
    let [b0, b1, b2] = F32_BLOCKS;
    let file = "a.f32.bin".to_string();

    // bytes 12 but shape [4,1] => 16.
    let wrong = b0.replace("[3,1]", "[4,1]");
    assert_eq!(refuse(&with_f32(36, &[&wrong, b1, b2])), ReferenceError::BlockShape { file: file.clone(), block: 0 });

    // bytes sized as if f64 (shape [2,2] would be 32 bytes as f8, 16 as f4).
    let as_f64 = b1.replace(r#""bytes":16"#, r#""bytes":32"#).replace(r#""offset":28"#, r#""offset":44"#);
    let b2_moved = b2.replace(r#""offset":28"#, r#""offset":44"#);
    assert_eq!(refuse(&with_f32(52, &[b0, &as_f64, &b2_moved])), ReferenceError::BlockShape { file: file.clone(), block: 1 });

    // Rank 1 and rank 3 against two axes.
    let rank1 = b0.replace("[3,1]", "[3]");
    assert_eq!(refuse(&with_f32(36, &[&rank1, b1, b2])), ReferenceError::BlockAxes { file: file.clone(), block: 0 });
    let rank3 = b2.replace("[1,2]", "[1,2,1]");
    assert_eq!(refuse(&with_f32(36, &[b0, b1, &rank3])), ReferenceError::BlockAxes { file: file.clone(), block: 2 });

    // A shape whose product overflows usize is refused, not a panic.
    let huge = b0.replace("[3,1]", "[4294967296,4294967296]");
    assert_eq!(refuse(&with_f32(36, &[&huge, b1, b2])), ReferenceError::BlockShape { file, block: 0 });
}

#[test]
fn reference_05_rejects_two_blocks_with_the_same_key() {
    let [b0, b1, b2] = F32_BLOCKS;
    let file = "a.f32.bin".to_string();

    // Block 2 becomes run 1, 1/50, seq 4 -- the same as block 1.
    let dup = b2.replace(r#""run":null"#, r#""run":1"#);
    assert_eq!(refuse(&with_f32(36, &[b0, b1, &dup])), ReferenceError::DuplicateBlock { file: file.clone(), block: 2 });

    // Duplicate keys with None in both run and sequence.
    let [c0, c1] = F64_BLOCKS;
    let c1_dup = c1.replace(r#""sequence":1"#, r#""sequence":0"#);
    let text = manifest_with(&[f32_file(36, &F32_BLOCKS), f64_file(32, &[c0, &c1_dup])]);
    assert_eq!(refuse(&text), ReferenceError::DuplicateBlock { file: "b.f64.bin".into(), block: 1 });

    // Differing in only one field is not a duplicate.
    let only_dt = b2.replace(r#""run":null"#, r#""run":1"#).replace(r#""dt":"1/50""#, r#""dt":"1/200""#);
    assert!(Manifest::parse(&with_f32(36, &[b0, b1, &only_dt])).is_ok());
}

#[test]
fn reference_06_file_lookup_by_name() {
    let m = Manifest::parse(&good()).unwrap();
    let a = m.file("a.f32.bin").expect("a.f32.bin present");
    assert_eq!((a.name.as_str(), a.dtype, a.bytes), ("a.f32.bin", Dtype::F32, 36));
    let b = m.file("b.f64.bin").expect("b.f64.bin present");
    assert_eq!((b.name.as_str(), b.dtype, b.bytes), ("b.f64.bin", Dtype::F64, 32));
    assert!(m.file("missing.bin").is_none());
    assert!(m.file("").is_none());
    assert!(m.file("a.f32").is_none());
    assert!(m.file("A.F32.BIN").is_none());
}

#[test]
fn reference_07_reads_the_matching_block_with_its_shape() {
    let m = Manifest::parse(&good()).unwrap();
    let a = m.file("a.f32.bin").unwrap();
    let bytes = f32_bytes();
    assert_eq!(
        a.read_f32(&bytes, &BlockKey { run: Some(0), dt: "1/200", sequence: None }),
        Ok(Array { shape: vec![3, 1], data: vec![1.0, 2.0, 3.0] })
    );
    assert_eq!(
        a.read_f32(&bytes, &BlockKey { run: Some(1), dt: "1/50", sequence: Some(4) }),
        Ok(Array { shape: vec![2, 2], data: vec![4.0, 5.0, 6.0, 7.0] })
    );
    // run: null is selected by dt and sequence.
    assert_eq!(
        a.read_f32(&bytes, &BlockKey { run: None, dt: "1/50", sequence: Some(4) }),
        Ok(Array { shape: vec![1, 2], data: vec![8.0, 9.0] })
    );

    let b = m.file("b.f64.bin").unwrap();
    let bytes = f64_bytes();
    let r0 = b.read_f64(&bytes, &BlockKey { run: None, dt: "1/50", sequence: Some(0) }).unwrap();
    assert_eq!(r0.shape, vec![2]);
    assert_eq!(r0.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), [10.5f64, -2.25].map(f64::to_bits));
    let r1 = b.read_f64(&bytes, &BlockKey { run: None, dt: "1/50", sequence: Some(1) }).unwrap();
    assert_eq!(r1.data.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), [1e-300f64, 7.0].map(f64::to_bits));

    // Little-endian, not native or big: a non-palindromic bit pattern.
    let le: Vec<u8> = [f32::from_bits(0x3F80_0001), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let first = a.read_f32(&le, &BlockKey { run: Some(0), dt: "1/200", sequence: None }).unwrap();
    assert_eq!(first.data[0].to_bits(), 0x3F80_0001);
}

#[test]
fn reference_08_read_errors_without_panicking() {
    let m = Manifest::parse(&good()).unwrap();
    let a = m.file("a.f32.bin").unwrap();
    let b = m.file("b.f64.bin").unwrap();
    let ka = BlockKey { run: Some(0), dt: "1/200", sequence: None };
    let kb = BlockKey { run: None, dt: "1/50", sequence: Some(0) };

    // Wrong dtype, both directions.
    assert_eq!(
        a.read_f64(&f32_bytes(), &ka),
        Err(ReferenceError::Dtype { file: "a.f32.bin".into(), expected: Dtype::F64, found: Dtype::F32 })
    );
    assert_eq!(
        b.read_f32(&f64_bytes(), &kb),
        Err(ReferenceError::Dtype { file: "b.f64.bin".into(), expected: Dtype::F32, found: Dtype::F64 })
    );

    // Byte slices shorter, longer, and empty.
    let full = f32_bytes();
    assert_eq!(a.read_f32(&full[..35], &ka), Err(ReferenceError::ByteLength { file: "a.f32.bin".into(), expected: 36, found: 35 }));
    let mut long = full.clone();
    long.extend_from_slice(&[0; 4]);
    assert_eq!(a.read_f32(&long, &ka), Err(ReferenceError::ByteLength { file: "a.f32.bin".into(), expected: 36, found: 40 }));
    assert_eq!(a.read_f32(&[], &ka), Err(ReferenceError::ByteLength { file: "a.f32.bin".into(), expected: 36, found: 0 }));
    assert_eq!(b.read_f64(&f64_bytes()[..8], &kb), Err(ReferenceError::ByteLength { file: "b.f64.bin".into(), expected: 32, found: 8 }));

    // Keys matching no block, including one that differs only in a None.
    let no_block = ReferenceError::NoBlock { file: "a.f32.bin".into() };
    for key in [
        BlockKey { run: Some(9), dt: "1/200", sequence: None },
        BlockKey { run: Some(0), dt: "1/50", sequence: None },
        BlockKey { run: Some(0), dt: "1/200", sequence: Some(0) },
        BlockKey { run: None, dt: "1/200", sequence: None },
        BlockKey { run: Some(1), dt: "1/50", sequence: None },
        BlockKey { run: Some(0), dt: "", sequence: None },
    ] {
        assert_eq!(a.read_f32(&full, &key), Err(no_block.clone()), "{key:?}");
    }
    assert_eq!(
        b.read_f64(&f64_bytes(), &BlockKey { run: Some(0), dt: "1/50", sequence: Some(0) }),
        Err(ReferenceError::NoBlock { file: "b.f64.bin".into() })
    );
}

fn reference_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/flyvis-reference")
}

fn real_manifest() -> Option<String> {
    let path = format!("{}/manifest.json", reference_dir());
    match std::fs::read_to_string(&path) {
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
fn reference_09_real_manifest_parses_and_nine_file_lengths_match() {
    let Some(text) = real_manifest() else { return };
    let m = Manifest::parse(&text).expect("data/flyvis-reference/manifest.json must parse");
    assert_eq!(m.files.len(), 9, "{:?}", m.files.iter().map(|f| &f.name).collect::<Vec<_>>());
    let mut names: Vec<&str> = m.files.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 9, "file names must be distinct");
    for f in &m.files {
        let path = format!("{}/{}", reference_dir(), f.name);
        let len = std::fs::metadata(&path).unwrap_or_else(|e| panic!("{path}: {e}")).len();
        assert_eq!(len as usize, f.bytes, "{}", f.name);
        // The real bytes are accepted by the reader for the file's first block.
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let blk = &f.blocks[0];
        let key = BlockKey { run: blk.run, dt: &blk.dt, sequence: blk.sequence };
        let shape = match f.dtype {
            Dtype::F32 => f.read_f32(&bytes, &key).map(|a| a.shape),
            Dtype::F64 => f.read_f64(&bytes, &key).map(|a| a.shape),
        }
        .unwrap_or_else(|e| panic!("{}: {e}", f.name));
        assert_eq!(shape, blk.shape, "{}", f.name);
    }
}

/// Removes `"key":<value>` for the first occurrence of `"key":` (value is a
/// scalar, string, or bracketed array/object), plus one adjacent comma.
fn drop_key(text: &str, key: &str, nth: usize) -> Option<String> {
    let pat = format!("\"{key}\":");
    let start = text.match_indices(&pat).nth(nth)?.0;
    let vstart = start + pat.len();
    let bytes = text.as_bytes();
    let mut i = vstart;
    let mut depth = 0i32;
    let mut in_str = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                in_str = false;
                if depth == 0 {
                    i += 1;
                    break;
                }
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'[' | b'{' => depth += 1,
                b']' | b'}' if depth == 0 => break,
                b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                b',' if depth == 0 => break,
                _ => {}
            }
        }
        i += 1;
    }
    let mut end = i;
    let rest = &text[end..];
    let trimmed = rest.trim_start();
    if trimmed.starts_with(',') {
        end += rest.len() - trimmed.len() + 1;
        Some(format!("{}{}", &text[..start], &text[end..]))
    } else {
        // Last key: remove the preceding comma instead.
        let before = text[..start].trim_end();
        let before = before.strip_suffix(',').unwrap_or(before);
        Some(format!("{}{}", before, &text[end..]))
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn reference_10_typed_errors_naming_field_or_file_no_panics_no_hashing() {
    // Every required key, removed, yields a typed error naming it.
    let g = good();
    let core = minimal();
    let required = [
        "schema_version", "byte_order", "array_order", "n_nodes", "t_pre", "pre_stim_value", "dts", "runs",
        "centre_nodes", "files", "id", "dt", "floor_dt_f32", "n_settle", "index", "model", "precision", "node",
        "cell_type", "name", "dtype", "axes", "bytes", "blocks", "run", "sequence", "offset", "shape",
    ];
    for key in required {
        let mut n = 0;
        while let Some(text) = drop_key(&core, key, n) {
            n += 1;
            let e = refuse(&text);
            let msg = e.to_string();
            assert!(msg.contains(key), "dropping {key} (#{n}): {e:?} / {msg:?} does not name the field");
        }
        assert!(n > 0, "{key} not found in the synthetic manifest");
    }

    // Wrong types name the field.
    for (from, to, field) in [
        (r#""n_nodes":7"#, r#""n_nodes":"7""#, "n_nodes"),
        (r#""n_nodes":7"#, r#""n_nodes":-7"#, "n_nodes"),
        (r#""t_pre":1.0"#, r#""t_pre":null"#, "t_pre"),
        (r#""dts":["#, r#""dts":{"a":1},"x":["#, "dts"),
        (r#""offset":12"#, r#""offset":-12"#, "offset"),
        (r#""shape":[2,2]"#, r#""shape":[2,"2"]"#, "shape"),
        (r#""run":1,"dt":"1/50","sequence":4"#, r#""run":1.5,"dt":"1/50","sequence":4"#, "run"),
        (r#""run":1,"dt":"1/50","sequence":4"#, r#""run":1,"dt":"1/50","sequence":"4""#, "sequence"),
        (r#""precision":"f32""#, r#""precision":"f16""#, "precision"),
        (r#""dtype":"<f4""#, r#""dtype":">f4""#, "dtype"),
    ] {
        let text = g.replacen(from, to, 1);
        assert_ne!(text, g, "{from}");
        let e = refuse(&text);
        assert!(e.to_string().contains(field), "{from} -> {to}: {e:?} / {e} does not name {field}");
    }

    // File-level errors name the file.
    let [b0, b1, b2] = F32_BLOCKS;
    for text in [
        with_f32(40, &F32_BLOCKS),
        with_f32(36, &[&b0.replace("[3,1]", "[3]"), b1, b2]),
        with_f32(36, &[&b0.replace("[3,1]", "[4,1]"), b1, b2]),
        with_f32(36, &[b0, &b1.replace(r#""offset":12"#, r#""offset":8"#), b2]),
        with_f32(36, &[b0, b1, &b2.replace(r#""run":null"#, r#""run":1"#)]),
    ] {
        let e = refuse(&text);
        assert!(e.to_string().contains("a.f32.bin"), "{e:?} / {e} does not name the file");
    }
    let m = Manifest::parse(&g).unwrap();
    let a = m.file("a.f32.bin").unwrap();
    for e in [
        a.read_f64(&f32_bytes(), &BlockKey { run: Some(0), dt: "1/200", sequence: None }).unwrap_err(),
        a.read_f32(&[0; 3], &BlockKey { run: Some(0), dt: "1/200", sequence: None }).unwrap_err(),
        a.read_f32(&f32_bytes(), &BlockKey { run: Some(7), dt: "1/200", sequence: None }).unwrap_err(),
    ] {
        assert!(e.to_string().contains("a.f32.bin"), "{e:?} / {e} does not name the file");
    }

    // No hashing: sha256 values are not checked (they are garbage here), and
    // bytes that match no hash are read. No PROVENANCE.json is involved: the
    // API takes text and bytes only, and this test runs with no such file
    // named anywhere.
    let garbage_hash = g.replace(r#""sha256":"0000""#, r#""sha256":"deadbeef""#);
    let m2 = Manifest::parse(&garbage_hash).expect("sha256 is not verified");
    assert_eq!(m2, m);
    assert!(m2.file("a.f32.bin").unwrap().read_f32(&[0xAB; 36], &BlockKey { run: Some(0), dt: "1/200", sequence: None }).is_ok());

    // Never panics: truncations, byte mutations, and random reads.
    let src = g.as_bytes();
    for cut in 0..src.len() {
        if let Ok(t) = std::str::from_utf8(&src[..cut]) {
            let _ = Manifest::parse(t);
        }
    }
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let alphabet = b"0123456789-.,:[]{}\"en tlf\\";
    for _ in 0..20_000 {
        let mut v = src.to_vec();
        for _ in 0..1 + rng.below(4) {
            let i = rng.below(v.len());
            match rng.below(3) {
                0 => v[i] = alphabet[rng.below(alphabet.len())],
                1 => {
                    v.remove(i);
                }
                _ => v.insert(i, alphabet[rng.below(alphabet.len())]),
            }
        }
        if let Ok(t) = std::str::from_utf8(&v) {
            if let Ok(m) = Manifest::parse(t) {
                for f in &m.files {
                    let len = rng.below(f.bytes.min(1 << 16) + 9);
                    let bytes = vec![0u8; len];
                    for blk in &f.blocks {
                        let key = BlockKey { run: blk.run, dt: &blk.dt, sequence: blk.sequence };
                        let _ = f.read_f32(&bytes, &key);
                        let _ = f.read_f64(&bytes, &key);
                    }
                }
            }
        }
    }
    // Large offsets and sizes are refused, not overflowed.
    let max = usize::MAX.to_string();
    let _ = Manifest::parse(&with_f32(36, &[&b0.replace(r#""offset":0"#, &format!(r#""offset":{max}"#)), b1, b2]));
    let _ = Manifest::parse(&with_f32(usize::MAX, &F32_BLOCKS));
}
