use std::collections::HashMap;

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
    let connection = zbus::Connection::system()
        .await
        .map_err(|e| e.to_string())?;

    let result = connection
        .call_method(
            Some("org.freedesktop.UPower"),
            "/org/freedesktop/UPower",
            Some("org.freedesktop.UPower"),
            "GetDisplayDevice",
            &(),
        )
        .await
        .map_err(|e| e.to_string())?;

    let device_path: zbus::zvariant::OwnedObjectPath = result
        .body()
        .deserialize()
        .map_err(|e| format!("parse error: {}", e))?;

    let device_result = connection
        .call_method(
            Some("org.freedesktop.UPower"),
            device_path.as_ref(),
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &("org.freedesktop.UPower.Device"),
        )
        .await
        .map_err(|e| e.to_string())?;

    let props: HashMap<String, zbus::zvariant::OwnedValue> = device_result
        .body()
        .deserialize()
        .map_err(|e| format!("parse error: {}", e))?;

    fn get_f64(props: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str) -> f64 {
        props.get(key)
            .and_then(|v| v.downcast_ref::<f64>().ok())
            .unwrap_or(0.0)
    }

    fn get_u32(props: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str) -> u32 {
        props.get(key)
            .and_then(|v| v.downcast_ref::<u32>().ok())
            .unwrap_or(0)
    }

    fn get_i64(props: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str) -> i64 {
        props.get(key)
            .and_then(|v| v.downcast_ref::<i64>().ok())
            .unwrap_or(0)
    }

    let energy_rate = get_f64(&props, "EnergyRate");
    let energy = get_f64(&props, "Energy");
    let energy_full = get_f64(&props, "EnergyFull");
    let percentage = get_f64(&props, "Percentage");
    let state = get_u32(&props, "State");
    let time_to_empty = get_i64(&props, "TimeToEmpty");
    let time_to_full = get_i64(&props, "TimeToFull");

    let status = match state {
        1 => "Charging",
        2 => "Discharging",
        3 => "Empty",
        4 => "Fully Charged",
        5 => "Pending Charge",
        6 => "Pending Discharge",
        _ => "Unknown",
    }
    .to_string();

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
