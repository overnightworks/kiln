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
tests_pid=""
group_teardown_seconds=10

# A member that has exited but is not yet reaped (a zombie) is gone.
group_is_alive() { ps -o stat= --pgid "$tests_pid" | grep -qv '^Z'; }

stop_tests() {
  # The tests run in a process group of their own (setsid), so a signal
  # aimed at this script alone still has to end cargo, the compilers and
  # the test processes, every member, before their directory goes away.
  [[ -n "$tests_pid" ]] || return 0
  kill -TERM -- "-$tests_pid" 2>/dev/null || true
  for ((waited = 0; waited < group_teardown_seconds * 10; waited++)); do
    group_is_alive || break
    sleep 0.1
  done
  kill -KILL -- "-$tests_pid" 2>/dev/null || true
  while group_is_alive; do sleep 0.1; done
  wait "$tests_pid" 2>/dev/null || true
}
trap 'stop_tests; rm -rf "$work_dir"' EXIT
trap 'exit 143' INT TERM

git clone --quiet --no-checkout "$repository" "$work_dir/source"
git -C "$work_dir/source" checkout --quiet --detach "$commit"

CARGO_TARGET_DIR="$work_dir/target" setsid cargo test --locked \
  --manifest-path "$work_dir/source/experiments/e4-sqlite-cost-anatomy/harness/Cargo.toml" \
  -- --ignored &
tests_pid=$!
tests_status=0
wait "$tests_pid" || tests_status=$?
exit "$tests_status"
