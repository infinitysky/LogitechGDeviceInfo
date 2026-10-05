//! 0x8070 ColorLEDEffects (zones) and 0x8071 RGBEffects (clusters).
//! Both share the same effect ids and effect parameter layouts.

use crate::device::Device;
use crate::hidpp::be_u16;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const FEAT_COLOR_LED_EFFECTS: u16 = 0x8070;
pub const FEAT_RGB_EFFECTS: u16 = 0x8071;

pub const EFFECT_OFF: u16 = 0x00;
pub const EFFECT_FIXED: u16 = 0x01;
pub const EFFECT_CYCLE: u16 = 0x03;
pub const EFFECT_WAVE: u16 = 0x04;
pub const EFFECT_STARLIGHT: u16 = 0x05;
pub const EFFECT_LIGHT_ON_PRESS: u16 = 0x06;
pub const EFFECT_AUDIO: u16 = 0x07;
pub const EFFECT_BOOT_UP: u16 = 0x08;
pub const EFFECT_BREATHING: u16 = 0x0A;
pub const EFFECT_RIPPLE: u16 = 0x0B;
pub const EFFECT_CUSTOM: u16 = 0x0C;

/// English effect name. The menu translates this; logs and the CLI keep it as-is.
pub fn effect_name(id: u16) -> String {
    match id {
        EFFECT_OFF => "Off".into(),
        EFFECT_FIXED => "Fixed color".into(),
        0x02 => "Pulse".into(),
        EFFECT_CYCLE => "Color cycle".into(),
        EFFECT_WAVE => "Color wave".into(),
        EFFECT_STARLIGHT => "Starlight".into(),
        EFFECT_LIGHT_ON_PRESS => "Lights up on press".into(),
        EFFECT_AUDIO => "Audio visualizer".into(),
        EFFECT_BOOT_UP => "Boot / demo".into(),
        EFFECT_BREATHING => "Breathing".into(),
        EFFECT_RIPPLE => "Ripple".into(),
        EFFECT_CUSTOM => "Custom".into(),
        other => format!("Effect 0x{other:02X}"),
    }
}

/// English zone location. The menu translates this; logs and the CLI keep it as-is.
pub fn location_name(loc: u16) -> String {
    match loc {
        0 => "Undefined".into(),
        1 => "Primary".into(),
        2 => "Logo".into(),
        3 => "Left".into(),
        4 => "Right".into(),
        5 => "Combined".into(),
        6..=11 => format!("Primary {}", loc - 5),
        other => format!("Location {other}"),
    }
}

#[derive(Debug, Clone)]
pub struct ZoneEffectSlot {
    pub slot: u8,
    pub effect_id: u16,
    /// Capability bits and default period as advertised by the device (diagnostic only).
    #[allow(dead_code)]
    pub caps: u16,
    #[allow(dead_code)]
    pub period: u16,
}

#[derive(Debug, Clone)]
pub struct Zone {
    pub index: u8,
    pub location: u16,
    pub persistency_caps: u8,
    pub effects: Vec<ZoneEffectSlot>,
}

impl Zone {
    pub fn name(&self) -> String {
        location_name(self.location)
    }
    pub fn slot_for(&self, effect_id: u16) -> Option<u8> {
        self.effects
            .iter()
            .find(|e| e.effect_id == effect_id)
            .map(|e| e.slot)
    }
    pub fn supports(&self, effect_id: u16) -> bool {
        self.slot_for(effect_id).is_some()
    }
}

#[derive(Debug, Clone)]
pub struct RgbInfo {
    pub feature: u16,
    pub nv_caps: u16,
    pub ext_caps: u16,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
    pub fn parse(s: &str) -> Option<Rgb> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

/// A user-facing lighting setting for one zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    Off,
    Fixed {
        color: Rgb,
    },
    Breathing {
        color: Rgb,
        period_ms: u16,
        brightness: u8,
    },
    Cycle {
        period_ms: u16,
        brightness: u8,
    },
    /// Any other on-device effect selected by id. `params` holds the raw 10 parameter
    /// bytes when known (e.g. read back from the device); empty = use defaults.
    Other {
        id: u16,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        params: Vec<u8>,
    },
}

impl Effect {
    pub fn id(&self) -> u16 {
        match self {
            Effect::Off => EFFECT_OFF,
            Effect::Fixed { .. } => EFFECT_FIXED,
            Effect::Breathing { .. } => EFFECT_BREATHING,
            Effect::Cycle { .. } => EFFECT_CYCLE,
            Effect::Other { id, .. } => *id,
        }
    }

    /// English label for logs and the CLI.
    pub fn label(&self) -> String {
        match self {
            Effect::Off => "Off".into(),
            Effect::Fixed { color } => format!("Fixed {}", color.hex()),
            Effect::Breathing {
                color, period_ms, ..
            } => {
                format!("Breathing {} / {period_ms} ms", color.hex())
            }
            Effect::Cycle { period_ms, .. } => format!("Color cycle / {period_ms} ms"),
            Effect::Other { id, .. } => effect_name(*id),
        }
    }

    /// Default parameters for effects we don't model in detail (offsets per Solaar/libratbag).
    fn default_other_params(id: u16) -> [u8; 10] {
        let mut p = [0u8; 10];
        let period_at = |p: &mut [u8; 10], off: usize, ms: u16| {
            p[off..off + 2].copy_from_slice(&ms.to_be_bytes())
        };
        match id {
            0x02 => {
                p[0..3].copy_from_slice(&[255, 255, 255]); // pulse: color, speed
                p[3] = 0x80;
            }
            EFFECT_WAVE => {
                // Observed factory values on a G512 (0x8070): [6..8] = speed (LE, <= 0x1FF),
                // [9] = direction (0..6). A BE 5000 ms period is rejected by that firmware.
                p[6] = 0xAC;
                p[7] = 0x01;
                p[9] = 0x04;
            }
            EFFECT_STARLIGHT => {
                p[0..3].copy_from_slice(&[0, 0, 32]); // sky
                p[3..6].copy_from_slice(&[255, 255, 255]); // star
            }
            EFFECT_RIPPLE => {
                p[0..3].copy_from_slice(&[255, 255, 255]);
                period_at(&mut p, 4, 20);
            }
            0x0E => period_at(&mut p, 6, 5000),
            0x0F | 0x10 => period_at(&mut p, 5, 5000),
            0x15 | 0x16 => {
                p[1] = 255; // saturation
                period_at(&mut p, 6, 5000);
            }
            _ => {}
        }
        p
    }

    /// The 10 effect parameter bytes (layout shared by 0x8070 / 0x8071).
    fn params(&self) -> [u8; 10] {
        let mut p = [0u8; 10];
        match *self {
            Effect::Off => {}
            Effect::Other { id, ref params } => {
                if params.is_empty() {
                    p = Self::default_other_params(id);
                } else {
                    let n = params.len().min(10);
                    p[..n].copy_from_slice(&params[..n]);
                }
            }
            Effect::Fixed { color } => {
                p[0] = color.0;
                p[1] = color.1;
                p[2] = color.2;
                p[3] = 0x02; // ramp mode used by G HUB / OpenRGB for "static"
            }
            Effect::Breathing {
                color,
                period_ms,
                brightness,
            } => {
                p[0] = color.0;
                p[1] = color.1;
                p[2] = color.2;
                p[3..5].copy_from_slice(&period_ms.to_be_bytes());
                p[5] = 0; // waveform: default
                p[6] = intensity(brightness);
            }
            Effect::Cycle {
                period_ms,
                brightness,
            } => {
                p[5..7].copy_from_slice(&period_ms.to_be_bytes());
                p[7] = intensity(brightness);
            }
        }
        p
    }
}

/// Device encodes 1..=100 percent, 0 meaning 100.
fn intensity(brightness: u8) -> u8 {
    match brightness.min(100) {
        0 | 100 => 0,
        b => b,
    }
}

pub fn read(dev: &Device) -> Result<Option<RgbInfo>> {
    if dev.has(FEAT_RGB_EFFECTS) {
        return read_8071(dev).map(Some);
    }
    if dev.has(FEAT_COLOR_LED_EFFECTS) {
        return read_8070(dev).map(Some);
    }
    Ok(None)
}

fn read_8070(dev: &Device) -> Result<RgbInfo> {
    let f = FEAT_COLOR_LED_EFFECTS;
    // getInfo() -> [zoneCount, nvCaps(2), extCaps(2)]
    let r = dev.call(f, 0x00, &[])?;
    let zone_count = r[0];
    let mut info = RgbInfo {
        feature: f,
        nv_caps: be_u16(&r[1..]),
        ext_caps: be_u16(&r[3..]),
        zones: Vec::new(),
    };
    for z in 0..zone_count {
        // getZoneInfo(zone) -> [zone, location(2), numEffects, persistencyCaps]
        let r = dev.call(f, 0x01, &[z])?;
        let mut zone = Zone {
            index: z,
            location: be_u16(&r[1..]),
            persistency_caps: r[4],
            effects: Vec::new(),
        };
        for s in 0..r[3] {
            // getZoneEffectInfo(zone, slot) -> [zone, slot, effectId(2), caps(2), period(2)]
            let e = dev.call(f, 0x02, &[z, s])?;
            zone.effects.push(ZoneEffectSlot {
                slot: s,
                effect_id: be_u16(&e[2..]),
                caps: be_u16(&e[4..]),
                period: be_u16(&e[6..]),
            });
        }
        info.zones.push(zone);
    }
    Ok(info)
}

fn read_8071(dev: &Device) -> Result<RgbInfo> {
    let f = FEAT_RGB_EFFECTS;
    // getInfo(cluster=0xFF, effect=0xFF, typeOfInfo=0) -> [0xFF, 0xFF, clusterCount, nvCaps(2), extCaps(2)]
    let r = dev.call(f, 0x00, &[0xFF, 0xFF, 0x00])?;
    let cluster_count = r[2];
    let mut info = RgbInfo {
        feature: f,
        nv_caps: be_u16(&r[3..]),
        ext_caps: be_u16(&r[5..]),
        zones: Vec::new(),
    };
    for c in 0..cluster_count {
        // getInfo(cluster, 0xFF, 0) -> [cluster, 0xFF, location(2), numEffects, persistencyCaps]
        let r = dev.call(f, 0x00, &[c, 0xFF, 0x00])?;
        let mut zone = Zone {
            index: c,
            location: be_u16(&r[2..]),
            persistency_caps: r[5],
            effects: Vec::new(),
        };
        for s in 0..r[4] {
            // getInfo(cluster, slot, 0) -> [cluster, slot, effectId(2), caps(2), period(2)]
            let e = dev.call(f, 0x00, &[c, s, 0x00])?;
            zone.effects.push(ZoneEffectSlot {
                slot: s,
                effect_id: be_u16(&e[2..]),
                caps: be_u16(&e[4..]),
                period: be_u16(&e[6..]),
            });
        }
        info.zones.push(zone);
    }
    Ok(info)
}

/// Apply `effect` to one zone. `persist` additionally stores it in the device's flash.
pub fn set(
    dev: &Device,
    info: &RgbInfo,
    zone_index: u8,
    effect: &Effect,
    persist: bool,
) -> Result<()> {
    let zone = info
        .zones
        .iter()
        .find(|z| z.index == zone_index)
        .ok_or_else(|| anyhow::anyhow!("zone {zone_index} not found"))?;
    let slot = match zone.slot_for(effect.id()) {
        Some(s) => s,
        None => bail!(
            "{} does not support {}",
            zone.name(),
            effect_name(effect.id())
        ),
    };
    let mut params = [0u8; 13];
    params[0] = zone_index;
    params[1] = slot;
    params[2..12].copy_from_slice(&effect.params());
    params[12] = if persist { 0x01 } else { 0x00 };
    let function = match info.feature {
        FEAT_COLOR_LED_EFFECTS => 0x03, // setZoneEffect
        FEAT_RGB_EFFECTS => {
            ensure_sw_control(dev, info)?;
            0x01 // setRgbClusterEffect
        }
        _ => bail!("no RGB feature"),
    };
    dev.call(info.feature, function, &params)?;
    Ok(())
}

/// 0x8071 rejects effect writes (LogitechInternal) while the onboard profile owns the LEDs.
/// Claim software control (mode 3, flags 0x04 = firmware keeps handling idle power saving).
pub fn ensure_sw_control(dev: &Device, info: &RgbInfo) -> Result<()> {
    if info.feature != FEAT_RGB_EFFECTS {
        return Ok(());
    }
    if let Some((mode, _)) = sw_control(dev, info)?
        && mode == 0
    {
        dev.call(info.feature, 0x05, &[0x01, 0x03, 0x04])?;
    }
    Ok(())
}

/// Raw readback of the current zone effect (0x8070 only, when the device supports it).
pub fn get_zone_effect_raw(
    dev: &Device,
    info: &RgbInfo,
    zone_index: u8,
) -> Result<Option<[u8; 16]>> {
    if info.feature != FEAT_COLOR_LED_EFFECTS || info.ext_caps & 0x01 == 0 {
        return Ok(None);
    }
    Ok(Some(dev.call(info.feature, 0x0E, &[zone_index])?))
}

/// Current effect of a zone as a user-facing `Effect` (0x8070 readback only).
pub fn get_zone_effect(dev: &Device, info: &RgbInfo, zone: &Zone) -> Result<Option<Effect>> {
    let Some(raw) = get_zone_effect_raw(dev, info, zone.index)? else {
        return Ok(None);
    };
    let slot = raw[1];
    let id = zone
        .effects
        .iter()
        .find(|e| e.slot == slot)
        .map(|e| e.effect_id)
        .unwrap_or(slot as u16);
    let p = &raw[2..12];
    Ok(Some(match id {
        EFFECT_OFF => Effect::Off,
        EFFECT_FIXED => Effect::Fixed {
            color: Rgb(p[0], p[1], p[2]),
        },
        EFFECT_BREATHING => Effect::Breathing {
            color: Rgb(p[0], p[1], p[2]),
            period_ms: be_u16(&p[3..]),
            brightness: if p[6] == 0 { 100 } else { p[6] },
        },
        EFFECT_CYCLE => Effect::Cycle {
            period_ms: be_u16(&p[5..]),
            brightness: if p[7] == 0 { 100 } else { p[7] },
        },
        other => Effect::Other {
            id: other,
            params: p.to_vec(),
        },
    }))
}

/// 0x8071 only: who drives the LEDs. Returns (mode, flags); mode 0 = device/onboard, 3 = software.
pub fn sw_control(dev: &Device, info: &RgbInfo) -> Result<Option<(u8, u8)>> {
    if info.feature != FEAT_RGB_EFFECTS {
        return Ok(None);
    }
    // manageSwControl(get)
    let r = dev.call(info.feature, 0x05, &[0x00])?;
    Ok(Some((r[1], r[2])))
}

/// 0x8071 only: hand LED control back to the device's onboard profile.
pub fn release_sw_control(dev: &Device, info: &RgbInfo) -> Result<()> {
    if info.feature == FEAT_RGB_EFFECTS {
        dev.call(info.feature, 0x05, &[0x01, 0x00, 0x00])?;
    }
    Ok(())
}
