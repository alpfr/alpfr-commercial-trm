"""Database tests: need TEST_DATABASE_URL pointing at a database with the schema loaded."""
import os
import subprocess
import sys
import uuid

import psycopg
import pytest

from conftest import ROOT


def run_seed(dsn, fixture_path):
    env = {**os.environ, "DATABASE_URL": dsn}
    return subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "seed_catalog.py"), "--from-file", fixture_path],
        env=env, capture_output=True, text=True, check=True,
    )


def counts(conn):
    tech = conn.execute("SELECT count(*) FROM technology WHERE tenant_id IS NULL").fetchone()[0]
    vers = conn.execute("SELECT count(*) FROM technology_version").fetchone()[0]
    return tech, vers


def test_seed_is_idempotent(dsn, fixture_path):
    run_seed(dsn, fixture_path)
    with psycopg.connect(dsn) as conn:
        first = counts(conn)
    run_seed(dsn, fixture_path)
    with psycopg.connect(dsn) as conn:
        assert counts(conn) == first
    assert first[0] >= 3 and first[1] >= 3


@pytest.fixture()
def two_tenants(dsn, fixture_path):
    """Two tenants, a user in tenant A, and one decision owned by tenant A."""
    run_seed(dsn, fixture_path)
    a, b, user = uuid.uuid4(), uuid.uuid4(), uuid.uuid4()
    with psycopg.connect(dsn) as conn:
        conn.execute("INSERT INTO tenant (id, name, slug) VALUES (%s, 'A', %s), (%s, 'B', %s)",
                     (a, f"a-{a}", b, f"b-{b}"))
        conn.execute("GRANT trm_app TO CURRENT_USER")
    with as_tenant(dsn, a) as conn:
        conn.execute(
            "INSERT INTO app_user (id, tenant_id, email, idp_subject, role) "
            "VALUES (%s, %s, 'arch@example.com', %s, 'architect')", (user, a, str(user)))
        conn.execute(
            "INSERT INTO decision (tenant_id, technology_id, version_range, status, valid_to, decided_by) "
            "SELECT %s, id, '>=16 <18', 'approved', current_date + 365, %s "
            "FROM technology WHERE slug = 'postgresql' AND tenant_id IS NULL", (a, user))
    return a, b


def as_tenant(dsn, tenant_id):
    """Connection acting as the application role, scoped to one tenant."""
    conn = psycopg.connect(dsn)
    conn.execute("SET ROLE trm_app")
    conn.execute("SELECT set_config('app.tenant_id', %s, false)", (str(tenant_id),))
    return conn


def test_tenant_sees_own_decisions_only(dsn, two_tenants):
    a, b = two_tenants
    with as_tenant(dsn, a) as conn:
        assert conn.execute("SELECT count(*) FROM current_decision").fetchone()[0] == 1
    with as_tenant(dsn, b) as conn:
        assert conn.execute("SELECT count(*) FROM current_decision").fetchone()[0] == 0
        assert conn.execute("SELECT count(*) FROM app_user").fetchone()[0] == 0


def test_every_tenant_reads_global_catalog(dsn, two_tenants):
    _, b = two_tenants
    with as_tenant(dsn, b) as conn:
        assert conn.execute("SELECT count(*) FROM technology").fetchone()[0] >= 3


def test_cannot_write_into_another_tenant(dsn, two_tenants):
    a, b = two_tenants
    with pytest.raises(psycopg.errors.InsufficientPrivilege):
        with as_tenant(dsn, b) as conn:
            conn.execute("INSERT INTO category (tenant_id, name) VALUES (%s, 'x')", (a,))


def test_tenant_cannot_write_global_catalog(dsn, two_tenants):
    _, b = two_tenants
    with pytest.raises(psycopg.errors.InsufficientPrivilege):
        with as_tenant(dsn, b) as conn:
            conn.execute("INSERT INTO technology (tenant_id, slug, name) VALUES (NULL, 'evil', 'x')")


def test_decisions_are_append_only_for_the_app(dsn, two_tenants):
    a, _ = two_tenants
    with pytest.raises(psycopg.errors.InsufficientPrivilege):
        with as_tenant(dsn, a) as conn:
            conn.execute("UPDATE decision SET status = 'prohibited'")


def test_decisions_are_append_only_for_the_owner(dsn, two_tenants):
    with pytest.raises(psycopg.errors.RaiseException):
        with psycopg.connect(dsn) as conn:
            conn.execute("UPDATE decision SET status = 'prohibited'")
