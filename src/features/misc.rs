//! Small read/write helpers: battery (0x1004 / 0x1000), onboard profiles (0x8100),
//! report rate (0x8060), brightness (0x8040).

use crate::device::Device;
use crate::hidpp::be_u16;
use anyhow::Result;

pub const FEAT_BATTERY_LEVEL_STATUS: u16 = 0x1000;
pub const FEAT_BATTERY_VOLTAGE: u16 = 0x1001;
pub const FEAT_UNIFIED_BATTERY: u16 = 0x1004;
pub const FEAT_BRIGHTNESS_CONTROL: u16 = 0x8040;
pub const FEAT_ADJUSTABLE_REPORT_RATE: u16 = 0x8060;
pub const FEAT_ONBOARD_PROFILES: u16 = 0x8100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeNote {
    None,
    Charging,
    Slow,
    Full,
    AlmostFull,
    ChargeError,
    BatteryError,
}

impl ChargeNote {
    /// English, for logs and the CLI.
    pub fn en(self) -> &'static str {
        match self {
            ChargeNote::None => "",
            ChargeNote::Charging => "charging",
            ChargeNote::Slow => "slow charging",
            ChargeNote::Full => "full",
            ChargeNote::AlmostFull => "almost full",
            ChargeNote::ChargeError => "charging error",
            ChargeNote::BatteryError => "battery error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Battery {
    pub percent: Option<u8>,
    pub charging: bool,
    pub note: ChargeNote,
}

impl Battery {
    /// English label for logs and the CLI.
    pub fn label_en(&self) -> String {
        let p = self
            .percent
            .map(|p| format!("{p}%"))
            .unwrap_or_else(|| "?".into());
        let note = self.note.en();
        if note.is_empty() {
            format!("battery {p}")
        } else {
            format!("battery {p} ({note})")
        }
    }
}

pub fn battery(dev: &Device) -> Result<Option<Battery>> {
    if dev.has(FEAT_UNIFIED_BATTERY) {
        // getStatus() -> [stateOfCharge, level flags, chargingStatus, externalPower]
        let r = dev.call(FEAT_UNIFIED_BATTERY, 0x01, &[])?;
        let (charging, note) = match r[2] {
            0 => (false, ChargeNote::None),
            1 => (true, ChargeNote::Charging),
            2 => (true, ChargeNote::Slow),
            3 => (false, ChargeNote::Full),
            _ => (false, ChargeNote::ChargeError),
        };
        return Ok(Some(Battery {
            percent: Some(r[0]),
            charging,
            note,
        }));
    }
    if dev.has(FEAT_BATTERY_LEVEL_STATUS) {
        // getBatteryLevelStatus() -> [level, nextLevel, status]
        let r = dev.call(FEAT_BATTERY_LEVEL_STATUS, 0x00, &[])?;
        let (charging, note) = match r[2] {
            0 => (false, ChargeNote::None),
            1 | 4 => (true, ChargeNote::Charging),
            2 => (true, ChargeNote::AlmostFull),
            3 => (false, ChargeNote::Full),
            _ => (false, ChargeNote::BatteryError),
        };
        return Ok(Some(Battery {
            percent: Some(r[0]),
            charging,
            note,
        }));
    }
    if dev.has(FEAT_BATTERY_VOLTAGE) {
        let r = dev.call(FEAT_BATTERY_VOLTAGE, 0x00, &[])?;
        let mv = be_u16(&r[0..]);
        let charging = r[2] & 0x80 != 0;
        // Rough Li-ion curve: 3.5 V ~ 0 %, 4.2 V ~ 100 %
        let pct = ((mv as i32 - 3500) * 100 / 700).clamp(0, 100) as u8;
        return Ok(Some(Battery {
            percent: Some(pct),
            charging,
            note: if charging {
                ChargeNote::Charging
            } else {
                ChargeNote::None
            },
        }));
    }
    Ok(None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardMode {
    Onboard,
    Host,
}

pub fn onboard_mode(dev: &Device) -> Result<Option<OnboardMode>> {
    if !dev.has(FEAT_ONBOARD_PROFILES) {
        return Ok(None);
    }
    let r = dev.call(FEAT_ONBOARD_PROFILES, 0x02, &[])?;
    Ok(Some(if r[0] == 2 {
        OnboardMode::Host
    } else {
        OnboardMode::Onboard
    }))
}

pub fn set_onboard_mode(dev: &Device, mode: OnboardMode) -> Result<()> {
    let m = match mode {
        OnboardMode::Onboard => 1u8,
        OnboardMode::Host => 2u8,
    };
    dev.call(FEAT_ONBOARD_PROFILES, 0x01, &[m])?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ReportRate {
    /// Supported intervals in ms.
    pub supported: Vec<u8>,
    pub current_ms: u8,
}

pub fn report_rate(dev: &Device) -> Result<Option<ReportRate>> {
    if !dev.has(FEAT_ADJUSTABLE_REPORT_RATE) {
        return Ok(None);
    }
    let flags = dev.call(FEAT_ADJUSTABLE_REPORT_RATE, 0x00, &[])?[0];
    let supported = (0..8u8)
        .filter(|b| flags & (1 << b) != 0)
        .map(|b| b + 1)
        .collect();
    let current_ms = dev.call(FEAT_ADJUSTABLE_REPORT_RATE, 0x01, &[])?[0];
    Ok(Some(ReportRate {
        supported,
        current_ms,
    }))
}

pub fn set_report_rate(dev: &Device, ms: u8) -> Result<()> {
    dev.call(FEAT_ADJUSTABLE_REPORT_RATE, 0x02, &[ms])?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Brightness {
    pub max: u16,
    pub current: u16,
}

pub fn brightness(dev: &Device) -> Result<Option<Brightness>> {
    if !dev.has(FEAT_BRIGHTNESS_CONTROL) {
        return Ok(None);
    }
    let info = dev.call(FEAT_BRIGHTNESS_CONTROL, 0x00, &[])?;
    let cur = dev.call(FEAT_BRIGHTNESS_CONTROL, 0x01, &[])?;
    Ok(Some(Brightness {
        max: be_u16(&info[0..]),
        current: be_u16(&cur[0..]),
    }))
}

pub fn set_brightness(dev: &Device, value: u16) -> Result<()> {
    let b = value.to_be_bytes();
    dev.call(FEAT_BRIGHTNESS_CONTROL, 0x02, &[b[0], b[1]])?;
    Ok(())
}
