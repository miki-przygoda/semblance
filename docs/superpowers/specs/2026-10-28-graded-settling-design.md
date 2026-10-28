# Graded dynamics, gated on flyvis's settling

**Date:** 2026-10-28
**Status:** reviewed once, adversarially (18 findings, all addressed below);
awaiting owner review
**Epic:** 0.3.0 graded dynamics, tasks 2, 3 and 4 —
`docs/project-outline/0.3.0-graded-dynamics/` (not in git)
**Builds on:**
[`2026-10-26-flyvis-reference-traces-design.md`](2026-10-26-flyvis-reference-traces-design.md)
(task 1, which produced the reference this gate reads) and its
[research file](2026-10-26-flyvis-reference-traces-research.md), cited here by
id (F = fact, P = practice, C = correction, D = deferred question).
**Decisions it implements:** epic decisions 2, 3, 5 and 6, and the owner's
choices of 2026-10-28:

- 0.3.0 is finished in three pieces. This is piece 1 (tasks 2, 3, 4). Piece 2
  is tasks 5 and 6, and piece 3 is task 7 plus a `validate` command, the README
  headline and the 0.3.0 bump.
- The integrator runs in both f32 and f64.
- **The gate is exact.** The review found that the design reproduces flyvis
  bit for bit once it uses the synapse counts flyvis itself computed. The
  owner chose "exact + libm check": an exact gate on flyvis's own counts,
  plus a second gate on counts computed by Rust's `exp`, under tolerances
  fixed before any comparison runs.
- For that second gate, after reading P1, the owner chose an f32 bound of 2×
  the measured gap, not 1×.

## Why

Everything after 0.3.0 reports numbers produced by this workspace's simulator.
If that simulator does not reproduce flyvis, a later size-vs-fidelity curve
measures the port's bugs, and it would look the same either way. This piece
builds the simulator, and the first gate that can catch it being wrong: the
settling gate (epic "stage 1").

Settling runs flyvis's network from its bias state for 1.0 s of uniform grey.
This gate compares against flyvis's own output, in every neuron, at both time
steps, for the best and the worst trained model, in both precisions. No
stimulus geometry is involved, so a failure here is a failure of the dynamics,
the parameters or the wiring.

## Goal

Three library additions, one generated table, and two gates:

1. **`semblance_flyvis::ensemble`.** Reads `data/flyvis-pretrained/ensemble.csv`
   into per-model parameters, so all 50 trained models are reachable (task 2).
2. **`semblance_sim::graded`.** flyvis's graded dynamics, as an explicit Euler
   integrator generic over f32 and f64. It touches none of `sim`'s spiking
   traits (task 3).
3. **`semblance_flyvis::reference`.** Reads task 1's
   `data/flyvis-reference/manifest.json` and returns typed, shaped arrays from
   its `.bin` files.
4. **`semblance_flyvis::syn_count_torch`.** A generated table of the 2,355
   synapse counts exactly as flyvis's PyTorch computed them, in f32 and f64,
   written by a new offline script, `tools/export_syn_count.py`.
5. **The exact settling gate.** Eight tests, one per reference run, using the
   torch-computed counts, each requiring every settled value and every
   centre-neuron settling value to equal flyvis's exactly (task 4).
6. **The libm settling gate.** The same eight runs with counts computed by
   Rust's `exp`, each under a bound fixed in this spec (task 4).

`semblance_flyvis` also gains two wiring functions: each edge's synapse-count
group, and the input (photoreceptor) nodes. No version bump. That waits for
piece 3.

## Facts the design rests on

Read from flyvis 1.2.0's installed source (`tools/.venv/.../flyvis/`), from
`tools/export_reference.py`, or measured on 2026-10-28. Line numbers are
flyvis 1.2.0's unless a file is named. The review checked every row.

| # | Fact | Source |
|---|---|---|
| G1 | velocity = `1 / max(time_const, torch.tensor(dt).float()) * (-activity + bias + target_sum(weight * relu(source activity)) + x_t)`, associated left to right. `1 / x` is `x.reciprocal() * 1`, measured correctly rounded on this build: 0 mismatches against `1/x` over 10⁶ values in each precision | `dynamics.py:207-218`; `torch/_tensor.py:1121-1124` |
| G2 | the floor is f32(dt) **in both precisions**: `torch.tensor(dt).float()` is f32 even in a float64 run, and `torch.max` widens it. f32(0.005) = 0.004999999888241291, f32(0.02) = 0.019999999552965164 | `dynamics.py:209`; F23; manifest `dts[].floor_dt_f32` |
| G3 | Euler: `activity + vel * dt`, with the Python-float dt cast to the tensor's dtype, so f32 runs step by f32(dt) and f64 runs by the f64 dt | `network.py:406`; F23 |
| G4 | `weight = sign * syn_count * syn_strength`, evaluated left to right, where `sign` is the checkpoint's per-connection-type sign | `dynamics.py:165-167` |
| G5 | `syn_count` is a lognormal parameter whose value is `raw.exp()`, computed by torch in the run's precision. The raw value is ln(count), stored as f32 | `initialization.py:199-202, 451` |
| G6 | `syn_strength` has `clamp="non_negative"`, applied by `clamp_(0)` (if `requires_grad`) at the start of every `forward` | `network.py:118, 489-498, 527` |
| G7 | `target_sum` starts from `torch.zeros` and calls `scatter_add_` over the edges. That `zeros` takes the default dtype, which the export set to float64 for f64 runs (`export_reference.py:1378`). The f64 runs also call `network.double()` (`:1102`) | `network.py:333-351`; F24 |
| G8 | the initial state is `bias`. Settling is `int(t_pre / dt)` steps (200 at 1/200, 50 at 0.02), with `0.5` added to the input-unit nodes only and 0 elsewhere. The export used this stock path, with no fade-in | `dynamics.py:183`; `network.py:580-585`; `stimulus.py:128-130, 238-242`; `export_reference.py:1122` |
| G9 | the input units are the cell types in the spec's `input_units` (R1–R8), all of their nodes. flyvis's `input_index` is [8, 721]: `input_units` order, then node order within each type | `stimulus.py:128-130`; `connectome.py:183` |
| G10 | output is stacked after each step: settle step i is the state after i+1 steps, and `settled` equals settle step `n_settle - 1` | manifest `frame_semantics` |
| G11 | this workspace's edge order and node order equal flyvis's | `every_edge_matches_flyvis`, `node_ordering_matches_flyvis` in `crates/flyvis/src/expand_checks.rs` |
| G12 | `SYN_COUNT`'s 2,355 entries are positional over the first-appearance order of `(connection type, du, dv)` groups in the built edge list | `trained_synapse_counts_match_the_expanded_network` |
| G13 | the reference parameters equal `ensemble.csv` for all 50 models, and model 000's equal `pretrained.rs`. Every checkpoint's `sign` and `syn_count` equal `pretrained.rs`'s `SIGN` and `SYN_COUNT`, so model 049 shares them | PROVENANCE `parameter_equality`; `export_reference.py:807, 824` |
| G14 | in `ensemble.csv`, no `syn_strength` is negative (919 are exactly 0) and 412 biases are negative. At dt 0.02 the floor binds for 51 cell types in model 000 and 36 in model 049. At 1/200 it binds for none, in any model | measured on the file; PROVENANCE |
| G15 | flyvis's own f32-vs-f64 gaps (max abs over all values, in f64), per run pair and quantity: see the table in [The libm gate's bounds](#the-libm-gates-bounds) | PROVENANCE `f32_f64_gap` |
| G16 | **flyvis's settling is reproducible bit for bit.** A numpy model of section 3 below matched flyvis in every settled value and every settling-centre value, in all 8 runs and both precisions. It used torch's own computed counts, summed each node's inputs sequentially in ascending edge index from zero, and followed G1–G3's operation order. So flyvis's CPU `scatter_add_` is sequential in edge order on this layout, which settles F25's UNVERIFIED | review measurement, 2026-10-28 (scratchpad `sim.py`, `sim64.py`) |
| G17 | **`exp` is the only obstacle to exactness.** torch's vectorised `exp` (torch 2.14.0, arm64, CPU capability DEFAULT) differs from the macOS libm on 104–106 of the 2,355 counts in f32 and 108 in f64. With libm's `exp`, the same model is within about 1e-14 of flyvis in f64, and within 0.04–0.37 of the f32 bounds below | review measurement, independently re-measured |

Two consequences shape the design.

**Counts come from flyvis's logarithms, never from `edge_n_syn`.**
`TypedNetwork::edge_n_syn` holds the spec's counts, which `exp(ln count)` only
approximates. Feeding them to an f64 run moves the settled state by 4.8e-7 to
6.7e-6 (measured). That is wrong for a reason unrelated to the dynamics, so it
becomes a negative control below.

**The integrator takes counts, not logarithms.** The two gates differ only in
where the 2,355 counts come from: torch's table (exact gate), or Rust's `exp`
of `SYN_COUNT` (libm gate). So `exp` is not part of the integrator at all.
The caller supplies counts in the run's precision.

## Design

### 1. `semblance_flyvis::ensemble` (task 2)

```rust
pub struct ModelParams {
    pub bias: [f32; 65],
    pub time_const: [f32; 65],
    pub syn_strength: [f32; 604],
}

impl ModelParams {
    /// Model 000, from the compiled-in constants in `pretrained.rs`.
    pub fn pretrained() -> Self;
}

pub struct Ensemble { /* 50 (id, ModelParams), in file order */ }

impl Ensemble {
    pub fn parse(csv: &str) -> Result<Self, EnsembleError>;
    pub fn model(&self, id: &str) -> Option<&ModelParams>;   // "000".."049"
    pub fn ids(&self) -> impl Iterator<Item = &str>;
}
```

The parser does no file I/O. It is strict, because a misread row would
silently attribute a parameter to the wrong cell type:

- Lines are separated by `\n`. A trailing newline is accepted. `\r` anywhere
  is an error, because the file is LF-only and a CR would otherwise end up
  inside a field.
- `#` lines are allowed only before the header. The header must be exactly
  `model,kind,index,name,value`.
- Every row has five fields. `model` is three ASCII digits, and `kind` is
  `bias`, `time_const` or `syn_strength`.
- Within each model the rows come in this order: `bias` 0..65, `time_const`
  0..65, `syn_strength` 0..604. `index` must equal the row's position.
- `name` must equal `CELL_TYPES[index]` for `bias` and `time_const`, and must
  equal `"{src}>{tar}"` from `CONN_TYPES[index]` for `syn_strength`.
- `value` parses with `str::parse::<f32>()` (one correctly rounded step) and
  must be finite.
- Each model appears once, in increasing order, and there are exactly 50. That
  makes 36,700 data rows, not counting the 3 `#` lines and the header.

Every violation is an `EnsembleError` that names the line number and what was
expected. Nothing panics.

### 2. Wiring in `semblance_flyvis`

```rust
/// For each edge, the index (0..2355) of its (connection type, du, dv)
/// group, numbered in first-appearance order (G12).
pub fn edge_syn_groups(net: &TypedNetwork) -> Result<Vec<u32>, WiringError>;

/// The input nodes per input unit, in `spec.input_units` order, each in node
/// order: flyvis's [8, 721] `input_index` (G9).
pub fn input_nodes(spec: &ConnectomeSpec, net: &TypedNetwork) -> Result<Vec<Vec<u32>>, WiringError>;
```

- `edge_syn_groups` fails if the number of groups is not `SYN_COUNT.len()`.
  Its grouping logic moves out of
  `trained_synapse_counts_match_the_expanded_network`, which then checks the
  function's output. The grouping is implemented once.
- `input_nodes` fails if an input unit is not a cell type of `net`. On the real
  network it returns 8 lists of 721 nodes. Settling only needs the union.
  Piece 2 needs the structure.
- Per-edge counts are then `table[groups[e]]` for whichever 2,355-entry table
  the caller picks.

A new check, `conn_sign_equals_the_trained_sign`, asserts that
`net.conn_sign()` equals `pretrained::SIGN` bit for bit for all 604 types.
That ties the sign the integrator uses (the spec's) to the one flyvis
multiplies (the checkpoint's, G4, G13). It sits beside
`trained_parameter_ordering_matches_the_expanded_network`.

### 3. `semblance_flyvis::syn_count_torch` and its exporter

`tools/export_syn_count.py` follows `tools/export_params.py`'s pattern: it is
offline, run by hand in `tools/`'s uv environment, pinned to flyvis 1.2.0, and
never invoked by Rust. It does the following:

1. Loads model `flow/0000/000` through flyvis's stock path, then reads
   `syn_count`'s semantic values (`raw.exp()`) as flyvis's forward pass sees
   them:
   - in float32
   - again after `torch.set_default_dtype(torch.float64)` and
     `network.double()`, exactly as `export_reference.py` does (G7)
2. Asserts:
   - both have 2,355 entries
   - the raw f32 values equal `pretrained.rs`'s `SYN_COUNT` bit for bit
   - the f32 values equal `torch.exp(raw)` in f32, and the f64 values equal
     `torch.exp(raw.double())`
   - the counts are identical for model `flow/0000/049`
3. Writes `crates/flyvis/src/syn_count_torch.rs`, headed "GENERATED by
   `tools/export_syn_count.py`. Do not edit by hand." with provenance in the
   module doc: flyvis, torch and numpy versions, `platform.machine()`, torch's
   CPU capability, and the export date. It contains:

```rust
/// syn_count as flyvis's PyTorch computed it in float32: torch.exp of SYN_COUNT.
pub static SYN_COUNT_TORCH_F32: [f32; 2355] = [ ... ];
/// The same in float64: torch.exp of SYN_COUNT widened.
pub static SYN_COUNT_TORCH_F64: [f64; 2355] = [ ... ];
```

Values are written with Python's `repr` of the float64 value (for f32, of its
exact widening). That is the shortest decimal that round-trips, and Rust's
correctly rounded literal parsing recovers each value exactly. A Rust test pins
both tables to `SYN_COUNT`:
- each entry is within 2 ulps of the std `exp` of the same input
- each table differs from std `exp` on at most 200 entries (G17 measured
  about 105), so a table written from the wrong source fails

The table records one machine's torch `exp`, and the module doc says so. That
is what "exact" means here: Rust reproduces flyvis's arithmetic bit for bit,
given the counts flyvis computed. The libm gate (section 6) shows the port
does not depend on that table to be right.

This adds no fixture and no dependency. Like `pretrained.rs`, it is data
exported once from flyvis and compiled in.

### 4. `semblance_sim::graded` (task 3)

A new module beside the spiking path. It depends only on
`semblance-connectome`, so `sim` never learns about flyvis.

```rust
/// f32 or f64. Sealed: implemented for exactly those two.
pub trait Real: Copy + PartialOrd + Add<Output = Self> + Sub<Output = Self>
    + Mul<Output = Self> + Div<Output = Self> + Neg<Output = Self> + Debug + sealed::Sealed
{
    const ZERO: Self;
    const ONE: Self;
    fn from_f32(x: f32) -> Self;   // exact
    fn from_f64(x: f64) -> Self;   // `as` cast: rounds to nearest for f32
    fn to_f64(self) -> f64;        // exact
}

pub struct GradedParams<'a, R: Real> {
    pub bias: &'a [f32],           // one per cell type
    pub time_const: &'a [f32],     // one per cell type
    pub syn_strength: &'a [f32],   // one per connection type
    pub edge_syn_count: &'a [R],   // one per edge, already in R
}

pub struct Graded<R: Real> { /* see below */ }

impl<R: Real> Graded<R> {
    pub fn new(net: &TypedNetwork, params: &GradedParams<R>, dt: f64) -> Result<Self, GradedError>;
    pub fn reset(&mut self);                                        // state = bias, per node
    pub fn step(&mut self, input: &[R]) -> Result<(), GradedError>; // one Euler step
    pub fn state(&self) -> &[R];
    pub fn dt(&self) -> f64;
}
```

**Precision.** `Graded<f32>` mirrors flyvis's float32 run, and `Graded<f64>`
mirrors its float64 run. Parameters are f32 in both, as in flyvis. The f64 run
widens them exactly with `from_f32`, as `.double()` does. The sign is
`net.conn_sign()`, which section 2 pins equal to the checkpoint's.

**Construction** precomputes, in precision `R`:

- `max(a, b)` is `if a > b { a } else { b }`. NaN cannot reach it: `new`
  rejects non-finite inputs.
- per edge e with connection type c:
  `weight = from_f32(conn_sign[c]) * edge_syn_count[e] * max(from_f32(syn_strength[c]), ZERO)`,
  multiplied left to right (G4, G6)
- per node, `bias` and
  `inv_tau = ONE / max(from_f32(time_const), from_f32(dt as f32))`. The floor
  is f32(dt), widened, in both precisions (G1, G2).
- `step_dt = from_f64(dt)`: f32(dt) in f32, and dt in f64 (G3)
- a by-target index: each node's incoming edges in ascending edge index, stored
  as contiguous (source, weight) pairs

`new` fails on:

- any slice whose length does not match the network
- any non-finite parameter or count
- `dt` that is not finite and positive

It does not validate time constants beyond finiteness, because the floor makes
every effective one positive. The state starts at bias.

**`step(input)`** requires `input.len() == num_nodes`, and otherwise fails
without touching the state. For each node i, in exactly this order of
operations:

```
s   = ZERO
for (src, w) in incoming(i), ascending edge index:
    s = s + w * relu(v[src])                 // relu(x) = if x > ZERO { x } else { ZERO }
vel = inv_tau[i] * (((-v[i] + bias[i]) + s) + input[i])
v'[i] = v[i] + vel * step_dt
```

Every synaptic sum reads the old state, as flyvis's tensor update does: the
update is Jacobi-style, not Gauss–Seidel. No fused multiply-add is used. Rust
never contracts `a * b + c` on its own, and the code must not call `mul_add`.

Not here: the hexal input mapping (piece 2), recording, batching, threads,
`exp`. A caller that wants the trajectory reads `state()` after each `step`.

### 5. `semblance_flyvis::reference`

```rust
pub struct Manifest {
    pub n_nodes: usize,
    pub t_pre: f64,
    pub pre_stim_value: f64,
    pub dts: Vec<DtInfo>,              // id, dt, floor_dt_f32, n_settle
    pub runs: Vec<RunInfo>,            // index, model, dt id, precision
    pub centre_nodes: Vec<CentreNode>, // node, cell_type
    pub files: Vec<FileInfo>,          // name, dtype, axes, bytes, blocks
}

pub enum Dtype { F32, F64 }
pub struct Block { pub run: Option<u32>, pub dt: String, pub sequence: Option<u32>,
                   pub offset: usize, pub bytes: usize, pub shape: Vec<usize> }
pub struct BlockKey<'a> { pub run: Option<u32>, pub dt: &'a str, pub sequence: Option<u32> }
pub struct Array<T> { pub shape: Vec<usize>, pub data: Vec<T> }

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, ReferenceError>;
    pub fn file(&self, name: &str) -> Option<&FileInfo>;
}

impl FileInfo {
    pub fn read_f32(&self, bytes: &[u8], key: &BlockKey) -> Result<Array<f32>, ReferenceError>;
    pub fn read_f64(&self, bytes: &[u8], key: &BlockKey) -> Result<Array<f64>, ReferenceError>;
}
```

`BlockKey` matches all three fields, `None` included, so `input.f64.bin`'s
blocks (`run: null`, keyed by dt and sequence) can be addressed too.

**Parsing** uses the workspace's own JSON parser (`json.rs`):

- It checks that `schema_version` is 1, `byte_order` is `little`, and
  `array_order` is `C`.
- Keys this piece does not read are ignored: `generator`, `flyvis_version`,
  `n_hexals`, `batch_size`, `models`, `frame_semantics`,
  `transients_visible_on`, `sequences`, and per-file `sha256`.
- Duplicate keys are not detected, because `json.rs` returns the first. The
  file is hash-pinned by `ci/fixtures.sha256`, so this is recorded rather than
  fixed.

**Rejected per file:**

- blocks that overlap or leave gaps, or whose total differs from the file's
  `bytes`
- any block whose `bytes` is not product(shape) × dtype size
- any block whose `shape.len()` differs from the file's `axes.len()`
- two blocks with the same (run, dt, sequence)

**`read_*` fails on:**

- a dtype other than the one asked for
- a byte slice whose length differs from the manifest's
- no block matching the key

The returned `Array` carries the block's shape, and callers assert the shape
they expect.

The reader does not hash files. `ci/fetch-fixtures.sh` already verifies every
fixture against `ci/fixtures.sha256`, and hashing in Rust would need a SHA-256
implementation in a workspace with no external dependencies. The library
never reads `PROVENANCE.json`. The gate reads it once, for the cross-check
below. Piece 2 adds parsing of `sequences`.

### 6. The settling gates (task 4)

`crates/flyvis/src/settling_checks.rs`, `#[cfg(test)]`, beside the other gates.
`semblance-flyvis` gains `semblance-sim` as a **dev-dependency**.

The gate lives in the flyvis crate rather than in the `semblance` app crate
(the brainstorm's first sketch) so that it reuses `test_support.rs`'s fixture
loaders and `SEMBLANCE_REQUIRE_FIXTURES` handling instead of copying them.
Both crates are in the workspace, so `Cargo.lock` still lists 8 packages.
`test_support.rs` gains loaders for `data/flyvis-reference/*` (bytes and
text) and for `data/flyvis-pretrained/ensemble.csv`, following the existing
pattern: skip with a notice, or fail under `SEMBLANCE_REQUIRE_FIXTURES=1`.

**One settling run** takes a run from the manifest and a count source, which
is either `torch` or `libm`:

1. Take the run's model parameters from `Ensemble`, the network from
   `expand(spec, DEFAULT_EXTENT, DEFAULT_N_SYN_FILL)`, and `dt` from the
   manifest's `dts`, asserted equal to `1.0 / 200.0` or `0.02`.
2. Counts per edge, in the run's precision:

   | Source | f32 | f64 |
   |---|---|---|
   | `torch` | `SYN_COUNT_TORCH_F32[g]` | `SYN_COUNT_TORCH_F64[g]` |
   | `libm` | `SYN_COUNT[g].exp()` (std `f32::exp`) | `(SYN_COUNT[g] as f64).exp()` (std `f64::exp`) |

   Here `g = edge_syn_groups[e]`.
3. Build `Graded<f32>` or `Graded<f64>`, per the run's precision.
4. Input: `pre_stim_value` (0.5) on every node in `input_nodes`, and 0
   elsewhere (G8).
5. Step `n_settle` times, asserted equal to `(t_pre / dt) as usize`. After
   every step, scan the full state for non-finite values. The first one fails
   the run, naming the node, its cell type and the step. After step k
   (1-based), record the 65 `centre_nodes` as settle step k − 1 (G10).
6. Compare against the reference:
   - the final state against the run's `settled.{f32,f64}.bin` block, shape
     [45669]
   - the recorded centre values against `settling_centre.{f32,f64}.bin`,
     shape [n_settle, 65]

   Both shapes are asserted. Errors are |rust − flyvis| in f64, after exact
   widening.

Network, spec, groups, input nodes, ensemble and manifest are built once and
shared through a `OnceLock`.

**The exact gate: 8 tests**, `settling_exact_run_0_000_1_200_f32` through
`settling_exact_run_7_049_1_50_f64`, using the `torch` counts. Pass means
every value in both comparisons equals flyvis's: `rust == flyvis` in the
run's precision, for all 45,669 settled values and all n_settle × 65 centre
values. On failure, the test prints:

- how many values differ
- the first differing settle step, with its centre cell type
- the largest error, with its node and cell type

**The libm gate: 8 tests**, `settling_libm_run_…`, using the `libm` counts.
Pass means both maxima are within the run's bound
(see [The libm gate's bounds](#the-libm-gates-bounds)). Each test prints:

- each maximum, with its node or settle step and cell type
- each maximum's ratio to its bound
- the three cell types with the largest settled error
- the count of bit-equal settled values

With the order already exact (G16), that count measures how far `exp`'s
disagreement propagates, and nothing else.

The bounds are pure absolute maxima, not P3's mixed tolerance and not P4's
windowed trend. An absolute max is the strictest summary, and the bounds come
from a max-abs gap. The first-differing step and the arg-max step stand in for
P4's trend. The exact gate makes trend analysis moot for the shipped
configuration.

**Supporting tests:**

- **`reference_runs_are_the_eight_expected`:**
  - the manifest's runs are exactly {000, 049} × {1/200, 1/50} × {f32, f64}
    at indices 0–7 (`export_reference.py`'s `RUNS`)
  - every centre node's `cell_type` equals the expanded network's cell type
    for that node
- **`pre_registered_bounds_match_provenance`:** the eight gaps hard-coded in
  the gate equal `PROVENANCE.json`'s `f32_f64_gap`, bit for bit. The bounds
  are in the source where review sees them, and cannot drift from the data
  they came from.
- **`the_gate_tells_models_apart`:** Rust f64 with model 049's parameters and
  the torch counts at dt 0.02, against the reference for model 000 f64 at
  0.02. The settled maximum must exceed 1e-3, so a gate that compared nothing
  to nothing would fail.
- **`the_gate_resolves_integer_counts`:** run 3 (model 000, dt 0.02, f64) with
  `edge_n_syn` widened to f64 as the counts. The settled maximum must exceed
  the f64 libm bound, 1e-10. The review measured 4.8e-7 to 6.7e-6 across runs.
  This shows the libm bound resolves the class of error it exists to catch.
  The exact gate catches it trivially.

**What passing validates.** Together the gates validate:

- all 1,513,231 edges, in flyvis's places and order
- both index maps, and the sign
- weight derivation, explicit Euler, and presynaptic relu
- the input units

The time-constant floor is exercised at dt 0.02 in both precisions. Its exact
value, f32(dt) rather than dt, is observable only in the f64 runs at 0.02:
in f32 the two coincide. Replacing it with the f64 dt moves model 000's
settled state by 1.2e-9 and its settling centre by 7.2e-7, and model 049's by
8.2e-7 and 1.4e-7 (measured). The exact gate catches all of these, and so does
the libm bound.

**Not validated by the gates:** the `syn_strength` clamp. No trained strength
is negative (G14), so the clamp is never exercised on real data. Only its unit
test covers it.

### The libm gate's bounds

Fixed before any comparison runs, per precision, and applied to both
quantities in every run.

**f64: max abs ≤ 1e-10.** With the order exact (G16), only `exp`'s last bit
differs (G17). The recursive-sum bound (P1) and the heuristic f64 floor of gap
× 2⁻²⁹ (P2) both put the result near 1e-14, and the review measured 9.4e-15 at
worst. 1e-10 still catches every real error named above.

**f32: max abs ≤ 2 × that run's measured flyvis f32/f64 gap**, separately for
`settled` and `settling_centre` (G15). Rust-f32 with libm counts and
flyvis-f32 are two f32 computations that differ in their inputs. Each is
within about 1× gap of the exact answer, possibly on opposite sides, so they
can legitimately differ by up to about 2× (P1, C11).

| Model | dt | Gap, `settled` | Bound, `settled` | Gap, `settling_centre` | Bound, `settling_centre` |
|---|---|---|---|---|---|
| 000 | 1/200 | 1.4114366875261908e-06 | 2.8228733750523816e-06 | 1.347661143569212e-06 | 2.695322287138424e-06 |
| 049 | 1/200 | 6.1578304686449314e-06 | 1.2315660937289863e-05 | 1.9363140433270587e-06 | 3.872628086654117e-06 |
| 000 | 1/50 | 1.276739533651039e-06 | 2.553479067302078e-06 | 2.6109457484935206e-06 | 5.221891496987041e-06 |
| 049 | 1/50 | 6.4440782598373936e-06 | 1.2888156519674787e-05 | 1.1488733433040466e-05 | 2.297746686608093e-05 |

The gate stores the gaps and multiplies by 2.0, which is exact in binary.

**If any gate fails: stop and diagnose.** Use the epic's diagnostic ladder:
magnitude, which cell types, which dt, and growth with step count. Never relax
a criterion: the exact gate stays `==`, and no libm bound is changed after a
comparison has run. Any fix must be a discrepancy found against flyvis's
source, recorded as an amendment to this spec. If the exact gate fails on
another machine while the libm gate passes, that is a platform `exp` or
summation question, and it gets recorded rather than tolerated. It is the
same rule `CLAUDE.md` applies to every comparison that could be exact.

### 7. Documentation in the same branch

- **`CLAUDE.md`:**
  - the test count
  - the skip count, re-measured as task 1 did: run with
    `SEMBLANCE_REQUIRE_FIXTURES=1` and `data/` absent, and count the failures
  - "no test reads either yet" becomes a statement of what reads them
  - "`crates/sim` is on the way out" becomes "the **spiking path** in
    `crates/sim` is on the way out" (epic decision 3, due when the module
    lands)
  - a paragraph naming the exact settling gate as an exact comparison, and
    the libm gate's bounds as measured rather than invented, beside the
    existing geometry-gate paragraph
  - `syn_count_torch.rs` as generated, like `pretrained.rs`
- **`README.md`:**
  - the skip count
  - the `sim` and `flyvis` rows of the crate table: graded dynamics exist,
    and settling reproduces flyvis exactly. The headline waits for piece 3.
- **`tools/README.md`:** the new exporter.
- **The epic's local README and task files** (not in git): tasks 2, 3 and 4
  done, with the achieved results, and F25's question answered (G16).

## Testing, test-first

Each unit lands with its tests written first.

**`ensemble`** (synthetic CSV strings, no fixtures):

- a minimal valid file parses, with and without a trailing newline
- each of these is rejected, with its line number:
  - a header with a field missing
  - a shuffled row
  - a wrong `name`
  - a non-finite value
  - a `\r`
  - a 49-model file
  - a duplicate model
  - a `#` line after the header

With the fixture:

- model 000 equals `ModelParams::pretrained()` bit for bit, all 734 values
- there are 50 ids, `000` to `049`
- every value satisfies `s.parse::<f32>() == s.parse::<f64>() as f32`, which
  checks that Rust's single rounding agrees with the exporter's double
  rounding for every model, not just 000

**Wiring:**

- on a hand-built `TypedNetwork`:
  - the group numbering is right
  - `input_nodes` keeps the `input_units` order
  - a missing input unit is an error
- with the fixture:
  - 1,513,231 groups and 8 × 721 input nodes
  - `trained_synapse_counts_match_the_expanded_network` passes through
    `edge_syn_groups`
  - `conn_sign_equals_the_trained_sign`

**`syn_count_torch`:** the two table checks in section 3. These need no
fixtures.

**`graded`** (hand-built networks, both precisions):

- one neuron with no edges, starting at bias with a constant input `x`,
  approaches `bias + x`. After k steps it matches the closed form
  `bias + x(1 − (1 − dt/τ)^k)` within 1e-12 relative in f64 and 1e-5 in f32.
  That compares round-off against a formula. It is not a bound on the port.
- relu reads the source: a negative source contributes nothing, whatever the
  target's own sign
- a negative `syn_strength` is clamped to 0
- the floor:
  - with τ > dt the rate is 1/τ
  - in f64, τ = 0.001 at dt 0.005 steps with `1 / f64(f32(0.005))`, not
    `1 / 0.005`, which one step distinguishes. No f32 τ lies strictly between
    f32(dt) and dt, so the floor's value is the thing to pin.
- input is a current: it enters the velocity, and a constant input does not
  set the state
- `step` reads the old state for every node: a two-node mutual network,
  checked against a hand-computed step
- the weight is sign × count × strength, with the count used as given
- each of these is an error: wrong slice lengths, a non-finite parameter or
  count, dt ≤ 0, and a wrong input length. A failed `step` leaves the state
  unchanged.

**`reference`** (synthetic manifests built in the test):

- a valid manifest parses
- each of these is rejected:
  - an overlapping block
  - a gap
  - a wrong `bytes`
  - a shape/axes length mismatch
  - two blocks with the same (run, dt, sequence)
  - schema version 2
  - a big-endian `byte_order`
- `read_f32`:
  - returns little-endian values with their shape
  - selects a `run: null` block by dt and sequence
  - fails on a dtype mismatch, a short byte slice, and a key that matches no block

With the fixture: the real manifest parses, and all nine files' lengths
match.

**The gates:** as described in section 6.

**Running time** is measured when the gates first pass, as the wall-clock time
of `cargo test -p semblance-flyvis settling` in the debug profile, all 16 runs
plus controls. If it exceeds 120 s, add `[profile.dev.package.semblance-sim]`
and `[profile.dev.package.semblance-flyvis]` with `opt-level = 3` to the
workspace `Cargo.toml`. The generic integrator is monomorphised in the calling
crate, so both entries are needed. Record the time before and after. The
integrator is not otherwise optimised (epic: scale is a non-issue here).

**CI** runs all of it as it stands. `ci/fetch-fixtures.sh` already fetches
`data/flyvis-reference/` and `ensemble.csv`, and CI already sets
`SEMBLANCE_REQUIRE_FIXTURES=1`, so no workflow change is needed.

## Failure handling

| Situation | Behaviour |
|---|---|
| fixture absent, `SEMBLANCE_REQUIRE_FIXTURES` unset | the test prints a skip notice and passes, as every other gate does |
| fixture absent, `SEMBLANCE_REQUIRE_FIXTURES=1` | the test fails, naming the path |
| malformed CSV, malformed manifest, or binary length mismatch | a typed error naming the line, field or file. Library code never panics on input. |
| `export_syn_count.py` finds a disagreement (raw ≠ `SYN_COUNT`, or 000 ≠ 049) | the script exits non-zero before writing anything |
| an exact-gate value differs | the test fails with the differing-value report. Work stops for diagnosis (section 6). |
| a libm bound exceeded | the test fails with its table. Work stops for diagnosis. |
| non-finite Rust state | the run fails, naming the first non-finite node, cell type and step |

## Non-goals

- Piece 2: the hexal-to-cell input mapping and the stimulus gate.
- Piece 3: the result record, the `validate` command, the README headline and
  the 0.3.0 bump.
- Migrating `sim`'s spiking traits (0.4.0, epic decision 3).
- Porting torch's `exp` to Rust. The review rejected it as fragile and tied to
  one platform. The generated table makes it unnecessary.
- Measuring the f64 floor by a permuted-order rerun (D2). The exact gate
  supersedes it.
- Performance beyond the 120 s rule above, threads, batching.
- Changing any fixture (`fixtures-1`, `fixtures-2`, `data/`) or `TOLERANCE`.
- Publishing anything to crates.io.

## Done when

- `cargo test --workspace` passes with the fixtures present and
  `SEMBLANCE_REQUIRE_FIXTURES=1`, including:
  - all 8 exact runs bit-equal to flyvis
  - all 8 libm runs within their bounds
  - all four supporting tests
- `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- `cargo run --quiet -- roundtrip` still reads `0.000000` twice.
- `Cargo.lock` lists 8 packages.
- The printed per-run results are copied into the PR description and the
  epic's local task file.
- `CLAUDE.md`, `README.md` and `tools/README.md` are updated per section 7.
- CI passes on the PR (self-hosted runner). Merge waits for the owner.
