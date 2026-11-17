//! Seed the global technology catalog from endoflife.date (API v1).
//!
//! Loads products and their release lines into `technology` and
//! `technology_version` as global rows (tenant_id IS NULL). Safe to re-run:
//! existing rows are updated in place. Behaviour matches
//! `scripts/seed_catalog.py`.
//!
//! Connect as the schema owner, not as trm_app: global rows are owner-only.

use std::collections::HashSet;
use std::fs;
use std::process::ExitCode;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDate;
use clap::Parser;
use serde_json::Value;

const API: &str = "https://endoflife.date/api/v1";
const USER_AGENT: &str = "trm-catalog-seed/0.1";

#[derive(Parser, Debug)]
#[command(version, about = "Seed the global technology catalog from endoflife.date (API v1)")]
struct Args {
    /// Product slugs to load (default: all)
    slugs: Vec<String>,

    /// Read a saved /products/full or /products/<slug> response instead of calling the API
    #[arg(long, value_name = "PATH")]
    from_file: Option<String>,

    /// Parse and count only; write nothing
    #[arg(long)]
    dry_run: bool,

    /// Extra CA certificates (PEM) to trust for the database connection,
    /// e.g. the Amazon RDS global bundle
    #[arg(long, value_name = "PATH")]
    ca_file: Option<String>,
}

#[derive(Debug, PartialEq)]
struct Technology {
    slug: String,
    name: String,
    kind: &'static str,
    homepage: Option<String>,
    source: String,
}

#[derive(Debug, PartialEq)]
struct Version {
    version: String,
    latest_patch: Option<String>,
    release_date: Option<NaiveDate>,
    eol_date: Option<NaiveDate>,
    is_eol: Option<bool>,
    is_lts: bool,
    cpe: Option<String>,
    purl: Option<String>,
}

/// endoflife.date category -> technology_type enum in db/schema.sql
fn technology_type(category: Option<&str>) -> &'static str {
    match category {
        Some("device") => "hardware",
        Some("service") => "cloud_service",
        Some("standard") => "standard",
        _ => "software",
    }
}

/// A date for an ISO string, else None (the API also sends null and booleans).
fn parse_date(value: &Value) -> Option<NaiveDate> {
    let text = value.as_str()?;
    NaiveDate::parse_from_str(text.get(..10)?, "%Y-%m-%d").ok()
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

fn first_identifier(product: &Value, kind: &str) -> Option<String> {
    product["identifiers"]
        .as_array()?
        .iter()
        .find(|ident| ident["type"].as_str() == Some(kind))
        .and_then(|ident| text(&ident["id"]))
}

fn technology_row(product: &Value) -> Option<Technology> {
    let slug = text(&product["name"])?;
    let links = &product["links"];
    Some(Technology {
        name: text(&product["label"]).unwrap_or_else(|| slug.clone()),
        kind: technology_type(product["category"].as_str()),
        homepage: text(&links["releasePolicy"]).or_else(|| text(&links["html"])),
        source: text(&links["html"]).unwrap_or_else(|| format!("https://endoflife.date/{slug}")),
        slug,
    })
}

fn version_rows(product: &Value) -> Vec<Version> {
    let cpe = first_identifier(product, "cpe");
    let purl = first_identifier(product, "purl");
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for rel in product["releases"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
        // Release names are normally strings; tolerate bare numbers.
        let name = match &rel["name"] {
            Value::String(s) => s.trim().to_owned(),
            Value::Number(n) => n.to_string(),
            _ => continue,
        };
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        rows.push(Version {
            version: name,
            latest_patch: text(&rel["latest"]["name"]),
            release_date: parse_date(&rel["releaseDate"]),
            eol_date: parse_date(&rel["eolFrom"]),
            is_eol: rel["isEol"].as_bool(),
            is_lts: rel["isLts"].as_bool().unwrap_or(false),
            cpe: cpe.clone(),
            purl: purl.clone(),
        });
    }
    rows
}

fn fetch_json(url: &str) -> Result<Value> {
    const RETRIES: u64 = 3;
    for attempt in 1..=RETRIES {
        match ureq::get(url)
            .set("User-Agent", USER_AGENT)
            .timeout(Duration::from_secs(60))
            .call()
        {
            Ok(resp) => return resp.into_json().with_context(|| format!("invalid JSON from {url}")),
            Err(ureq::Error::Status(code, _))
                if matches!(code, 429 | 500 | 502 | 503 | 504) && attempt < RETRIES =>
            {
                sleep(Duration::from_secs(2 * attempt));
            }
            Err(ureq::Error::Status(code, _)) => bail!("{url} returned HTTP {code}"),
            Err(err) if attempt < RETRIES => {
                eprintln!("retrying {url}: {err}");
                sleep(Duration::from_secs(2 * attempt));
            }
            Err(err) => return Err(err).with_context(|| format!("request to {url} failed")),
        }
    }
    bail!("gave up on {url}")
}

/// The `result` member of an API response, or the value itself when a saved
/// file holds a bare product or list.
fn unwrap_result(payload: Value) -> Value {
    match payload {
        Value::Object(mut map) if map.contains_key("result") => map.remove("result").unwrap(),
        other => other,
    }
}

/// Product objects, each with a `releases` array.
fn load_products(slugs: &[String], from_file: Option<&str>) -> Result<Vec<Value>> {
    let mut products: Vec<Value> = if let Some(path) = from_file {
        let raw = fs::read_to_string(path).with_context(|| format!("cannot read {path}"))?;
        let payload = serde_json::from_str(&raw).with_context(|| format!("invalid JSON in {path}"))?;
        match unwrap_result(payload) {
            Value::Array(items) => items,
            single => vec![single],
        }
    } else if !slugs.is_empty() {
        slugs
            .iter()
            .map(|slug| fetch_json(&format!("{API}/products/{slug}")).map(unwrap_result))
            .collect::<Result<_>>()?
    } else {
        match unwrap_result(fetch_json(&format!("{API}/products/full"))?) {
            Value::Array(items) => items,
            _ => bail!("unexpected response shape from {API}/products/full"),
        }
    };

    if !slugs.is_empty() {
        products.retain(|p| p["name"].as_str().is_some_and(|n| slugs.iter().any(|s| s == n)));
        let mut missing: Vec<&str> = slugs
            .iter()
            .map(String::as_str)
            .filter(|s| !products.iter().any(|p| p["name"].as_str() == Some(s)))
            .collect();
        if !missing.is_empty() {
            missing.sort_unstable();
            bail!("not found: {}", missing.join(", "));
        }
    }
    Ok(products)
}

/// Connect with TLS when the server offers it (sslmode=prefer, the default),
/// or without when the URL says sslmode=disable.
fn connect(dsn: &str, ca_file: Option<&str>) -> Result<postgres::Client> {
    let config: postgres::Config = dsn.parse().context("DATABASE_URL is not a valid connection string")?;
    if config.get_ssl_mode() == postgres::config::SslMode::Disable {
        return config.connect(postgres::NoTls).context("database connection failed");
    }

    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(path) = ca_file {
        let pem = fs::read(path).with_context(|| format!("cannot read {path}"))?;
        let mut added = 0;
        for cert in rustls_pemfile::certs(&mut pem.as_slice()) {
            roots.add(cert.with_context(|| format!("invalid certificate in {path}"))?)?;
            added += 1;
        }
        if added == 0 {
            bail!("no certificates found in {path}");
        }
    }
    let tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config
        .connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls))
        .context("database connection failed (for a private CA pass --ca-file; for no TLS add ?sslmode=disable)")
}

const UPSERT_TECHNOLOGY: &str = "
INSERT INTO technology (tenant_id, slug, name, type, homepage, source)
VALUES (NULL, $1, $2, $3::text::technology_type, $4, $5)
ON CONFLICT (slug) WHERE tenant_id IS NULL
DO UPDATE SET name = EXCLUDED.name,
              type = EXCLUDED.type,
              homepage = EXCLUDED.homepage,
              source = EXCLUDED.source,
              updated_at = now()
RETURNING id::text";

const UPSERT_VERSION: &str = "
INSERT INTO technology_version
    (technology_id, version, latest_patch, release_date, eol_date, is_eol, is_lts, cpe, purl)
VALUES ($1::text::uuid, $2, $3, $4, $5, $6, $7, $8, $9)
ON CONFLICT (technology_id, version)
DO UPDATE SET latest_patch = EXCLUDED.latest_patch,
              release_date = EXCLUDED.release_date,
              eol_date = EXCLUDED.eol_date,
              is_eol = EXCLUDED.is_eol,
              is_lts = EXCLUDED.is_lts,
              cpe = COALESCE(EXCLUDED.cpe, technology_version.cpe),
              purl = COALESCE(EXCLUDED.purl, technology_version.purl),
              updated_at = now()";

/// Write everything in one transaction: all or nothing.
fn write_catalog(client: &mut postgres::Client, parsed: &[(Technology, Vec<Version>)]) -> Result<()> {
    let mut tx = client.transaction()?;
    let upsert_tech = tx.prepare(UPSERT_TECHNOLOGY)?;
    let upsert_version = tx.prepare(UPSERT_VERSION)?;
    for (tech, versions) in parsed {
        let row = tx
            .query_one(
                &upsert_tech,
                &[&tech.slug, &tech.name, &tech.kind, &tech.homepage, &tech.source],
            )
            .with_context(|| format!("failed to upsert technology {}", tech.slug))?;
        let tech_id: String = row.get(0);
        for v in versions {
            tx.execute(
                &upsert_version,
                &[
                    &tech_id,
                    &v.version,
                    &v.latest_patch,
                    &v.release_date,
                    &v.eol_date,
                    &v.is_eol,
                    &v.is_lts,
                    &v.cpe,
                    &v.purl,
                ],
            )
            .with_context(|| format!("failed to upsert {} {}", tech.slug, v.version))?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn run(args: &Args) -> Result<()> {
    let products = load_products(&args.slugs, args.from_file.as_deref())?;
    let parsed: Vec<(Technology, Vec<Version>)> = products
        .iter()
        .filter_map(|p| Some((technology_row(p)?, version_rows(p))))
        .collect();
    let n_versions: usize = parsed.iter().map(|(_, v)| v.len()).sum();

    if args.dry_run {
        println!("dry run: {} technologies, {} versions", parsed.len(), n_versions);
        return Ok(());
    }

    let dsn = std::env::var("DATABASE_URL").map_err(|_| anyhow!("DATABASE_URL is not set"))?;
    let mut client = connect(&dsn, args.ca_file.as_deref())?;
    write_catalog(&mut client, &parsed)?;
    println!("loaded {} technologies, {} versions", parsed.len(), n_versions);
    Ok(())
}

fn main() -> ExitCode {
    match run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/products_full.json");

    fn load(slugs: &[&str]) -> Vec<Value> {
        let slugs: Vec<String> = slugs.iter().map(|s| s.to_string()).collect();
        load_products(&slugs, Some(FIXTURE)).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(y, m, d)
    }

    #[test]
    fn loads_all_products() {
        let names: Vec<_> = load(&[]).iter().map(|p| p["name"].as_str().unwrap().to_owned()).collect();
        assert_eq!(names, ["postgresql", "amazon-eks", "no-releases"]);
    }

    #[test]
    fn slug_filter_and_missing_slug() {
        assert_eq!(load(&["amazon-eks"]).len(), 1);
        let err = load_products(&["nope".to_string()], Some(FIXTURE)).unwrap_err();
        assert_eq!(err.to_string(), "not found: nope");
    }

    #[test]
    fn technology_row_maps_category_and_links() {
        let products = load(&[]);
        let pg = technology_row(&products[0]).unwrap();
        assert_eq!(pg.slug, "postgresql");
        assert_eq!(pg.name, "PostgreSQL");
        assert_eq!(pg.kind, "software");
        assert_eq!(pg.homepage.as_deref(), Some("https://www.postgresql.org/support/versioning/"));
        assert_eq!(technology_row(&products[1]).unwrap().kind, "cloud_service");
    }

    #[test]
    fn version_rows_parse_dates_and_skip_duplicates() {
        let rows = version_rows(&load(&[])[0]);
        let versions: Vec<_> = rows.iter().map(|r| r.version.as_str()).collect();
        assert_eq!(versions, ["17", "12"]);
        let v17 = &rows[0];
        assert_eq!(v17.eol_date, date(2029, 11, 8));
        assert_eq!(v17.release_date, date(2024, 9, 26));
        assert_eq!(v17.latest_patch.as_deref(), Some("17.5"));
        assert_eq!(v17.is_eol, Some(false));
        assert_eq!(v17.cpe.as_deref(), Some("cpe:2.3:a:postgresql:postgresql"));
        assert_eq!(v17.purl.as_deref(), Some("pkg:generic/postgresql"));
    }

    #[test]
    fn missing_fields_become_null() {
        let rows = version_rows(&load(&[])[1]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].eol_date, None);
        assert_eq!(rows[0].latest_patch, None);
        assert_eq!(rows[0].cpe, None);
        assert!(version_rows(&load(&[])[2]).is_empty());
    }

    #[test]
    fn parse_date_tolerates_non_dates() {
        assert_eq!(parse_date(&Value::Null), None);
        assert_eq!(parse_date(&json!(true)), None);
        assert_eq!(parse_date(&json!("not-a-date")), None);
        assert_eq!(parse_date(&json!("2026-01-31")), date(2026, 1, 31));
        assert_eq!(parse_date(&json!("2026-01-31T00:00:00Z")), date(2026, 1, 31));
    }
}
