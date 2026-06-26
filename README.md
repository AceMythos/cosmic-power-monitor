# Cosmic Power Monitor

A COSMIC desktop applet that displays real-time battery power draw (watts) in the panel.

## Features

- Panel shows **text only** — e.g. `-12.7W` when discharging, `+26.5W` when charging (no battery icon)
- Popup with detailed info: percentage, status, charge/discharge rate, energy capacity, time remaining
- Polls every 3 seconds via UPower over D-Bus

## How accurate is the reading?

The applet displays UPower's `EnergyRate` property — the same value the OS battery stack reports from your battery driver (typically `power_now` in `/sys/class/power_supply/BAT0/`). The app does not calculate or convert power itself.

**Good for:** seeing whether you're drawing more or less power, comparing idle vs load, and watching charge rate.

**Limitations:**

- Measures **battery charge/discharge rate**, not total laptop power draw
- Values come from the battery firmware/driver, not an external watt meter
- Updates every 3 seconds, so readings can lag slightly and fluctuate between samples
- The label is hidden when no rate is available (e.g. fully charged, `0 W`)

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
