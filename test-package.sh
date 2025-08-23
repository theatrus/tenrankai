#!/bin/bash

# Package Testing Script
# Tests the Debian package structure and files

set -e

echo "Testing Debian package structure..."
echo "=================================="

# Check required debian files exist
REQUIRED_FILES=(
    "debian/control"
    "debian/rules"
    "debian/changelog"
    "debian/compat"
    "debian/postinst"
    "debian/postrm" 
    "debian/prerm"
    "debian/copyright"
    "debian/README.Debian"
    "debian/logrotate"
)

echo "Checking required files..."
for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$file" ]; then
        echo "✓ $file exists"
    else
        echo "✗ $file missing"
        exit 1
    fi
done

# Check file permissions
echo ""
echo "Checking file permissions..."
for script in debian/postinst debian/postrm debian/prerm debian/rules; do
    if [ -x "$script" ]; then
        echo "✓ $script is executable"
    else
        echo "✗ $script is not executable"
        exit 1
    fi
done

# Check systemd service file
echo ""
echo "Checking systemd service file..."
if [ -f "dynserver.service" ]; then
    echo "✓ dynserver.service exists"
    
    # Basic syntax check
    if grep -q "\[Unit\]" dynserver.service && grep -q "\[Service\]" dynserver.service && grep -q "\[Install\]" dynserver.service; then
        echo "✓ systemd service file has required sections"
    else
        echo "✗ systemd service file missing required sections"
        exit 1
    fi
else
    echo "✗ dynserver.service missing"
    exit 1
fi

# Check config example
echo ""
echo "Checking configuration example..."
if [ -f "config.toml.example" ]; then
    echo "✓ config.toml.example exists"
    
    # Check it has required sections
    if grep -q "\[server\]" config.toml.example && grep -q "\[gallery\]" config.toml.example; then
        echo "✓ config.toml.example has required sections"
    else
        echo "✗ config.toml.example missing required sections"
        exit 1
    fi
else
    echo "✗ config.toml.example missing"
    exit 1
fi

# Check templates and static directories exist
echo ""
echo "Checking assets..."
if [ -d "templates" ]; then
    echo "✓ templates directory exists"
    template_count=$(find templates -name "*.liquid" | wc -l)
    echo "  Found $template_count template files"
else
    echo "✗ templates directory missing"
    exit 1
fi

if [ -d "static" ]; then
    echo "✓ static directory exists"
    static_count=$(find static -type f | wc -l)
    echo "  Found $static_count static files"
else
    echo "✗ static directory missing"
    exit 1
fi

# Check Cargo.toml for version
echo ""
echo "Checking project metadata..."
if [ -f "Cargo.toml" ]; then
    echo "✓ Cargo.toml exists"
    if grep -q 'version = "0.1.0"' Cargo.toml; then
        echo "✓ Version matches package version"
    else
        echo "⚠ Version might not match package version"
    fi
else
    echo "✗ Cargo.toml missing"
    exit 1
fi

echo ""
echo "Package structure test completed successfully!"
echo ""
echo "To build the package on Ubuntu 22.04:"
echo "1. Install dependencies: make install-deps"
echo "2. Build package: make deb-build"
echo "3. Test package: make package-lint"
echo ""
echo "To install for development: ./install.sh"