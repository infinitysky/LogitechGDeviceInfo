//! Persistent settings: %APPDATA%\LogiTray\config.json

use crate::features::rgb::{Effect, Rgb};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorPreset {
    pub name: String,
    pub hex: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_rate_ms: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u16>,
    /// zone index -> last applied effect
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zones: BTreeMap<u8, Effect>,
    /// Remembered breathing/cycle period and brightness per zone (used when switching effects).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zone_period_ms: BTreeMap<u8, u16>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zone_brightness: BTreeMap<u8, u8>,
}

/// What the tray icon itself shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayMetric {
    /// Current DPI as digits (stacked in two rows when >= 1000).
    Dpi,
    /// Battery percentage with a coloured charge bar.
    Battery,
    /// Current lighting colour of the first zone.
    Color,
}

impl TrayMetric {
    pub fn label(self) -> &'static str {
        match self {
            TrayMetric::Dpi => "DPI",
            TrayMetric::Battery => "Battery",
            TrayMetric::Color => "Lighting color",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayDisplay {
    /// Device key (name) whose metric is shown.
    pub device: String,
    pub metric: TrayMetric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `None` = plain app icon; otherwise render the chosen metric into the tray icon.
    pub tray_display: Option<TrayDisplay>,
    /// DPI values offered in the menu (snapped to what the sensor supports).
    pub dpi_presets: Vec<u16>,
    /// Colour palette offered for fixed/breathing effects.
    pub colors: Vec<ColorPreset>,
    /// Also write lighting to the device's non-volatile memory (survives power cycles).
    pub persist_lighting: bool,
    /// Re-apply the last settings when the app starts or a device (re)appears.
    pub restore_on_start: bool,
    /// Poll interval for battery / DPI readback.
    pub refresh_secs: u64,
    /// Last applied state per device name.
    pub devices: BTreeMap<String, DeviceState>,
}

impl Default for Config {
    fn default() -> Self {
        let colors = [
            ("White", "#FFFFFF"),
            ("Red", "#FF0000"),
            ("Orange", "#FF6A00"),
            ("Yellow", "#FFD400"),
            ("Green", "#00FF2A"),
            ("Cyan", "#00FFFF"),
            ("Blue", "#0066FF"),
            ("Purple", "#8A2BE2"),
            ("Pink", "#FF2D95"),
            ("Warm white", "#FFD9A0"),
        ]
        .iter()
        .map(|(n, h)| ColorPreset {
            name: (*n).into(),
            hex: (*h).into(),
        })
        .collect();
        Self {
            tray_display: None,
            dpi_presets: vec![400, 800, 1200, 1600, 2400, 3200, 6400, 12800],
            colors,
            persist_lighting: false,
            restore_on_start: true,
            refresh_secs: 60,
            devices: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("LogiTray").join("config.json")
    }

    pub fn load() -> Config {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            // Tolerate a UTF-8 BOM (Notepad / PowerShell often add one).
            Ok(s) => match serde_json::from_str::<Config>(s.trim_start_matches('\u{feff}')) {
                Ok(c) => c,
                Err(e) => {
                    let msg = format!("config parse error ({}): {e}; using defaults", p.display());
                    eprintln!("{msg}");
                    crate::app::log_line(&msg);
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        let s = serde_json::to_string_pretty(self)?;
        std::fs::write(&p, s).with_context(|| format!("write {}", p.display()))?;
        Ok(())
    }

    pub fn palette(&self) -> Vec<(String, Rgb)> {
        self.colors
            .iter()
            .filter_map(|c| Rgb::parse(&c.hex).map(|rgb| (c.name.clone(), rgb)))
            .collect()
    }

    pub fn device_mut(&mut self, key: &str) -> &mut DeviceState {
        self.devices.entry(key.to_owned()).or_default()
    }
}
