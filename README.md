# Cosmic Power Monitor

A COSMIC desktop applet that displays real-time battery charge/discharge power (watts) in the panel.

## Features

- Panel shows **text only** — e.g. `-12.7W` when discharging, `+26.5W` when charging (no battery icon)
- Popup with detailed info: percentage, status, charge/discharge rate, energy capacity, time remaining
- Polls about 4 times per second from `/sys/class/power_supply`

## How accurate is the reading?

The applet displays the battery power rate reported by the kernel power-supply interface, usually `power_now` in `/sys/class/power_supply/BAT0/`. If `power_now` is unavailable, it derives watts from `current_now * voltage_now`.

**Good for:** comparing battery power use between idle and load, and watching charge rate.

**Limitations:**

- Shows **battery charge/discharge power**, not total laptop power draw
- Values come from the battery firmware/driver, not an external watt meter
- Updates about every 250 ms, so readings can still fluctuate between samples
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
