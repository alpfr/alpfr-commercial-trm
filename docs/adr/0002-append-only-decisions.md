# 2. Decision history is append-only

Status: accepted · 2026-11-17

## Context

Auditors ask who approved what and when. An editable row cannot answer that.

## Decision

Decisions and audit events cannot be updated or deleted. A change inserts a new row with `supersedes_id`; the `current_decision` view returns the head of each chain. Triggers and grants enforce this.

## Consequences

The audit trail needs no separate bookkeeping. Corrections cost a new row, and queries must go through the view.
