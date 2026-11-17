# 4. Tenancy by PostgreSQL row-level security

Status: accepted · 2026-11-17

## Context

A database per tenant is costly to run at the start; filtering by tenant in application code is easy to get wrong.

## Decision

One database. Tenant tables carry `tenant_id` with forced row-level security keyed on the `app.tenant_id` setting, and the application uses the `trm_app` role.

## Consequences

Isolation holds even if a query forgets its filter. Every new tenant table needs a policy and an isolation test. A dedicated-database tier remains possible later.
