#!/usr/bin/env bash
# Owner evidence steps for epic graded-settling (0.3.0, piece 1).
#
# These need the owner's GitHub account, the self-hosted runner on the owner's
# Mac, or the network. No agent runs them. Read
# docs/superpowers/temper/graded-settling/owner-checklist.md first; it says
# what each step proves and what to do with the result.
#
# Usage: run one step at a time, from any checkout of this repo:
#   bash docs/superpowers/temper/graded-settling/owner-steps.sh <step>
#
# Steps, in order:
#   preflight     check gh auth, the branch, CI_RUNNER and the runner (read-only)
#   push          push graded-settling to origin (asks first)
#   pr            open the PR from pr-body.md (asks first)
#   ci            wait for CI on the PR, then save its log as evidence
#   evidence      check the saved CI log for what "Done when" needs
#   fixtures      optional: fetch fixtures-2 into a scratch dir and verify it
#
# No sudo. Installs nothing. Never merges, never deletes a branch.
# Written for bash 3.2 (macOS's /bin/bash).
set -euo pipefail

repo_slug="miki-przygoda/semblance"
branch="graded-settling"
pr_title="Add graded dynamics and the flyvis settling gates"
root="$(git rev-parse --show-toplevel)"
body="$root/docs/superpowers/temper/graded-settling/pr-body.md"
evid="$root/.temper/graded-settling/evidence"

say() { echo "owner-steps: $*" >&2; }
die() { say "$*"; exit 1; }
confirm() {
  printf 'owner-steps: %s [y/N] ' "$1" >&2
  read -r ans
  [ "$ans" = "y" ] || [ "$ans" = "Y" ] || die "stopped; nothing done"
}

pr_number() {
  gh pr list --repo "$repo_slug" --head "$branch" --state all \
    --json number --jq '.[0].number // empty'
}

step_preflight() {
  command -v gh >/dev/null || die "gh is not installed"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in: run 'gh auth login'"
  say "gh: logged in"

  git -C "$root" rev-parse --verify --quiet "refs/heads/$branch" >/dev/null \
    || die "no local branch $branch"
  say "local $branch at $(git -C "$root" rev-parse --short "$branch")"
  ahead="$(git -C "$root" rev-list --count main.."$branch")"
  [ "$ahead" -ge 1 ] || die "$branch has no commits beyond main"
  say "$branch is $ahead commits ahead of local main"

  # The branch must still have exactly 8 packages, all semblance*.
  n="$(git -C "$root" show "$branch:Cargo.lock" | grep -c '^name = ')"
  other="$(git -C "$root" show "$branch:Cargo.lock" | grep '^name = ' | grep -vc '"semblance' || true)"
  [ "$n" -eq 8 ] && [ "$other" -eq 0 ] \
    || die "Cargo.lock on $branch lists $n packages ($other not semblance*)"
  say "Cargo.lock: 8 packages, all semblance*"

  v="$(gh variable get CI_RUNNER --repo "$repo_slug" 2>/dev/null || echo '<unset>')"
  say "repo variable CI_RUNNER = $v"
  [ "$v" = "self-hosted" ] \
    || say "WARNING: CI will queue on ubuntu-latest, which has no minutes left"

  # Needs admin on the repo; a failure here is not fatal.
  if st="$(gh api "repos/$repo_slug/actions/runners" \
        --jq '.runners[] | "\(.name) \(.status) busy=\(.busy)"' 2>/dev/null)"; then
    say "runners: ${st:-<none registered>}"
    printf '%s\n' "$st" | grep -q ' online ' \
      || say "no runner online: start ~/actions-runner/semblance/start.sh in another terminal"
  else
    say "could not list runners (needs repo admin); check the Actions settings page"
  fi

  if git -C "$root" ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then
    say "origin/$branch exists at $(git -C "$root" ls-remote --heads origin "$branch" | cut -c1-7)"
  else
    say "origin/$branch does not exist yet (next: push)"
  fi
}

step_push() {
  local_sha="$(git -C "$root" rev-parse "$branch")"
  say "about to push $branch ($local_sha) to origin, no force"
  confirm "push $branch?"
  git -C "$root" push origin "refs/heads/$branch:refs/heads/$branch"
}

step_pr() {
  [ -f "$body" ] || die "missing $body"
  existing="$(pr_number)"
  [ -z "$existing" ] || die "PR #$existing already exists for $branch"
  say "about to open a PR: '$pr_title', $branch -> main, body from pr-body.md"
  confirm "open the PR?"
  gh pr create --repo "$repo_slug" --base main --head "$branch" \
    --title "$pr_title" --body-file "$body"
}

step_ci() {
  n="$(pr_number)"
  [ -n "$n" ] || die "no PR for $branch yet (run: pr)"
  say "PR #$n; the runner must be online (~/actions-runner/semblance/start.sh)"
  # Exits non-zero if any check fails; keep going so the log is saved anyway.
  set +e
  gh pr checks "$n" --repo "$repo_slug" --watch --interval 30
  checks=$?
  set -e
  head_sha="$(gh pr view "$n" --repo "$repo_slug" --json headRefOid --jq .headRefOid)"
  run_id="$(gh run list --repo "$repo_slug" --workflow CI --commit "$head_sha" \
    --json databaseId --jq '.[0].databaseId // empty')"
  [ -n "$run_id" ] || die "no CI run found for $head_sha"
  mkdir -p "$evid"
  gh run view "$run_id" --repo "$repo_slug" --json conclusion,url,headSha,jobs \
    > "$evid/ci-run-$run_id.json"
  gh run view "$run_id" --repo "$repo_slug" --log > "$evid/ci-run-$run_id.log"
  say "saved $evid/ci-run-$run_id.{json,log}"
  say "run: $(gh run view "$run_id" --repo "$repo_slug" --json url --jq .url)"
  [ "$checks" -eq 0 ] || die "CI checks did not all pass (exit $checks); read the log"
  say "CI passed on $head_sha"
}

step_evidence() {
  log="$(ls -t "$evid"/ci-run-*.log 2>/dev/null | head -1 || true)"
  [ -n "$log" ] || die "no saved CI log in $evid (run: ci)"
  say "reading $log"
  fail=0
  check() { # description, grep pattern
    if grep -qE "$2" "$log"; then say "ok:   $1"; else say "MISSING: $1"; fail=1; fi
  }
  check "fixtures fetched and verified" 'fixtures present and verified|fetched and verified'
  check "settling exact runs ran" 'settling_exact_run_7_049_1_50_f64 \.\.\. ok'
  check "settling libm runs ran" 'settling_libm_run_7_049_1_50_f64 \.\.\. ok'
  check "integer-count control ran" 'the_gate_resolves_integer_counts \.\.\. ok'
  check "model control ran" 'the_gate_tells_models_apart \.\.\. ok'
  check "bounds pinned to provenance" 'pre_registered_bounds_match_provenance \.\.\. ok'
  check "moving-edge gate: 1 passed" '1 passed'
  check "roundtrip structural 0.000000" 'structural distance:  0\.000000'
  check "roundtrip behavioural 0.000000" 'behavioural distance: 0\.000000'
  if grep -E 'test result: FAILED' "$log" >/dev/null; then
    say "FAILED test results in the log"; fail=1
  fi
  passed="$(grep -oE 'test result: ok\. [0-9]+ passed' "$log" \
    | awk '{s += $4} END {print s + 0}')"
  say "tests passed across the log: $passed (local count on the branch head: 332, plus the moving-edge rerun)"
  if grep -E 'skipping:' "$log" >/dev/null; then
    say "a 'skipping:' line appears in the log; with SEMBLANCE_REQUIRE_FIXTURES=1 that should not happen"
    fail=1
  fi
  [ "$fail" -eq 0 ] || die "evidence incomplete; see above"
  say "evidence complete"
}

step_fixtures() {
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/semblance-fixtures.XXXXXX")"
  say "fetching fixtures-2 into $scratch (does not touch data/)"
  SEMBLANCE_DATA_DIR="$scratch" "$root/ci/fetch-fixtures.sh"
  say "verified a fresh download; delete $scratch when done"
}

case "${1:-}" in
  preflight) step_preflight ;;
  push) step_push ;;
  pr) step_pr ;;
  ci) step_ci ;;
  evidence) step_evidence ;;
  fixtures) step_fixtures ;;
  *) sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
