#!/usr/bin/env bash
# Tests run-pilot-night.sh's argument handling, build step and
# preflight-failure path against a fake build step and fake "harness
# binaries" (plain shell scripts standing in for the real release
# binaries) — never a real build or pilot run. Each fake binary's
# `preflight`/`pilot` subcommands just record how they were called, so
# this stays mechanics-only and fast.
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

calls_dir="$work_dir/calls"

# The fake build step: records its arguments, then leaves fake binaries and
# a record in --output-dir. FAKE_BUILD_FAILS makes it fail, FAKE_NO_RECORD
# makes it leave no record and FAKE_PREFLIGHT_EXIT makes the fake
# instrumented binary's preflight exit with that code.
fake_build_step="$work_dir/fake-build-step"
cat >"$fake_build_step" <<FAKE
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$calls_dir"
echo "\$*" >>"$calls_dir/build-\$\$"
[[ -z "\${FAKE_BUILD_FAILS:-}" ]] || exit 1
output_dir="\$4"
for role in instrumented passthrough; do
  cat >"\$output_dir/\$role" <<BINARY
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$calls_dir"
case "\\\${1:-}" in
  preflight)
    echo "manifest" >"\\\${2:-/dev/null}"
    echo "fake-preflight \\\$*" >>"$calls_dir/preflight-\\\$\\\$"
    exit \${FAKE_PREFLIGHT_EXIT:-0}
    ;;
  pilot)
    echo "fake-pilot \\\$*" >>"$calls_dir/pilot-\\\$\\\$"
    exit \${FAKE_PILOT_EXIT:-0}
    ;;
  *)
    echo "fake binary: unknown subcommand \\\$1" >&2
    exit 2
    ;;
esac
BINARY
  chmod +x "\$output_dir/\$role"
done
[[ -n "\${FAKE_NO_RECORD:-}" ]] || echo "fake build record" >"\$output_dir/build-record.txt"
FAKE
chmod +x "$fake_build_step"

lock_file="$work_dir/probe-stack.lock"
output_root="$work_dir/pilot-output"

night() {
  # night ARGS...: the night script against the fake build step, any
  # FAKE_* variable the caller set, and a private lock.
  KILN_E4_BUILD_STEP="$fake_build_step" KILN_E4_PROBE_LOCK="$lock_file" "$night_script" "$@"
}

no_calls() {
  # no_calls KIND: true when no fake of that kind was called
  ! ls "$calls_dir"/"$1"-* >/dev/null 2>&1
}

# --- argument handling -------------------------------------------------

if night >/dev/null 2>"$work_dir/stderr-missing-args"; then
  fail "a missing commit must exit non-zero"
elif grep -q "required" "$work_dir/stderr-missing-args"; then
  pass "a missing commit exits non-zero with a clear message"
else
  fail "missing-commit message was not clear: $(cat "$work_dir/stderr-missing-args")"
fi

if night --commit abc123 --unknown-flag value >/dev/null 2>"$work_dir/stderr-unknown-flag"; then
  fail "an unknown flag must exit non-zero"
else
  pass "an unknown flag exits non-zero"
fi
if no_calls build; then
  pass "no build starts for a refused command line"
else
  fail "a refused command line still started a build"
fi

# --- the build step -----------------------------------------------------

FAKE_BUILD_FAILS=1 night --commit abc123 --output-root "$output_root" \
  >/dev/null 2>"$work_dir/stderr-build-fail" && build_fail_exit=0 || build_fail_exit=$?
if [[ "$build_fail_exit" -ne 0 ]] && grep -qi "building commit 'abc123' failed" "$work_dir/stderr-build-fail"; then
  pass "a failing build exits non-zero with a clear message"
else
  fail "build failure was not handled (exit $build_fail_exit): $(cat "$work_dir/stderr-build-fail")"
fi
if no_calls preflight && no_calls pilot && [[ ! -e "$output_root" ]]; then
  pass "neither preflight nor the pilot runs, and no evidence directory exists, after a failed build"
else
  fail "something ran after a failed build"
fi
rm -rf "$calls_dir"

FAKE_NO_RECORD=1 night --commit abc123 --output-root "$output_root" \
  >/dev/null 2>"$work_dir/stderr-no-record" && no_record_exit=0 || no_record_exit=$?
if [[ "$no_record_exit" -ne 0 ]] && grep -q "no build record" "$work_dir/stderr-no-record" \
    && no_calls preflight && no_calls pilot; then
  pass "a build step that leaves no build record starts neither preflight nor the pilot"
else
  fail "a missing build record was not refused (exit $no_record_exit): $(cat "$work_dir/stderr-no-record")"
fi
rm -rf "$calls_dir"

# --- preflight-failure path --------------------------------------------

FAKE_PREFLIGHT_EXIT=1 night --commit abc123 --output-root "$output_root" \
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

if no_calls pilot; then
  pass "the pilot never runs once preflight has failed"
else
  fail "the pilot ran after a failed preflight"
fi
rm -rf "$calls_dir" "$output_root"

# --- happy path builds first, then reaches the pilot with the right arguments

night --commit abc123 --output-root "$output_root" \
  >"$work_dir/stdout-happy" 2>"$work_dir/stderr-happy" \
  && happy_exit=0 || happy_exit=$?

if [[ "$happy_exit" -eq 0 ]]; then
  pass "a passing build, preflight and pilot together exit zero"
else
  fail "the happy path exited $happy_exit: $(cat "$work_dir/stderr-happy")"
fi

build_call="$(cat "$calls_dir"/build-* 2>/dev/null || true)"
pilot_call="$(cat "$calls_dir"/pilot-* 2>/dev/null || true)"
preflight_call="$(cat "$calls_dir"/preflight-* 2>/dev/null || true)"
build_dir="${build_call##*--output-dir }"
if [[ "$build_call" == "--commit abc123 --output-dir "* && -n "$build_dir" ]]; then
  pass "the build step is asked to build the given commit into a directory of its own"
else
  fail "the build step was not called as expected: $build_call"
fi

run_dirs=("$output_root"/*/)
if [[ ${#run_dirs[@]} -eq 1 && -d "${run_dirs[0]}" ]]; then
  pass "exactly one timestamped output directory was created"
else
  fail "expected exactly one timestamped output directory under $output_root, found: ${run_dirs[*]:-none}"
fi

run_dir="${run_dirs[0]%/}"
if [[ "$pilot_call" == *"--output-dir $run_dir"* \
    && "$pilot_call" == *"--passthrough-binary $build_dir/passthrough"* \
    && "$pilot_call" == *"--manifest $run_dir/manifest.txt"* \
    && "$pilot_call" == *"--build-record $build_dir/build-record.txt"* ]]; then
  pass "the pilot is called with its output directory, built passthrough binary, manifest and build record"
else
  fail "the pilot was not called with the expected arguments: $pilot_call"
fi

if [[ "$preflight_call" == *"$run_dir/manifest.txt $build_dir/passthrough $build_dir/build-record.txt" ]]; then
  pass "preflight is handed the manifest path, the built passthrough binary and the build record"
else
  fail "preflight was not handed the built binaries' record: $preflight_call"
fi

if [[ "$(cat "$run_dir/build-record.txt")" == "fake build record" ]]; then
  pass "the build record is kept with the run's evidence"
else
  fail "the run directory holds no copy of the build record"
fi

if [[ ! -e "$build_dir" ]]; then
  pass "the temporary build directory is removed when the script ends"
else
  fail "the build directory $build_dir was left behind"
fi

FAKE_PILOT_EXIT=1 night --commit abc123 --output-root "$work_dir/failing-pilot-output" \
  >/dev/null 2>&1 && pilot_fail_exit=0 || pilot_fail_exit=$?
if [[ "$pilot_fail_exit" -eq 1 ]]; then
  pass "the script exits with the pilot's own status"
else
  fail "a failing pilot exited $pilot_fail_exit instead of 1"
fi

# --- a run never reuses an earlier run's directory ---------------------

# A `date` stub makes every run in this block share one timestamp, the
# case a second-resolution name would collide on.
mkdir -p "$work_dir/fixed-clock"
printf '#!/usr/bin/env bash\necho 20260101T000000Z\n' >"$work_dir/fixed-clock/date"
chmod +x "$work_dir/fixed-clock/date"
same_second_root="$work_dir/same-second-output"

for _ in 1 2; do
  PATH="$work_dir/fixed-clock:$PATH" night --commit abc123 --output-root "$same_second_root" \
    >/dev/null 2>&1
  if [[ -z "${first_run_dir:-}" ]]; then
    first_run_dir="$(echo "$same_second_root"/*/)"
    echo "completed evidence" >"${first_run_dir}pilot-report.txt"
  fi
done

same_second_dirs=("$same_second_root"/*/)
if [[ ${#same_second_dirs[@]} -eq 2 ]]; then
  pass "two runs with the same timestamp get two separate directories"
else
  fail "expected two directories for two same-second runs, found: ${same_second_dirs[*]:-none}"
fi

if [[ "$(cat "${first_run_dir}pilot-report.txt")" == "completed evidence" ]]; then
  pass "an earlier run's completed evidence is left untouched"
else
  fail "the earlier run's evidence was changed"
fi

if [[ $failures -eq 0 ]]; then
  echo "all run-pilot-night.sh tests passed"
  exit 0
else
  echo "$failures run-pilot-night.sh test(s) failed"
  exit 1
fi
