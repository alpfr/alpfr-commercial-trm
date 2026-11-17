"""Policy core for the Commercial TRM (Python implementation).

Mirrors the Rust crate ``crates/trm-policy`` rule for rule. Both are held to
the shared cases in ``tests/vectors/policy.json``; change the rules in both
places and add a vector. See ``docs/VERSION_RANGES.md``.

    from trm_policy import Decision, Query, Range, Status, evaluate

The package does no I/O and has no dependencies outside the standard library.
"""
from .decision import Decision, Outcome, Query, Status, Verdict, evaluate
from .range import Range, RangeError
from .version import Version, VersionError

__all__ = [
    "Decision", "Outcome", "Query", "Status", "Verdict", "evaluate",
    "Range", "RangeError", "Version", "VersionError",
]
