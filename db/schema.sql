-- Commercial Technology Reference Model: Phase 1 schema (PostgreSQL 15+)
-- Layers: global catalog (shared), tenant governance (row-level security).
-- Run as the schema owner. The application connects as role trm_app and
-- sets the tenant per transaction:  SET LOCAL app.tenant_id = '<uuid>';

BEGIN;

CREATE EXTENSION IF NOT EXISTS pgcrypto;   -- gen_random_uuid()
CREATE EXTENSION IF NOT EXISTS pg_trgm;    -- fuzzy catalog search

-- ---------------------------------------------------------------------------
-- Enumerations
-- ---------------------------------------------------------------------------
CREATE TYPE technology_type AS ENUM
    ('software', 'saas', 'cloud_service', 'library', 'hardware', 'standard');

CREATE TYPE decision_status AS ENUM
    ('approved', 'approved_with_constraints', 'divest', 'prohibited');
    -- "unapproved" is the absence of a decision, so it is not stored.

CREATE TYPE user_role AS ENUM
    ('viewer', 'requester', 'reviewer', 'architect', 'admin');

-- ---------------------------------------------------------------------------
-- Platform
-- ---------------------------------------------------------------------------
CREATE TABLE tenant (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL,
    slug        text NOT NULL UNIQUE,
    created_at  timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE app_user (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    uuid NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    email        text NOT NULL,
    idp_subject  text NOT NULL,              -- "sub" claim from the tenant's IdP
    role         user_role NOT NULL DEFAULT 'viewer',
    created_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, email),
    UNIQUE (tenant_id, idp_subject)
);

-- ---------------------------------------------------------------------------
-- Global catalog (shared by all tenants, curated by the operator)
-- A row with tenant_id set is a private addition awaiting promotion.
-- ---------------------------------------------------------------------------
CREATE TABLE vendor (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   uuid REFERENCES tenant(id) ON DELETE CASCADE,  -- NULL = global
    name        text NOT NULL,
    website     text,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX vendor_global_name_uq ON vendor (lower(name)) WHERE tenant_id IS NULL;
CREATE UNIQUE INDEX vendor_tenant_name_uq ON vendor (tenant_id, lower(name)) WHERE tenant_id IS NOT NULL;

CREATE TABLE technology (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      uuid REFERENCES tenant(id) ON DELETE CASCADE,  -- NULL = global
    vendor_id      uuid REFERENCES vendor(id),
    slug           text NOT NULL,            -- stable key, e.g. "postgresql"
    name           text NOT NULL,
    type           technology_type NOT NULL DEFAULT 'software',
    licence_model  text,                     -- e.g. "open source", "subscription"
    description    text,
    homepage       text,
    source         text,                     -- where the record came from
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX technology_global_slug_uq ON technology (slug) WHERE tenant_id IS NULL;
CREATE UNIQUE INDEX technology_tenant_slug_uq ON technology (tenant_id, slug) WHERE tenant_id IS NOT NULL;
CREATE INDEX technology_name_trgm ON technology USING gin (name gin_trgm_ops);

CREATE TABLE technology_version (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    technology_id   uuid NOT NULL REFERENCES technology(id) ON DELETE CASCADE,
    version         text NOT NULL,           -- release line, e.g. "16" or "3.12"
    latest_patch    text,                    -- e.g. "16.4"
    release_date    date,
    eol_date        date,                    -- NULL = no announced end of life
    is_eol          boolean,                 -- for sources that give only yes/no
    is_lts          boolean NOT NULL DEFAULT false,
    cpe             text,                    -- links to vulnerability data
    purl            text,                    -- links to SBOM components
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (technology_id, version)
);
CREATE INDEX technology_version_eol_idx ON technology_version (eol_date);
CREATE INDEX technology_version_purl_idx ON technology_version (purl);

-- ---------------------------------------------------------------------------
-- Tenant governance
-- ---------------------------------------------------------------------------
CREATE TABLE category (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  uuid NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    parent_id  uuid REFERENCES category(id) ON DELETE CASCADE,
    name       text NOT NULL,
    UNIQUE (tenant_id, parent_id, name)
);

CREATE TABLE technology_category (
    tenant_id      uuid NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    technology_id  uuid NOT NULL REFERENCES technology(id) ON DELETE CASCADE,
    category_id    uuid NOT NULL REFERENCES category(id) ON DELETE CASCADE,
    PRIMARY KEY (tenant_id, technology_id, category_id)
);

-- Append-only. A change inserts a new row that points at the one it replaces.
-- Target is either one version row or a version range on the technology.
CREATE TABLE decision (
    id                     uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id              uuid NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    technology_id          uuid NOT NULL REFERENCES technology(id),
    technology_version_id  uuid REFERENCES technology_version(id),
    version_range          text,             -- e.g. ">=16.0 <17.0"; NULL with no version = all versions
    status                 decision_status NOT NULL,
    constraints            text,             -- required when approved_with_constraints
    rationale              text,
    environment            text,             -- NULL = all; e.g. "production"
    data_classification    text,             -- NULL = all; e.g. "phi", "pci"
    valid_from             date NOT NULL DEFAULT current_date,
    valid_to               date NOT NULL,    -- every decision expires
    decided_by             uuid NOT NULL REFERENCES app_user(id),
    supersedes_id          uuid UNIQUE REFERENCES decision(id),
    created_at             timestamptz NOT NULL DEFAULT now(),
    CHECK (valid_to > valid_from),
    CHECK (NOT (technology_version_id IS NOT NULL AND version_range IS NOT NULL)),
    CHECK (status <> 'approved_with_constraints' OR constraints IS NOT NULL)
);
CREATE INDEX decision_lookup_idx ON decision (tenant_id, technology_id, valid_to);

-- A decision is current when nothing supersedes it.
CREATE VIEW current_decision WITH (security_invoker = true) AS
SELECT d.*,
       (d.valid_to < current_date) AS is_expired
FROM decision d
WHERE NOT EXISTS (SELECT 1 FROM decision n WHERE n.supersedes_id = d.id);

CREATE TABLE audit_event (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id    uuid NOT NULL REFERENCES tenant(id) ON DELETE CASCADE,
    actor_id     uuid REFERENCES app_user(id),
    action       text NOT NULL,              -- e.g. "decision.created"
    entity_type  text NOT NULL,
    entity_id    uuid,
    before       jsonb,
    after        jsonb,
    at           timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_event_tenant_at_idx ON audit_event (tenant_id, at DESC);

-- ---------------------------------------------------------------------------
-- Immutability: decisions and audit events cannot be changed or removed.
-- ---------------------------------------------------------------------------
CREATE FUNCTION forbid_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% on % is not allowed: table is append-only', TG_OP, TG_TABLE_NAME;
END $$;

CREATE TRIGGER decision_append_only
    BEFORE UPDATE OR DELETE ON decision
    FOR EACH ROW EXECUTE FUNCTION forbid_change();

CREATE TRIGGER audit_event_append_only
    BEFORE UPDATE OR DELETE ON audit_event
    FOR EACH ROW EXECUTE FUNCTION forbid_change();

-- ---------------------------------------------------------------------------
-- Row-level security
-- ---------------------------------------------------------------------------
CREATE FUNCTION current_tenant() RETURNS uuid LANGUAGE sql STABLE AS $$
    SELECT nullif(current_setting('app.tenant_id', true), '')::uuid
$$;

-- Tenant-owned tables: a session sees and writes only its own tenant's rows.
DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['app_user', 'category', 'technology_category',
                             'decision', 'audit_event']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', t);
        EXECUTE format(
            'CREATE POLICY tenant_isolation ON %I
                 USING (tenant_id = current_tenant())
                 WITH CHECK (tenant_id = current_tenant())', t);
    END LOOP;
END $$;

-- Catalog tables: everyone reads global rows; a tenant also reads and writes
-- its own private additions. Global rows are written only by the owner role.
DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['vendor', 'technology']
    LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        EXECUTE format(
            'CREATE POLICY catalog_read ON %I FOR SELECT
                 USING (tenant_id IS NULL OR tenant_id = current_tenant())', t);
        EXECUTE format(
            'CREATE POLICY catalog_insert ON %I FOR INSERT
                 WITH CHECK (tenant_id = current_tenant())', t);
        EXECUTE format(
            'CREATE POLICY catalog_update ON %I FOR UPDATE
                 USING (tenant_id = current_tenant())
                 WITH CHECK (tenant_id = current_tenant())', t);
        EXECUTE format(
            'CREATE POLICY catalog_delete ON %I FOR DELETE
                 USING (tenant_id = current_tenant())', t);
    END LOOP;
END $$;

-- ---------------------------------------------------------------------------
-- Application role (no login here; grant it to your service account)
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'trm_app') THEN
        CREATE ROLE trm_app NOLOGIN;
    END IF;
END $$;

GRANT SELECT ON tenant, technology_version, current_decision TO trm_app;
GRANT SELECT, INSERT, UPDATE, DELETE
    ON app_user, vendor, technology, category, technology_category TO trm_app;
GRANT SELECT, INSERT ON decision, audit_event TO trm_app;

COMMIT;
