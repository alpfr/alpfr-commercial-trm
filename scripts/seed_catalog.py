#!/usr/bin/env python3
"""Seed the global technology catalog from endoflife.date (API v1).

Loads products and their release lines into `technology` and
`technology_version` as global rows (tenant_id IS NULL). Safe to re-run:
existing rows are updated in place.

Usage:
    export DATABASE_URL=postgresql://owner:secret@localhost:5432/trm
    python seed_catalog.py                      # every product
    python seed_catalog.py postgresql nodejs    # only these slugs
    python seed_catalog.py --from-file full.json  # offline, from a saved response
    python seed_catalog.py --dry-run            # parse and count, write nothing

Connect as the schema owner, not as trm_app: global rows are owner-only.
Requires: pip install "psycopg[binary]" requests
Data licence: endoflife.date content is community-maintained; keep the
`source` attribution when you display it.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import date
from typing import Any, Iterable

API = "https://endoflife.date/api/v1"
USER_AGENT = "trm-catalog-seed/0.1"

# endoflife.date category -> technology_type enum in schema.sql
CATEGORY_TYPE = {
    "device": "hardware",
    "service": "cloud_service",
    "standard": "standard",
}


def parse_date(value: Any) -> date | None:
    """Return a date for an ISO string, else None (the API also sends null/booleans)."""
    if isinstance(value, str):
        try:
            return date.fromisoformat(value[:10])
        except ValueError:
            return None
    return None


def first_identifier(product: dict, kind: str) -> str | None:
    for ident in product.get("identifiers") or []:
        if isinstance(ident, dict) and ident.get("type") == kind and ident.get("id"):
            return ident["id"]
    return None


def fetch_json(url: str, retries: int = 3) -> Any:
    import requests

    for attempt in range(1, retries + 1):
        resp = requests.get(url, headers={"User-Agent": USER_AGENT}, timeout=60)
        if resp.status_code == 200:
            return resp.json()
        if resp.status_code in (429, 500, 502, 503, 504) and attempt < retries:
            time.sleep(2 * attempt)
            continue
        resp.raise_for_status()
    raise RuntimeError(f"gave up on {url}")


def load_products(slugs: list[str], from_file: str | None) -> list[dict]:
    """Return product dicts, each with a `releases` list."""
    if from_file:
        with open(from_file, encoding="utf-8") as fh:
            payload = json.load(fh)
        result = payload.get("result", payload) if isinstance(payload, dict) else payload
        products = result if isinstance(result, list) else [result]
    elif slugs:
        products = [fetch_json(f"{API}/products/{slug}")["result"] for slug in slugs]
    else:
        products = fetch_json(f"{API}/products/full")["result"]

    if slugs:
        wanted = set(slugs)
        products = [p for p in products if p.get("name") in wanted]
        missing = wanted - {p.get("name") for p in products}
        if missing:
            raise SystemExit(f"not found: {', '.join(sorted(missing))}")
    return products


def technology_row(product: dict) -> dict:
    links = product.get("links") or {}
    return {
        "slug": product["name"],
        "name": product.get("label") or product["name"],
        "type": CATEGORY_TYPE.get(product.get("category"), "software"),
        "homepage": links.get("releasePolicy") or links.get("html"),
        "source": links.get("html") or f"https://endoflife.date/{product['name']}",
    }


def version_rows(product: dict) -> Iterable[dict]:
    cpe = first_identifier(product, "cpe")
    purl = first_identifier(product, "purl")
    seen: set[str] = set()
    for rel in product.get("releases") or []:
        name = str(rel.get("name") or "").strip()
        if not name or name in seen:
            continue
        seen.add(name)
        latest = rel.get("latest") or {}
        is_eol = rel.get("isEol")
        yield {
            "version": name,
            "latest_patch": latest.get("name") if isinstance(latest, dict) else None,
            "release_date": parse_date(rel.get("releaseDate")),
            "eol_date": parse_date(rel.get("eolFrom")),
            "is_eol": is_eol if isinstance(is_eol, bool) else None,
            "is_lts": bool(rel.get("isLts")),
            "cpe": cpe,
            "purl": purl,
        }


UPSERT_TECHNOLOGY = """
INSERT INTO technology (tenant_id, slug, name, type, homepage, source)
VALUES (NULL, %(slug)s, %(name)s, %(type)s::technology_type, %(homepage)s, %(source)s)
ON CONFLICT (slug) WHERE tenant_id IS NULL
DO UPDATE SET name = EXCLUDED.name,
              type = EXCLUDED.type,
              homepage = EXCLUDED.homepage,
              source = EXCLUDED.source,
              updated_at = now()
RETURNING id
"""

UPSERT_VERSION = """
INSERT INTO technology_version
    (technology_id, version, latest_patch, release_date, eol_date, is_eol, is_lts, cpe, purl)
VALUES
    (%(technology_id)s, %(version)s, %(latest_patch)s, %(release_date)s,
     %(eol_date)s, %(is_eol)s, %(is_lts)s, %(cpe)s, %(purl)s)
ON CONFLICT (technology_id, version)
DO UPDATE SET latest_patch = EXCLUDED.latest_patch,
              release_date = EXCLUDED.release_date,
              eol_date = EXCLUDED.eol_date,
              is_eol = EXCLUDED.is_eol,
              is_lts = EXCLUDED.is_lts,
              cpe = COALESCE(EXCLUDED.cpe, technology_version.cpe),
              purl = COALESCE(EXCLUDED.purl, technology_version.purl),
              updated_at = now()
"""


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("slugs", nargs="*", help="product slugs to load (default: all)")
    ap.add_argument("--from-file", help="read a saved /products/full or /products/<slug> response")
    ap.add_argument("--dry-run", action="store_true", help="parse and count only")
    args = ap.parse_args()

    products = load_products(args.slugs, args.from_file)
    parsed = [(technology_row(p), list(version_rows(p))) for p in products if p.get("name")]
    n_versions = sum(len(v) for _, v in parsed)

    if args.dry_run:
        print(f"dry run: {len(parsed)} technologies, {n_versions} versions")
        return 0

    dsn = os.environ.get("DATABASE_URL")
    if not dsn:
        print("DATABASE_URL is not set", file=sys.stderr)
        return 2

    import psycopg

    with psycopg.connect(dsn) as conn:  # one transaction: all or nothing
        with conn.cursor() as cur:
            for tech, versions in parsed:
                cur.execute(UPSERT_TECHNOLOGY, tech)
                tech_id = cur.fetchone()[0]
                if versions:
                    cur.executemany(
                        UPSERT_VERSION,
                        [{**v, "technology_id": tech_id} for v in versions],
                    )
    print(f"loaded {len(parsed)} technologies, {n_versions} versions")
    return 0


if __name__ == "__main__":
    sys.exit(main())
