#!/usr/bin/env bash
# Run TLC on one scenario: check.sh <Scenario> [<config>]
# Needs TLA2TOOLS=/path/to/tla2tools.jar (see specs/README.md for the pinned release).
set -euo pipefail
cd "$(dirname "$0")"
scenario=$1
config=${2:-$scenario.cfg}
log=${config%.cfg}.tlc.log
java -XX:+UseSerialGC -jar "${TLA2TOOLS:?set TLA2TOOLS to the tla2tools.jar path}" \
  -workers 4 -cleanup -config "$config" "$scenario.tla" | grep -v '^Parsing file\|^Semantic processing' | tee "$log"
