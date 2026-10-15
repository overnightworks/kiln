"""The records the extractor produces; `crates/kiln-e1/src/unit.rs` reads them."""

from dataclasses import dataclass
from enum import Enum

UNRESOLVED_RECEIVER_TYPE = "Any"


class UnitRule(Enum):
    S2 = "S2"
    S3 = "S3"


@dataclass(frozen=True, order=True)
class SourcePosition:
    line: int
    column: int


@dataclass(frozen=True, order=True)
class SiteLocation:
    file: str
    position: SourcePosition


@dataclass(frozen=True)
class OrmSite:
    """One analyzer report of an ORM call, with the receiver evidence for S4."""

    location: SiteLocation
    method: str
    receiver: str
    receiver_types: tuple[str, ...]

    @property
    def receiver_resolved(self) -> bool:
        return bool(self.receiver_types) and (
            self.receiver_types[0] != UNRESOLVED_RECEIVER_TYPE
        )


@dataclass(frozen=True)
class Unit:
    application: str
    location: SiteLocation
    rule: UnitRule
    parameters: tuple[str, ...]
    absorbed_sites: tuple[SiteLocation, ...]

    @property
    def order_key(self) -> tuple[SiteLocation, str]:
        return (self.location, self.rule.value)
