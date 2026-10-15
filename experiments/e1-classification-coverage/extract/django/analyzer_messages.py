"""Builds analyzer JSON for tests from source text, so a message sits exactly
where the call is written."""

import json
from dataclasses import dataclass


@dataclass(frozen=True)
class Anchor:
    line: int
    column: int


def anchor_of(source: str, needle: str, occurrence: int = 1) -> Anchor:
    position = -1
    for _ in range(occurrence):
        position = source.index(needle, position + 1)
    line_start = source.rfind("\n", 0, position) + 1
    return Anchor(source.count("\n", 0, position) + 1, position - line_start)


def _message(file: str, anchor: Anchor, content: dict) -> dict:
    return {
        "filePath": file,
        "fromLine": anchor.line,
        "toLine": anchor.line,
        "fromColumn": anchor.column,
        "toColumn": anchor.column + 1,
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
) -> dict:
    content = {
        "name": method,
        "methodType": method_type,
        "object": receiver,
        "objectTypes": list(receiver_types),
        "attributes": [],
        "type": "method",
    }
    return _message(file, anchor_of(source, needle, occurrence), content)


def marker_message(file: str, source: str, needle: str, name: str = "with") -> dict:
    content = {
        "name": name,
        "methodType": "transaction",
        "object": "django.db.transaction.atomic",
        "objectTypes": ["django.db.transaction.atomic"],
        "attributes": [],
        "type": "method",
    }
    return _message(file, anchor_of(source, needle), content)


def model_message(file: str, source: str, needle: str) -> dict:
    return _message(
        file, anchor_of(source, needle), {"name": "app.Item", "type": "model"}
    )


def analyzer_json(*messages: dict) -> str:
    return json.dumps({"messages": list(messages)})
