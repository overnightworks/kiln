#!/usr/bin/env bash
# Tests run-pilot-night.sh's argument handling and preflight-failure
# path against fake "harness binaries" (plain shell scripts standing in
# for the real release binaries) — never a real pilot run. Each fake
# binary's `preflight`/`pilot` subcommands just record how they were
# called, so this stays mechanics-only and fast.
#
# Run directly: ./run-pilot-night-test.sh

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
night_script="$script_dir/run-pilot-night.sh"

failures=0
pass() { echo "PASS [KILN-E4-19]: $1"; }
fail() {
  echo "FAIL [KILN-E4-19]: $1" >&2
  failures=$((failures + 1))
}

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

make_fake_binary() {
  # make_fake_binary PATH PREFLIGHT_EXIT_CODE
  local path="$1" preflight_exit="$2"
  cat >"$path" <<FAKE
#!/usr/bin/env bash
set -euo pipefail
marker_dir="$work_dir/calls"
mkdir -p "\$marker_dir"
case "\${1:-}" in
  preflight)
    echo "manifest" >"\${2:-/dev/null}"
    echo "fake-preflight \$*" >>"\$marker_dir/preflight-$$"
    exit $preflight_exit
    ;;
  pilot)
    echo "fake-pilot \$*" >>"\$marker_dir/pilot-$$"
    exit 0
    ;;
  measure-cell)
    echo "mean_wall_ns=1000"
    exit 0
    ;;
  *)
    echo "fake binary: unknown subcommand \$1" >&2
    exit 2
    ;;
esac
FAKE
  chmod +x "$path"
}

instrumented_ok="$work_dir/instrumented-ok"
instrumented_failing_preflight="$work_dir/instrumented-failing-preflight"
passthrough="$work_dir/passthrough"
make_fake_binary "$instrumented_ok" 0
make_fake_binary "$instrumented_failing_preflight" 1
make_fake_binary "$passthrough" 0

lock_file="$work_dir/probe-stack.lock"
output_root="$work_dir/pilot-output"

# --- argument handling -------------------------------------------------

if "$night_script" >/dev/null 2>"$work_dir/stderr-missing-args"; then
  fail "missing required arguments must exit non-zero"
else
  if grep -q "required" "$work_dir/stderr-missing-args"; then
    pass "missing required arguments exits non-zero with a clear message"
  else
    fail "missing-arguments message was not clear: $(cat "$work_dir/stderr-missing-args")"
  fi
fi

if "$night_script" --instrumented-binary "$work_dir/does-not-exist" \
    --passthrough-binary "$passthrough" --output-root "$output_root" \
    >/dev/null 2>"$work_dir/stderr-missing-binary"; then
  fail "a nonexistent instrumented binary must exit non-zero"
else
  if grep -q "does not exist" "$work_dir/stderr-missing-binary"; then
    pass "a nonexistent instrumented binary exits non-zero with a clear message"
  else
    fail "nonexistent-binary message was not clear: $(cat "$work_dir/stderr-missing-binary")"
  fi
fi

if "$night_script" --instrumented-binary "$instrumented_ok" \
    --unknown-flag value >/dev/null 2>"$work_dir/stderr-unknown-flag"; then
  fail "an unknown flag must exit non-zero"
else
  pass "an unknown flag exits non-zero"
fi

# --- preflight-failure path --------------------------------------------

KILN_E4_PROBE_LOCK="$lock_file" "$night_script" \
  --instrumented-binary "$instrumented_failing_preflight" \
  --passthrough-binary "$passthrough" \
  --output-root "$output_root" \
  >"$work_dir/stdout-preflight-fail" 2>"$work_dir/stderr-preflight-fail" \
  && preflight_fail_exit=0 || preflight_fail_exit=$?

if [[ "$preflight_fail_exit" -eq 0 ]]; then
  fail "a failing preflight must make the script exit non-zero"
else
  pass "a failing preflight makes the script exit non-zero (exit $preflight_fail_exit)"
fi

if grep -qi "preflight failed" "$work_dir/stderr-preflight-fail"; then
  pass "a failing preflight prints a clear message"
else
  fail "preflight-failure message was not clear: $(cat "$work_dir/stderr-preflight-fail")"
fi

if ls "$work_dir/calls"/pilot-* >/dev/null 2>&1; then
  fail "the pilot must never run once preflight has failed"
else
  pass "the pilot never runs once preflight has failed"
fi
rm -rf "$work_dir/calls"

# --- happy path reaches the pilot with the right arguments -------------

KILN_E4_PROBE_LOCK="$lock_file" "$night_script" \
  --instrumented-binary "$instrumented_ok" \
  --passthrough-binary "$passthrough" \
  --output-root "$output_root" \
  >"$work_dir/stdout-happy" 2>"$work_dir/stderr-happy" \
  && happy_exit=0 || happy_exit=$?

if [[ "$happy_exit" -eq 0 ]]; then
  pass "a passing preflight and pilot together exit zero"
else
  fail "the happy path exited $happy_exit: $(cat "$work_dir/stderr-happy")"
fi

pilot_call="$(cat "$work_dir/calls"/pilot-* 2>/dev/null || true)"
if [[ "$pilot_call" == *"--output-dir"* && "$pilot_call" == *"--passthrough-binary $passthrough"* ]]; then
  pass "the pilot is called with --output-dir and --passthrough-binary"
else
  fail "the pilot was not called with the expected arguments: $pilot_call"
fi

run_dirs=("$output_root"/*/)
if [[ ${#run_dirs[@]} -eq 1 && -d "${run_dirs[0]}" ]]; then
  pass "exactly one timestamped output directory was created"
else
  fail "expected exactly one timestamped output directory under $output_root, found: ${run_dirs[*]:-none}"
fi

if [[ $failures -eq 0 ]]; then
  echo "all run-pilot-night.sh tests passed"
  exit 0
else
  echo "$failures run-pilot-night.sh test(s) failed"
  exit 1
fi
