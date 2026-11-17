# Commercial TRM

A Technology Reference Model for companies: one governed catalog of the
technologies an organisation may use, with a dated decision on every version.
Modelled on government TRMs, built for commercial compliance (SOC 2, ISO 27001,
HIPAA, PCI DSS) and multi-tenant from the first table.

**Status:** Phase 1 foundation. This repository holds the database schema, the
catalog seed script, and tests. There is no API or web app yet.

## What is here

| Path | Purpose |
| --- | --- |
| `db/schema.sql` | PostgreSQL schema: shared catalog, per-tenant decisions, audit log, row-level security |
| `scripts/seed_catalog.py` | Loads products and release lines from endoflife.date into the shared catalog |
| `tests/` | Parsing tests (no database) and database tests (tenant isolation, append-only rules, idempotent seed) |
| `docker-compose.yml` | Local PostgreSQL 16 with the schema loaded on first start |
| `.github/workflows/ci.yml` | Runs the tests against PostgreSQL 16 on every push and pull request |
| `docs/DESIGN.md` | Design summary and the key decisions behind the schema |

## Quick start

Requires Docker, Python 3.10+ and `make`.

```bash
cp .env.example .env          # then change the password
make install                  # Python dependencies
make db-up                    # start PostgreSQL and load the schema
export $(grep -v '^#' .env | xargs)
make seed-dry                 # fetch and parse, write nothing
make seed                     # load the shared catalog
```

Run the tests:

```bash
export TEST_DATABASE_URL=$DATABASE_URL
make test
```

Without `TEST_DATABASE_URL` the database tests are skipped and only the parsing
tests run. The database tests create tenants with random ids and leave them in
place, so point them at a development database, not a real one.

## How the data model works

- **Global catalog.** `vendor`, `technology` and `technology_version` rows with
  `tenant_id IS NULL` are shared by every tenant and written only by the schema
  owner. A tenant can add private entries of its own.
- **Decisions attach to versions.** A `decision` targets one version or a
  version range, carries a status (`approved`, `approved_with_constraints`,
  `divest`, `prohibited`) and always has an expiry date. No decision means
  unapproved.
- **Append-only history.** Decisions and audit events cannot be updated or
  deleted. A change inserts a new decision that points at the one it replaces;
  the `current_decision` view returns the latest of each chain.
- **Tenant isolation.** Tenant tables use row-level security. The application
  connects as the `trm_app` role and sets the tenant for each transaction:

  ```sql
  BEGIN;
  SET LOCAL app.tenant_id = '<tenant uuid>';
  -- queries here see only that tenant's rows
  COMMIT;
  ```

  With no tenant set, tenant tables return no rows.

## Seeding the catalog

```bash
python scripts/seed_catalog.py                       # every product
python scripts/seed_catalog.py postgresql nodejs     # selected products
python scripts/seed_catalog.py --from-file full.json # from a saved API response
python scripts/seed_catalog.py --dry-run             # parse and count only
```

The script uses the endoflife.date v1 API, which that project describes as beta,
so a breaking change is possible. The parsing tests run against a fixture shaped
like the v1 response; run `make seed-dry` after cloning to confirm the live API
still parses. Connect as the schema owner, because global rows are owner-only.
Re-running updates rows in place.

## Roadmap

1. **Catalog** (this phase): schema, seed, then a read API and search UI with SSO.
2. **Workflow:** requests, parallel reviews, expiry reminders.
3. **Discovery and enforcement:** SBOM upload, cluster agent, findings, decision API, CI plugin.
4. **Commercial readiness:** tenant onboarding, configurable frameworks, billing.

## Licence

No licence has been chosen yet, so all rights are reserved by default. Decide
before making the repository public or accepting contributions.
