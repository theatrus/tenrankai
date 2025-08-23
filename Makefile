# DynServer Build and Package Makefile

.PHONY: all build test clean package install-deps deb-build deb-clean

# Build the binary in release mode
build:
	cargo build --release

# Run tests
test:
	cargo test

# Clean build artifacts
clean:
	cargo clean
	rm -rf debian/dynserver
	rm -rf target/

# Install dependencies for Debian packaging
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

# Build Debian package
deb-build: clean
	@echo "Building Debian package..."
	dpkg-buildpackage -us -uc -b

# Clean Debian build artifacts
deb-clean:
	rm -rf debian/dynserver
	rm -rf debian/.debhelper
	rm -rf debian/cargo
	rm -f debian/files
	rm -f debian/debhelper-build-stamp
	rm -f debian/dynserver.debhelper.log
	rm -f debian/dynserver.substvars

# Quick test of the systemd service file
check-systemd:
	systemd-analyze verify dynserver.service

# Lint the package
package-lint:
	lintian ../dynserver_*.deb

# All build tasks
all: build test

# Help target
help:
	@echo "Available targets:"
	@echo "  build        - Build the release binary"
	@echo "  test         - Run cargo tests"
	@echo "  clean        - Clean all build artifacts"
	@echo "  install-deps - Install Debian packaging dependencies"
	@echo "  deb-build    - Build Debian package"
	@echo "  deb-clean    - Clean Debian build artifacts"
	@echo "  check-systemd - Verify systemd service file"
	@echo "  package-lint - Lint the built package"
	@echo "  all          - Build and test"
	@echo "  help         - Show this help"