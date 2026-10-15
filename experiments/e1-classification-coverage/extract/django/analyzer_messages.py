"""Builds analyzer JSON for tests from source text, so a message sits exactly
where the call is written."""

import json
from dataclasses import dataclass


@dataclass(frozen=True)
class Anchor:
    line: int
    column: int


def anchor_at(source: str, offset: int) -> Anchor:
    line_start = source.rfind("\n", 0, offset) + 1
    return Anchor(source.count("\n", 0, offset) + 1, offset - line_start)


def offset_of(source: str, needle: str, occurrence: int = 1) -> int:
    offset = -1
    for _ in range(occurrence):
        offset = source.index(needle, offset + 1)
    return offset


def anchor_of(source: str, needle: str, occurrence: int = 1) -> Anchor:
    return anchor_at(source, offset_of(source, needle, occurrence))


def _end_offset(source: str, start: int, needle: str, end_after: str | None) -> int:
    """Where the reported expression ends: after `end_after`, else after the
    needle and the balanced parentheses that follow it."""
    if end_after:
        return source.index(end_after, start) + len(end_after)
    end = start + len(needle)
    if source[end : end + 1] != "(":
        return end
    depth = 0
    for offset in range(end, len(source)):
        depth += {"(": 1, ")": -1}.get(source[offset], 0)
        if depth == 0:
            return offset + 1
    raise ValueError(f"unbalanced parentheses after {needle!r}")


def _message(file: str, source: str, needle: str, occurrence: int, end_after: str | None, content: dict) -> dict:
    start = offset_of(source, needle, occurrence)
    begin, end = anchor_at(source, start), anchor_at(source, _end_offset(source, start, needle, end_after))
    return {
        "filePath": file,
        "fromLine": begin.line,
        "toLine": end.line,
        "fromColumn": begin.column,
        "toColumn": end.column,
        "content": content,
    }


def site_message(
    file: str,
    source: str,
    needle: str,
    method: str = "get",
    method_type: str = "read",
    receiver: str = "objects",
    receiver_types: tuple[str, ...] = ("django.db.models.manager.Manager[app.Item]",),
    occurrence: int = 1,
    end_after: str | None = None,
) -> dict:
    content = {
        "name": method,
        "methodType": method_type,
        "object": receiver,
        "objectTypes": list(receiver_types),
        "attributes": [],
        "type": "method",
    }
    return _message(file, source, needle, occurrence, end_after, content)


def marker_message(
    file: str, source: str, needle: str, name: str = "with", occurrence: int = 1, end_after: str | None = None
) -> dict:
    content = {
        "name": name,
        "methodType": "transaction",
        "object": "django.db.transaction.atomic",
        "objectTypes": ["django.db.transaction.atomic"],
        "attributes": [],
        "type": "method",
    }
    return _message(file, source, needle, occurrence, end_after, content)


def model_message(file: str, source: str, needle: str) -> dict:
    return _message(file, source, needle, 1, None, {"name": "app.Item", "type": "model"})


def analyzer_json(*messages: dict) -> str:
    return json.dumps({"messages": list(messages)})
