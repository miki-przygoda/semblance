#!/usr/bin/env bash
# Owner evidence steps for epic stimulus-traces.
#
# These run Python from tools/ (flyvis, torch) or need flyvis's pretrained
# checkpoints. No Rust test runs them: CLAUDE.md says no crate invokes
# tools/, so what they prove is checked here, by hand, by the owner.
#
# Usage: run one step at a time, from any checkout of this repo:
#   bash docs/superpowers/temper/stimulus-traces/owner-steps.sh <step>
#
# Steps:
#   syn-count     MINORS-05: rerun tools/export_syn_count.py and require it
#                 to rewrite crates/flyvis/src/syn_count_torch.rs byte for byte
#
# No sudo. Installs nothing. Never pushes, merges, or deletes a branch.
# Written for bash 3.2 (macOS's /bin/bash).
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
out="crates/flyvis/src/syn_count_torch.rs"

say() { echo "owner-steps: $*" >&2; }
die() { say "$*"; exit 1; }

step_syn_count() {
  python="$root/tools/.venv/bin/python"
  [ -x "$python" ] \
    || die "no $python: create it with 'uv sync --project tools'"
  for m in 000 049; do
    c="$root/data/flyvis-pretrained/results/flow/0000/$m/chkpts/chkpt_00000"
    [ -e "$c" ] || die "$c not found; run tools/download_pretrained.py"
  done
  # The rerun writes the tracked file in place; start from a clean copy so
  # any difference afterwards is the script's.
  git -C "$root" diff --quiet HEAD -- "$out" \
    || die "$out already has local changes; commit or restore it first"
  say "rerunning tools/export_syn_count.py"
  (cd "$root" && "$python" -B tools/export_syn_count.py)
  if git -C "$root" diff --exit-code HEAD -- "$out"; then
    say "ok: $out rewritten byte for byte"
  else
    die "$out changed (diff above); restore it with: git -C \"$root\" checkout -- $out"
  fi
}

case "${1:-}" in
  syn-count) step_syn_count ;;
  *) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
