"""The range grammar stored in ``decision.version_range``.

* comparators ``>=``, ``>``, ``<=``, ``<``, ``=`` (or ``==``), ``!=``
* a bare version or wildcard is a prefix: ``16``, ``16.*`` and ``16.x`` all
  match every ``16.y.z``; ``*`` matches every version
* whitespace or a comma means AND; ``||`` means OR

A pre-release matches only when the same AND-group names a pre-release of that
exact release: ``>=16 <18`` does not admit ``17.0-rc1``.
"""
from __future__ import annotations

from .version import Version, VersionError

_OPERATORS = (">=", "<=", "==", "!=", ">", "<", "=")
_CANONICAL = {"==": "="}


class RangeError(ValueError):
    def __init__(self, why: str):
        super().__init__(f"invalid version range: {why}")


def _split_operator(token: str) -> tuple[str | None, str]:
    for symbol in _OPERATORS:
        if token.startswith(symbol):
            return _CANONICAL.get(symbol, symbol), token[len(symbol):]
    return None, token


# A comparator is ("any",), ("prefix", segments) or ("cmp", op, Version).
def _parse_comparator(token: str) -> tuple:
    def fail(why: str) -> RangeError:
        return RangeError(f'{why} in "{token}"')

    if token[:1] in ("^", "~"):
        raise fail("^ and ~ are not supported; write the bounds out, e.g. >=1.2 <2")
    op, rest = _split_operator(token)
    if not rest:
        raise fail("missing version")
    prefix_ok = op in (None, "=")

    if rest in ("*", "x", "X"):
        if not prefix_ok:
            raise fail("wildcard cannot follow this operator")
        return ("any",)
    if rest.endswith((".*", ".x", ".X")):
        if not prefix_ok:
            raise fail("wildcard cannot follow this operator")
        try:
            version = Version.parse(rest[:-2])
        except VersionError:
            raise fail("bad version") from None
        if version.is_pre_release:
            raise fail("wildcard cannot follow a pre-release")
        return ("prefix", version.segments)

    try:
        version = Version.parse(rest)
    except VersionError:
        raise fail("bad version") from None
    if op is None:
        # A bare release number is a prefix: "16" means every 16.y.z.
        return ("cmp", "=", version) if version.is_pre_release else ("prefix", version.segments)
    return ("cmp", op, version)


def _comparator_matches(comparator: tuple, version: Version) -> bool:
    kind = comparator[0]
    if kind == "any":
        return True
    if kind == "prefix":
        return version.starts_with(comparator[1])
    _, op, bound = comparator
    return {
        ">=": version >= bound,
        ">": version > bound,
        "<=": version <= bound,
        "<": version < bound,
        "=": version == bound,
        "!=": version != bound,
    }[op]


def _admits_pre_release(group: list[tuple], version: Version) -> bool:
    return any(
        c[0] == "cmp" and c[2].is_pre_release and c[2].same_release(version) for c in group
    )


class Range:
    __slots__ = ("_alternatives", "_source")

    def __init__(self, alternatives: list[list[tuple]], source: str):
        self._alternatives = alternatives
        self._source = source

    @classmethod
    def parse(cls, text: str) -> "Range":
        source = text.strip()
        if not source:
            raise RangeError("empty range")
        alternatives = []
        for part in source.split("||"):
            comparators = []
            pending_op = None
            for token in part.replace(",", " ").split():
                # Allow a space after the operator: ">= 16".
                if pending_op is not None:
                    token, pending_op = pending_op + token, None
                if token in _OPERATORS:
                    pending_op = token
                    continue
                comparators.append(_parse_comparator(token))
            if pending_op is not None:
                raise RangeError(f'operator without a version in "{part}"')
            if not comparators:
                raise RangeError(f'empty alternative in "{source}"')
            alternatives.append(comparators)
        return cls(alternatives, source)

    def matches(self, version: Version | str) -> bool:
        """True when the range admits ``version``. A string that is not a
        version never matches."""
        if isinstance(version, str):
            try:
                version = Version.parse(version)
            except VersionError:
                return False
        return any(
            all(_comparator_matches(c, version) for c in group)
            and (not version.is_pre_release or _admits_pre_release(group, version))
            for group in self._alternatives
        )

    def __str__(self) -> str:
        return self._source

    def __repr__(self) -> str:
        return f"Range({self._source!r})"

    def __eq__(self, other):
        return isinstance(other, Range) and self._source == other._source

    def __hash__(self):
        return hash(self._source)
