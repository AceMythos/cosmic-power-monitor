#!/bin/bash
set -e

echo "=== Installing system dependencies for libcosmic ==="
sudo apt update
sudo apt install -y \
    libxkbcommon-dev \
    libfontconfig-dev \
    libfreetype-dev \
    libexpat1-dev \
    cmake \
    pkgconf

echo "=== Building cosmic-power-monitor ==="
. "$HOME/.cargo/env" 2>/dev/null || true
cargo build --release

echo "=== Installing to system ==="
sudo cp target/release/cosmic-power-monitor /usr/local/bin/
sudo mkdir -p /usr/share/applications/
sudo cp resources/com.acemythos.CosmicPowerMonitor.desktop /usr/share/applications/

echo ""
echo "=== Done! ==="
echo "Now add the applet to your COSMIC panel:"
echo "  COSMIC Settings → Desktop → Panel → Add applet → 'Cosmic Power Monitor'"
