"""Reads the analyzer's `messages.json` into typed sites and transaction markers."""

import json
from dataclasses import dataclass
from typing import Any

from file_selection import is_excluded_path
from unit_records import OrmSite, SiteLocation, SourcePosition, Span

ORM_METHOD_TYPES = frozenset({"read", "write", "other"})
TRANSACTION_METHOD_TYPE = "transaction"


class AnalyzerReportError(Exception):
    """The analyzer output or its source text is not what the rule can read."""


@dataclass(frozen=True)
class TransactionMarker:
    """An expression the analyzer resolved to `django.db.transaction.atomic`."""

    file: str
    span: Span


@dataclass(frozen=True)
class AnalyzerReport:
    sites: tuple[OrmSite, ...]
    markers: tuple[TransactionMarker, ...]


def parse_analyzer_json(text: str, application_root: str) -> AnalyzerReport:
    """`application_root` is the root spelling the analyzer was run with."""
    sites: list[OrmSite] = []
    markers: list[TransactionMarker] = []
    for message in json.loads(text)["messages"]:
        file = _relative_file(message["filePath"], application_root)
        if file is None or is_excluded_path(file):
            continue
        start = SourcePosition(message["fromLine"], message["fromColumn"])
        end = SourcePosition(message["toLine"], message["toColumn"])
        location = SiteLocation(file, start)
        content = message["content"]
        if content["type"] == "method":
            _add_method_message(content, location, end, sites, markers)
        elif content["type"] != "model":
            raise AnalyzerReportError(f"unknown message type {content['type']!r} in {file}")
    return AnalyzerReport(tuple(sites), tuple(markers))


def _add_method_message(
    content: dict[str, Any],
    location: SiteLocation,
    end: SourcePosition,
    sites: list[OrmSite],
    markers: list[TransactionMarker],
) -> None:
    method_type = content["methodType"]
    if method_type == TRANSACTION_METHOD_TYPE:
        markers.append(TransactionMarker(location.file, Span(location.position, end)))
    elif method_type in ORM_METHOD_TYPES:
        receiver_types = tuple(content["objectTypes"])
        sites.append(OrmSite(location, end, content["name"], content["object"], receiver_types))
    else:
        raise AnalyzerReportError(
            f"unknown method type {method_type!r} in {location.file}"
        )


def _relative_file(file_path: str, application_root: str) -> str | None:
    """The path below the application root; None for a followed module outside it."""
    prefix = application_root.rstrip("/") + "/"
    return file_path[len(prefix) :] if file_path.startswith(prefix) else None
