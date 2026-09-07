#!/usr/bin/env bash
# check-action-runners.sh: refuse a Docker container action on a non-Linux job.
#
# GitHub runs `runs: using: "docker"` actions on Linux runners ONLY. Referencing
# one from a job whose runners include Windows or macOS fails at run time with
# "Container action is only supported on Linux" (spec 003 D-1). `actionlint`
# does not catch this: it validates syntax and expressions, not the runner
# compatibility of a referenced action's implementation.
#
# That failure is invisible while the step is skipped behind a guard, which is
# how D-1 survived from phase 0 until the first crate landed in phase 1. This
# check is static, so it catches the mistake on the day it is written rather
# than on the day the guard opens.
#
#   ok          exit 0
#   violation   exit 1
#   usage       exit 2
#
# An action whose metadata cannot be fetched is reported as unverified and does
# not fail the run; a confirmed Docker action always does.
set -uo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
dir="${1:-$root/.github/workflows}"
[ -d "$dir" ] || { echo "check-action-runners: no such directory: $dir" >&2; exit 2; }

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

violations=0
checked=0

# Emit "<job>\t<line>" for every line inside a job body. A job key is exactly
# two spaces deep under a top-level `jobs:`.
job_lines() {
  awk '
    /^jobs:[[:space:]]*$/            { injobs = 1; next }
    injobs && /^[^[:space:]]/        { injobs = 0 }
    injobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ {
      job = $1; sub(/:$/, "", job); next
    }
    injobs && job != ""              { print job "\t" $0 }
  ' "$1"
}

for wf in "$dir"/*.yml "$dir"/*.yaml; do
  [ -f "$wf" ] || continue
  job_lines "$wf" > "$tmp"

  while read -r job; do
    [ -n "$job" ] || continue
    body="$(awk -F'\t' -v j="$job" '$1 == j { print $2 }' "$tmp")"
    # Only jobs that can land on a non-Linux runner are at risk.
    printf '%s\n' "$body" | grep -qE 'windows-|macos-' || continue

    while read -r ref; do
      [ -n "$ref" ] || continue
      case "$ref" in ./*) continue ;; esac   # local reusable workflow
      checked=$((checked + 1))
      repo="${ref%@*}"
      gitref="${ref##*@}"
      meta=""
      for f in action.yml action.yaml; do
        meta="$(curl -fsSL "https://raw.githubusercontent.com/${repo}/${gitref}/${f}" 2>/dev/null)" && break
      done
      if [ -z "$meta" ]; then
        echo "  ? $(basename "$wf") [$job] $ref: metadata not fetched; unverified" >&2
        continue
      fi
      if printf '%s\n' "$meta" | grep -qE '^[[:space:]]*using:[[:space:]]*"?docker"?[[:space:]]*$'; then
        echo "  x $(basename "$wf") [$job] $ref is a Docker container action, but this job targets a non-Linux runner" >&2
        violations=$((violations + 1))
      fi
    done < <(printf '%s\n' "$body" | sed -n 's/^[[:space:]]*-\{0,1\}[[:space:]]*uses:[[:space:]]*//p' | tr -d '\r')
  done < <(cut -f1 "$tmp" | sort -u)
done

if [ "$violations" -gt 0 ]; then
  echo "check-action-runners: $violations violation(s)" >&2
  exit 1
fi
echo "check-action-runners: ok ($checked action reference(s) on non-Linux jobs)"
