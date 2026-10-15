"""S2, S3 and the S4 evidence: from the analyzer report and the source text to
units, mechanically."""

from collections import defaultdict
from dataclasses import dataclass
from typing import Mapping

from analyzer_report import AnalyzerReport, AnalyzerReportError
from source_scopes import SourceScopes, TransactionBlock, read_scopes
from unit_records import OrmSite, SiteLocation, Span, Unit, UnitRule


@dataclass(frozen=True)
class Enumeration:
    units: list[Unit]
    receiver_evidence: list[OrmSite]
    unmatched_markers: list[SiteLocation]


def enumerate_units(
    application: str, report: AnalyzerReport, sources: Mapping[str, str]
) -> Enumeration:
    sites_by_file: dict[str, list[OrmSite]] = defaultdict(list)
    for site in report.sites:
        sites_by_file[site.location.file].append(site)
    markers_by_file: dict[str, set[Span]] = defaultdict(set)
    for marker in report.markers:
        markers_by_file[marker.file].add(marker.span)

    units: list[Unit] = []
    unmatched: list[SiteLocation] = []
    for file in sorted(sites_by_file.keys() | markers_by_file.keys()):
        scopes = _scopes_of(file, sources, frozenset(markers_by_file[file]))
        units.extend(_units_of_file(application, file, sites_by_file[file], scopes))
        unmatched.extend(SiteLocation(file, span.start) for span in scopes.unmatched_markers)
    ordered_sites = sorted(report.sites, key=lambda site: site.location)
    return Enumeration(sorted(units, key=lambda unit: unit.order_key), ordered_sites, unmatched)


def _scopes_of(
    file: str, sources: Mapping[str, str], markers: frozenset[Span]
) -> SourceScopes:
    if file not in sources:
        raise AnalyzerReportError(f"{file}: no source text for the reported file")
    return read_scopes(file, sources[file], markers)


def _units_of_file(
    application: str, file: str, sites: list[OrmSite], scopes: SourceScopes
) -> list[Unit]:
    for site in sites:
        if not scopes.contains(site.location.position):
            raise AnalyzerReportError(f"{file}: site at {site.location.position} is outside the source text")
    outermost = _outermost(scopes.transactions)
    absorbed: dict[TransactionBlock, list[SiteLocation]] = {block: [] for block in outermost}
    units: list[Unit] = []
    for site in sites:
        owner = next((b for b in outermost if b.body.contains(site.location.position)), None)
        if owner is None:
            units.append(_autocommit_unit(application, site, scopes))
        else:
            absorbed[owner].append(site.location)
    for block, locations in absorbed.items():
        units.append(_transaction_unit(application, file, block, locations))
    return units


def _outermost(blocks: tuple[TransactionBlock, ...]) -> list[TransactionBlock]:
    return [
        block
        for block in blocks
        if not any(other is not block and other.body.contains(block.position) for other in blocks)
    ]


def _transaction_unit(
    application: str, file: str, block: TransactionBlock, locations: list[SiteLocation]
) -> Unit:
    return Unit(application, SiteLocation(file, block.position), UnitRule.S2, block.parameters, tuple(sorted(locations)))


def _autocommit_unit(application: str, site: OrmSite, scopes: SourceScopes) -> Unit:
    position = site.location.position
    enclosing = scopes.function_at(position)
    parameters = ()
    if enclosing:
        parameters = enclosing.feeding(position, scopes.names_in_call(site))
    return Unit(application, site.location, UnitRule.S3, parameters, (site.location,))
