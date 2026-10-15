"""Shared arrangement for the unit tests: one source file, its analyzer
messages, and the units the extractor makes of them."""

from analyzer_messages import analyzer_json, marker_message, site_message
from analyzer_report import parse_analyzer_json
from enumeration import Enumeration, enumerate_units
from unit_records import Unit

APPLICATION = "acme/shop"
ROOT = "shop"
FILE = "app/views.py"


def units_of(source: str, sites: list[dict], markers: list[dict] = ()) -> list[Unit]:
    return enumeration_of(source, sites, markers).units


def enumeration_of(
    source: str, sites: list[dict], markers: list[dict] = ()
) -> Enumeration:
    report = parse_analyzer_json(analyzer_json(*sites, *markers), ROOT)
    return enumerate_units(APPLICATION, report, {FILE: source})


def site(source: str, needle: str, occurrence: int = 1, **fields) -> dict:
    fields.setdefault("method", needle.rsplit(".", 1)[-1])
    return site_message(f"{ROOT}/{FILE}", source, needle, occurrence=occurrence, **fields)


def marker(source: str, needle: str, name: str = "with") -> dict:
    return marker_message(f"{ROOT}/{FILE}", source, needle, name)


def shape(unit: Unit) -> tuple:
    """A unit as (rule, line, parameters, absorbed site lines)."""
    return (
        unit.rule.value,
        unit.location.position.line,
        unit.parameters,
        tuple(absorbed.position.line for absorbed in unit.absorbed_sites),
    )
