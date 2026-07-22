use std::fs;
use std::path::{Path, PathBuf};

const POWER_SUPPLY_DIR: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, Default)]
pub struct BatteryData {
    pub energy_rate: f64,
    pub percentage: f64,
    pub status: String,
    pub time_to_empty: i64,
    pub time_to_full: i64,
    pub energy: f64,
    pub energy_full: f64,
}

pub async fn poll_battery() -> Result<BatteryData, String> {
    let battery_path = battery_path()?;

    let status = read_trimmed(&battery_path, "status")?;
    let percentage = read_f64(&battery_path, "capacity")
        .or_else(|_| {
            let energy = read_f64(&battery_path, "energy_now")?;
            let energy_full = read_f64(&battery_path, "energy_full")?;
            if energy_full <= 0.0 {
                return Err("energy_full is zero".to_string());
            }

            Ok((energy / energy_full) * 100.0)
        })?;

    let energy = read_energy_wh(&battery_path, "energy_now")
        .or_else(|_| read_charge_as_energy_wh(&battery_path, "charge_now"))?;
    let energy_full = read_energy_wh(&battery_path, "energy_full")
        .or_else(|_| read_charge_as_energy_wh(&battery_path, "charge_full"))?;
    let energy_rate = read_power_watts(&battery_path)?;

    let (time_to_empty, time_to_full) = estimate_times(&status, energy, energy_full, energy_rate);

    Ok(BatteryData {
        energy_rate,
        percentage,
        status,
        time_to_empty,
        time_to_full,
        energy,
        energy_full,
    })
}

fn battery_path() -> Result<PathBuf, String> {
    let entries = fs::read_dir(POWER_SUPPLY_DIR).map_err(|e| e.to_string())?;

    // read_dir order isn't guaranteed, so a peripheral's battery node could come
    // before the real one; prefer scope != "Device" and only fall back otherwise.
    let mut fallback: Option<PathBuf> = None;

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

        let scope = read_trimmed(&path, "scope").unwrap_or_default();
        if scope == "Device" {
            fallback.get_or_insert(path);
            continue;
        }

        return Ok(path);
    }

    fallback.ok_or_else(|| "No battery detected".to_string())
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
