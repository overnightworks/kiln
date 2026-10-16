#!/usr/bin/env bash
# Tests build-pilot-binaries.sh against a throwaway git repository and a
# stand-in `cargo` that "compiles" a shell script reporting the role and
# commit the real harness would — never a real build, never the network.
#
# Run directly: ./build-pilot-binaries-test.sh

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

failures=0
pass() { echo "PASS [KILN-E4-27]: $1"; }
fail() {
  echo "FAIL [KILN-E4-27]: $1" >&2
  failures=$((failures + 1))
}

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

# --- the throwaway repository: the script, a harness manifest, a source file

fixture="$work_dir/fixture"
experiment="$fixture/experiments/e4-sqlite-cost-anatomy"
mkdir -p "$experiment/harness"
cp "$script_dir/build-pilot-binaries.sh" "$experiment/"
echo "source of the reviewed commit" >"$experiment/harness/source.txt"
touch "$experiment/harness/Cargo.toml"
git -C "$fixture" init --quiet
git -C "$fixture" config user.email test@example.com
git -C "$fixture" config user.name test
git -C "$fixture" add -A
git -C "$fixture" commit --quiet -m "reviewed commit"
reviewed_commit="$(git -C "$fixture" rev-parse HEAD)"

# A later commit and an uncommitted edit that the reviewed commit's build must not see.
echo "source of a later commit" >"$experiment/harness/source.txt"
git -C "$fixture" commit --quiet -am "later commit"
echo "uncommitted edit" >>"$experiment/harness/source.txt"

# --- the stand-in cargo -------------------------------------------------

stub_bin="$work_dir/stub-bin"
mkdir -p "$stub_bin"
cat >"$stub_bin/cargo" <<STUB
#!/usr/bin/env bash
set -euo pipefail
echo "\$*" >>"$work_dir/cargo-calls"
[[ -n "\${STUB_CARGO_FAILS:-}" ]] && exit 101
manifest_dir=""
role="instrumented"
while [[ \$# -gt 0 ]]; do
  case "\$1" in
    --manifest-path) manifest_dir="\$(dirname "\$2")"; shift 2 ;;
    --features) [[ "\$2" == "passthrough_shim" ]] && role="passthrough"; shift 2 ;;
    *) shift ;;
  esac
done
commit="\$(git -C "\$manifest_dir" rev-parse HEAD)"
reported_role="\${STUB_REPORTED_ROLE:-\$role}"
mkdir -p "\$CARGO_TARGET_DIR/release"
cat >"\$CARGO_TARGET_DIR/release/kiln-e4-harness" <<BINARY
#!/usr/bin/env bash
# source: \$(cat "\$manifest_dir/source.txt")
echo build=\$reported_role commit=\$commit
BINARY
chmod +x "\$CARGO_TARGET_DIR/release/kiln-e4-harness"
STUB
chmod +x "$stub_bin/cargo"

build() {
  # build OUTPUT_DIR [extra environment assignments handled by the caller]
  PATH="$stub_bin:$PATH" "$fixture/experiments/e4-sqlite-cost-anatomy/build-pilot-binaries.sh" \
    --commit "$reviewed_commit" --output-dir "$1"
}

record_field() {
  # record_field RECORD KEY
  sed -n "s/^$2 = \"\\(.*\\)\"\$/\\1/p" "$1"
}

# --- a successful build -------------------------------------------------

out="$work_dir/out"
if build "$out" >/dev/null 2>"$work_dir/stderr-ok"; then
  pass "a build of the reviewed commit succeeds"
else
  fail "the build failed: $(cat "$work_dir/stderr-ok")"
fi

record="$out/build-record.txt"
if [[ "$(record_field "$record" source_commit)" == "$reviewed_commit" ]]; then
  pass "the record names the full reviewed commit, not the later one"
else
  fail "wrong commit in the record: $(cat "$record")"
fi

if [[ "$(record_field "$record" instrumented_features)" == "" \
    && "$(record_field "$record" passthrough_features)" == "passthrough_shim" ]]; then
  pass "the record names each build's cargo features"
else
  fail "wrong features in the record: $(cat "$record")"
fi

expected_instrumented="$(sha256sum "$out/instrumented" | cut -d' ' -f1)"
expected_passthrough="$(sha256sum "$out/passthrough" | cut -d' ' -f1)"
if [[ "$(record_field "$record" instrumented_sha256)" == "$expected_instrumented" \
    && "$(record_field "$record" passthrough_sha256)" == "$expected_passthrough" \
    && "$expected_instrumented" != "$expected_passthrough" ]]; then
  pass "the record holds the SHA-256 of each of the two different binaries"
else
  fail "wrong hashes in the record: $(cat "$record")"
fi

if grep -q "source of the reviewed commit" "$out/instrumented" \
    && grep -q "source of the reviewed commit" "$out/passthrough"; then
  pass "both binaries are built from the reviewed commit's files, not the working tree or a later commit"
else
  fail "a binary was built from other sources: $(cat "$out/instrumented")"
fi

if [[ "$(wc -l <"$work_dir/cargo-calls")" -eq 2 \
    && "$(grep -c -- "--release --locked" "$work_dir/cargo-calls")" -eq 2 \
    && "$(grep -c -- "--features passthrough_shim" "$work_dir/cargo-calls")" -eq 1 ]]; then
  pass "cargo builds the release configuration twice, locked, once with the passthrough feature"
else
  fail "unexpected cargo calls: $(cat "$work_dir/cargo-calls")"
fi

# --- refusals -----------------------------------------------------------

if PATH="$stub_bin:$PATH" "$fixture/experiments/e4-sqlite-cost-anatomy/build-pilot-binaries.sh" \
    --commit "$reviewed_commit" --output-dir "$out" >/dev/null 2>"$work_dir/stderr-again"; then
  fail "a second build into the same directory must be refused"
else
  pass "a second build never replaces an existing record"
fi

unknown_out="$work_dir/unknown-out"
if PATH="$stub_bin:$PATH" "$fixture/experiments/e4-sqlite-cost-anatomy/build-pilot-binaries.sh" \
    --commit does-not-exist --output-dir "$unknown_out" >/dev/null 2>"$work_dir/stderr-unknown"; then
  fail "an unknown commit must be refused"
elif [[ ! -e "$unknown_out/build-record.txt" ]]; then
  pass "an unknown commit is refused and records nothing"
else
  fail "a record was written for an unknown commit"
fi

failed_out="$work_dir/failed-out"
if STUB_CARGO_FAILS=1 build "$failed_out" >/dev/null 2>&1; then
  fail "a failing cargo build must fail the script"
elif [[ ! -e "$failed_out/build-record.txt" ]]; then
  pass "a failing build records nothing"
else
  fail "a record was written after a failed build"
fi

lying_out="$work_dir/lying-out"
if STUB_REPORTED_ROLE=passthrough build "$lying_out" >/dev/null 2>"$work_dir/stderr-lying"; then
  fail "a binary reporting the wrong role must be refused"
elif [[ ! -e "$lying_out/build-record.txt" ]] && grep -q "reports" "$work_dir/stderr-lying"; then
  pass "a binary that does not report the build it was made as is refused and records nothing"
else
  fail "the wrong-role binary was recorded or not explained: $(cat "$work_dir/stderr-lying")"
fi

if "$script_dir/build-pilot-binaries.sh" >/dev/null 2>"$work_dir/stderr-args"; then
  fail "missing arguments must be refused"
else
  pass "missing arguments are refused"
fi

if [[ $failures -eq 0 ]]; then
  echo "all build-pilot-binaries.sh tests passed"
  exit 0
else
  echo "$failures build-pilot-binaries.sh test(s) failed"
  exit 1
fi
