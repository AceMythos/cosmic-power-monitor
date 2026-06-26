# Cosmic Power Monitor

A COSMIC desktop applet that displays real-time battery power draw (watts) in the panel.

## Features

- Shows current battery discharge/charge rate in watts in the panel
- Color-coded: discharging (red), charging (green)
- Popup with detailed info: percentage, time remaining, energy capacity
- Polls every 3 seconds via UPower DBus

## Requirements

- Pop!_OS 24.04+ with COSMIC desktop (or any COSMIC-based system)
- Rust toolchain: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- System dev packages (run setup.sh)

## Quick Install

```bash
chmod +x setup.sh
./setup.sh
```

Then add **Cosmic Power Monitor** to your panel:
**COSMIC Settings → Desktop → Panel → Add applet**

## Manual Build

```bash
# Install system deps
sudo apt install libxkbcommon-dev libfontconfig-dev libfreetype-dev libexpat1-dev cmake pkgconf

# Build
. "$HOME/.cargo/env"
cargo build --release

# Install
sudo cp target/release/cosmic-power-monitor /usr/local/bin/
sudo cp resources/com.acemythos.CosmicPowerMonitor.desktop /usr/share/applications/
```

## Uninstall

```bash
just uninstall
```

## License

MIT
