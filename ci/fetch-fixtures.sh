#!/usr/bin/env bash
# Fetch and verify the fixtures the flyvis acceptance gates read.
#
# Two sources, both pinned by ci/fixtures.sha256:
#   - the connectome JSON, from the Nature supplementary zip (Springer)
#   - flyvis's own exports (data/flyvis-build, data/flyvis-stimuli,
#     data/flyvis-reference, data/flyvis-pretrained/ensemble.csv), from the
#     private release named in ci/fixtures-tag
#
# Fetches only what is missing. A file that exists but fails verification is
# never overwritten -- locally it may be a deliberate re-export -- and the
# script fails naming it. Written for bash 3.2 (macOS's /bin/bash).
#
# Every export is dated -- in its provenance file, and ensemble.csv in its
# header -- so re-running an exporter (tools/export_params.py included) makes
# this script refuse the new file. To return to the published set, delete the
# re-exported files and run this script again. A deliberate re-export needs a
# new fixtures-N tag, with ci/fixtures.sha256 and ci/fixtures-tag changed in
# one commit.
#
# Re-exporting fixtures (tarball, release, manifest): fixtures-1 in
# docs/superpowers/plans/2026-10-01-ci.md, Task 3 (manifest: Task 2 Step 1);
# fixtures-2, which added data/flyvis-reference and ensemble.csv, in
# docs/superpowers/plans/2026-10-26-flyvis-reference-traces.md, Task 9.
#
# Design: docs/superpowers/specs/2026-10-01-ci-design.md, extended for
# fixtures-2 by docs/superpowers/specs/2026-10-26-flyvis-reference-traces-design.md
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
# SEMBLANCE_DATA_DIR only redirects this script (used to test it on scratch
# copies); the Rust tests always read <repo>/data.
data="${SEMBLANCE_DATA_DIR:-$repo/data}"
manifest="$repo/ci/fixtures.sha256"
tag="$(tr -d '[:space:]' < "$repo/ci/fixtures-tag")"
asset="flyvis-fixtures.tar.gz"
json="flyvis-supplementary/Lappalainen_et_al_connectome_avg_filters.json"
springer="https://static-content.springer.com/esm/art%3A10.1038%2Fs41586-024-07939-3/MediaObjects/41586_2024_7939_MOESM3_ESM.zip"

say() { echo "fetch-fixtures: $*" >&2; }

missing=""
bad=""
total=0
while read -r want path || [ -n "${want:-}" ]; do
  [ -z "${path:-}" ] && continue
  total=$((total + 1))
  if [ ! -f "$data/$path" ]; then
    missing="$missing$path"$'\n'
  elif [ "$(shasum -a 256 "$data/$path" | cut -d' ' -f1)" != "$want" ]; then
    bad="$bad$path"$'\n'
  fi
done < "$manifest"

if [ -n "$bad" ]; then
  say "these fixtures exist but do not match ci/fixtures.sha256:"
  printf '%s' "$bad" | sed 's/^/  /' >&2
  say "not overwriting or deleting them. Two likely causes:"
  say "  - ci/fixtures-tag or ci/fixtures.sha256 changed since you last fetched:"
  say "    these are stale; delete them and rerun this script."
  say "  - they are a deliberate local re-export: publish a new fixtures tag and"
  say "    update ci/fixtures-tag and ci/fixtures.sha256 together."
  exit 1
fi

if [ -z "$missing" ]; then
  say "all $total fixtures present and verified in $data"
  exit 0
fi

need_json=0
need_exports=0
if printf '%s' "$missing" | grep -qxF "$json"; then need_json=1; fi
if printf '%s' "$missing" | grep -vqxF "$json"; then need_exports=1; fi

# Check every tool this run needs before downloading anything.
need_tool() { # tool hint
  command -v "$1" >/dev/null 2>&1 || { say "$1 not found: $2"; exit 1; }
}
if [ "$need_json" = 1 ]; then
  need_tool curl "install curl"
  need_tool unzip "install unzip"
fi
if [ "$need_exports" = 1 ]; then
  need_tool gh "install the GitHub CLI and run \`gh auth login\`"
  need_tool tar "install tar"
  gh auth status >/dev/null 2>&1 || { say "gh is not authenticated: run \`gh auth login\`"; exit 1; }
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# Verify a fetched file against its manifest line while it is still in $tmp,
# so nothing that fails verification is ever copied into $data.
install_verified() { # src relpath
  want="$(awk -v p="$2" '$2 == p { print $1 }' "$manifest")"
  got="$(shasum -a 256 "$1" | cut -d' ' -f1)"
  if [ "$got" != "$want" ]; then
    say "$2 does not match the pinned hash in ci/fixtures.sha256: the upstream"
    say "source differs from what was pinned. Nothing was written to $data for it."
    exit 1
  fi
  mkdir -p "$data/$(dirname "$2")"
  cp "$1" "$data/$2"
}

if [ "$need_json" = 1 ]; then
  say "fetching the connectome JSON from the Nature supplementary zip"
  curl -fsSL --retry 3 --connect-timeout 30 --max-time 600 -o "$tmp/supplementary.zip" "$springer"
  unzip -q -j "$tmp/supplementary.zip" "*/$(basename "$json")" -d "$tmp/supplementary"
  install_verified "$tmp/supplementary/$(basename "$json")" "$json"
fi

if [ "$need_exports" = 1 ]; then
  say "fetching $asset from release $tag"
  (cd "$repo" && gh release download "$tag" -R "${GITHUB_REPOSITORY:-miki-przygoda/semblance}" --pattern "$asset" --dir "$tmp")
  mkdir "$tmp/x"
  tar -xzf "$tmp/$asset" -C "$tmp/x"
  for path in $missing; do
    [ "$path" = "$json" ] && continue
    if [ ! -f "$tmp/x/$path" ]; then
      say "$path is listed in ci/fixtures.sha256 but is not in $tag's $asset"
      exit 1
    fi
    install_verified "$tmp/x/$path" "$path"
  done
fi

if ! (cd "$data" && shasum -a 256 -c --quiet "$manifest"); then
  say "verification failed after fetching"
  exit 1
fi
say "fetched $(printf '%s' "$missing" | grep -c .) of $total fixtures; all $total verified"
