# Behaviour spec: graded-settling

Derived only from `docs/superpowers/specs/2026-10-28-graded-settling-design.md`.
Each behaviour is a given/when/then statement with a stable id.

## ensemble: Reading flyvis's trained ensemble

### ENSEMBLE-01
- Given a well-formed ensemble CSV with exactly 50 models, with or without a trailing newline
- When `Ensemble::parse` reads it
- Then it succeeds, `ids()` yields the model ids in file order, and `model(id)` returns that model's `ModelParams` (65 biases, 65 time constants, 604 synapse strengths)

### ENSEMBLE-02
- Given any CSV input
- When `Ensemble::parse` reads it
- Then it performs no file I/O and never panics; every violation is an `EnsembleError` naming the line number and what was expected

### ENSEMBLE-03
- Given a CSV containing a `\r` anywhere
- When it is parsed
- Then it is rejected with the line number

### ENSEMBLE-04
- Given a CSV whose header is not exactly `model,kind,index,name,value` (for example, a field missing)
- When it is parsed
- Then it is rejected with the line number

### ENSEMBLE-05
- Given a CSV with `#` comment lines
- When the `#` lines appear before the header, they are accepted; when a `#` line appears after the header
- Then that is rejected with its line number

### ENSEMBLE-06
- Given a data row without exactly five fields, a `model` that is not three ASCII digits, or a `kind` other than `bias`, `time_const` or `syn_strength`
- When it is parsed
- Then it is rejected with the line number

### ENSEMBLE-07
- Given a model whose rows are not in the order `bias` 0..65, `time_const` 0..65, `syn_strength` 0..604, or whose `index` does not equal the row's position (for example, a shuffled row)
- When it is parsed
- Then it is rejected with the line number

### ENSEMBLE-08
- Given a row whose `name` is not `CELL_TYPES[index]` (for `bias`/`time_const`) or not `"{src}>{tar}"` from `CONN_TYPES[index]` (for `syn_strength`)
- When it is parsed
- Then it is rejected with the line number

### ENSEMBLE-09
- Given a row whose `value` does not parse as `f32`, or parses to a non-finite value
- When it is parsed
- Then it is rejected with the line number; finite values are taken via `str::parse::<f32>()`

### ENSEMBLE-10
- Given a CSV where a model appears twice, models are not in increasing order, or the model count is not exactly 50 (for example, 49 models)
- When it is parsed
- Then it is rejected with a line number

### ENSEMBLE-11
- Given `ModelParams::pretrained()`
- When it is called
- Then it returns model 000's parameters from the compiled-in constants in `pretrained.rs`

### ENSEMBLE-12
- Given the real `data/flyvis-pretrained/ensemble.csv`
- When it is parsed
- Then it has 50 ids `000` to `049` (36,700 data rows), model 000 equals `ModelParams::pretrained()` bit for bit in all 734 values, and every value string `s` satisfies `s.parse::<f32>() == s.parse::<f64>() as f32`

### ENSEMBLE-13
- Given an unknown model id
- When `model(id)` is called
- Then it returns `None`

## integrator: The graded integrator's dynamics and errors

### INTEGRATOR-01
- Given a network, `GradedParams` and `dt`
- When `Graded::new` is called with any slice whose length does not match the network (bias/time_const per cell type, syn_strength per connection type, edge_syn_count per edge)
- Then it returns a `GradedError`

### INTEGRATOR-02
- Given parameters or counts containing a non-finite value
- When `Graded::new` is called
- Then it returns a `GradedError`

### INTEGRATOR-03
- Given a `dt` that is not finite or not positive (including dt <= 0)
- When `Graded::new` is called
- Then it returns a `GradedError`

### INTEGRATOR-04
- Given a successfully constructed `Graded`
- When it is first inspected, or after `reset()`
- Then `state()` equals each node's cell-type bias (in precision R), and `dt()` returns the dt it was built with

### INTEGRATOR-05
- Given an input slice whose length is not the number of nodes
- When `step` is called
- Then it returns a `GradedError` and leaves the state unchanged

### INTEGRATOR-06
- Given a valid input
- When `step` is called
- Then each node's new state is `v + (inv_tau * (((-v + bias) + s) + input)) * step_dt`, where `s` starts at zero and adds `w * relu(v[src])` over the node's incoming edges in ascending edge index, in exactly that order of operations and without fused multiply-add

### INTEGRATOR-07
- Given a network with mutual connections
- When `step` is called
- Then every synaptic sum reads the old state (Jacobi-style), matching a hand-computed step for a two-node mutual network

### INTEGRATOR-08
- Given an edge of connection type c
- When the integrator is constructed
- Then its weight is `from_f32(conn_sign[c]) * edge_syn_count[e] * max(from_f32(syn_strength[c]), ZERO)`, multiplied left to right, with the count used as given

### INTEGRATOR-09
- Given a negative `syn_strength`
- When the integrator is constructed and stepped
- Then that strength is clamped to 0 and its edges contribute nothing

### INTEGRATOR-10
- Given a source node with negative state
- When a target is stepped
- Then that source contributes nothing (relu on the presynaptic value), whatever the target's own sign

### INTEGRATOR-11
- Given a time constant greater than dt
- When the integrator steps
- Then the rate is 1/τ

### INTEGRATOR-12
- Given a time constant at or below dt
- When the integrator is constructed
- Then `inv_tau = ONE / max(from_f32(time_const), from_f32(dt as f32))`: the floor is f32(dt) widened, in both precisions; in f64 with τ = 0.001 and dt = 0.005, one step uses `1 / f64(f32(0.005))`, not `1 / 0.005`

### INTEGRATOR-13
- Given an f32 or f64 integrator
- When it steps
- Then it steps by `from_f64(dt)`: f32(dt) in f32 and dt itself in f64; f32 parameters are widened exactly with `from_f32` in f64

### INTEGRATOR-14
- Given one neuron with no edges, starting at bias, under constant input x
- When stepped k times
- Then the state matches `bias + x(1 - (1 - dt/τ)^k)` within 1e-12 relative in f64 and 1e-5 in f32, approaching `bias + x`; the input acts as a current and does not set the state directly

### INTEGRATOR-15
- Given the `Real` trait
- When used
- Then it is sealed and implemented for exactly f32 and f64, with `from_f32` exact, `from_f64` an `as` cast, and `to_f64` exact

## reference: Reading the reference manifest and binaries

### REFERENCE-01
- Given a valid manifest JSON
- When `Manifest::parse` reads it
- Then it returns n_nodes, t_pre, pre_stim_value, dts, runs, centre_nodes and files, ignoring keys this piece does not read (`generator`, `flyvis_version`, `n_hexals`, `batch_size`, `models`, `frame_semantics`, `transients_visible_on`, `sequences`, per-file `sha256`)

### REFERENCE-02
- Given a manifest whose `schema_version` is not 1 (for example 2), whose `byte_order` is not `little` (for example big-endian), or whose `array_order` is not `C`
- When it is parsed
- Then it is rejected with a `ReferenceError`

### REFERENCE-03
- Given a file whose blocks overlap, leave gaps, or total a size different from the file's `bytes`
- When the manifest is parsed
- Then it is rejected

### REFERENCE-04
- Given a block whose `bytes` is not product(shape) x dtype size, or whose `shape.len()` differs from the file's `axes.len()`
- When the manifest is parsed
- Then it is rejected

### REFERENCE-05
- Given a file with two blocks sharing the same (run, dt, sequence)
- When the manifest is parsed
- Then it is rejected

### REFERENCE-06
- Given a file name
- When `Manifest::file(name)` is called
- Then it returns that file's info, or `None` if absent

### REFERENCE-07
- Given a file's bytes and a `BlockKey`
- When `read_f32`/`read_f64` is called
- Then it returns the matching block's little-endian values with the block's shape, matching all three key fields including `None` (so `run: null` blocks are selected by dt and sequence)

### REFERENCE-08
- Given a dtype other than the one asked for, a byte slice whose length differs from the manifest's, or a key matching no block
- When `read_f32`/`read_f64` is called
- Then it returns a `ReferenceError` without panicking

### REFERENCE-09
- Given the real `data/flyvis-reference/manifest.json`
- When it is parsed
- Then it succeeds and all nine files' lengths match the manifest

### REFERENCE-10
- Given any malformed manifest or length mismatch
- When the library processes it
- Then it returns a typed error naming the field or file, never panics, and does not hash files or read `PROVENANCE.json`

## settling: The settling gates and their controls

### SETTLING-01
- Given each of the eight reference runs and the torch-computed counts (`SYN_COUNT_TORCH_F32`/`F64` indexed by `edge_syn_groups`)
- When the run's model is settled from bias for `n_settle` steps with 0.5 on every input node and 0 elsewhere, in the run's precision
- Then every one of the 45,669 settled values and every n_settle x 65 centre value equals flyvis's exactly (`settling_exact_run_0_000_1_200_f32` through `settling_exact_run_7_049_1_50_f64`)

### SETTLING-02
- Given an exact-gate run with a differing value
- When it fails
- Then it prints how many values differ, the first differing settle step with its centre cell type, and the largest error with its node and cell type

### SETTLING-03
- Given each of the eight runs and counts from Rust's std `exp` of `SYN_COUNT`
- When settled as above
- Then the settled and settling-centre maxima of |rust - flyvis| (in f64) are within 1e-10 for f64 runs, and within 2 x that run's flyvis f32/f64 gap (per quantity) for f32 runs

### SETTLING-04
- Given a libm-gate run
- When it completes
- Then it prints each maximum with its node or settle step and cell type, each maximum's ratio to its bound, the three cell types with the largest settled error, and the count of bit-equal settled values

### SETTLING-05
- Given a settling run
- When it executes
- Then dt is asserted to be 1/200 or 0.02, n_settle equals `(t_pre / dt) as usize`, both reference shapes ([45669] and [n_settle, 65]) are asserted, and the centre values recorded after step k are compared as settle step k - 1

### SETTLING-06
- Given a step that produces a non-finite value anywhere in the state
- When the run scans the state after that step
- Then the run fails naming the first non-finite node, its cell type and the step

### SETTLING-07
- Given the manifest
- When `reference_runs_are_the_eight_expected` runs
- Then the runs are exactly {000, 049} x {1/200, 1/50} x {f32, f64} at indices 0-7, and every centre node's cell type equals the expanded network's cell type for that node

### SETTLING-08
- Given the eight f32/f64 gaps hard-coded in the gate
- When `pre_registered_bounds_match_provenance` runs
- Then they equal `PROVENANCE.json`'s `f32_f64_gap` bit for bit

### SETTLING-09
- Given model 049's parameters with torch counts at dt 0.02 in f64
- When compared against model 000's f64 reference at 0.02 (`the_gate_tells_models_apart`)
- Then the settled maximum exceeds 1e-3

### SETTLING-10
- Given run 3 (model 000, dt 0.02, f64) using `edge_n_syn` widened to f64 as counts
- When settled and compared (`the_gate_resolves_integer_counts`)
- Then the settled maximum exceeds the f64 libm bound, 1e-10

### SETTLING-11
- Given a fixture (reference data or ensemble.csv) is absent
- When a gate runs without `SEMBLANCE_REQUIRE_FIXTURES`, it prints a skip notice and passes; with `SEMBLANCE_REQUIRE_FIXTURES=1`
- Then it fails naming the missing path

### SETTLING-12
- Given the generated `SYN_COUNT_TORCH_F32` and `SYN_COUNT_TORCH_F64` tables
- When checked against `SYN_COUNT`
- Then each entry is within 2 ulps of std `exp` of the same input, and each table differs from std `exp` on at most 200 entries

### SETTLING-13
- Given the expanded real network
- When the wiring helpers run
- Then `edge_syn_groups` returns 1,513,231 group indices over exactly 2,355 groups in first-appearance order (failing otherwise), `input_nodes` returns 8 lists of 721 nodes in `input_units` order (failing if an input unit is not a cell type of the network), and `net.conn_sign()` equals `pretrained::SIGN` bit for bit for all 604 types
