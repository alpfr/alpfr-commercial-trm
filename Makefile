.PHONY: install db-up db-down db-reset seed seed-dry test \
        rust-build rust-test rust-lint rust-seed rust-seed-dry check-range check-range-py

# --- Python and database -----------------------------------------------------
install:
	pip install -r requirements-dev.txt

db-up:
	docker compose up -d --wait db

db-down:
	docker compose down

db-reset:
	docker compose down -v
	docker compose up -d --wait db

seed-dry:
	python scripts/seed_catalog.py --dry-run

seed:
	python scripts/seed_catalog.py

test:
	pytest -q

# --- Rust workspace (crates/trm-policy, tools/seed-catalog) ------------------
rust-build:
	cargo build --release --workspace

rust-test:
	cargo test --workspace

rust-lint:
	cargo clippy --workspace --all-targets -- -D warnings

rust-seed-dry: rust-build
	target/release/seed-catalog --dry-run

rust-seed: rust-build
	target/release/seed-catalog

# Example: make check-range RANGE='>=16 <18' VERSIONS='16.4 18.0'
check-range: rust-build
	target/release/trm-check '$(RANGE)' $(VERSIONS)

# Same check with the Python implementation
check-range-py:
	python -m trm_policy '$(RANGE)' $(VERSIONS)
