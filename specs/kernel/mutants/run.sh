#!/usr/bin/env bash
# Check one mutant against one property: run.sh <Mutant> <Scenario> <Property|deadlock>
# Applies mutants/<Mutant>.patch to a temporary copy of the modules and runs TLC
# with only that property (none for the deadlock check), as an INVARIANT or, when
# the scenario's cfg lists it so, a PROPERTY under the cfg's own SPECIFICATION.
# One worker with a fixed fingerprint function makes the counterexample reproducible.
# Expected outcome: a violation; the log lands in mutants/<Mutant>.<Scenario>.<Property>.tlc.log.
set -euo pipefail
cd "$(dirname "$0")"
mutant=$1; scenario=$2; property=$3
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
specification=$(sed -n 's/^SPECIFICATION //p' "../$scenario.cfg")
kind=INVARIANT
grep -qx "PROPERTY $property" "../$scenario.cfg" && kind=PROPERTY
cp ../*.tla "$work"/
patch -s -d "$work" -p0 < "$mutant.patch"
{
  printf 'CONSTANTS\n  Prog <- %sProg\n  InitDb <- %sInitDb\n\nSPECIFICATION %s\n' "$scenario" "$scenario" "$specification"
  [ "$property" = deadlock ] || printf '\n%s %s\n' "$kind" "$property"
} > "$work/$scenario.cfg"
java -XX:+UseSerialGC -jar "${TLA2TOOLS:?set TLA2TOOLS to the tla2tools.jar path}" \
  -workers 1 -fp 0 -cleanup -config "$work/$scenario.cfg" "$work/$scenario.tla" \
  | grep -v '^Parsing file\|^Semantic processing' | sed "s#$work/##g" \
  | tee "$mutant.$scenario.$property.tlc.log"
