#!/bin/bash

# rusty-crunch installer
# This script removes macOS quarantine attributes and optionally installs to /usr/local/bin

set -e

BINARY="rusty-crunch"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

echo "🔧 rusty-crunch installer"
echo ""

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo "❌ Error: '$BINARY' binary not found in current directory"
    echo "Please download the binary from https://github.com/pablogonz12/rusty-crunch/releases"
    exit 1
fi

# Make executable
chmod +x "$BINARY"
echo "✓ Made binary executable"

# Remove macOS quarantine attribute (if needed)
if [[ "$OSTYPE" == "darwin"* ]]; then
    if xattr "$BINARY" 2>/dev/null | grep -q "com.apple.quarantine"; then
        echo "🔓 Removing macOS quarantine attribute..."
        xattr -d com.apple.quarantine "$BINARY"
        echo "✓ Quarantine attribute removed"
    else
        echo "✓ No quarantine attribute found (binary is safe)"
    fi
else
    echo "ℹ️  Not running on macOS (skipping quarantine removal)"
fi

echo ""
echo "Installation complete!"
echo ""
echo "You can now run: ./$BINARY"
echo ""
read -p "Would you like to install to $INSTALL_DIR? (y/n) " -n 1 -r
echo ""

if [[ $REPLY =~ ^[Yy]$ ]]; then
    if [ ! -w "$INSTALL_DIR" ]; then
        echo "ℹ️  $INSTALL_DIR is not writable, attempting with sudo..."
        sudo cp "$BINARY" "$INSTALL_DIR/$BINARY"
    else
        cp "$BINARY" "$INSTALL_DIR/$BINARY"
    fi
    echo "✓ Installed to $INSTALL_DIR/$BINARY"
    echo "You can now run 'rusty-crunch' from anywhere"
else
    echo "ℹ️  Skipped system installation"
fi
