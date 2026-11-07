# Behaviour spec: fidelity-measures

Derived only from `docs/superpowers/specs/2026-11-04-response-indices-design.md`
(groups indices, agreement, refusals) and, for the groups similarity and
controls, which that spec places out of its scope as task 4, from the sibling
spec it names, `docs/superpowers/specs/2026-11-04-representational-axis-design.md`.
Each behaviour is a given/when/then statement with a stable id.

## indices: flyvis's response indices from a battery's responses

### INDICES-01
- Given a `Responses` and a `BatteryLayout` describing its stimuli by position (each a label, a `Stimulus::Flash { radius, intensity }` or `Stimulus::Edge { angle_deg, speed, intensity }`, and an inclusive `Window { first, last }`)
- When any index function is called
- Then it is pure: it reads no files, runs no simulator, does not depend on `semblance-flyvis`, and takes ground truth only as data passed in

### INDICES-02
- Given a time step `dt`
- When `fri_window(dt)` is computed
- Then it reproduces flyvis's float expression (time axis `frame * dt - t_pre`, the `-0.005 <= t` query as written), giving frames 200..=400 at flyvis's `dt`, with frame 199's exclusion computed from float rounding rather than hard-coded

### INDICES-03
- Given an edge speed of 2.4, 4.8, 9.7, 13, 19 or 25 and flyvis's `dt`
- When `edge_window(speed, dt, …)` is computed from flyvis's own float expressions
- Then the window ends at frame 878, 539, 367, 325, 285 or 265 respectively, and unit tests pin these tabled windows; a fixture-gated test also holds the windows to task 2's index manifest

### INDICES-04
- Given a battery's responses and layout
- When `fri_peaks` runs
- Then it uses only radius-6 flashes and the central column, takes per cell type the `min` of raw voltage over the FRI window and over both intensities, shifts by `+= abs(min)`, and returns raw (unnormalised) peaks shaped [65][off, on]

### INDICES-05
- Given FRI peaks
- When `fri` runs
- Then it returns 65 values `(on - off) / (on + off + 1e-16)`, with an f32 numerator, an f64 epsilon, and the result stored as f32 for an f32 run (so the values are f32-valued)

### INDICES-06
- Given a `Responses` whose `precision()` is F32
- When the FRI is computed
- Then each operation runs in the dtype flyvis runs it in, operation by operation: the shift, the peaks and the FRI numerator in f32, the epsilon add and divide following numpy's promotion, and the stored FRI in f32; for an F64 run every step is f64

### INDICES-07
- Given a battery's responses and layout
- When `edge_peaks` runs
- Then each value is `max(0, max over the window of raw voltage)` with frames outside the window zeroed then clipped, no baseline subtraction and no normalisation, shaped [144][65]

### INDICES-08
- Given edge peaks
- When `dsi` runs
- Then per intensity it computes `abs(Σ_θ r·e^{iθ}) / (max over intensity of Σ_θ abs(r) + 1e-15)`, the denominator shared by ON and OFF, averaged over the 6 speeds, returning one vector per intensity

### INDICES-09
- Given edge peaks
- When `preferred_direction` runs
- Then per intensity it returns `angle(Σ over angle and speed of r·e^{iθ})`, un-normalised

### INDICES-10
- Given edge peaks and known tuning curves passed in as `(name, [f32; 12])` (Maisak 2013, `groundtruth::TUNING_CURVES`)
- When `tuning_r` runs
- Then it returns the Pearson r over the 12 angles per type, intensity and speed ([8][2][6]), with a flat curve's undefined r filled as 0 (`fillna(0)`), plus the best speed's r kept per type and intensity

### INDICES-11
- Given edge peaks from any precision
- When the DSI, preferred-direction and tuning reductions run
- Then they run in f64 (numpy's complex128 and float64) from the peaks

### INDICES-12
- Given cell types requested by name
- When an index function looks them up
- Then it finds them through `Responses::type_index`

### INDICES-13
- Given per-type FRIs and known polarities passed in
- When `fri_correlation_to_known` runs
- Then it returns the Pearson r over the 32 polarised types, as flyvis's helper does

### INDICES-14
- Given per-type DSIs and known 0/1 direction-selectivity labels passed in
- When `dsi_correlation_to_known` runs
- Then it returns the Pearson r over the 18 types, with the DSI maxed over intensity, against the 0/1 label

### INDICES-15
- Given preferred directions and known angles passed in
- When `angular_distance_to_known` runs
- Then it pairs T4 types at intensity 1 and T5 types at intensity 0 with their known 180/0/90/270° directions and returns the angular distances

### INDICES-16
- Given flyvis's model 000 and 049 at f32 and f64, run over task 2's flash and edge `IndexBattery`
- When gate A compares Rust's indices with `IndexFixture`
- Then FRI peaks, FRI and edge peaks equal flyvis's on every value (`==`, refusing a signed-zero difference); DSI, preferred direction, tuning r and the three helpers are within 1e-10 at f64 and within twice flyvis's own f32/f64 gap for that index (from `PROVENANCE.json`) at f32; preferred directions compare as angles the shorter way round; and every row prints its worst difference in `{:.3e}`

### INDICES-17
- Given model 049's responses scored against model 000's fixture values
- When gate A's control runs
- Then the FRI and the edge peaks miss by more than `CONTROL_MISS`, so the control fails as it must

### INDICES-18
- Given `semblance validate`
- When it runs without `--ensemble`
- Then it adds rows for gate A (models 000 and 049, both precisions) and its control to the existing table, keeps the 0.3.0 rows and their output byte-identical, exits 0/1/2 as before (a missing `flyvis-indices/` is exit 2), runs models in parallel under `--threads` with one `respond` per worker, and prints per-model wall time

### INDICES-19
- Given `semblance validate --ensemble`
- When it runs
- Then it also runs gate A over all 50 models at f32 and gate B; plain CI does not pass `--ensemble`

## agreement: the paper's agreement rules and constants

### AGREEMENT-01
- Given the agreement constants
- When they are read
- Then they live in one place, each with its citation: `D_THRESH = 0.357`, `BINOMIAL_P0 = 0.1`, `BINOMIAL_ALPHA = 0.05`, `PD_TOLERANCE_DEG = 45.0`, the last documented as borrowed from the paper's model selection; the rules are recorded as the paper's (flyvis's code contains none), or as ours where the paper has none

### AGREEMENT-02
- Given per-type FRIs, their type names and known polarities `(name, i8)`
- When `polarity_agreement` runs
- Then it returns an `Agreement` holding the count agreeing in sign, out of how many, and the missed types

### AGREEMENT-03
- Given per-model FRI vectors
- When `median_fri` runs
- Then it returns the per-type median across models

### AGREEMENT-04
- Given n, k and p0
- When `binomial_upper_tail(n, k, p0)` runs
- Then it returns the exact one-sided upper tail P(X >= k) for X ~ Binomial(n, p0), summed in f64, matching values computed by hand for small n

### AGREEMENT-05
- Given per-model DSI vectors, `d_thresh`, `p0` and `alpha`
- When `dsi_class` runs
- Then a type is classed direction selective when the number of models with DSI > `d_thresh` passes the one-sided binomial test against H0 p = `p0` at P < `alpha`

### AGREEMENT-06
- Given preferred directions, known directions in degrees and a tolerance
- When `pd_within` runs
- Then it returns an `Agreement` counting the types whose preferred direction is within the tolerance of the known one, compared as angles the shorter way round, with the misses named

### AGREEMENT-07
- Given flyvis's model 000 at f32 under Rust's indices and the paper's rules (gate B, in `validate --ensemble`)
- When polarity agreement is counted
- Then it agrees on exactly 30 of the 32 polarised types

### AGREEMENT-08
- Given all 50 models at f32
- When the median FRI's polarity agreement is counted over all 50 and over the top 10 (models 000–009)
- Then gate B holds if either agrees on 32 of 32, and the output names which did

### AGREEMENT-09
- Given all 50 models' DSIs and the paper's DSI classification of the 18 types, committed as a cited constant
- When the binomial rule classifies them
- Then the classification equals the paper's; any difference between the paper's classification and `groundtruth::KNOWN_DSI_TYPES`'s labels is printed and recorded, not resolved by choosing

### AGREEMENT-10
- Given model 000's preferred directions
- When `angular_distance_to_known`'s pairing (T4 at intensity 1, T5 at intensity 0) is checked with the 45° tolerance
- Then all 8 of T4a–d and T5a–d fall within 45°

### AGREEMENT-11
- Given model 000 and the ensemble median
- When tuning r is computed in gate B
- Then r per type is printed and not gated

### AGREEMENT-12
- Given flyvis's own exported index values for all 50 models
- When the same rules run over them
- Then they give the same counts, sets and verdicts as Rust's values

### AGREEMENT-13
- Given the paper's numbers do not reproduce
- When gate B runs
- Then it fails and the record says what was found; no rule is loosened to make it pass

## refusals: scoring never fails toward perfect

### REFUSALS-01
- Given two recordings whose channel count, neuron count or tick count differ (for example a busy recording against an empty one)
- When `compare_behaviour(a, b)` runs
- Then it returns `Err(FidelityError)` rather than 0.0

### REFUSALS-02
- Given an empty battery (for example against a network shifted by +5.0)
- When `behavioural_distance(…)` runs
- Then it returns `Err(FidelityError)` rather than 0.0

### REFUSALS-03
- Given either legacy error
- When `size_fidelity_curve` runs
- Then it propagates the error

### REFUSALS-04
- Given a non-empty probe battery
- When `behavioural_distance` compares a network with a different network, and with itself
- Then the distance is positive for the different network and exactly 0 for the same one (replacing the vacuous `== 0.0` assertion)

### REFUSALS-05
- Given `app`'s `roundtrip` and `evolve`
- When they call the now-fallible legacy functions
- Then they handle the `Result`, and `roundtrip` still prints `0.000000` for both distances

### REFUSALS-06
- Given a `Responses` and a `BatteryLayout` with a different stimulus count, a label mismatch, or a window past a stimulus's last frame
- When `BatteryLayout::check` or any index function using the layout runs
- Then it returns the matching `IndexError` refusal, never a score

### REFUSALS-07
- Given a cell type name absent from the `Responses`
- When an index function looks it up
- Then it returns `IndexError::MissingCellType`

### REFUSALS-08
- Given a non-finite value, empty input, or the wrong number of angles or speeds for a reduction
- When an index or agreement function that returns a `Result` runs
- Then it returns the matching `IndexError` variant; every `IndexError` and `ResponseError` variant has a test, including a `Responses` whose shape disagrees with the layout and an empty battery

### REFUSALS-09
- Given two response matrices with different row counts or column counts, fewer than 4 rows, a non-positive `H(X,X)`, or non-finite values
- When `hsic_unbiased` or `cka` runs
- Then it returns a `SimilarityError` rather than a similarity

## similarity: the representational axis

### SIMILARITY-01
- Given a `Responses` and each stimulus's onset frame
- When `ResponseMatrix::from_responses(&Responses, onset)` builds the matrix
- Then rows are (stimulus, frame) for every frame from each stimulus's onset to its end, in battery order, columns are the 65 cell types, the grey pre-stimulus frames are excluded, and values are stored as f32 for an F32 run (losslessly) and f64 otherwise

### SIMILARITY-02
- Given two response matrices
- When `hsic_unbiased` runs
- Then it computes Song et al. (2012)'s unbiased HSIC with linear kernels in feature space (from `XᵀY`, the row norms and the column sums, in O(n·p²), accumulated in f64) without forming the n × n Gram matrix, and agrees with a direct Gram-matrix implementation on small random matrices (n ≤ 50) to within 1e-12 relative

### SIMILARITY-03
- Given two response matrices
- When `cka` runs
- Then it returns `H(X,Y) / sqrt(H(X,X) · H(Y,Y))`, with `H(X,X)` computed by the same function as `H(X,Y)` called with the same argument twice

### SIMILARITY-04
- Given any valid response matrix x
- When `cka(x, x)` runs
- Then it returns exactly 1.0, by construction and without a special case for identical inputs

### SIMILARITY-05
- Given any two valid response matrices a and b
- When `cka(a, b)` and `cka(b, a)` run
- Then they are equal bit for bit, because `hsic_unbiased` sums in an order symmetric in its arguments

### SIMILARITY-06
- Given a response matrix and an orthogonal transform or an isotropic scaling of it
- When CKA is computed against the transformed matrix
- Then it is unchanged to within 1e-12; under a per-column scaling it changes

### SIMILARITY-07
- Given a `Responses`
- When `peak_vectors` runs
- Then it returns, per condition, the peak central raw voltage per cell type over the whole trace (65 values), as Zhou & Hasler's response

### SIMILARITY-08
- Given per-condition peak vectors
- When `cosine_rdm` runs
- Then it returns the cosine-distance RDM with Zhou & Hasler's `+ 1e-10`, matching hand-computed small cases

### SIMILARITY-09
- Given two RDMs
- When `spearman` and `kendall_tau_a` run
- Then they return the Spearman correlation (ties averaged) and Kendall τA over the upper triangle, matching hand-computed small cases including ties

### SIMILARITY-10
- Given two sets of ensemble-mean peak vectors
- When `cka_biased` runs
- Then it returns Zhou & Hasler's biased linear CKA; their measures are reported only, and no Nili permutation test or p-value is reproduced

### SIMILARITY-11
- Given all 50 trained models at f32 on the flash and edge batteries (gate D, in `validate --ensemble`)
- When CKA is computed
- Then `cka(m, m) == 1.0` exactly for every model, and the 1,225 pairwise scores are printed as min, 5th percentile, median and max and kept for the record, the minimum being gate C's line

### SIMILARITY-12
- Given Zhou & Hasler's runs (`fade_in_state` initialisation as flyvis 1.2.0 implements it, then 12 ON and 12 OFF edges at speed 19 through Rust's `MovingEdge` generator) for the 50 trained models and the 200 control draws
- When their variant is computed
- Then the pairwise RDM Spearman mean ± sd is printed beside their 0.721 ± 0.150 and 0.838 ± 0.059, and trained-vs-control values beside their 0.686 and 0.846, not gated, with the output saying Rust's generator was used and these are compared only with the paper's printed statistics

### SIMILARITY-13
- Given `validate --ensemble`
- When gates C and D run
- Then each model's response matrix is built as its responses arrive and the `Responses` dropped (peak memory the 50 matrices plus one model in flight), per-model and total times are printed, and the same exit codes apply; plain CI does not run them

## controls: gate C's four controls

### CONTROLS-01
- Given each of the 50 trained models and each of the four controls
- When a control draw is made
- Then it is seeded through `semblance_rng::Rng::seed_from_u64` with a seed derived from (control index, model index) by a fixed formula written in the code and the record, and the same (control, model) gives the same draw

### CONTROLS-02
- Given a trained model
- When the zeroed control is built
- Then it keeps the model's biases and time constants and sets all 604 strengths to 0, deterministically

### CONTROLS-03
- Given a trained model
- When the shuffled control is built
- Then its 604 strengths are a Fisher–Yates permutation (via `Rng::below`) of the model's across connection types, and every sign is unchanged because signs stay with the connection type

### CONTROLS-04
- Given the 50 trained models
- When the random-parameters control is built
- Then each of the 734 parameters is drawn uniformly (`Rng::next_f32`) between that parameter's minimum and maximum over the 50 models, and lies within that range

### CONTROLS-05
- Given a trained model
- When the rewired control is built
- Then each connection type keeps its target type, offsets, synapse counts, sign and strength and draws a new presynaptic type; its edges are rebuilt from the new type's node in the same source column, an edge whose source column the new type lacks is dropped and the dropped count printed, and the result passes `TypedNetwork::new`

### CONTROLS-06
- Given a rewiring draw that would give two connection types the same (source, target) pair
- When the rewired control is built
- Then that draw is redrawn

### CONTROLS-07
- Given a rewired `TypedNetwork` and parameters
- When `respond_network(f, net, params, …)` runs
- Then it responds on that topology with the torch synapse counts following each edge, so exactness is unaffected; it is a control-only entry point, and `respond` and `score` keep taking a 734-vector over the fixed topology

### CONTROLS-08
- Given a control draw whose state goes non-finite
- When it is simulated
- Then it is redrawn with the next seed in its sequence, up to 10 times, every redraw printed and recorded; ten in a row on one draw is a gate C failure

### CONTROLS-09
- Given each control's 50 draws (gate C, in `validate --ensemble`)
- When each draw is scored by CKA against its own parent model
- Then every one falls below the trained pairwise minimum

### CONTROLS-10
- Given each control's 50 draws
- When gate B's binomial rule classifies the 18 types
- Then it agrees with the paper's classification on fewer types than over the trained 50

### CONTROLS-11
- Given each control's 50 draws
- When preferred directions are checked with the 45° tolerance
- Then the median, over the 50 draws, of how many of the 8 motion types fall within 45° is below model 000's count

### CONTROLS-12
- Given each control
- When polarity and tuning are computed
- Then they are printed for every control and not gated
