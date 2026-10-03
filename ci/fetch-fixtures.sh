#!/usr/bin/env bash
# Fetch and verify the fixtures the flyvis acceptance gates read.
#
# Two sources, both pinned by ci/fixtures.sha256:
#   - the connectome JSON, from the Nature supplementary zip (Springer)
#   - flyvis's own exports (data/flyvis-build, data/flyvis-stimuli), from the
#     private release named in ci/fixtures-tag
#
# Fetches only what is missing. A file that exists but fails verification is
# never overwritten -- locally it may be a deliberate re-export -- and the
# script fails naming it. Written for bash 3.2 (macOS's /bin/bash).
#
# Design: docs/superpowers/specs/2026-10-01-ci-design.md
set -euo pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
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
while read -r want path; do
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
  say "not overwriting them. If they are a deliberate re-export, publish a new"
  say "fixtures tag and update ci/fixtures-tag and ci/fixtures.sha256 together."
  exit 1
fi

if [ -z "$missing" ]; then
  say "all $total fixtures present and verified in $data"
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if printf '%s' "$missing" | grep -qxF "$json"; then
  say "fetching the connectome JSON from the Nature supplementary zip"
  curl -fsSL --retry 3 -o "$tmp/supplementary.zip" "$springer"
  unzip -q -j "$tmp/supplementary.zip" "*/$(basename "$json")" -d "$tmp/supplementary"
  mkdir -p "$data/$(dirname "$json")"
  cp "$tmp/supplementary/$(basename "$json")" "$data/$json"
fi

if printf '%s' "$missing" | grep -vqxF "$json"; then
  say "fetching $asset from release $tag"
  (cd "$repo" && gh release download "$tag" --pattern "$asset" --dir "$tmp")
  mkdir "$tmp/x"
  tar -xzf "$tmp/$asset" -C "$tmp/x"
  for path in $missing; do
    [ "$path" = "$json" ] && continue
    if [ ! -f "$tmp/x/$path" ]; then
      say "$path is listed in ci/fixtures.sha256 but is not in $tag's $asset"
      exit 1
    fi
    mkdir -p "$data/$(dirname "$path")"
    cp "$tmp/x/$path" "$data/$path"
  done
fi

if ! (cd "$data" && shasum -a 256 -c --quiet "$manifest"); then
  say "verification failed after fetching"
  exit 1
fi
say "fetched $(printf '%s' "$missing" | grep -c .) of $total fixtures; all $total verified"
