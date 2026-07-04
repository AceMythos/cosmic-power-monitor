# Flatpak Submission Guide

## Files created

- `io.github.AceMythos.CosmicPowerMonitor.json` — Flatpak manifest
- `resources/io.github.AceMythos.CosmicPowerMonitor.metainfo.xml` — AppStream metadata
- `generate-flatpak-sources.sh` — Script to generate cargo-sources.json

## Steps to submit

### 1. Install flatpak-cargo-generator

```bash
git clone https://github.com/nickvdyck/flatpak-builder-tools.git
cd flatpak-builder-tools/cargo
sudo cp flatpak-cargo-generator.py /usr/local/bin/
```

### 2. Generate cargo-sources.json

```bash
cd /home/igris/cosmic-power-monitor
chmod +x generate-flatpak-sources.sh
./generate-flatpak-sources.sh
```

### 3. Update the desktop file

Rename your desktop file to match the Flatpak ID:
```bash
mv resources/com.acemythos.CosmicPowerMonitor.desktop resources/io.github.AceMythos.CosmicPowerMonitor.desktop
```

Update the `Exec=` line in the desktop file to just the binary name:
```
Exec=cosmic-power-monitor
```

### 4. Commit and push

```bash
git add -A
git commit -m "Add Flatpak packaging files"
git push
```

### 5. Fork and submit to cosmic-flatpak

1. Fork https://github.com/pop-os/cosmic-flatpak
2. Create directory: `app/io.github.AceMythos.CosmicPowerMonitor/`
3. Copy your files there:
   - `io.github.AceMythos.CosmicPowerMonitor.json`
   - `cargo-sources.json`
4. Submit a PR

### 6. Test locally (optional)

```bash
sudo apt install flatpak flatpak-builder
flatpak remote-add --user cosmic https://apt.pop-os.org/cosmic/cosmic.flatpakrepo
just build io.github.AceMythos.CosmicPowerMonitor
```
