# 5. One policy core, written in Rust

Status: amended by ADR 6 · 2026-11-17

## Context

The decision API, the findings engine and the CI plugin all need to answer whether a version is allowed. Separate implementations would drift. The decision API is also on the blocking path of builds and deployments.

## Decision

Version parsing, the range grammar and decision precedence live in the `trm-policy` crate, which does no I/O. `^` and `~` are not supported because their meaning differs between ecosystems. The APIs will be written in Rust and call the crate directly.

## Consequences

One set of rules with one test suite. Components in other languages must call the decision API instead of reimplementing the rules. The Python seed script is kept for now and should be retired.
