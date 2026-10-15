"""JSONL rendering: one line per record, byte-stable."""

import json
from typing import Any, Iterable

from unit_records import OrmSite, SiteLocation, Unit


def render_units(units: Iterable[Unit]) -> str:
    return _lines(_unit_json(unit) for unit in units)


def render_receiver_evidence(sites: Iterable[OrmSite]) -> str:
    return _lines(_evidence_json(site) for site in sites)


def _lines(records: Iterable[dict[str, Any]]) -> str:
    return "".join(json.dumps(record, ensure_ascii=False) + "\n" for record in records)


def _location_json(location: SiteLocation) -> dict[str, Any]:
    return {"file": location.file, "line": location.position.line, "column": location.position.column}


def _unit_json(unit: Unit) -> dict[str, Any]:
    return {
        "subject": {"application": unit.application},
        **_location_json(unit.location),
        "rule": unit.rule.value,
        "parameters": list(unit.parameters),
        "absorbed_sites": [_location_json(site) for site in unit.absorbed_sites],
    }


def _evidence_json(site: OrmSite) -> dict[str, Any]:
    return {
        **_location_json(site.location),
        "method": site.method,
        "receiver": site.receiver,
        "receiver_types": list(site.receiver_types),
        "receiver_resolved": site.receiver_resolved,
    }
