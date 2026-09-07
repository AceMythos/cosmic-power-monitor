use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use serde::{Deserialize, Serialize};

/// What the applet renders in the panel. The popup always shows the full detail
/// regardless of this setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelDisplay {
    /// Charge level only, e.g. `53%`.
    Percentage,
    /// Power draw with a time estimate, e.g. `-12.7W (2h 10m)`. Upstream behaviour.
    #[default]
    Power,
    /// Charge level and power draw, e.g. `53%  -12.7W`. The time estimate is
    /// dropped here to keep the panel entry narrow.
    Both,
}

impl PanelDisplay {
    pub const ALL: [Self; 3] = [Self::Percentage, Self::Power, Self::Both];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Percentage => "%",
            Self::Power => "W",
            Self::Both => "Both",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, CosmicConfigEntry)]
#[version = 1]
pub struct PowerMonitorConfig {
    pub panel_display: PanelDisplay,
}
