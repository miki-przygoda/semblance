# fidelity-measures: owner checklist

Epic `fidelity-measures` (0.4.0, tasks 3 and 4, the parts that read no
fixtures-3), on branch `fidelity-measures`, stacked on `graded-responses`
(task 1a, PR #19).

Agents built and gated this epic on beast. What remains needs the owner's
GitHub account, the network, the owner's flyvis install, or the owner's
judgement. No agent ran any of it.

Each step's commands are in `owner-steps.sh`, next to this file. Run them
from a checkout that has the head's commits on `fidelity-measures` (the
build host never pushes):

```
bash docs/superpowers/temper/fidelity-measures/owner-steps.sh <step>
```

Evidence is saved under `.temper/fidelity-measures/evidence/`, which is
local and not committed.

## Already proved on beast (no action needed)

These are listed so you know what CI is re-proving, not proving for the
first time.

- `gate-full` on the final tree passed (jobq `jq-01M6VFWJMA4HTJT3FYCMV32ADF`,
  log `/srv/bulk/Projects/semblance/logs/gate-full-20261107T171121-3290368.log`):
  - 715 passed, 0 failed, 19 ignored
  - no skip notices
  - `roundtrip` printed `0.000000` twice
- The MVP checkpoint's release flows (`jq-01M6QCK6Z0DPP8TSEKPTPYZ4G0`):
  - `validate`'s 0.3.0 rows were unchanged: 16 exact rows `unequal 0`, and
    16 libm rows within bounds (worst `7.428e-1`)
  - all 4 controls held, `verdict: PASS`, exit 0

## Ship: push, PR and CI

- [ ] **Preflight** (`preflight`, read-only). It checks:
  - `gh` is logged in
  - `fidelity-measures` contains `graded-responses`
  - `Cargo.lock` still lists exactly 8 `semblance*` packages
    (`semblance-rng` is new as a workspace dependency of `semblance-flyvis`
    and a dev-dependency of `semblance-fidelity`)
  - the workspace version is still `0.3.1`
  - PR #19's state, which decides this PR's base
- [ ] **Push** (`push`): `fidelity-measures` to origin, no force. It asks
  first.
- [ ] **Open the PR** (`pr`), using `pr-body.md` as the body.
  - While PR #19 is open, the base is `graded-responses`, so the diff shows
    only this epic.
  - If #19 has merged, the base is `main`.
  - Otherwise the step refuses, and you choose the base by hand.
  - It asks first.
- [ ] **CI** (`ci`): waits on hosted CI, then saves the run's JSON and full
  log. CI runs on `pull_request` whatever the base.
- [ ] **Check the evidence** (`evidence`). It requires:
  - the fixture fetch-and-verify step ran
  - `roundtrip` `0.000000` for both distances
  - the moving-edge gate's `1 passed`
  - no `FAILED` and no `skipping:` line
  - exactly **715 passed, 0 failed, 19 ignored** across the two "Test, with
    every fixture required" steps (the `test` job and the `acceptance` job)
  - run conclusion `success`

  A count mismatch means the PR body and `CLAUDE.md` are wrong, or CI ran
  something different from beast. Find out which. Do not just edit the
  numbers.
- [ ] **Read the moving-edge distribution** in the saved log. Do not just
  check that it passed.
- [ ] **Check the record did not move** (`record`). This downloads CI's
  `parity-record` artifact and requires its generated block (host section
  aside) to equal `docs/results/0.3.0-flyvis-parity.md`'s.
  - This epic changes `settle_core`'s signature but must not change
    `validate`'s output.
  - If the block differs, do not regenerate the record from this PR. Find
    what moved first.

## Decisions only the owner makes

Record each answer in the PR or in the outline. None of them blocks CI. All
of them should be settled before 0.4.0 bumps.

- [ ] **Licensing (open item 8). Settle it before the next publish.**
  - `semblance-fidelity` is licensed Apache-2.0 only, but `indices` ports
    flyvis's MIT-licensed index code: the FRI and edge windows, the peaks,
    DSI, preferred direction, and the to-known helpers.
  - `semblance-flyvis` handles the same kind of port with a `NOTICE` entry
    and `license = "Apache-2.0 AND MIT"`.
  - `licence` prints flyvis's licence from your `tools/.venv`, beside both
    crates' licence fields.
  - Choose one:
    - make `semblance-fidelity` `Apache-2.0 AND MIT` and add its files to
      `NOTICE`
    - rewrite the ported parts from the paper instead of the code
    - accept it as it is, with a reason
  - No agent changed a licence; it is your call.
- [ ] **The overnight deviations (open item 7).** Nobody reviewed the plan
  before the build. Check these against `plan.md` (A1–A20) and
  `plan-review.md` (16 applied findings), and accept or reopen each:
  - A5/A6: the `Result` returns (gate E's functions now refuse malformed
    input instead of scoring it as a perfect 0)
  - A8: the tuning curves widened to f64
  - A14: the 65-type refusal in Zhou & Hasler's measures, and where their
    cosine epsilon is placed (not yet checked against their numbers;
    planned for `fidelity-gates`, open item 6)
  - `MAX_REDRAWS = 10`, as the review revised it
- [ ] **`RESOURCES.md` entries (open item 9).** No entries were added for
  the CKA papers or for Zhou & Hasler. The file promises every link was checked live, and agents
  had no network. Add them after checking the links, or record that they
  wait for `fidelity-gates`.
- [ ] **The 19 ignored acceptance-failing tests (open item 5).** Confirm that
  `fidelity-gates` owns un-ignoring them: INDICES-03 and -16–19,
  AGREEMENT-07–13, SIMILARITY-11–13 and CONTROLS-09–12.
- [ ] **Sign off fixtures-3 (`data/flyvis-indices/`)** when ready. It blocks
  `fidelity-gates` (gates A–D, `validate --ensemble`, exit 2 on a missing
  `flyvis-indices/`), not this PR.

## Project outline (local only, not in git)

- [ ] **Update `docs/project-outline/0.4.0-fidelity-against-recordings/`.**
  - Mark the fixture-free parts of tasks 3 and 4 as done in this epic.
  - Record the scope split from `overnight-2026-11-05.md`, decision 3, as
    carried out, with the rest moved to `fidelity-gates`.
  - Copy in the answers to the decisions above.

  No agent can do this, because the directory is not in any worktree.

## Merge

- [ ] **Merge only after CI is green** and the licensing decision is at least
  recorded. Merge PR #19 first, then make sure this PR's base is `main`
  before merging it. GitHub retargets it on its own only if
  `graded-responses` is deleted on merge.
- [ ] **No version bump.** 0.4.0 is not finished: `fidelity-gates`, `score`
  and the 0.4.0 record are still to come. The workspace stays at `0.3.1`.
- [ ] **No crates.io publish.** `semblance-fidelity`'s package description
  changed, but it reaches crates.io only at the next publish. That publish
  waits for your go-ahead, and for the licensing decision above.
