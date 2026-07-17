#!/usr/bin/env bash
set -e

echo "=== Installing system dependencies ==="
sudo apt install -y \
    libxkbcommon-dev \
    libfontconfig-dev \
    libfreetype-dev \
    libexpat1-dev \
    cmake \
    pkgconf

echo "=== Checking Rust toolchain ==="
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
. "$HOME/.cargo/env"

echo "=== Building applet ==="
cargo build --release

echo "=== Installing applet ==="
sudo install -Dm0755 target/release/cosmic-power-monitor /usr/local/bin/cosmic-power-monitor
sudo install -Dm0644 resources/io.github.AceMythos.cosmic-ext-applet-power-monitor.desktop \
    /usr/share/applications/io.github.AceMythos.cosmic-ext-applet-power-monitor.desktop
sudo install -Dm0644 resources/io.github.AceMythos.cosmic-ext-applet-power-monitor.svg \
    /usr/share/icons/hicolor/scalable/apps/io.github.AceMythos.cosmic-ext-applet-power-monitor.svg
sudo install -Dm0644 resources/io.github.AceMythos.cosmic-ext-applet-power-monitor.metainfo.xml \
    /usr/share/metainfo/io.github.AceMythos.cosmic-ext-applet-power-monitor.metainfo.xml

echo "=== Done ==="
echo "Add Power Monitor to your panel: COSMIC Settings -> Desktop -> Panel -> Add applet"
