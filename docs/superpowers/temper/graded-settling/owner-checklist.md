# graded-settling: owner checklist

Epic 0.3.0, piece 1 (epic tasks 2, 3 and 4). Agents have built and gated the
branch locally. The steps below need the owner's GitHub account, the
self-hosted runner on the owner's Mac, or the local-only project outline, so
no agent ran them.

The commands are in `owner-steps.sh`, in this directory. Run them one step at
a time:

```bash
bash docs/superpowers/temper/graded-settling/owner-steps.sh <step>
```

The script asks before it pushes or opens the PR. It never merges, never
deletes a branch and never uses sudo. It saves CI evidence under
`.temper/graded-settling/evidence/`, which is local and not in git.

## Before CI

- [ ] **Preflight** (`preflight`). This step only reads. It checks:
  - `gh` is logged in
  - `graded-settling` exists and is ahead of `main`
  - `Cargo.lock` on the branch lists 8 packages, all `semblance*`
  - the repo variable `CI_RUNNER` is `self-hosted`
  - whether a runner is online

  If `CI_RUNNER` is unset, CI queues on `ubuntu-latest`, which has no
  minutes left.
- [ ] **Start the runner.** In a separate terminal, run
  `~/actions-runner/semblance/start.sh`. It runs in the foreground, and
  Ctrl-C stops it. Jobs queue on GitHub while it is off.

## Publishing the branch

- [ ] **Push** (`push`). This pushes `graded-settling` to `origin` without
  force. Agents never push.
- [ ] **Open the PR** (`pr`). The PR goes from `graded-settling` to `main`,
  titled "Add graded dynamics and the flyvis settling gates", with
  `pr-body.md` as its body. Read the body first. Its numbers were measured
  locally on the branch head:
  - 332 tests
  - 66 skips without data
  - the exact and libm tables
  - five open items

## CI evidence

- [ ] **Wait for CI** (`ci`). This step watches the PR's checks, then saves
  the run's JSON and full log. Both jobs must pass: `test`, and `msrv` on
  1.85. Note that this CI is macOS arm64, not Linux, so it is evidence for
  one platform only.
- [ ] **Check the log** (`evidence`). This step greps the saved log for:
  - the fixture fetch being verified
  - all 8 exact and 8 libm settling runs passing
  - both controls passing, and the bounds-to-provenance check
  - the moving-edge gate's `1 passed`
  - roundtrip `0.000000` twice
  - no `FAILED` result and no `skipping:` line

  It also prints the total number of passed tests.
- [ ] **Read the moving-edge distribution** in the log. Do not just check
  that it passed. Its table is also the sign that the fixture gates ran
  rather than skipped.
- [ ] **Optional: check the published fixtures** (`fixtures`). This fetches
  `fixtures-2` into a fresh scratch directory and verifies all 35 files
  against `ci/fixtures.sha256`, without touching `data/`. CI's cache can
  hide a broken release asset; this step cannot. Delete the scratch
  directory afterwards.

CI does not run the settling gates with `--nocapture`, so the per-run tables
in `pr-body.md` come from the local run, not from CI. CI's evidence is that
every gate passed with `SEMBLANCE_REQUIRE_FIXTURES=1`.

## Decisions only the owner makes

Record each answer in the PR or in the outline. None of them blocks CI.

- [ ] **The f32 integer-count control** (review finding, kind `owner`).
  `the_gate_resolves_integer_counts` checks only f64 run 3 against 1e-10.
  The review settled f32 runs 0, 2 and 4 with integer counts, and all three
  pass the f32 libm bound (ratios 0.21 to 0.60). Only run 6 fails it.

  Spec section 6 says the control shows that the libm bound catches this
  class of error. In f32, only the exact gate does. Choose one:
  - narrow the spec's wording and the test's doc comment, or
  - add an f32 control that is expected to fail, or
  - accept it as it is.
- [ ] **Open items 1 to 3 in `pr-body.md`.** Decide whether each is fixed in
  this PR or deferred:
  - silent settling skips from the shared `OnceLock`
  - f64 ratios printed as `0.000 of bound`
  - the dated header in `syn_count_torch.rs`

  If deferred, record them in the epic's README as deferred, so the epic can
  still bump at piece 3.

## Project outline (local only, not in git)

- [ ] **Update `docs/project-outline/0.3.0-graded-dynamics/`.** Spec
  section 7 and "Done when" ask for:
  - tasks 2, 3 and 4 marked done
  - the printed per-run results copied in from `pr-body.md`, "Settling gate
    output"
  - F25's question answered (G16)

  No agent can do this, because the directory is not in any worktree.

## Merge

- [ ] **Merge only after CI is green** and the decisions above are recorded.
  Merging is the owner's call.
- [ ] **No version bump.** This is piece 1 of epic 0.3.0, and the workspace
  stays at 0.2.0. The bump comes with piece 3, once every task in the epic
  is done or recorded as deferred.
- [ ] **No crates.io publish.** The spec's non-goals exclude it.

## Not part of this epic, but do not forget

The self-hosted runner must be deregistered before the repo goes public.
`CI_RUNNER` must be deleted, and `runs-on` set back to a literal
`ubuntu-latest`. On a public repo, a fork PR could otherwise run code on the
Mac. The go-public checklist tracks this; nothing in this epic changes it.
