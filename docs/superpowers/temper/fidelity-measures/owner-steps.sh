#!/usr/bin/env bash
# Owner evidence steps for epic fidelity-measures (0.4.0, tasks 3 and 4,
# the parts that read no fixtures-3).
#
# These need the owner's GitHub account, the network, or the owner's own
# flyvis install under tools/.venv. No agent runs them. Read
# docs/superpowers/temper/fidelity-measures/owner-checklist.md first; it says
# what each step proves and what to do with the result.
#
# Usage: run one step at a time, from any checkout of this repo:
#   bash docs/superpowers/temper/fidelity-measures/owner-steps.sh <step>
#
# Steps, in order:
#   preflight   check gh auth, the branch, Cargo.lock and the base PR (read-only)
#   push        push fidelity-measures to origin, no force (asks first)
#   pr          open the PR from pr-body.md (asks first)
#   ci          wait for CI on the PR, then save its logs as evidence
#   evidence    check the saved CI logs for what the PR body claims
#   record      check CI's parity-record artifact against the committed record
#   licence     show flyvis's licence from tools/.venv, for open item 8
#
# No sudo. Installs nothing. Never merges, never deletes a branch.
# Written for bash 3.2 (macOS's /bin/bash).
set -euo pipefail

repo_slug="miki-przygoda/semblance"
branch="fidelity-measures"
stack_base="graded-responses"
pr_title="Add the fidelity measures: response indices, CKA, gate C's controls and gate E"
root="$(git rev-parse --show-toplevel)"
body="$root/docs/superpowers/temper/fidelity-measures/pr-body.md"
record="$root/docs/results/0.3.0-flyvis-parity.md"
evid="$root/.temper/fidelity-measures/evidence"

# What the PR body and CLAUDE.md claim; evidence checks CI against these.
want_passed=715
want_ignored=19

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

# The state of the base branch's own PR (#19): OPEN, MERGED, CLOSED or empty.
base_pr_state() {
  gh pr list --repo "$repo_slug" --head "$stack_base" --state all \
    --json state --jq '.[0].state // empty'
}

latest_run_id() {
  n="$(pr_number)"
  [ -n "$n" ] || die "no PR for $branch yet (run: pr)"
  head_sha="$(gh pr view "$n" --repo "$repo_slug" --json headRefOid --jq .headRefOid)"
  gh run list --repo "$repo_slug" --workflow CI --commit "$head_sha" \
    --json databaseId --jq '.[0].databaseId // empty'
}

step_preflight() {
  command -v gh >/dev/null || die "gh is not installed"
  gh auth status >/dev/null 2>&1 || die "gh is not logged in: run 'gh auth login'"
  say "gh: logged in"

  git -C "$root" rev-parse --verify --quiet "refs/heads/$branch" >/dev/null \
    || die "no local branch $branch"
  say "local $branch at $(git -C "$root" rev-parse --short "$branch")"
  git -C "$root" log -1 --format='%s' "$branch" | grep -q . \
    || die "cannot read $branch's head commit"

  # The epic is stacked on graded-responses (task 1a, PR #19).
  git -C "$root" rev-parse --verify --quiet "refs/heads/$stack_base" >/dev/null \
    || die "no local branch $stack_base"
  git -C "$root" merge-base --is-ancestor "$stack_base" "$branch" \
    || die "$branch does not contain $stack_base; it was built on top of it"
  ahead="$(git -C "$root" rev-list --count "$stack_base".."$branch")"
  say "$branch is $ahead commits ahead of $stack_base"

  # Exactly 8 packages, all semblance*: semblance-rng is a new workspace
  # dependency of semblance-flyvis and a dev-dependency of semblance-fidelity,
  # and nothing from crates.io came with it.
  n="$(git -C "$root" show "$branch:Cargo.lock" | grep -c '^name = ')"
  other="$(git -C "$root" show "$branch:Cargo.lock" | grep '^name = ' | grep -vc '"semblance' || true)"
  [ "$n" -eq 8 ] && [ "$other" -eq 0 ] \
    || die "Cargo.lock on $branch lists $n packages ($other not semblance*)"
  say "Cargo.lock: 8 packages, all semblance*"

  # No version bump: 0.4.0 is not finished (fidelity-gates and later tasks).
  v="$(git -C "$root" show "$branch:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)"
  say "workspace version on $branch: ${v:-<not found>} (expected 0.3.1)"
  [ "$v" = "0.3.1" ] || say "WARNING: the version moved; this epic does not bump"

  st="$(base_pr_state)"
  say "PR for $stack_base: ${st:-<none>}"
  case "$st" in
    OPEN)   say "the PR will target $stack_base (stacked)";;
    MERGED) say "the PR will target main; $stack_base is merged";;
    *)      say "WARNING: $stack_base has no open or merged PR; decide the base by hand";;
  esac

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
  st="$(base_pr_state)"
  case "$st" in
    OPEN)   base="$stack_base";;
    MERGED) base="main";;
    *)      die "$stack_base's PR is '${st:-none}'; open this PR by hand once its base is settled";;
  esac
  say "about to open a PR: '$pr_title', $branch -> $base, body from pr-body.md"
  confirm "open the PR?"
  gh pr create --repo "$repo_slug" --base "$base" --head "$branch" \
    --title "$pr_title" --body-file "$body"
}

step_ci() {
  n="$(pr_number)"
  [ -n "$n" ] || die "no PR for $branch yet (run: pr)"
  say "PR #$n: waiting for CI (hosted runners)"
  # Exits non-zero if any check fails; keep going so the logs are saved anyway.
  set +e
  gh pr checks "$n" --repo "$repo_slug" --watch --interval 30
  checks=$?
  set -e
  run_id="$(latest_run_id)"
  [ -n "$run_id" ] || die "no CI run found for the PR's head"
  mkdir -p "$evid"
  gh run view "$run_id" --repo "$repo_slug" --json conclusion,url,headSha,jobs \
    > "$evid/ci-run-$run_id.json"
  gh run view "$run_id" --repo "$repo_slug" --log > "$evid/ci-run-$run_id.log"
  say "saved $evid/ci-run-$run_id.json and .log"
  [ "$checks" -eq 0 ] || die "CI did not pass (gh pr checks exited $checks); read the log"
  say "CI passed (next: evidence)"
}

step_evidence() {
  run_id="$(latest_run_id)"
  [ -n "$run_id" ] || die "no CI run found"
  log="$evid/ci-run-$run_id.log"
  [ -f "$log" ] || die "missing $log (run: ci)"
  fail=0
  check() { if grep -qF -- "$2" "$log"; then say "ok: $1"; else say "MISSING: $1"; fail=1; fi; }

  check "fixtures fetched and verified (step ran)" "Fetch and verify fixtures"
  check "roundtrip structural distance 0.000000" "structural distance:  0.000000"
  check "roundtrip behavioural distance 0.000000" "behavioural distance: 0.000000"
  check "moving-edge gate ran (1 passed)" "1 passed"

  if grep -q 'FAILED' "$log"; then
    say "FOUND a FAILED result:"; grep -n 'FAILED' "$log" | head -20 >&2; fail=1
  else
    say "ok: no FAILED result"
  fi
  if grep -q 'skipping:' "$log"; then
    say "FOUND a skip notice (a fixture gate verified nothing):"
    grep -n 'skipping:' "$log" | head -20 >&2; fail=1
  else
    say "ok: no skipping: line"
  fi

  # Count only the two "Test, with every fixture required" steps (the test
  # job and the acceptance job together cover the workspace once); the
  # moving-edge step reruns one test and must not be counted twice.
  totals="$(grep -F 'Test, with every fixture required' "$log" \
    | grep -o 'test result: ok\. [0-9]* passed; [0-9]* failed; [0-9]* ignored' \
    | awk '{p+=$4; f+=$6; i+=$8} END {printf "%d %d %d", p, f, i}')"
  set -- $totals
  say "CI test totals: $1 passed, $2 failed, $3 ignored (claimed: $want_passed passed, 0 failed, $want_ignored ignored)"
  [ "${1:-0}" -eq "$want_passed" ] && [ "${2:-1}" -eq 0 ] && [ "${3:-0}" -eq "$want_ignored" ] \
    || { say "MISMATCH: the counts differ from the PR body and CLAUDE.md"; fail=1; }

  conclusion="$(gh run view "$run_id" --repo "$repo_slug" --json conclusion --jq .conclusion)"
  say "run conclusion: $conclusion"
  [ "$conclusion" = "success" ] || fail=1

  [ "$fail" -eq 0 ] || die "evidence incomplete; see above"
  say "evidence complete for run $run_id"
}

# The record (docs/results/0.3.0-flyvis-parity.md) is regenerated only from
# CI's parity-record artifact. This epic must not change validate's output,
# so the artifact's generated block, host-specific section aside, must equal
# the committed one. app/tests/record.rs already holds CI to this; this step
# lets the owner see it with their own eyes.
step_record() {
  run_id="$(latest_run_id)"
  [ -n "$run_id" ] || die "no CI run found"
  dir="$evid/parity-record-$run_id"
  [ ! -e "$dir" ] || die "$dir already exists; remove it to download again"
  gh run download "$run_id" --repo "$repo_slug" --name parity-record --dir "$dir"
  art="$dir/parity-record.md"
  [ -f "$art" ] || die "the artifact has no parity-record.md"
  block() {
    sed -n '/^<!-- BEGIN semblance validate --markdown -->$/,/^<!-- END semblance validate --markdown -->$/p' "$1" \
      | sed '/^<!-- BEGIN host-specific -->$/,/^<!-- END host-specific -->$/d'
  }
  if diff <(block "$record") <(block "$art") >/dev/null; then
    say "ok: CI's generated block equals the committed record's (host section aside)"
  else
    diff <(block "$record") <(block "$art") >&2 || true
    die "the generated block moved; do not edit the record by hand -- find out why"
  fi
}

# Open item 8: semblance-fidelity is Apache-2.0 only, but its indices module
# ports flyvis's MIT-licensed index code. This only shows the licence text
# from the owner's own flyvis install; the decision is the owner's.
step_licence() {
  venv="$root/tools/.venv"
  [ -d "$venv" ] || die "no $venv: create it with 'uv sync --project tools'"
  lic="$(find "$venv/lib" -path '*flyvis-*.dist-info/*' \( -iname 'LICENSE*' -o -iname 'METADATA' \) 2>/dev/null | head -5)"
  [ -n "$lic" ] || die "no flyvis dist-info under $venv"
  for f in $lic; do
    say "== $f"
    case "$f" in
      *METADATA) grep -i '^License' "$f" >&2 || true ;;
      *) sed -n '1,25p' "$f" >&2 ;;
    esac
  done
  say "semblance-fidelity: $(git -C "$root" show "$branch:crates/fidelity/Cargo.toml" | grep '^license' || echo 'license.workspace')"
  say "semblance-flyvis:   $(git -C "$root" show "$branch:crates/flyvis/Cargo.toml" | grep '^license')"
  say "decide per owner-checklist.md, 'Decisions only the owner makes'"
}

case "${1:-}" in
  preflight) step_preflight ;;
  push)      step_push ;;
  pr)        step_pr ;;
  ci)        step_ci ;;
  evidence)  step_evidence ;;
  record)    step_record ;;
  licence)   step_licence ;;
  *) sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
