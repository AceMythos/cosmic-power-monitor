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
    let percentage = if energy_full > 0.0 {
        100.0 * energy / energy_full
    } else {
        0.0
    };

    let status = if infos.iter().all(|b| b.status == "Full" || b.status == "Fully Charged") {
        "Full".to_string()
    } else {
        infos
            .iter()
            .find(|b| b.status != "Full" && b.status != "Fully Charged")
            .map(|b| b.status.clone())
            .unwrap_or_default()
    };

    let (time_to_empty, time_to_full) = estimate_times(&status, energy, energy_full, energy_rate);

    Ok(BatteryData {
        energy_rate,
        percentage,
        status,
        time_to_empty,
        time_to_full,
        energy,
        energy_full,
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

        debug!("battery found: {}", path.display());
        batteries.push(path);
    }

    if batteries.is_empty() {
        error!("no battery detected under {}", POWER_SUPPLY_DIR);
        return Err("No battery detected".to_string());
    }

    Ok(batteries)
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
