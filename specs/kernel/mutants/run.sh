#!/usr/bin/env bash
# Check one mutant against one property: run.sh <Mutant> <Scenario> <Property|deadlock>
# Applies mutants/<Mutant>.patch to a temporary copy of the modules and runs TLC
# with only that property (none for the deadlock check). Expected outcome: a
# violation; the log lands in mutants/<Mutant>.<Scenario>.<Property>.tlc.log.
set -euo pipefail
cd "$(dirname "$0")"
mutant=$1; scenario=$2; property=$3
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp ../*.tla "$work"/
patch -s -d "$work" -p0 < "$mutant.patch"
{
  printf 'CONSTANTS\n  Prog <- %sProg\n  InitDb <- %sInitDb\n\nSPECIFICATION Spec\n' "$scenario" "$scenario"
  [ "$property" = deadlock ] || printf '\nINVARIANT %s\n' "$property"
} > "$work/$scenario.cfg"
java -XX:+UseSerialGC -jar "${TLA2TOOLS:?set TLA2TOOLS to the tla2tools.jar path}" \
  -workers 4 -cleanup -config "$work/$scenario.cfg" "$work/$scenario.tla" \
  | grep -v '^Parsing file\|^Semantic processing' | sed "s#$work/##g" \
  | tee "$mutant.$scenario.$property.tlc.log"
