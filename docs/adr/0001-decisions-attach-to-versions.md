# 1. Decisions attach to versions and always expire

Status: accepted · 2026-11-17

## Context

A product-level approval says nothing about an unsupported release of that product, and approvals that never lapse go stale.

## Decision

A decision targets a release line, a version range, or all versions of a technology, and `valid_to` is mandatory.

## Consequences

Governance matches how risk actually changes. Every decision needs re-review, so the workflow must send reminders, and ranges need an evaluator (see ADR 5).
