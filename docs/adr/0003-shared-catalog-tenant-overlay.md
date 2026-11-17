# 3. Shared catalog with a tenant overlay

Status: accepted · 2026-11-17

## Context

Every customer would otherwise retype the same vendors, products and end-of-life dates.

## Decision

Catalog rows with `tenant_id IS NULL` are global and owner-written. Tenants read them, add private rows, and own their decisions and categories.

## Consequences

New tenants start with a filled catalog. The operator carries a curation duty, and the seed data source's licence terms apply to what is redistributed.
