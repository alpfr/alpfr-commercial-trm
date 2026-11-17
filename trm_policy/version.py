"""Lenient parsing and ordering of real-world version strings."""
from __future__ import annotations

import re
from functools import total_ordering

_DIGITS = "0123456789"
_U64_MAX = 2**64 - 1

# Suffixes that mean "not yet the release". Anything else after the numbers
# ("-alpine", " LTSC", "u412") is a variant label and is ignored.
_PRE_MARKERS = (
    "alpha", "beta", "rc", "preview", "pre", "dev", "snapshot", "milestone", "nightly", "canary", "ea",
)
_PRE_TOKEN = re.compile(r"[0-9]+|[A-Za-z]+")


class VersionError(ValueError):
    def __init__(self, text: str):
        super().__init__(f"invalid version: {text}")


def ascii_lower(text: str) -> str:
    return "".join(c.lower() if c.isascii() else c for c in text)


def _is_pre_release(suffix: str) -> bool:
    # The marker must stand alone or be followed by a number or separator, so
    # "rc1" and "beta.2" qualify but "precise" and "eap" do not.
    for marker in _PRE_MARKERS:
        if suffix.startswith(marker):
            rest = suffix[len(marker):]
            if not rest or rest[0] in _DIGITS or rest[0] in ".-_":
                return True
    # PEP 440 and Maven shorthands: a1, b2, m3
    return len(suffix) >= 2 and suffix[0] in "abm" and suffix[1] in _DIGITS


def _pre_key(token: str):
    """Numbers sort before words; numbers compare as numbers."""
    if token[0] in _DIGITS and int(token) <= _U64_MAX:
        return (0, int(token), "")
    return (1, 0, token)


@total_ordering
class Version:
    """A version as it appears in the wild: ``16``, ``v1.33.2``, ``3.12.0rc1``,
    ``16.4-alpine``, ``21.0.2+13``.

    Missing trailing segments count as zero, so ``16 == 16.0 == 16.0.0``.
    A pre-release sorts before the release with the same numbers.
    """

    __slots__ = ("segments", "pre")

    def __init__(self, segments: tuple[int, ...], pre: str | None = None):
        self.segments = tuple(segments)
        self.pre = pre

    @classmethod
    def parse(cls, text: str) -> "Version":
        s = text.strip()
        if len(s) > 1 and s[0] in "vV" and s[1] in _DIGITS:
            s = s[1:]
        s = s.split("+", 1)[0]  # build metadata never affects ordering

        core_end = next((i for i, c in enumerate(s) if c not in _DIGITS and c != "."), len(s))
        core = s[:core_end].rstrip(".")
        if not core:
            raise VersionError(text)
        segments = []
        for part in core.split("."):
            if not part or int(part) > _U64_MAX:
                raise VersionError(text)
            segments.append(int(part))

        suffix = ascii_lower(s[core_end:].lstrip("-_. ").strip())
        pre = suffix if suffix and _is_pre_release(suffix) else None
        return cls(tuple(segments), pre)

    def segment(self, index: int) -> int:
        """Numeric segment at ``index``, zero when absent."""
        return self.segments[index] if index < len(self.segments) else 0

    @property
    def is_pre_release(self) -> bool:
        return self.pre is not None

    def same_release(self, other: "Version") -> bool:
        """True when both have the same numbers, ignoring any pre-release."""
        return self._numbers(other) == other._numbers(self)

    def starts_with(self, prefix: tuple[int, ...]) -> bool:
        """True when this version starts with ``prefix`` (16.4 starts with (16,))."""
        return all(self.segment(i) == want for i, want in enumerate(prefix))

    def _numbers(self, other: "Version") -> tuple[int, ...]:
        width = max(len(self.segments), len(other.segments))
        return tuple(self.segment(i) for i in range(width))

    def _pre_key(self):
        if self.pre is None:
            return (1, ())
        return (0, tuple(_pre_key(t) for t in _PRE_TOKEN.findall(self.pre)))

    def _key(self, other: "Version"):
        return (self._numbers(other), self._pre_key())

    def __eq__(self, other):
        if not isinstance(other, Version):
            return NotImplemented
        return self._key(other) == other._key(self)

    def __lt__(self, other):
        if not isinstance(other, Version):
            return NotImplemented
        return self._key(other) < other._key(self)

    __hash__ = None  # 16 == 16.0, so there is no stable hash

    def __str__(self) -> str:
        text = ".".join(str(n) for n in self.segments)
        return f"{text}-{self.pre}" if self.pre else text

    def __repr__(self) -> str:
        return f"Version({str(self)!r})"
