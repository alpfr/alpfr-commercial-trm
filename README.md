# Commercial TRM

A Technology Reference Model for companies: one governed catalog of the
technologies an organisation may use, with a dated decision on every version.
Modelled on government TRMs, built for commercial compliance (SOC 2, ISO 27001,
HIPAA, PCI DSS) and multi-tenant from the first table.

**Status:** Phase 1 foundation. The repository holds the database schema, the
catalog seed tool, and the policy core that decides whether a version is
allowed. There is no API or web app yet.

## What is here

| Path | Purpose |
| --- | --- |
| `db/schema.sql` | PostgreSQL schema: shared catalog, per-tenant decisions, audit log, row-level security |
| `crates/trm-policy/` | Policy core in Rust: version parsing, range matching, decision evaluation. Includes the `trm-check` command |
| `trm_policy/` | The same policy core in Python, standard library only. Run it with `python -m trm_policy` |
| `tests/vectors/policy.json` | Shared conformance cases that both policy implementations must pass |
| `tools/seed-catalog/` | Rust binary that loads products and release lines from endoflife.date into the shared catalog |
| `scripts/seed_catalog.py` | The same seed tool in Python |
| `tests/` | Python tests: policy core, seed parsing, tenant isolation, append-only rules, idempotent seed |
| `docker-compose.yml` | Local PostgreSQL 16 with the schema loaded on first start |
| `.github/workflows/ci.yml` | Runs the Python tests against PostgreSQL 16, and the Rust linter and tests |
| `docs/` | Design summary, version-range rules, architecture decision records |

## Quick start

Requires Docker, `make`, Python 3.10+ and a Rust toolchain (`cargo`).

```bash
cp .env.example .env          # then change the password
make install                  # Python dependencies
make db-up                    # start PostgreSQL and load the schema
export $(grep -v '^#' .env | xargs)
make rust-build               # build seed-catalog and trm-check
make rust-seed-dry            # fetch and parse, write nothing
make rust-seed                # load the shared catalog
```

Run the tests:

```bash
make rust-test rust-lint
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
- **Decisions attach to versions.** A `decision` targets one release line, a
  version range, or every version of a technology. It carries a status
  (`approved`, `approved_with_constraints`, `divest`, `prohibited`) and always
  has an expiry date. No decision means unapproved.
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

## Policy core: is this version allowed?

The policy core answers the question every enforcement point asks. It exists
in Rust (`crates/trm-policy`) and in Python (`trm_policy/`) with the same rules,
so a service in either language can link it directly. Neither does any I/O: the
caller loads a tenant's current decisions for one technology and passes them in
with the version and scope.

Rust:

```rust
use trm_policy::{evaluate, Query, Verdict};

let outcome = evaluate(&decisions, &Query {
    version: "16.4",
    environment: Some("production"),
    data_classification: None,
    on: today,
});
match outcome.verdict {
    Verdict::Allow | Verdict::AllowWithConstraints => { /* proceed */ }
    Verdict::Warn => { /* divest: allow existing use, flag it */ }
    Verdict::Deny => { /* block */ }
    Verdict::Expired | Verdict::Unapproved => { /* tenant policy: warn or deny */ }
}
```

Python:

```python
from datetime import date
from trm_policy import Decision, Query, Range, Status, Verdict, evaluate

decisions = [
    Decision("d1", Status.APPROVED, date(2026, 1, 1), date(2026, 12, 31),
             range=Range.parse(">=16 <18")),
    Decision("d2", Status.PROHIBITED, date(2026, 1, 1), date(2026, 12, 31),
             line="12"),
]
outcome = evaluate(decisions, Query("16.4", on=date.today(), environment="production"))
if outcome.verdict is Verdict.DENY:
    ...  # block; outcome.decision.id says which decision governed
```

**Keeping the two in step.** `tests/vectors/policy.json` holds the shared cases
for version parsing, ordering, ranges and decision precedence. `make test` runs
them through Python and `make rust-test` through Rust. When a rule changes,
change both implementations and add a case to that file.

Range grammar in one line: comparators (`>=16 <18`), prefixes (`16`, `16.*`),
`||` for alternatives, and pre-releases only when a bound names one. The full
rules and the order of precedence between decisions are in
[`docs/VERSION_RANGES.md`](docs/VERSION_RANGES.md).

Try a range from the shell:

```bash
make check-range RANGE='>=16 <18' VERSIONS='16.4 18.0 17.0-rc1'
# 16.4      match
# 18.0      no match
# 17.0-rc1  no match
```

The Python command prints the same lines: `python -m trm_policy '>=16 <18' 16.4`.
Both exit 0 when every version matches, 1 when any does not, and 2 when the
range is invalid, so they can be used in scripts.

## Seeding the catalog

Both tools take the same arguments and write the same rows.

```bash
target/release/seed-catalog                       # every product
target/release/seed-catalog postgresql nodejs     # selected products
target/release/seed-catalog --from-file full.json # from a saved API response
target/release/seed-catalog --dry-run             # parse and count only
python scripts/seed_catalog.py --dry-run          # Python equivalent
```

They use the endoflife.date v1 API, which that project describes as beta, so a
breaking change is possible. The parsing tests run against a fixture shaped like
the v1 response; run a dry run after cloning to confirm the live API still
parses. Connect as the schema owner, because global rows are owner-only.
Re-running updates rows in place. Check the endoflife.date licence terms before
redistributing its data in a commercial product.

Database TLS in the Rust tool: it uses TLS when the server offers it and
verifies the server certificate against public roots. For a server with a
private CA, such as Amazon RDS, pass `--ca-file <bundle.pem>`. For a local
database you can add `?sslmode=disable` to `DATABASE_URL`.

## Roadmap

1. **Catalog** (in progress): schema, seed and policy core are done. Next: read
   API, search UI, SSO.
2. **Workflow:** requests, parallel reviews, expiry reminders.
3. **Discovery and enforcement:** SBOM upload, cluster agent, findings, decision
   API built on the policy core, CI plugin.
4. **Commercial readiness:** tenant onboarding, configurable frameworks, billing.

Known gaps: no migration tool yet (the schema is one file). The seed tool and
the policy core are each maintained in two languages by choice, so every change
to either has to be made twice.

## Licence

No licence has been chosen yet, so all rights are reserved by default. Decide
before making the repository public or accepting contributions.
