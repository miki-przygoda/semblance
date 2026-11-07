# The representational axis and the controls: gates C and D

**Date:** 2026-11-04
**Status:** design approved by the owner in chat 2026-11-04; spec approved
2026-11-04.
**Epic:** 0.4.0 fidelity against recordings, task 4 —
`docs/project-outline/0.4.0-fidelity-against-recordings/` (not in git):
gates C and D as rewritten 2026-11-04, owner's answers Q3 (revised
2026-11-04) and Q5, decisions 6 and 7.
**Builds on:**
[`2026-11-04-graded-responses-design.md`](2026-11-04-graded-responses-design.md)
(task 1a) and
[`2026-11-04-response-indices-design.md`](2026-11-04-response-indices-design.md)
(task 3: the indices, gate B's rules, `validate --ensemble`).
**Facts:** `task-0-facts.md` (local), "Zhou & Hasler's representational axis".

## Why

The indices say whether a network reproduces the fly's published response
properties. They do not say whether it represents a stimulus battery the
way its parent does, which is what a compressed network must keep. This
piece adds that second axis, gives it a scale (gate D), and proves that both
axes can fail: networks that should not look like the fly do not (gate C).

Zhou & Hasler's own measure barely separates trained flyvis models from
random ones (pairwise 0.721 ± 0.150 vs random 0.686, their Exp 1), so the
owner chose (Q3, revised) to compute theirs exactly and report it, and to
gate on a measure that can tell the two apart.

## Goal

1. `semblance_fidelity::similarity`: debiased linear CKA over central
   response time courses, and Zhou & Hasler's variant, as pure functions.
2. `semblance_flyvis::parity::controls`: the four Q5 controls, 50 seeded
   draws each.
3. Gate D: the trained ensemble's pairwise scale, with exact identity.
4. Gate C: every control scores worse than the trained models.
5. Zhou & Hasler's numbers, reproduced and printed beside theirs.

All of it runs in the record step (`validate --ensemble`), not plain CI.

## Facts the design rests on

| Fact | Source |
|---|---|
| Z&H: flyvis `MovingEdge`, speed 19 columns/s, offsets −10..10, height 80, `t_pre` = `t_post` = 1 s, dt 1/200; Exp 1 = 12 ON directions, Exp 2 = 12 × ON/OFF | task 0, Z&H |
| Z&H initialise with `fade_in_state`, a 1 s fade-in, not the index pipeline's `steady_state` | task 0, Z&H |
| Z&H's response: peak central raw voltage per type over the whole trace, 65 values per condition; cosine RDM (+1e-10); Spearman and Kendall τA over the upper triangle; biased linear CKA on ensemble means | task 0, Z&H |
| Z&H: pairwise trained RDM Spearman 0.721 ± 0.150 (Exp 1), 0.838 ± 0.059 (Exp 2); trained vs random 0.686 and 0.846; controls are sign-preserving shuffles rejection-sampled for stability; no rewiring | task 0, Z&H |
| `syn_strength` is one scalar per connection type (604); an edge's weight is `conn_sign[ct] * n_syn * syn_strength[ct]`, so the sign belongs to the connection type, not the strength | `crates/connectome/src/typed.rs:212-232` |
| A `TypedNetwork` is built from `TypedNetworkParts` (cell types, nodes with `u`/`v`, `conn_types` as (source type, target type), `conn_sign`, edges) and validated by `TypedNetwork::new` | `typed.rs:18-40,113` |
| `semblance-rng`'s stream is frozen; using its existing methods does not change it | `CLAUDE.md` |
| The moving-edge battery's stimulus phase is frames 200 onward of each 1078-frame edge, and of each 600-frame flash | task 0 |

## Design

### 1. `semblance_fidelity::similarity`

Pure, over task 1a's `Responses`.

**The response matrix.** `ResponseMatrix::from_responses(&Responses,
onset: &[usize])`: rows are (stimulus, frame) for every frame from each
stimulus's onset to its end, in battery order; columns the 65 cell types.
The grey pre-stimulus frames are excluded (owner's choice): they repeat the
settled state. About 125k rows for the flash and edge batteries. Stored as
f32 when `Responses::precision()` is F32 (every value is exactly an f32, so
nothing is lost) and f64 otherwise. 50 models at f32 are about 1.6 GB.

**Debiased linear CKA.**

```rust
pub fn hsic_unbiased(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError>;
pub fn cka(x: &ResponseMatrix, y: &ResponseMatrix) -> Result<f64, SimilarityError>;
```

`hsic_unbiased` is Song et al. (2012)'s unbiased estimator with linear
kernels, computed in feature space so the n × n Gram matrix is never
formed: from `XᵀY` (65 × 65), the row norms, and the column sums, in
O(n·p²), accumulated in f64. `cka = H(X,Y) / sqrt(H(X,X) · H(Y,Y))`.

- **Exact identity, by construction.** `H(X,X)` is the same function as
  `H(X,Y)` called with the same argument twice, so `cka(x, x)` divides `H`
  by `sqrt(H · H)`, which in binary floating point returns exactly `|H|`
  (barring overflow or underflow). No special case for identical inputs.
- **Symmetry.** `hsic_unbiased(x, y)` sums in an order symmetric in its
  arguments, so `cka(a, b) == cka(b, a)` bit for bit.
- **Errors:** row-count or column-count mismatch, fewer than 4 rows (the
  estimator divides by n − 3), a non-positive `H(X,X)`, non-finite values.

**Zhou & Hasler's variant** (reported only):

```rust
pub fn peak_vectors(r: &Responses) -> Result<Vec<[f64; 65]>, SimilarityError>; // per condition
pub fn cosine_rdm(v: &[[f64; 65]]) -> Result<Rdm, SimilarityError>;            // + 1e-10, as theirs
pub fn spearman(a: &Rdm, b: &Rdm) -> Result<f64, SimilarityError>;             // upper triangle, ties averaged
pub fn kendall_tau_a(a: &Rdm, b: &Rdm) -> Result<f64, SimilarityError>;
pub fn cka_biased(x: &[[f64; 65]], y: &[[f64; 65]]) -> Result<f64, SimilarityError>; // on ensemble means
```

**Revised 2026-11-07:** these return `Result`, as the agreement rules do
(deviation A5/A6, accepted by the owner 2026-11-05): a malformed input is a
refusal, never a score. `peak_vectors` refuses a type count other than 65;
the others refuse mismatched or too-small inputs, non-finite values, and an
input with no variance (deviation A14).

No Nili permutation test: their p-values are not reproduced, only their
correlations.

### 2. The controls: `semblance_flyvis::parity::controls`

Each control is applied to each of the 50 trained models, giving 50 draws
per control, seeded through `semblance_rng::Rng::seed_from_u64` with a seed
derived from (control index, model index) by a fixed formula written in the
code and the record.

| Control | Construction |
|---|---|
| **zeroed** | the model's biases and time constants; all 604 strengths 0. Deterministic |
| **shuffled** | the model's 604 strengths permuted across connection types (Fisher–Yates via `Rng::below`). Signs stay with the connection type, so every sign is kept, as Z&H's sign-preserving shuffles |
| **random parameters** | each of the 734 drawn uniformly between that parameter's minimum and maximum over the 50 models (`Rng::next_f32`) |
| **rewired** | the model's parameters on a new topology: each connection type keeps its target type, offsets, synapse counts, sign and strength, and draws a new presynaptic type. Its edges are rebuilt from the new type's node in the same source column; an edge whose source column the new type lacks is dropped, and the count is printed. A draw that would give two connection types the same (source, target) pair is redrawn |

The rewired network goes through `respond_network(f, net: &TypedNetwork,
params, …)`, a control-only entry point (Q5, decision 6): `respond` and
`score` keep taking a 734-vector over the fixed topology. The torch synapse
counts follow each edge, so exactness is unaffected.

**Unstable draws.** If a draw's state goes non-finite, it is redrawn with
the next seed in its sequence, up to 10 times, as Z&H rejection-sample.
Every redraw is printed and recorded. Ten in a row on one draw is a gate C
failure.

### 3. Gate D: the axis is calibrated

All 50 trained models at f32, on the flash and edge batteries:

- `cka(m, m) == 1.0` exactly, for every model.
- The 1,225 pairwise scores, printed as min, 5th percentile, median and
  max, and kept for the record. The minimum is the line gate C uses.
- Z&H's variant on its own runs (section 5): pairwise RDM Spearman mean ± sd
  over the 50, printed beside their 0.721 ± 0.150 and 0.838 ± 0.059, with
  trained-vs-control values beside their 0.686 and 0.846. Not gated; a
  mismatch is a finding to investigate.

### 4. Gate C: the controls fail

For each of the four controls:

- **Representational axis:** every one of its 50 draws, scored against its
  own parent model, falls below the trained pairwise minimum.
- **DSI class:** gate B's binomial rule over the control's 50 draws agrees
  with the paper's classification on fewer of the 18 types than over the
  trained 50.
- **Preferred direction:** the median, over the 50 draws, of how many of
  the 8 motion types fall within 45° is below model 000's count.
- **Polarity and tuning:** printed for every control, not gated (the paper
  finds random parameters still get polarity right).

### 5. Zhou & Hasler's runs

`fade_in_state` as flyvis implements it (the plan's first step reads and
cites it from flyvis 1.2.0's source; it is a new initialisation in
`parity`, beside `settle_state`), then 12 ON and 12 OFF edges at speed 19
through Rust's `MovingEdge` generator. Using Rust's generator here is
deliberate: these numbers are compared only with a paper's printed
statistics, never with flyvis's values, and no gate rests on them. The
output says so. 50 trained models plus the 200 control draws.

### 6. `validate --ensemble`

Gates C and D join task 3's ensemble rows, under the same exit codes. The
response matrices are built as each model's responses arrive and the
`Responses` dropped, so peak memory is the 50 matrices plus one model in
flight. Per-model and total times are printed.

## Proof, beyond the gates

- `hsic_unbiased` against a direct Gram-matrix implementation on small random
  matrices (test-only code, n ≤ 50), equal to within 1e-12 relative.
- `cka(x, x) == 1.0` and `cka(a, b) == cka(b, a)` on random matrices.
- CKA invariance: unchanged (to 1e-12) under an orthogonal transform and an
  isotropic scaling of one argument; changed by a per-column scaling.
- `spearman`, `kendall_tau_a` and `cosine_rdm` against hand-computed small
  cases, ties included.
- Each control's construction: zeroed has 604 zeros; shuffled is a
  permutation of the original with signs unchanged; random parameters lie
  within the per-parameter range; rewired keeps every connection type's
  target, offsets and counts, and passes `TypedNetwork::new`.
- Seeds: the same (control, model) gives the same draw.

## Out of scope

- Scoring compressed networks: 0.5.0.
- Z&H's permutation tests and their fixture-level reproduction.
- Natural-scene stimuli: Sintel stays deferred.

## Versioning

`semblance-fidelity` and `semblance-flyvis` gain modules. No version number
changes in this piece.
