.PHONY: install db-up db-down db-reset seed seed-dry test

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
