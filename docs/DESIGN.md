# Design summary

The full design document (personas, data model, decision lifecycle,
architecture, integrations, security, delivery plan) is kept here:
https://claude.ai/artifact/1L6nQSPRMGtHhfxwMJhvoi

That link is private to its owner until shared. Export it to Markdown and
replace this file if collaborators need the full text in the repository.

## Decisions that shape the code

1. **Decisions attach to versions, not products.** "PostgreSQL 16 approved,
   12 divest" is the unit of governance. Ranges are supported so a patch
   release does not need a new decision.
2. **Every decision expires.** `valid_to` is mandatory; expiry triggers
   re-review.
3. **Decision history is append-only.** The audit trail is the data model,
   enforced by triggers and by grants.
4. **Shared catalog, tenant overlay.** Catalog records are curated once.
   Tenants own only their decisions, categories and private additions.
5. **Row-level security for tenancy.** One database, isolation enforced in
   PostgreSQL, with a dedicated-database tier possible later.

## Not yet in the schema

Planned for later phases: `request` and `review` (workflow), `framework`,
`control` and `control_mapping` (compliance), `asset`, `observation` and
`finding` (discovery), `contract` (procurement), and `decision_scope` as a
separate table (Phase 1 keeps environment and data classification as columns
on `decision`).
