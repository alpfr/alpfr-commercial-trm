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
6. **One set of policy rules, two implementations.** Range matching and
   decision precedence exist in Rust (`crates/trm-policy`) and in Python
   (`trm_policy/`). Both must pass the shared cases in
   `tests/vectors/policy.json`, so the decision API, the findings engine and
   the CI plugin cannot disagree whichever language they use. See
   `VERSION_RANGES.md`.

Each of these has a record in `docs/adr/`.

## How a query is answered

1. The caller sends technology, version, environment and data classification.
2. The service loads the tenant's rows from `current_decision` for that
   technology.
3. `evaluate` in the policy core drops decisions that do not apply, then picks one by
   the order of precedence.
4. The verdict (allow, allow with constraints, warn, deny, expired,
   unapproved) and the governing decision id go back to the caller.
5. For expired and unapproved, tenant policy turns the verdict into warn or
   deny.

## Components in this repository

| Component | Status | Language |
| --- | --- | --- |
| Schema (`db/schema.sql`) | Done for Phase 1 | SQL |
| Policy core (`crates/trm-policy`) | Done; not yet wired to the database | Rust |
| Policy core (`trm_policy/`) | Done; not yet wired to the database | Python |
| Shared policy cases (`tests/vectors/policy.json`) | Done; run by both test suites | JSON |
| Catalog seed (`tools/seed-catalog`) | Done | Rust |
| Catalog seed (`scripts/seed_catalog.py`) | Done | Python |
| Catalog API, decision API, web app | Not started | Either language; both can link the policy core |

## Not yet in the schema

Planned for later phases: `request` and `review` (workflow), `framework`,
`control` and `control_mapping` (compliance), `asset`, `observation` and
`finding` (discovery), `contract` (procurement), and `decision_scope` as a
separate table (Phase 1 keeps environment and data classification as columns
on `decision`).
