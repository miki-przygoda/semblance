# fidelity-measures — temper log

## Plan

**Plan:** `docs/superpowers/temper/fidelity-measures/plan.md`; **args:** `args.json`; **review of the plan:** `plan-review.md` (16 findings, all applied). Branch `fidelity-measures` from `graded-responses` (task 1a's `Responses` and `respond`), default branch `main`. Builds and gates run on beast as jobq jobs.

**Went ahead without the owner's review.** `overnight_without_plan_review: yes`. On 2026-11-05 at 00:40 the owner handed the approved 0.4.0 epics over for the night, to decide, record and continue alone. Nobody reviewed this plan before the build. Every decision below is recorded so it can be checked afterwards.

**Scope split** (decision 3 in `docs/project-outline/0.4.0-fidelity-against-recordings/overnight-2026-11-05.md`, local-only and gitignored, so it is restated here). Tasks 3 and 4 of 0.4.0 are split by the fixtures they need. This epic, `fidelity-measures`, builds the parts that read no fixtures-3: task 3 §1 (`semblance_fidelity::indices`) and §4 (gate E), and task 4 §1 (`semblance_fidelity::similarity`) and §2 (gate C's controls). A later epic, `fidelity-gates`, runs once fixtures-3 is staged and builds gates A–D, `validate --ensemble`, Zhou & Hasler's 50-model runs and task 5's `score`. The reason: the gates link the shared data directory, which has no `flyvis-indices/` until the owner signs off fixtures-3, so any test that read it would fail under `SEMBLANCE_REQUIRE_FIXTURES=1`. The risk is that the split puts a boundary in an awkward place. If so, the gates epic can move code.

**Waves** (two tasks each, for gate_slots 2; a count owner in each wave re-measures CLAUDE.md's test total):

| Wave | Tasks |
|---|---|
| 0 | `idx-layout`, `ctl-network` |
| 1 | `idx-fri`, `ctl-params` |
| 2 | `idx-edges`, `ctl-rewire` |
| 3 | `idx-known`, `sim-cka` |
| 4 | `sim-zh`, `gate-e-legacy` |
| 5 | `docs-counts` |

**Key decisions** (the plan's "Decisions" section has the full list, A1–A20):
- `semblance-fidelity` never depends on `semblance-flyvis`. Ground truth arrives as arguments. `semblance-flyvis` gains `semblance-rng`; `semblance-fidelity` gains it only as a dev-dependency. `Cargo.lock` stays at 8 `semblance*` packages.
- The indices emulate flyvis's dtypes operation by operation. An F32 run's FRI uses an f32 numerator and sum, adds the epsilon and divides in f64, and stores the result as f32. An F32 run's window value that is not an f32 is refused, never rounded. Reductions ignore the layout's order: speeds ascend and angles are read from their values.
- Agreement rules return `Result` (deviation A5/A6), and `pd_within` pairs T4 with ON and T5 with OFF. `binomial_upper_tail` uses exact `u128` coefficients. `tuning_r` takes Maisak's curves as f64, widened through each value's shortest decimal string (deviation A8).
- Zhou & Hasler's functions return `Result` and refuse any type count other than 65 (deviation A14). The cosine epsilon is placed per A14 and checked in fidelity-gates.
- Controls: `control_seed = 0x5EB1_A9CE_0000_0000 ^ (control << 24) ^ (model << 8) ^ attempt`. Shuffling is Durstenfeld's Fisher–Yates. Random parameters are drawn uniformly within the ensemble's range, one draw per parameter. Rewiring redraws a source only while an earlier connection type holds the pair, at most 1000 draws per connection type. An unstable draw is redrawn **up to 10 times** (`MAX_REDRAWS`, attempts 0..=10), as RA §2 says. This was revised in the plan review from the requirements extract's 10 attempts in total, which would have made gate C stricter than the spec. Zeroed gets one attempt. Library code returns redraws and dropped counts and does not print them.
- `respond_network` shares `respond`'s path through `settle_core(net, groups, …)`, so `respond` stays bit-identical (P10). `parity::controls` is a public module and is not re-exported.
- Gate E: `compare_behaviour`, `behavioural_distance` and `size_fidelity_curve` return `Result<_, FidelityError>`. `roundtrip` still prints `0.000000` twice.

## Checkpoint (MVP)

**Full gate: passed.** `sh ~/work/semblance/gates/gate-full.sh` ran at `9d912fa` as jobq job `jq-01M6QBAZDBYHBBDHH3VKTCKB12`. Its log is `/srv/bulk/Projects/semblance/logs/gate-full-20261106T023452-1695893.log` (on beast). 656 tests passed, 0 failed, with no skip notices. `roundtrip` printed both distances as `0.000000`, and the gate ended `gate-full: passed`. It ran once and was not rerun. CLAUDE.md's count (656) matches.

**Flows.** `docs/superpowers/specs/2026-11-04-response-indices-design.md` has no section headed "user flows". The parts a user sees are §5 (`semblance validate`) and the "Proof" list (`roundtrip` at 0.000000, `validate`'s 0.3.0 rows unchanged). The flows below come from those. They ran on the release binary built at `9d912fa`, in jobq job `jq-01M6QCK6Z0DPP8TSEKPTPYZ4G0`.

| Flow | Result | Evidence |
|---|---|---|
| `semblance roundtrip` stays lossless after gate E changed `behavioural_distance` to return `Result` | ok | printed `structural distance:  0.000000` and `behavioural distance: 0.000000`, exit 0 |
| `semblance validate` leaves the 0.3.0 rows and their output unchanged | ok | 16 exact rows `unequal 0`, 16 libm rows within bounds (worst `7.428e-1`, traces run 4, the record's value), 4 controls hold, `verdict: PASS`, exit 0 |
| `semblance validate` shows the new gate A rows (000 and 049, both precisions) and their control | gap | no gate A rows in the table; `data/flyvis-indices/` does not exist on beast (`ls: cannot access 'data/flyvis-indices'`) |
| `semblance validate --ensemble` runs gate A over all 50 at f32, and gate B | gap | `validate: unknown argument --ensemble`, usage printed, exit 2 |
| `semblance validate` exits 2 when `flyvis-indices/` is missing | gap | cannot be tested: `validate` does not read `flyvis-indices/` yet, so a missing directory still gives exit 0 |

**Gaps** (recorded, not fixed):
1. `validate` has no rows or control for gate A. This was planned: the scope split above moves them to the `fidelity-gates` epic, which waits for fixtures-3 (`data/flyvis-indices/`).
2. `validate --ensemble` (gate A over 50 models, and gate B) does not exist. The same split moved it to `fidelity-gates`.
3. Exit 2 for a missing `flyvis-indices/` cannot be checked until gate A reads it (`fidelity-gates`).
4. The spec has no user flows section. The flows above come from §5 and "Proof".
