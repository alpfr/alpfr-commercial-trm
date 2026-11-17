.PHONY: install db-up db-down db-reset seed seed-dry test rust-build rust-test rust-seed rust-seed-dry

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

RUST_DIR := tools/seed-catalog
RUST_BIN := $(RUST_DIR)/target/release/seed-catalog

rust-build:
	cargo build --release --manifest-path $(RUST_DIR)/Cargo.toml

rust-test:
	cargo test --manifest-path $(RUST_DIR)/Cargo.toml

rust-seed-dry: rust-build
	$(RUST_BIN) --dry-run

rust-seed: rust-build
	$(RUST_BIN)
