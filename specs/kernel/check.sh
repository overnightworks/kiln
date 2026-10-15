#!/usr/bin/env bash
# Run TLC on one scenario: check.sh <Scenario> [<config>]
# Needs TLA2TOOLS=/path/to/tla2tools.jar (see specs/README.md for the pinned release).
set -euo pipefail
cd "$(dirname "$0")"
scenario=$1
config=${2:-$scenario.cfg}
log=${config%.cfg}.tlc.log
# Exhaustive checks use 4 workers; a witness counterexample is one worker with a fixed
# fingerprint function, so the trace is reproducible.
workers="-workers 4"
[ "$config" = "$scenario.cfg" ] || workers="-workers 1 -fp 0"
java -XX:+UseSerialGC -jar "${TLA2TOOLS:?set TLA2TOOLS to the tla2tools.jar path}" \
  $workers -cleanup -config "$config" "$scenario.tla" | grep -v '^Parsing file\|^Semantic processing' | tee "$log"
