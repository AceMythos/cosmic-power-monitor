build:
    cargo build --release

install: build
    sudo cp target/release/cosmic-power-monitor /usr/local/bin/
    sudo cp resources/com.acemythos.CosmicPowerMonitor.desktop /usr/share/applications/

run:
    cargo run

clean:
    cargo clean

uninstall:
    sudo rm -f /usr/local/bin/cosmic-power-monitor
    sudo rm -f /usr/share/applications/com.acemythos.CosmicPowerMonitor.desktop
