#!/usr/bin/env bash
# Runs the harness tests that need the measuring machine's CPU topology
# (CPU 2, its sibling 8 and a further CPU for the monitor, protocol.md §9).
# They are `#[ignore]`d so a machine without that topology (CI's two-CPU
# runner) skips them; run-pilot-night.sh runs them here, on the measuring
# machine, before preflight.
#
# The source is a fresh clone of this repository checked out at REV, the
# commit the night script builds, so the tests judge the measured code and
# not whatever the working tree holds. They run in the default dev profile:
# they prove mechanics (pinning, child protocol, monitor), not timings, and
# that profile compiles fastest.
#
# Usage:
#   run-topology-tests.sh --commit REV

set -euo pipefail

if [[ $# -ne 2 || "$1" != "--commit" || -z "$2" ]]; then
  echo "usage: $0 --commit REV" >&2
  exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository="$(git -C "$script_dir" rev-parse --show-toplevel)"
commit="$(git -C "$repository" rev-parse --verify --quiet "$2^{commit}")" \
  || { echo "run-topology-tests.sh: '$2' is not a commit of $repository" >&2; exit 1; }

work_dir="$(mktemp -d)"
# Cargo runs in the foreground, in this script's own process group: Bash holds
# a trapped signal until it returns, so the directory is never removed under
# a running test, and Ctrl-C or a unit stop reaches cargo directly.
trap 'rm -rf "$work_dir"' EXIT
trap 'exit 143' INT TERM

git clone --quiet --no-checkout "$repository" "$work_dir/source"
git -C "$work_dir/source" checkout --quiet --detach "$commit"

CARGO_TARGET_DIR="$work_dir/target" cargo test --locked \
  --manifest-path "$work_dir/source/experiments/e4-sqlite-cost-anatomy/harness/Cargo.toml" \
  -- --ignored
