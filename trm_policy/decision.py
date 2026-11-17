"""Pick the decision that governs a version in a scope, and its verdict."""
from __future__ import annotations

import enum
from dataclasses import dataclass
from datetime import date

from .range import Range
from .version import Version, VersionError, ascii_lower


class Status(enum.IntEnum):
    """Mirrors the ``decision_status`` enum in db/schema.sql, ordered from
    least to most restrictive."""

    APPROVED = 0
    APPROVED_WITH_CONSTRAINTS = 1
    DIVEST = 2
    PROHIBITED = 3

    @classmethod
    def from_db(cls, text: str) -> "Status":
        """Parse the database enum text, e.g. ``approved_with_constraints``."""
        try:
            return _STATUS_BY_DB_TEXT[text]
        except KeyError:
            raise ValueError(f"unknown decision status: {text}") from None


_STATUS_BY_DB_TEXT = {s.name.lower(): s for s in Status}


class Verdict(str, enum.Enum):
    ALLOW = "allow"                                    # approved
    ALLOW_WITH_CONSTRAINTS = "allow_with_constraints"  # surface the constraints
    WARN = "warn"                                      # divest
    DENY = "deny"                                      # prohibited
    EXPIRED = "expired"                                # only expired decisions apply
    UNAPPROVED = "unapproved"                          # nothing applies


_VERDICT = {
    Status.APPROVED: Verdict.ALLOW,
    Status.APPROVED_WITH_CONSTRAINTS: Verdict.ALLOW_WITH_CONSTRAINTS,
    Status.DIVEST: Verdict.WARN,
    Status.PROHIBITED: Verdict.DENY,
}


@dataclass(frozen=True)
class Decision:
    """One current (not superseded) decision for a technology.

    Target: set ``line`` for a release line from the catalog (``"16"``), or
    ``range`` for a range expression, or neither for every version.
    """

    id: str
    status: Status
    valid_from: date
    valid_to: date
    line: str | None = None
    range: Range | None = None
    constraints: str | None = None
    environment: str | None = None          # None applies to every environment
    data_classification: str | None = None  # None applies to every classification

    def __post_init__(self):
        if self.line is not None and self.range is not None:
            raise ValueError("a decision targets a line or a range, not both")

    def _target_specificity(self) -> int:
        return 2 if self.line is not None else 1 if self.range is not None else 0

    def _scope_specificity(self) -> int:
        return (self.environment is not None) + (self.data_classification is not None)

    def _target_matches(self, raw: str, parsed: Version | None) -> bool:
        if self.range is not None:
            return parsed is not None and self.range.matches(parsed)
        if self.line is None:
            return True
        try:
            line_version = Version.parse(self.line)
        except VersionError:
            line_version = None
        if (
            line_version is not None and parsed is not None
            and not parsed.is_pre_release and not line_version.is_pre_release
        ):
            return parsed.starts_with(line_version.segments)
        # Named lines ("subscription") and pre-releases match by exact text only.
        return ascii_lower(self.line.strip()) == ascii_lower(raw.strip())

    def _applies_to(self, query: "Query", parsed: Version | None) -> bool:
        return (
            self.valid_from <= query.on
            and _scope_applies(self.environment, query.environment)
            and _scope_applies(self.data_classification, query.data_classification)
            and self._target_matches(query.version, parsed)
        )


def _scope_applies(decision: str | None, query: str | None) -> bool:
    if decision is None:
        return True
    # A scoped decision applies only when the caller states that scope.
    return query is not None and ascii_lower(decision.strip()) == ascii_lower(query.strip())


@dataclass(frozen=True)
class Query:
    """What an enforcement point is asking about. ``on`` is normally today."""

    version: str
    on: date
    environment: str | None = None
    data_classification: str | None = None


@dataclass(frozen=True)
class Outcome:
    verdict: Verdict
    decision: Decision | None  # the decision the verdict came from, if any


def evaluate(decisions: list[Decision], query: Query) -> Outcome:
    """Pick the governing decision among ``decisions`` (one technology, one
    tenant, current rows only) and return the verdict.

    Order of precedence among decisions that apply:
    1. decisions in force beat expired ones
    2. more specific target: release line, then range, then all versions
    3. more specific scope: more of environment and data classification set
    4. more restrictive status
    5. most recent ``valid_from``
    """
    try:
        parsed = Version.parse(query.version)
    except VersionError:
        parsed = None

    applicable = [d for d in decisions if d._applies_to(query, parsed)]
    if not applicable:
        return Outcome(Verdict.UNAPPROVED, None)
    best = min(
        applicable,
        key=lambda d: (
            d.valid_to < query.on,
            -d._target_specificity(),
            -d._scope_specificity(),
            -d.status,
            -d.valid_from.toordinal(),
        ),
    )
    if best.valid_to < query.on:
        return Outcome(Verdict.EXPIRED, best)
    return Outcome(_VERDICT[best.status], best)
