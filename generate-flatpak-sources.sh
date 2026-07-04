#!/bin/bash
# Generate cargo-sources.json for Flatpak builds
# Requires: python3, flatpak-builder-tools (or cargo-flatpak)

set -e

echo "Generating cargo-sources.json..."

# Method 1: Using flatpak-cargo-generator (recommended)
if command -v flatpak-cargo-generator.py &> /dev/null; then
    flatpak-cargo-generator.py -o cargo-sources.json Cargo.lock
    echo "Generated with flatpak-cargo-generator.py"
    exit 0
fi

# Method 2: Using the script from flatpak-builder-tools
if [ -f /usr/share/flatpak-builder-tools/cargo/flatpak-cargo-generator.py ]; then
    python3 /usr/share/flatpak-builder-tools/cargo/flatpak-cargo-generator.py -o cargo-sources.json Cargo.lock
    echo "Generated with flatpak-builder-tools"
    exit 0
fi

echo "ERROR: flatpak-cargo-generator.py not found."
echo ""
echo "Install it with:"
echo "  git clone https://github.com/nickvdyck/flatpak-builder-tools.git"
echo "  cd flatpak-builder-tools/cargo"
echo "  sudo cp flatpak-cargo-generator.py /usr/local/bin/"
echo ""
echo "Or install the Fedora package:"
echo "  sudo dnf install flatpak-builder-tools-cargo"
echo ""
echo "Then run this script again."
exit 1
