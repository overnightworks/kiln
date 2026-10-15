#!/usr/bin/env bash
# Runs the E4 feasibility pilot (protocol.md §13) for a scheduled night
# run: builds nothing — both release binaries (instrumented and
# passthrough) must already exist at the given paths — takes the shared
# probe-stack lock (waiting for it, never skipping the proof), runs
# preflight against the instrumented binary, and then runs the pilot
# (which gates every comparison attempt against the interference monitor
# itself, see harness/src/comparison.rs) against the manifest preflight
# just wrote. Outputs land under
# experiments/e4-sqlite-cost-anatomy/pilot/<UTC timestamp>.<unique suffix>/,
# a directory created for this run alone so a night never overwrites or
# reuses the last one's record.
#
# Usage:
#   run-pilot-night.sh --instrumented-binary PATH --passthrough-binary PATH
#                       [--output-root DIR]
#
# KILN_E4_PROBE_LOCK overrides the probe-stack lock file path (default
# /tmp/probe-stack.lock, the one shared across repositories) — a test
# seam only; a real night run always uses the shared default.

set -euo pipefail

usage() {
  echo "usage: $0 --instrumented-binary PATH --passthrough-binary PATH [--output-root DIR]" >&2
}

instrumented_binary=""
passthrough_binary=""
output_root=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --instrumented-binary)
      instrumented_binary="${2:-}"
      shift 2
      ;;
    --passthrough-binary)
      passthrough_binary="${2:-}"
      shift 2
      ;;
    --output-root)
      output_root="${2:-}"
      shift 2
      ;;
    *)
      echo "run-pilot-night.sh: unknown argument '$1'" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$instrumented_binary" || -z "$passthrough_binary" ]]; then
  echo "run-pilot-night.sh: --instrumented-binary and --passthrough-binary are required" >&2
  usage
  exit 2
fi

if [[ ! -x "$instrumented_binary" ]]; then
  echo "run-pilot-night.sh: instrumented binary '$instrumented_binary' does not exist or is not executable (this script builds nothing)" >&2
  exit 1
fi
if [[ ! -x "$passthrough_binary" ]]; then
  echo "run-pilot-night.sh: passthrough binary '$passthrough_binary' does not exist or is not executable (this script builds nothing)" >&2
  exit 1
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -z "$output_root" ]]; then
  output_root="$script_dir/pilot"
fi

lock_file="${KILN_E4_PROBE_LOCK:-/tmp/probe-stack.lock}"

# One probe stack per machine (the operator's global contract): wait for
# the lock, held across both preflight and the pilot, released only when
# this script exits.
exec 9>"$lock_file"
flock 9

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$output_root"
output_dir="$(mktemp -d "$output_root/$timestamp.XXXXXX")"

manifest_path="$output_dir/manifest.txt"
if ! "$instrumented_binary" preflight "$manifest_path"; then
  echo "run-pilot-night.sh: preflight failed against '$instrumented_binary'; no pilot run started (see output above)" >&2
  exit 1
fi

exec "$instrumented_binary" pilot --output-dir "$output_dir" \
  --passthrough-binary "$passthrough_binary" --manifest "$manifest_path"
