use std::fs;
use std::path::{Path, PathBuf};

use log::{debug, error, warn};

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, Default)]
pub struct BatteryInfo {
    pub name: String,
    pub energy_rate: f64,
    pub percentage: f64,
    pub status: String,
    pub energy: f64,
    pub energy_full: f64,
    /// Optional: not every driver exposes energy_full_design.
    pub energy_full_design: Option<f64>,
    /// Optional: kernel reports cycles as an integer, absent on many devices.
    pub cycle_count: Option<u64>,
    /// Optional: degrees C, the kernel reports tenths of a degree.
    pub temperature: Option<f64>,
}

impl BatteryInfo {
    /// Capacity-based health for this specific battery. Kept per-device so a
    /// multi-battery machine never attributes one battery's ratio to another.
    pub fn health_percentage(&self) -> Option<f64> {
        let design = self.energy_full_design?;
        if design <= 0.0 || self.energy_full <= 0.0 {
            return None;
        }
        Some(self.energy_full / design * 100.0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatteryData {
    pub energy_rate: f64,
    pub percentage: f64,
    pub status: String,
    pub time_to_empty: i64,
    pub time_to_full: i64,
    pub energy: f64,
    pub energy_full: f64,
    pub energy_full_design: Option<f64>,
    pub cycle_count: Option<u64>,
    pub temperature: Option<f64>,
    pub batteries: Vec<BatteryInfo>,
}

fn read_battery_info(path: &Path) -> Result<BatteryInfo, String> {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let status = read_trimmed(path, "status").unwrap_or_default();
    let percentage = read_f64(path, "capacity")
        .or_else(|_| {
            let energy = read_f64(path, "energy_now")?;
            let energy_full = read_f64(path, "energy_full")?;
            if energy_full <= 0.0 {
                return Err("energy_full is zero".to_string());
            }
            Ok((energy / energy_full) * 100.0)
        })
        .unwrap_or(0.0);

    let energy = read_energy_wh(path, "energy_now")
        .or_else(|_| read_charge_as_energy_wh(path, "charge_now"))
        .unwrap_or(0.0);
    let energy_full = read_energy_wh(path, "energy_full")
        .or_else(|_| read_charge_as_energy_wh(path, "charge_full"))
        .unwrap_or(0.0);
    let energy_rate = read_power_watts(path).unwrap_or(0.0);

    // Optional attributes. Never `?` these: a driver missing one of them must
    // still report its battery, just without the values that depend on it.
    let energy_full_design = read_energy_wh(path, "energy_full_design").ok();
    let cycle_count = read_trimmed(path, "cycle_count")
        .ok()
        .and_then(|v| v.parse().ok());
    let temperature = read_f64(path, "temp").ok().map(|t| t / 10.0);

    debug!(
        "poll: battery={} {:.1}% status={} rate={:.3}W energy={:.2}Wh full={:.2}Wh",
        path.display(),
        percentage,
        status,
        energy_rate,
        energy,
        energy_full,
    );

    Ok(BatteryInfo {
        name,
        energy_rate,
        percentage,
        status,
        energy,
        energy_full,
        energy_full_design,
        cycle_count,
        temperature,
    })
}

pub async fn poll_batteries() -> Result<BatteryData, String> {
    let paths = find_batteries()?;
    let mut infos = Vec::with_capacity(paths.len());

    for path in &paths {
        match read_battery_info(path) {
            Ok(info) => infos.push(info),
            Err(e) => warn!("failed to read {}: {}", path.display(), e),
        }
    }

    if infos.is_empty() {
        return Err("No battery data readable".to_string());
    }

    let energy: f64 = infos.iter().map(|b| b.energy).sum();
    let energy_full: f64 = infos.iter().map(|b| b.energy_full).sum();
    let energy_rate: f64 = infos.iter().map(|b| b.energy_rate).sum();
    let percentage = aggregate_percentage(&infos);

    let status = aggregate_status(&infos);

    let (time_to_empty, time_to_full) = estimate_times(&status, energy, energy_full, energy_rate);

    // Design capacity only counts when every battery reports one, so a partial
    // set never yields a health figure built on half the inputs.
    let energy_full_design = infos.iter().try_fold(0.0, |acc, b| {
        b.energy_full_design.map(|d| acc + d)
    });

    // These are per-device properties, not totals, so the first battery that
    // reports one wins.
    let cycle_count = infos.iter().find_map(|b| b.cycle_count);
    let temperature = infos.iter().find_map(|b| b.temperature);

    Ok(BatteryData {
        energy_rate,
        percentage,
        status,
        time_to_empty,
        time_to_full,
        energy,
        energy_full,
        energy_full_design,
        cycle_count,
        temperature,
        batteries: infos,
    })
}

fn find_batteries() -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(POWER_SUPPLY_DIR).map_err(|e| e.to_string())?;
    let mut batteries = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let battery_type = read_trimmed(&path, "type").unwrap_or_default();
        if battery_type != "Battery" {
            continue;
        }

        if !has_battery_readings(&path) {
            debug!(
                "skipping battery-like supply without readings: {}",
                path.display()
            );
            continue;
        }

        debug!("battery found: {}", path.display());
        batteries.push(path);
    }

    if batteries.is_empty() {
        error!("no battery detected under {}", POWER_SUPPLY_DIR);
        return Err("No battery detected".to_string());
    }

    Ok(batteries)
}

/// A power supply tagged `Battery` is not necessarily a battery: the Apple MFi
/// fast-charge driver, for example, exposes one with no capacity or energy
/// attributes. Require at least one reading the applet can actually use.
fn has_battery_readings(path: &Path) -> bool {
    ["capacity", "energy_now", "charge_now"]
        .iter()
        .any(|attr| path.join(attr).exists())
}

/// Aggregate status across batteries, ignoring entries whose driver reports no
/// status at all. An empty one must not mask the real battery's state.
fn aggregate_status(infos: &[BatteryInfo]) -> String {
    let statuses: Vec<&str> = infos
        .iter()
        .map(|b| b.status.as_str())
        .filter(|status| !status.is_empty())
        .collect();

    if statuses.is_empty() {
        return String::new();
    }

    if statuses
        .iter()
        .all(|&status| status == "Full" || status == "Fully Charged")
    {
        return "Full".to_string();
    }

    statuses
        .iter()
        .copied()
        .find(|status| *status != "Full" && *status != "Fully Charged")
        .map(str::to_string)
        .unwrap_or_default()
}

/// Prefer the energy-weighted level when every battery exposes usable energy
/// values. Drivers that only publish `capacity` have no Wh readings, so fall
/// back to the mean capacity instead of rendering 0%.
fn aggregate_percentage(infos: &[BatteryInfo]) -> f64 {
    let all_have_energy = infos.iter().all(|b| {
        b.energy_full > 0.0 && (b.energy > 0.0 || b.percentage <= 0.0)
    });

    if all_have_energy {
        let energy: f64 = infos.iter().map(|b| b.energy).sum();
        let energy_full: f64 = infos.iter().map(|b| b.energy_full).sum();
        if energy_full > 0.0 {
            return 100.0 * energy / energy_full;
        }
    }

    let mut total = 0.0;
    let mut count = 0_u32;
    for battery in infos {
        if battery.percentage.is_finite() {
            total += battery.percentage;
            count += 1;
        }
    }

    if count > 0 {
        total / f64::from(count)
    } else {
        0.0
    }
}

fn estimate_times(status: &str, energy: f64, energy_full: f64, energy_rate: f64) -> (i64, i64) {
    if energy_rate <= 0.0 {
        return (0, 0);
    }

    match status {
        "Discharging" => (((energy / energy_rate) * 3600.0) as i64, 0),
        "Charging" => (0, (((energy_full - energy).max(0.0) / energy_rate) * 3600.0) as i64),
        _ => (0, 0),
    }
}

fn read_trimmed(base: &Path, file: &str) -> Result<String, String> {
    let path = base.join(file);
    let value = fs::read_to_string(&path).map_err(|e| format!("{}: {}", path.display(), e))?;
    Ok(value.trim().to_string())
}

fn read_f64(base: &Path, file: &str) -> Result<f64, String> {
    read_trimmed(base, file)?
        .parse::<f64>()
        .map_err(|e| format!("{}: {}", base.join(file).display(), e))
}

fn read_energy_wh(base: &Path, file: &str) -> Result<f64, String> {
    Ok(read_f64(base, file)? / 1_000_000.0)
}

fn read_charge_as_energy_wh(base: &Path, file: &str) -> Result<f64, String> {
    let charge_ua_h = read_f64(base, file)?;
    let voltage_uv = read_f64(base, "voltage_now")?;

    Ok((charge_ua_h * voltage_uv) / 1_000_000_000_000.0)
}

fn read_power_watts(base: &Path) -> Result<f64, String> {
    if let Ok(power_uw) = read_f64(base, "power_now") {
        return Ok(power_uw / 1_000_000.0);
    }

    let current_ua = read_f64(base, "current_now")?;
    let voltage_uv = read_f64(base, "voltage_now")?;
    Ok((current_ua * voltage_uv) / 1_000_000_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn battery(name: &str, status: &str) -> BatteryInfo {
        BatteryInfo {
            name: name.to_string(),
            status: status.to_string(),
            ..BatteryInfo::default()
        }
    }

    #[test]
    fn aggregate_status_ignores_empty_statuses() {
        let infos = [
            battery("apple_mfi_fastcharge_1-2", ""),
            battery("BAT0", "Charging"),
        ];
        assert_eq!(aggregate_status(&infos), "Charging");
    }

    #[test]
    fn aggregate_status_reports_full_when_known_batteries_are_full() {
        let infos = [
            battery("apple_mfi_fastcharge_1-2", ""),
            battery("BAT0", "Full"),
        ];
        assert_eq!(aggregate_status(&infos), "Full");
    }

    #[test]
    fn aggregate_status_is_empty_without_any_known_status() {
        assert_eq!(aggregate_status(&[battery("ghost", "")]), "");
    }

    #[test]
    fn aggregate_status_keeps_first_non_full_status() {
        let infos = [battery("BAT0", "Discharging"), battery("BAT1", "Charging")];
        assert_eq!(aggregate_status(&infos), "Discharging");
    }

    fn capacity_battery(name: &str, percentage: f64) -> BatteryInfo {
        BatteryInfo {
            name: name.to_string(),
            percentage,
            ..BatteryInfo::default()
        }
    }

    fn energy_battery(name: &str, energy: f64, energy_full: f64) -> BatteryInfo {
        BatteryInfo {
            name: name.to_string(),
            energy,
            energy_full,
            percentage: 100.0 * energy / energy_full,
            ..BatteryInfo::default()
        }
    }

    #[test]
    fn aggregate_percentage_uses_capacity_when_energy_is_missing() {
        let infos = [
            capacity_battery("BAT0", 80.0),
            capacity_battery("BAT1", 40.0),
        ];
        assert!((aggregate_percentage(&infos) - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aggregate_percentage_weights_energy_by_full_capacity() {
        let infos = [
            energy_battery("BAT0", 60.0, 100.0),
            energy_battery("BAT1", 20.0, 50.0),
        ];
        let percentage = aggregate_percentage(&infos);
        assert!((percentage - (80.0 / 150.0 * 100.0)).abs() < 0.0001);
    }

    #[test]
    fn aggregate_percentage_falls_back_when_any_energy_reading_is_missing() {
        let infos = [
            capacity_battery("BAT0", 80.0),
            energy_battery("BAT1", 25.0, 50.0),
        ];
        assert!((aggregate_percentage(&infos) - 65.0).abs() < f64::EPSILON);
    }

    #[test]
    fn health_percentage_is_per_battery() {
        let mut battery = energy_battery("BAT0", 40.0, 50.0);
        battery.energy_full_design = Some(50.0);
        assert!((battery.health_percentage().unwrap() - 100.0).abs() < 0.0001);

        battery.energy_full_design = Some(62.5);
        assert!((battery.health_percentage().unwrap() - 80.0).abs() < 0.0001);

        battery.energy_full_design = None;
        assert_eq!(battery.health_percentage(), None);
    }

    #[test]
    fn health_percentage_rejects_zero_design_or_full_capacity() {
        let mut battery = energy_battery("BAT0", 40.0, 50.0);

        battery.energy_full_design = Some(0.0);
        assert_eq!(battery.health_percentage(), None);

        battery.energy_full_design = Some(50.0);
        battery.energy_full = 0.0;
        assert_eq!(battery.health_percentage(), None);
    }

    #[test]
    fn temperature_keeps_one_decimal() {
        let dir = temp_supply("temperature");
        fs::write(dir.join("capacity"), "50").unwrap();
        fs::write(dir.join("temp"), "305").unwrap();

        let info = read_battery_info(&dir).unwrap();
        assert_eq!(info.temperature, Some(30.5));

        fs::remove_dir_all(&dir).unwrap();
    }

    fn temp_supply(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cosmic-power-monitor-test-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn battery_like_supply_without_readings_is_rejected() {
        let dir = temp_supply("ghost");
        fs::write(dir.join("type"), "Battery").unwrap();
        fs::write(dir.join("charge_type"), "Trickle").unwrap();

        assert!(!has_battery_readings(&dir));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn supply_with_any_reading_is_accepted() {
        for attr in ["capacity", "energy_now", "charge_now"] {
            let dir = temp_supply(attr);
            fs::write(dir.join("type"), "Battery").unwrap();
            fs::write(dir.join(attr), "88").unwrap();

            assert!(
                has_battery_readings(&dir),
                "{attr} should count as a reading"
            );

            fs::remove_dir_all(&dir).unwrap();
        }
    }
}
