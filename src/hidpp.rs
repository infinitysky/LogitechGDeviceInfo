//! Minimal Logitech HID++ 2.0 transport over a raw HID "long report" collection.
//!
//! Report layout (long, report id 0x11, 20 bytes total):
//!   [0x11, device_index, feature_index, (function << 4) | sw_id, params[16]]

use anyhow::{Result, anyhow, bail};
use hidapi::{HidApi, HidDevice};
use std::ffi::CStr;
use std::fmt;
use std::time::{Duration, Instant};

pub const VENDOR_LOGITECH: u16 = 0x046D;
/// Classic HID++ vendor page (receivers, most mice).
pub const USAGE_PAGE_HIDPP: u16 = 0xFF00;
/// Alternate vendor page used by the gaming line (e.g. G512: usages 0x0602 / 0x0604).
pub const USAGE_PAGE_HIDPP_GAMING: u16 = 0xFF43;
/// Low byte of the collection usage: 1 = short (7 B), 2 = long (20 B), 4 = very long (64 B).
pub const USAGE_SHORT: u16 = 0x0001;
pub const USAGE_LONG: u16 = 0x0002;

pub fn is_hidpp_page(usage_page: u16) -> bool {
    usage_page == USAGE_PAGE_HIDPP || usage_page == USAGE_PAGE_HIDPP_GAMING
}

pub const REPORT_SHORT: u8 = 0x10;
pub const REPORT_LONG: u8 = 0x11;
pub const REPORT_VERY_LONG: u8 = 0x20;

pub const DEVICE_IDX_WIRED: u8 = 0xFF;

/// Software id placed in the low nibble of byte 3; used to match responses.
pub const SW_ID: u8 = 0x09;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hidpp2Error(pub u8);

impl Hidpp2Error {
    pub fn name(self) -> &'static str {
        match self.0 {
            0 => "NoError",
            1 => "Unknown",
            2 => "InvalidArgument",
            3 => "OutOfRange",
            4 => "HardwareError",
            5 => "LogitechInternal",
            6 => "InvalidFeatureIndex",
            7 => "InvalidFunctionId",
            8 => "Busy",
            9 => "Unsupported",
            _ => "?",
        }
    }
}

impl fmt::Display for Hidpp2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HID++2.0 error 0x{:02X} ({})", self.0, self.name())
    }
}
impl std::error::Error for Hidpp2Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hidpp1Error(pub u8);

impl fmt::Display for Hidpp1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            0x01 => "InvalidSubId",
            0x02 => "InvalidAddress",
            0x03 => "InvalidValue",
            0x04 => "ConnectFail",
            0x05 => "TooManyDevices",
            0x06 => "AlreadyExists",
            0x07 => "Busy",
            0x08 => "UnknownDevice",
            0x09 => "ResourceError",
            0x0A => "RequestUnavailable",
            0x0B => "InvalidParamValue",
            0x0C => "WrongPinCode",
            _ => "?",
        };
        write!(f, "HID++1.0 error 0x{:02X} ({})", self.0, name)
    }
}
impl std::error::Error for Hidpp1Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeout;
impl fmt::Display for Timeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HID++ request timed out")
    }
}
impl std::error::Error for Timeout {}

/// One opened HID++ collection (one physical USB interface: a wired device or a receiver).
pub struct Transport {
    dev: HidDevice,
    pub product_id: u16,
    pub product_string: String,
}

impl Transport {
    pub fn open(api: &HidApi, path: &CStr, product_id: u16, product: &str) -> Result<Self> {
        let dev = api.open_path(path)?;
        Ok(Self {
            dev,
            product_id,
            product_string: product.to_owned(),
        })
    }

    /// Send a long HID++ request and wait for the matching response, retrying once on
    /// timeout or a transient `Busy` error. Returns the 16 parameter bytes of the response.
    pub fn request(
        &self,
        device_index: u8,
        feature_index: u8,
        function: u8,
        params: &[u8],
        timeout: Duration,
    ) -> Result<[u8; 16]> {
        match self.request_once(device_index, feature_index, function, params, timeout) {
            Err(e)
                if is_timeout(&e) || e.downcast_ref::<Hidpp2Error>() == Some(&Hidpp2Error(8)) =>
            {
                std::thread::sleep(Duration::from_millis(30));
                self.request_once(device_index, feature_index, function, params, timeout)
            }
            other => other,
        }
    }

    /// Single attempt without retry (used for presence probes where a timeout is the expected
    /// answer for an empty receiver slot).
    pub fn request_once(
        &self,
        device_index: u8,
        feature_index: u8,
        function: u8,
        params: &[u8],
        timeout: Duration,
    ) -> Result<[u8; 16]> {
        if params.len() > 16 {
            bail!("HID++ params too long ({} > 16)", params.len());
        }
        let func_sw = (function << 4) | (SW_ID & 0x0F);
        let mut out = [0u8; 20];
        out[0] = REPORT_LONG;
        out[1] = device_index;
        out[2] = feature_index;
        out[3] = func_sw;
        out[4..4 + params.len()].copy_from_slice(params);

        self.dev
            .write(&out)
            .map_err(|e| anyhow!("HID write failed: {e}"))?;

        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(Timeout.into());
            }
            let remaining = (deadline - now).as_millis().max(1) as i32;
            let n = self
                .dev
                .read_timeout(&mut buf, remaining)
                .map_err(|e| anyhow!("HID read failed: {e}"))?;
            if n == 0 {
                continue;
            }
            let r = &buf[..n];
            match r[0] {
                REPORT_LONG | REPORT_VERY_LONG if n >= 7 => {
                    if r[1] != device_index {
                        continue;
                    }
                    // HID++ 2.0 error: feature index 0xFF, then echoed feature index + func/sw.
                    if r[2] == 0xFF && r[3] == feature_index && r[4] == func_sw {
                        return Err(Hidpp2Error(r[5]).into());
                    }
                    if r[2] == feature_index && r[3] == func_sw {
                        let mut p = [0u8; 16];
                        let len = (n - 4).min(16);
                        p[..len].copy_from_slice(&r[4..4 + len]);
                        return Ok(p);
                    }
                    // Otherwise a notification or stale response: ignore.
                }
                REPORT_SHORT
                    if n >= 6
                    // HID++ 1.0 error from a receiver: [0x10, idx, 0x8F, sub_id, address, err]
                    && r[1] == device_index && r[2] == 0x8F && r[3] == feature_index && r[4] == func_sw =>
                {
                    return Err(Hidpp1Error(r[5]).into());
                }
                _ => {}
            }
        }
    }
}

pub fn is_timeout(e: &anyhow::Error) -> bool {
    e.downcast_ref::<Timeout>().is_some()
}

pub fn be_u16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
