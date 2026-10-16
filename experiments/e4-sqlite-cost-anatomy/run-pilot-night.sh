#!/usr/bin/env bash
# Runs the E4 feasibility pilot (protocol.md §13) for a scheduled night
# run. It first builds both binaries (instrumented and passthrough) from a
# clean checkout of the reviewed commit and a build record of what that
# build made (build-pilot-binaries.sh) — nothing else writes the record,
# and the binaries are held against it. It then takes the shared
# probe-stack lock (waiting for it, never skipping the proof), runs
# preflight from the instrumented binary (it checks both binaries against
# the build record and writes the source commit and both hashes to the
# manifest), and runs the pilot (which admits only the binaries the build
# record and the manifest vet and gates every comparison attempt against
# the interference monitor, see harness/src/pilot_run.rs). Outputs land
# under experiments/e4-sqlite-cost-anatomy/pilot/<UTC timestamp>.<unique
# suffix>/, a directory created for this run alone so a night never
# overwrites or reuses the last one's record. The binaries and the build
# record live in a temporary directory removed when the script ends; the
# build record is copied into the run's directory as part of its evidence.
#
# Usage:
#   run-pilot-night.sh --commit REV [--output-root DIR]
#
# KILN_E4_PROBE_LOCK overrides the probe-stack lock file path (default
# /tmp/probe-stack.lock, the one shared across repositories) and
# KILN_E4_BUILD_STEP the build script — test seams only; a real night run
# always uses the shared lock and the real build script.

set -euo pipefail

usage() {
  echo "usage: $0 --commit REV [--output-root DIR]" >&2
}

commit=""
output_root=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --commit)
      commit="${2:-}"
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

if [[ -z "$commit" ]]; then
  echo "run-pilot-night.sh: --commit is required" >&2
  usage
  exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -z "$output_root" ]]; then
  output_root="$script_dir/pilot"
fi

build_step="${KILN_E4_BUILD_STEP:-$script_dir/build-pilot-binaries.sh}"
build_dir="$(mktemp -d)"
trap 'rm -rf "$build_dir"' EXIT

if ! "$build_step" --commit "$commit" --output-dir "$build_dir"; then
  echo "run-pilot-night.sh: building commit '$commit' failed; no preflight or pilot started (see output above)" >&2
  exit 1
fi
instrumented_binary="$build_dir/instrumented"
passthrough_binary="$build_dir/passthrough"
build_record="$build_dir/build-record.txt"
for built in "$instrumented_binary" "$passthrough_binary"; do
  if [[ ! -x "$built" ]]; then
    echo "run-pilot-night.sh: the build step left no executable '$built'" >&2
    exit 1
  fi
done
if [[ ! -f "$build_record" ]]; then
  echo "run-pilot-night.sh: the build step left no build record '$build_record'" >&2
  exit 1
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
cp "$build_record" "$output_dir/build-record.txt"

manifest_path="$output_dir/manifest.txt"
if ! "$instrumented_binary" preflight "$manifest_path" "$passthrough_binary" "$build_record"; then
  echo "run-pilot-night.sh: preflight failed against '$instrumented_binary'; no pilot run started (see output above)" >&2
  exit 1
fi

pilot_status=0
"$instrumented_binary" pilot --output-dir "$output_dir" \
  --passthrough-binary "$passthrough_binary" --manifest "$manifest_path" \
  --build-record "$build_record" || pilot_status=$?
exit "$pilot_status"
