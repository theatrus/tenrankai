# Tenrankai Build Makefile
#
# Frontend assets are served from disk at runtime and are NOT embedded in the
# Rust binary. The frontend and backend can be built independently — this
# Makefile orchestrates them for convenience.

CARGO_FLAGS ?=
NPM_FLAGS ?=

# Default: full dev build (with AVIF)
.PHONY: all
all: frontend build

# ---------------------------------------------------------------------------
# Frontend
# ---------------------------------------------------------------------------

.PHONY: frontend frontend-deps frontend-admin-deps frontend-build

frontend: frontend-deps frontend-admin-deps frontend-build

frontend-deps:
	@if [ ! -d node_modules ]; then \
		echo "Installing frontend dependencies..."; \
		npm install $(NPM_FLAGS); \
	fi

frontend-admin-deps:
	@if [ -d admin ] && [ ! -d admin/node_modules ]; then \
		echo "Installing admin dependencies..."; \
		cd admin && npm install $(NPM_FLAGS); \
	fi

frontend-build: frontend-deps frontend-admin-deps
	npm run build

frontend-prod: frontend-deps frontend-admin-deps
	npm run build:prod

frontend-clean:
	npm run clean

# ---------------------------------------------------------------------------
# Backend (Rust)
# ---------------------------------------------------------------------------

.PHONY: build build-no-avif build-release build-release-no-avif

build:
	cargo build $(CARGO_FLAGS)

build-no-avif:
	cargo build --no-default-features $(CARGO_FLAGS)

build-release:
	cargo build --release $(CARGO_FLAGS)

build-release-no-avif:
	cargo build --release --no-default-features $(CARGO_FLAGS)

# ---------------------------------------------------------------------------
# Combined builds
# ---------------------------------------------------------------------------

.PHONY: dev dev-no-avif release release-no-avif run

dev: frontend build

dev-no-avif: frontend build-no-avif

run: release
	./target/release/tenrankai serve --config config.toml

release: frontend-prod build-release

release-no-avif: frontend-prod build-release-no-avif

# ---------------------------------------------------------------------------
# Testing & linting
# ---------------------------------------------------------------------------

.PHONY: test test-all lint lint-frontend lint-backend check

test:
	cargo test $(CARGO_FLAGS)

test-no-avif:
	cargo test --no-default-features $(CARGO_FLAGS)

lint: lint-frontend lint-backend

lint-frontend: frontend-deps frontend-admin-deps
	npm run lint

lint-backend:
	cargo clippy -- -D warnings
	cargo fmt --check

check: lint test
	@echo "All checks passed."

# ---------------------------------------------------------------------------
# Cleanup
# ---------------------------------------------------------------------------

.PHONY: clean clean-all

clean:
	cargo clean
	npm run clean

clean-all: clean
	rm -rf node_modules admin/node_modules

# ---------------------------------------------------------------------------
# Debian packaging (existing)
# ---------------------------------------------------------------------------

.PHONY: install-deps deb-build deb-clean check-systemd package-lint

install-deps:
	sudo apt-get update
	sudo apt-get install -y \
		build-essential \
		debhelper \
		devscripts \
		cargo \
		rustc \
		pkg-config \
		libssl-dev

deb-build: clean
	@echo "Building Debian package..."
	dpkg-buildpackage -us -uc -b

deb-clean:
	rm -rf debian/tenrankai
	rm -rf debian/.debhelper
	rm -rf debian/cargo
	rm -f debian/files
	rm -f debian/debhelper-build-stamp
	rm -f debian/tenrankai.debhelper.log
	rm -f debian/tenrankai.substvars

check-systemd:
	systemd-analyze verify tenrankai.service

package-lint:
	lintian ../tenrankai_*.deb

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------

.PHONY: help
help:
	@echo "Development:"
	@echo "  make                - Build frontend + backend (with AVIF)"
	@echo "  make dev            - Same as above"
	@echo "  make dev-no-avif    - Build frontend + backend (no AVIF, faster)"
	@echo "  make build          - Build backend only (with AVIF)"
	@echo "  make build-no-avif  - Build backend only (no AVIF)"
	@echo "  make frontend       - Build frontend only"
	@echo ""
	@echo "Release:"
	@echo "  make release           - Production frontend + release binary"
	@echo "  make release-no-avif   - Production frontend + release binary (no AVIF)"
	@echo "  make run               - Release build + run server (config.toml)"
	@echo ""
	@echo "Testing:"
	@echo "  make test           - Run Rust tests (with AVIF)"
	@echo "  make test-no-avif   - Run Rust tests (no AVIF, faster)"
	@echo "  make lint           - Lint frontend + backend"
	@echo "  make check          - Lint + test (pre-commit)"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make clean-all      - Clean everything including node_modules"
	@echo ""
	@echo "Debian packaging:"
	@echo "  make install-deps   - Install Debian packaging dependencies"
	@echo "  make deb-build      - Build Debian package"
	@echo "  make deb-clean      - Clean Debian build artifacts"
