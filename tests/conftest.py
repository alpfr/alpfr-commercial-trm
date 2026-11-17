import os
import pathlib
import sys

import pytest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT))  # the trm_policy package

FIXTURE = ROOT / "tests" / "fixtures" / "products_full.json"
SCHEMA = ROOT / "db" / "schema.sql"


@pytest.fixture(scope="session")
def fixture_path():
    return str(FIXTURE)


@pytest.fixture(scope="session")
def dsn():
    """Owner connection string for a database with db/schema.sql loaded.

    Database tests are skipped when TEST_DATABASE_URL is not set.
    """
    value = os.environ.get("TEST_DATABASE_URL")
    if not value:
        pytest.skip("TEST_DATABASE_URL not set")
    return value
