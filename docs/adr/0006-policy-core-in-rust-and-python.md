# 6. Policy core in Rust and Python, held together by shared cases

Status: accepted · 2026-11-17 · amends ADR 5

## Context

ADR 5 put the policy rules in one Rust crate so that components could not
disagree. The owner wants both a Python and a Rust version of the tooling, so
that services and scripts in either language can evaluate decisions without a
network call. Two hand-written implementations of the same rules will drift
unless something forces them to agree.

## Decision

Keep the Rust crate `trm-policy` and add a Python package `trm_policy` with the
same rules and no dependencies outside the standard library. The seed tool is
likewise kept in both languages.

`tests/vectors/policy.json` is the single statement of expected behaviour:
version parsing, ordering, range matching, invalid ranges, and decision
precedence. The Python suite (`tests/test_policy.py`) and the Rust suite
(`crates/trm-policy/tests/conformance.rs`) both run every case, and CI runs
both. A rule change is complete only when both implementations pass and a case
covering it has been added.

`^` and `~` stay unsupported in both.

## Consequences

Either language can host the catalog API, the decision API or the CI plugin.
Every rule change costs two edits and a new case. Behaviour not covered by a
shared case can still diverge, so new edge cases belong in the vectors file,
not only in one language's unit tests. If one implementation is later retired,
the vectors file remains as the specification for the other.
