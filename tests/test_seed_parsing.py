"""Parsing tests: no database or network needed."""
from datetime import date

import seed_catalog as sc


def load(fixture_path, slugs=()):
    return sc.load_products(list(slugs), fixture_path)


def test_loads_all_products(fixture_path):
    assert [p["name"] for p in load(fixture_path)] == ["postgresql", "amazon-eks", "no-releases"]


def test_slug_filter(fixture_path):
    assert [p["name"] for p in load(fixture_path, ["amazon-eks"])] == ["amazon-eks"]


def test_technology_row_maps_category_and_links(fixture_path):
    pg, eks, _ = load(fixture_path)
    row = sc.technology_row(pg)
    assert row["slug"] == "postgresql"
    assert row["name"] == "PostgreSQL"
    assert row["type"] == "software"
    assert row["homepage"] == "https://www.postgresql.org/support/versioning/"
    assert sc.technology_row(eks)["type"] == "cloud_service"


def test_version_rows_parse_dates_and_skip_duplicates(fixture_path):
    pg = load(fixture_path)[0]
    rows = list(sc.version_rows(pg))
    assert [r["version"] for r in rows] == ["17", "12"]
    v17 = rows[0]
    assert v17["eol_date"] == date(2029, 11, 8)
    assert v17["release_date"] == date(2024, 9, 26)
    assert v17["latest_patch"] == "17.5"
    assert v17["is_eol"] is False
    assert v17["cpe"] == "cpe:2.3:a:postgresql:postgresql"
    assert v17["purl"] == "pkg:generic/postgresql"


def test_missing_fields_become_null(fixture_path):
    eks = load(fixture_path)[1]
    (row,) = list(sc.version_rows(eks))
    assert row["eol_date"] is None
    assert row["latest_patch"] is None
    assert row["cpe"] is None


def test_parse_date_tolerates_non_dates():
    assert sc.parse_date(None) is None
    assert sc.parse_date(True) is None
    assert sc.parse_date("not-a-date") is None
    assert sc.parse_date("2026-01-31") == date(2026, 1, 31)
