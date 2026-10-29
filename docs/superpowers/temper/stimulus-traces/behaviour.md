# Behaviour spec: stimulus-traces

Derived from `docs/superpowers/specs/2026-10-28-stimulus-traces-design.md`
only. Each behaviour states what the spec states or clearly implies.

## mapping: Mapping a hexal frame onto the photoreceptors

**MAPPING-01**
- Given input node lists for each input cell type and a frame of matching length
- When `wiring::scatter_hexals(nodes, frame, out)` is called
- Then `out[nodes[k][h]] == frame[h]` for every input type k and hexal h

**MAPPING-02**
- Given a buffer whose non-input nodes hold some values
- When `scatter_hexals` writes a frame
- Then every node not listed in `nodes` is left as it was, so a buffer zeroed once stays zero outside the input nodes

**MAPPING-03**
- Given a hand-built network with two input types
- When a frame is scattered
- Then exactly the listed nodes of both types are written and all other entries are untouched

**MAPPING-04**
- Given a type's node list whose length differs from the frame's length
- When `scatter_hexals` is called
- Then it returns `WiringError::FrameLength { expected, found }`

**MAPPING-05**
- Given a node index that is past the end of `out`
- When `scatter_hexals` is called
- Then it returns `WiringError::NodeOutOfRange { node, len }`

**MAPPING-06**
- Given any call to `scatter_hexals` that returns an error
- When the call returns
- Then `out` is unchanged, because all inputs are validated before anything is written

**MAPPING-07**
- Given the real flyvis fixture and, for each hexal h, a frame that is 1 at h and 0 elsewhere
- When the frame is scattered onto the input nodes (`input_mapping_matches_flyviss_lattice`)
- Then exactly 8 ones appear in `out`, each on a node of a distinct input type, and each such node's (u, v) equals row h of `data/flyvis-stimuli/hex_coords.csv`

**MAPPING-08**
- Given the mapping were transposed or reflected
- When the mapping gate runs
- Then it fails before any dynamics are run

## sequences: Reading the reference sequences

**SEQUENCES-01**
- Given a manifest JSON whose `sequences` entries carry `index`, `family`, `flyvis_index`, `args` and `frames`
- When `reference::parse_sequences(json)` is called
- Then it returns one `SequenceInfo` per entry with those fields, and `frames` holds one `SequenceFrames` per dt key with `n_frames`, `stimulus_frames` as [start, end) and `snapshot_frames`

**SEQUENCES-02**
- Given `args` containing numeric and non-numeric entries (such as `offsets`, `post_pad_mode`)
- When the sequences are parsed
- Then `args` holds every numeric entry in file order and the non-numeric entries are skipped

**SEQUENCES-03**
- Given a parsed `SequenceInfo`
- When `arg(name)` and `frames(dt)` are called
- Then they return the named numeric argument and the dt's `SequenceFrames` respectively, or `None` when absent

**SEQUENCES-04**
- Given a sequence entry missing a listed field, or with a field of the wrong type (including a missing dt entry)
- When `parse_sequences` is called
- Then it returns a `ReferenceError`

**SEQUENCES-05**
- Given a `snapshot_frames` list that is not ascending or contains a frame not inside `0..n_frames`
- When `parse_sequences` is called
- Then it returns an error

**SEQUENCES-06**
- Given a `stimulus_frames` window whose start exceeds its end, or whose end exceeds `n_frames`
- When `parse_sequences` is called
- Then it returns an error

**SEQUENCES-07**
- Given a sequence with 5 snapshot frames
- When the sequences are read
- Then it is rejected

**SEQUENCES-08**
- Given the existing `Manifest::parse` and its tests, and synthetic manifests with no or partial `sequences`
- When the sequence parser is added
- Then `Manifest::parse`, its tests and `acceptance_reference.rs` are unchanged and still pass

**SEQUENCES-09**
- Given the real reference manifest
- When its sequences are parsed (fixture test)
- Then there are 12 sequences with indices 0–11, every dt id present and 6 snapshot frames each, matching H6: flashes 0–3 (OFF/ON x full-field/radius 6) with 600 frames at 1/200 and 150 at 1/50 and windows [200,400) and [50,100); moving edges 4–11 at angles 0/30/90/150, OFF then ON, speed 13, height 80, with 525 and 131 frames and windows [200,325) and [50,81)

## traces: The stimulus-trace gates, the stimulus check and their controls

**TRACES-01**
- Given a state of the right length with only finite values
- When `Graded::set_state(&state)` is called and then `step` is called
- Then the result equals stepping from that state

**TRACES-02**
- Given a state of the wrong length, or containing a non-finite value
- When `Graded::set_state` is called
- Then it returns a `GradedError` and leaves the integrator's state unchanged

**TRACES-03**
- Given a reference run
- When a stage-2 run executes
- Then it settles as in piece 1 to S0, zeroes the input buffer once, and for each sequence 0–11 resets to S0, reads that sequence's `input.f64.bin` block for the run's dt (shape [n_frames, 721]), and for each frame scatters `R::from_f64(x[i][h])` onto the input nodes and steps once

**TRACES-04**
- Given a stage-2 run in progress
- When each step completes
- Then the 65 centre values are recorded, the full state is recorded at the `snapshot_frames`, and the state is scanned for non-finite values

**TRACES-05**
- Given a completed stage-2 sequence
- When it is compared with `centre.<p>.bin` and `snapshots.<p>.bin` for (run, sequence)
- Then the shapes [n_frames, 65] and [6, 45669] are asserted

**TRACES-06**
- Given each of the 8 reference runs and the torch counts
- When `trace_exact_run_<n>_<model>_<dt>_<p>` runs
- Then every centre value at every frame and every snapshot value equals flyvis's (`==`) in all 12 sequences

**TRACES-07**
- Given an exact trace run in which some value differs
- When the test fails
- Then it prints, per sequence, the differ counts, the first differing frame and its cell type, and the largest error

**TRACES-08**
- Given each of the 8 reference runs and the libm counts
- When `trace_libm_run_…` runs
- Then each sequence's centre and snapshot maxima are within 1e-10 in f64, and within 2 x flyvis's f32/f64 gap for that (model, dt) in f32, centre and snapshots bounded separately

**TRACES-09**
- Given a libm trace run
- When it finishes
- Then it prints its worst sequence, each maximum with frame, node and cell type, the ratio to the bound in `{:.3e}`, and the bit-equal fraction

**TRACES-10**
- Given the hard-coded f32 trace bounds
- When `trace_bounds_match_provenance` runs
- Then the gaps equal PROVENANCE's `centre` and `snapshots` `f32_f64_gap` entries bit for bit

**TRACES-11**
- Given the reference data
- When `snapshots_agree_with_the_centre_column` runs
- Then `centre[snapshot_frames]` equals `snapshots[:, centre_nodes]` exactly

**TRACES-12**
- Given run 3 (000, 0.02, f64), sequence 8 (90 degree OFF edge), with each frame's hexals mirrored (u, v) -> (-u, -v) before scattering
- When `the_trace_gate_sees_a_wrong_mapping` runs
- Then the snapshot maximum error exceeds 1e-3

**TRACES-13**
- Given the trace files (`centre.*.bin`, `snapshots.*.bin`, `input.f64.bin`)
- When the tests run
- Then each is loaded lazily in its own `OnceLock` only by the trace tests, and the settling gates neither load nor depend on them

**TRACES-14**
- Given the shared settling machinery (fixture, `Precision`, `Counts`, `diff`, reporters, `settle`) moved into `gate_support.rs`
- When the settling tests run
- Then each keeps its name and module path `settling_checks::<name>`, its behaviour, and its panic and print strings byte-identical apart from the ratio format, so `tests/acceptance_settling.rs` still finds them

**TRACES-15**
- Given a large trace fixture is absent
- When a trace test runs without `SEMBLANCE_REQUIRE_FIXTURES=1`
- Then it skips with a notice; under `SEMBLANCE_REQUIRE_FIXTURES=1` it fails

**TRACES-16**
- Given `Flashes::new(FlashParams::flyvis_defaults(dt))` at both dts, each Rust sequence matched to a reference sequence by intensity and radius
- When the stimulus check runs
- Then `num_frames` and the window `[n_pre, n_pre + n_stim)` equal the manifest's, and every frame equals the reference input exactly after widening f32 to f64

**TRACES-17**
- Given `MovingEdge::new` with `flyvis_defaults(dt)` modified to height 80, speed 13 and angles 0/30/90/150, offsets left at `(-10, 10)`
- When the stimulus check runs
- Then it asserts the inclusive offsets correspond to the manifest's exclusive `[-10, 11)`, Rust sequences are matched by angle, intensity and speed, and `n_frames` and the stimulus window equal the manifest's

**TRACES-18**
- Given a `MovingEdge`
- When `stimulus_frames(sequence)` is called
- Then it returns the sequence's stimulus window as `Some((start, end))`, or `None` for a sequence that does not exist

**TRACES-19**
- Given the Rust moving-edge frames
- When compared with the reference input
- Then every frame lies within `TOLERANCE` 0.005, which is the `pub(crate)` constant imported from `moving_edge_checks.rs`, not a copy

**TRACES-20**
- Given the 0 degree moving-edge sequences
- When compared with the reference input
- Then each Rust f32 frame equals `x as f32` bit for bit

**TRACES-21**
- Given the moving-edge check
- When it runs
- Then it prints the per-angle maximum

**TRACES-22**
- Given a Rust stimulus frame outside `TOLERANCE`, or a frame-count mismatch
- When the stimulus check runs
- Then the test fails, naming the sequence, frame and hexal

**TRACES-23**
- Given the dev profile in the workspace `Cargo.toml`
- When `semblance-sim` and `semblance-flyvis` are built
- Then they build at `opt-level = 3`, with no fast-math or FMA flag, and the exact gates still pass bit for bit

## minors: The integrator and settling-gate fixes from PR #12

**MINORS-01**
- Given `test_support::read_fixture_bytes(path)` or `read_fixture_text(path)` and a missing file
- When called without `SEMBLANCE_REQUIRE_FIXTURES=1`
- Then it returns `Err(notice)` without printing; under `SEMBLANCE_REQUIRE_FIXTURES=1` it panics as the existing loaders do

**MINORS-02**
- Given the existing fixture loaders
- When they are rewritten as wrappers over the new readers
- Then they still print the notice and their behaviour is unchanged

**MINORS-03**
- Given a gate fixture that failed to load (`Result<Fixture, String>` is `Err`)
- When each gate test runs
- Then it prints `skipping: {notice}`, so every skipped test is visible under `--nocapture`

**MINORS-04**
- Given a ratio printed by the settling gates
- When it is reported
- Then it is formatted with `{:.3e}`, so an f64 ratio no longer reads `0.000`, and SETTLING-04 in `tests/acceptance_settling.rs` asserts the new text

**MINORS-05**
- Given `export_syn_count.py`
- When it regenerates `syn_count_torch.rs`
- Then the header contains no date (no `exported`), only the header changes, and no value changes

**MINORS-06**
- Given a dt whose `dt as f32` is subnormal, such as `f32::MIN_POSITIVE as f64 / 2.0` or `f32::from_bits(1)`
- When `Graded::new` is called
- Then it returns `BadDt`

**MINORS-07**
- Given a dt equal to `f32::MIN_POSITIVE as f64`
- When `Graded::new` is called
- Then it is accepted

**MINORS-08**
- Given a dt that is zero, negative, or non-finite
- When `Graded::new` is called
- Then it is still refused, because the `dt > 0` and finiteness checks remain alongside `is_normal`

**MINORS-09**
- Given a reference containing a NaN value
- When `diff` computes the maximum error
- Then the NaN error is treated as infinity and the maximum is infinite, not NaN; `max_err` in `tests/acceptance_settling.rs` behaves the same way

**MINORS-10**
- Given the sealed `Real` trait
- When `is_nan` is called on a value
- Then it reports whether the value is NaN

**MINORS-11**
- Given `relu` applied to NaN
- When it is evaluated
- Then it returns NaN, as `torch.relu` does; positive inputs return themselves and other inputs return zero

**MINORS-12**
- Given the integer-count control's doc comment and piece 1's spec
- When the wording is amended
- Then both say the control resolves integer counts against the f64 bound and that in f32 the exact gate is the guard, with no assertion changed

**MINORS-13**
- Given `acceptance_settling.rs` builds a shadow workspace
- When it runs
- Then it removes the copied source at `.../acceptance-settling/workspace` with `remove_dir_all`, ignoring NotFound, before copying, and keeps the sibling target dir

**MINORS-14**
- Given `acceptance_integrator.rs` builds a shadow workspace under `integrator_15_sealed`
- When it runs
- Then it removes the copied source with `remove_dir_all`, ignoring NotFound, before copying, and keeps its `target`

**MINORS-15**
- Given the minor fixes are in place
- When the settling and stimulus gates run
- Then their results are unchanged
