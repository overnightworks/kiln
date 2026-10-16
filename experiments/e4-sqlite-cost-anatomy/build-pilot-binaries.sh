#!/usr/bin/env bash
# Builds the two binaries the E4 pilot compares (protocol.md §5, §12) from
# a clean checkout of the reviewed commit, and writes the build record
# that preflight and the pilot's admission hold the measured binaries
# against: the commit, the cargo features of each build and the SHA-256
# of each binary. Nothing but this script writes the record, so a binary
# that merely claims a role and a commit is not admitted.
#
# Usage:
#   build-pilot-binaries.sh --commit REV --output-dir DIR
#
# DIR receives `instrumented` (the default build), `passthrough` (the
# `passthrough_shim` build) and `build-record.txt`. The source is a fresh
# clone of this repository checked out at REV: the working tree, its
# uncommitted changes and its target directory take no part in the build.

set -euo pipefail

usage() {
  echo "usage: $0 --commit REV --output-dir DIR" >&2
}

fail() {
  echo "build-pilot-binaries.sh: $1" >&2
  exit 1
}

commit_rev=""
output_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --commit)
      commit_rev="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    *)
      echo "build-pilot-binaries.sh: unknown argument '$1'" >&2
      usage
      exit 2
      ;;
  esac
done
if [[ -z "$commit_rev" || -z "$output_dir" ]]; then
  echo "build-pilot-binaries.sh: --commit and --output-dir are required" >&2
  usage
  exit 2
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository="$(git -C "$script_dir" rev-parse --show-toplevel)"
commit="$(git -C "$repository" rev-parse --verify --quiet "$commit_rev^{commit}")" \
  || fail "'$commit_rev' is not a commit of $repository"

if [[ -e "$output_dir/build-record.txt" ]]; then
  fail "$output_dir already holds a build record; a build never replaces one"
fi
mkdir -p "$output_dir"

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
source_dir="$work_dir/source"
git clone --quiet --no-checkout "$repository" "$source_dir"
git -C "$source_dir" checkout --quiet --detach "$commit"
harness_manifest="$source_dir/experiments/e4-sqlite-cost-anatomy/harness/Cargo.toml"

# build_binary ROLE [FEATURE]: builds the release binary into its own
# target directory and copies it to $output_dir/ROLE.
build_binary() {
  local role="$1" features="${2:-}"
  local feature_arguments=()
  if [[ -n "$features" ]]; then
    feature_arguments=(--features "$features")
  fi
  CARGO_TARGET_DIR="$work_dir/target-$role" \
    cargo build --release --locked --manifest-path "$harness_manifest" ${feature_arguments[@]+"${feature_arguments[@]}"}
  cp "$work_dir/target-$role/release/kiln-e4-harness" "$output_dir/$role"
}

sha256_of() {
  sha256sum "$1" | cut -d' ' -f1
}

# The features the harness crate reads them back as: `Build::required_features`.
instrumented_features=""
passthrough_features="passthrough_shim"
build_binary instrumented "$instrumented_features"
build_binary passthrough "$passthrough_features"

# A binary that does not report the role and commit it was just built as
# was not built from what this script thinks it built.
for role in instrumented passthrough; do
  reported="$("$output_dir/$role" build-role)"
  if [[ "$reported" != "build=$role commit=$commit" ]]; then
    fail "the $role binary reports '$reported', not 'build=$role commit=$commit'"
  fi
done

record="$output_dir/build-record.txt"
{
  echo "source_commit = \"$commit\""
  echo "instrumented_features = \"$instrumented_features\""
  echo "passthrough_features = \"$passthrough_features\""
  echo "instrumented_sha256 = \"$(sha256_of "$output_dir/instrumented")\""
  echo "passthrough_sha256 = \"$(sha256_of "$output_dir/passthrough")\""
} >"$record"
echo "build-pilot-binaries.sh: built commit $commit; record in $record"
