//! 0x2201 AdjustableDPI and 0x2202 ExtendedAdjustableDPI.

use crate::device::Device;
use crate::hidpp::be_u16;
use anyhow::{Result, bail};

pub const FEAT_ADJUSTABLE_DPI: u16 = 0x2201;
pub const FEAT_EXTENDED_ADJUSTABLE_DPI: u16 = 0x2202;

#[derive(Debug, Clone)]
pub struct DpiInfo {
    pub feature: u16,
    pub sensor: u8,
    /// Every DPI value the sensor accepts (expanded from ranges).
    pub supported: Vec<u16>,
    pub current: u16,
    pub default: u16,
    /// 0x2202 only
    pub current_y: Option<u16>,
    pub lod: Option<u8>,
}

impl DpiInfo {
    pub fn min(&self) -> u16 {
        self.supported.first().copied().unwrap_or(100)
    }
    pub fn max(&self) -> u16 {
        self.supported.last().copied().unwrap_or(25600)
    }
    /// Closest supported value to `want`.
    pub fn snap(&self, want: u16) -> u16 {
        self.supported
            .iter()
            .copied()
            .min_by_key(|v| (*v as i32 - want as i32).abs())
            .unwrap_or(want)
    }
}

/// Decode a HID++ DPI list: u16 values, where 0xE000|step introduces a range
/// from the previous value to the next value in `step` increments. Terminated by 0.
fn decode_dpi_list(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let v = be_u16(&bytes[i..]);
        if v == 0 {
            break;
        }
        if v >> 13 == 0b111 {
            let step = (v & 0x1FFF).max(1);
            if i + 3 >= bytes.len() {
                break;
            }
            let last = be_u16(&bytes[i + 2..]);
            if let Some(&prev) = out.last() {
                let mut x = prev + step;
                while x <= last {
                    out.push(x);
                    x += step;
                }
            }
            i += 4;
        } else {
            out.push(v);
            i += 2;
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

pub fn read(dev: &Device) -> Result<Option<DpiInfo>> {
    if dev.has(FEAT_EXTENDED_ADJUSTABLE_DPI) {
        return read_2202(dev).map(Some);
    }
    if dev.has(FEAT_ADJUSTABLE_DPI) {
        return read_2201(dev).map(Some);
    }
    Ok(None)
}

fn read_2201(dev: &Device) -> Result<DpiInfo> {
    let f = FEAT_ADJUSTABLE_DPI;
    let sensor = 0u8;
    // getSensorDpiList(sensor) -> [sensor, list...]
    let r = dev.call(f, 0x01, &[sensor])?;
    let supported = decode_dpi_list(&r[1..]);
    // getSensorDpi(sensor) -> [sensor, dpi(2), default(2)]
    let r = dev.call(f, 0x02, &[sensor])?;
    Ok(DpiInfo {
        feature: f,
        sensor,
        supported,
        current: be_u16(&r[1..]),
        default: be_u16(&r[3..]),
        current_y: None,
        lod: None,
    })
}

fn read_ranges_2202(dev: &Device, sensor: u8, direction: u8) -> Result<Vec<u16>> {
    let mut bytes = Vec::new();
    for i in 0..16u8 {
        // getSensorDpiRanges(sensor, direction, index) -> [sensor, direction, index, data...]
        let r = dev.call(FEAT_EXTENDED_ADJUSTABLE_DPI, 0x02, &[sensor, direction, i])?;
        bytes.extend_from_slice(&r[3..]);
        if bytes.len() >= 2 && bytes[bytes.len() - 2..] == [0, 0] {
            break;
        }
    }
    Ok(decode_dpi_list(&bytes))
}

fn read_2202(dev: &Device) -> Result<DpiInfo> {
    let f = FEAT_EXTENDED_ADJUSTABLE_DPI;
    let sensor = 0u8;
    // getSensorCapabilities(sensor) -> [sensor, numDpiLevels, caps]
    let caps = dev.call(f, 0x01, &[sensor])?;
    let has_y = caps[2] & 0x01 != 0;
    let has_lod = caps[2] & 0x02 != 0;
    let supported = read_ranges_2202(dev, sensor, 0)?;
    // getSensorDpiParameters(sensor) -> [sensor, dpiX(2), defX(2), dpiY(2), defY(2), lod]
    let r = dev.call(f, 0x05, &[sensor])?;
    Ok(DpiInfo {
        feature: f,
        sensor,
        supported,
        current: be_u16(&r[1..]),
        default: be_u16(&r[3..]),
        current_y: if has_y { Some(be_u16(&r[5..])) } else { None },
        lod: if has_lod { Some(r[9]) } else { None },
    })
}

pub fn set(dev: &Device, info: &DpiInfo, dpi: u16) -> Result<()> {
    if !info.supported.is_empty() && !info.supported.contains(&dpi) {
        bail!(
            "DPI {dpi} not supported (range {}..{})",
            info.min(),
            info.max()
        );
    }
    let d = dpi.to_be_bytes();
    match info.feature {
        FEAT_ADJUSTABLE_DPI => {
            // setSensorDpi(sensor, dpi)
            dev.call(info.feature, 0x03, &[info.sensor, d[0], d[1]])?;
        }
        FEAT_EXTENDED_ADJUSTABLE_DPI => {
            // setSensorDpiParameters(sensor, dpiX, dpiY, lod)
            let y = info.current_y.map(|_| dpi).unwrap_or(0).to_be_bytes();
            let lod = info.lod.unwrap_or(0);
            dev.call(
                info.feature,
                0x06,
                &[info.sensor, d[0], d[1], y[0], y[1], lod],
            )?;
        }
        _ => bail!("no DPI feature"),
    }
    Ok(())
}
