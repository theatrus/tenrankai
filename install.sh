#!/bin/bash

# DynServer Installation Script
# This script helps with development installation and testing

set -e

INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/tenrankai"
DATA_DIR="/var/lib/tenrankai"
LOG_DIR="/var/log/tenrankai"
SHARE_DIR="/usr/local/share/tenrankai"
SERVICE_FILE="/etc/systemd/system/tenrankai.service"

echo "DynServer Installation Script"
echo "============================="

# Check if running as root
if [[ $EUID -eq 0 ]]; then
   echo "This script should not be run as root for development installation."
   echo "For system-wide installation, use the Debian package instead."
   exit 1
fi

# Function to create directories with sudo
create_dir() {
    local dir=$1
    local owner=${2:-$USER}
    
    if [ ! -d "$dir" ]; then
        echo "Creating directory: $dir"
        sudo mkdir -p "$dir"
        sudo chown "$owner:$owner" "$dir"
    fi
}

# Build the project
echo "Building tenrankai..."
cargo build --release

# Install binary
echo "Installing binary to $INSTALL_DIR..."
sudo cp target/release/tenrankai "$INSTALL_DIR/"
sudo chmod 755 "$INSTALL_DIR/tenrankai"

# Create directories
echo "Creating directories..."
create_dir "$CONFIG_DIR"
create_dir "$DATA_DIR"
create_dir "$LOG_DIR"
create_dir "$SHARE_DIR"

# Install templates and static files
echo "Installing templates and static files..."
sudo cp -r templates "$SHARE_DIR/"
sudo cp -r static "$SHARE_DIR/"

# Install example configuration
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    echo "Installing example configuration..."
    sudo cp config.toml.example "$CONFIG_DIR/"
    echo "Please edit $CONFIG_DIR/config.toml.example and rename to config.toml"
else
    echo "Configuration file already exists at $CONFIG_DIR/config.toml"
fi

# Install systemd service (optional)
read -p "Install systemd service? (y/N): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Installing systemd service..."
    sudo cp tenrankai.service "$SERVICE_FILE"
    sudo systemctl daemon-reload
    echo "Service installed. Enable with: sudo systemctl enable tenrankai"
    echo "Start with: sudo systemctl start tenrankai"
fi

echo ""
echo "Installation complete!"
echo ""
echo "Next steps:"
echo "1. Copy and edit the configuration:"
echo "   sudo cp $CONFIG_DIR/config.toml.example $CONFIG_DIR/config.toml"
echo "   sudo nano $CONFIG_DIR/config.toml"
echo ""
echo "2. Add your images to $DATA_DIR/gallery/"
echo ""
echo "3. Start the server:"
echo "   tenrankai --config $CONFIG_DIR/config.toml"
echo ""
echo "   Or if using systemd:"
echo "   sudo systemctl enable tenrankai"
echo "   sudo systemctl start tenrankai"