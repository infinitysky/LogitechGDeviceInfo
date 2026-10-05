//! Device discovery (wired devices + receiver slots) and the HID++ 2.0 root/feature-set layer.

use crate::hidpp::{self, Transport};
use anyhow::{Result, anyhow, bail};
use hidapi::HidApi;
use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

pub const FEAT_ROOT: u16 = 0x0000;
pub const FEAT_FEATURE_SET: u16 = 0x0001;
pub const FEAT_DEVICE_NAME: u16 = 0x0005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Other,
}

pub struct Device {
    transport: Rc<Transport>,
    pub index: u8,
    pub name: String,
    pub kind: DeviceKind,
    pub protocol: (u8, u8),
    /// feature id -> (feature index, feature type flags)
    pub features: BTreeMap<u16, (u8, u8)>,
}

impl Device {
    /// Stable key used for config persistence.
    pub fn key(&self) -> String {
        self.name.clone()
    }

    pub fn is_wireless(&self) -> bool {
        self.index != hidpp::DEVICE_IDX_WIRED
    }

    pub fn has(&self, feature: u16) -> bool {
        self.features.contains_key(&feature)
    }

    pub fn feature_index(&self, feature: u16) -> Option<u8> {
        self.features.get(&feature).map(|f| f.0)
    }

    /// Call `function` of `feature` (by feature id) with `params`.
    pub fn call(&self, feature: u16, function: u8, params: &[u8]) -> Result<[u8; 16]> {
        let idx = self
            .feature_index(feature)
            .ok_or_else(|| anyhow!("feature 0x{feature:04X} not supported by {}", self.name))?;
        self.transport
            .request(self.index, idx, function, params, hidpp::DEFAULT_TIMEOUT)
    }

    pub fn describe(&self) -> String {
        let link = if self.is_wireless() {
            format!(
                "receiver {} slot {}",
                self.transport.product_string, self.index
            )
        } else {
            "wired".to_owned()
        };
        format!(
            "{} [{:?}] HID++ {}.{} via {} (PID {:04X})",
            self.name, self.kind, self.protocol.0, self.protocol.1, link, self.transport.product_id
        )
    }
}

fn probe_protocol(t: &Transport, index: u8, timeout: Duration) -> Result<(u8, u8)> {
    // IRoot.getProtocolVersion(ping byte); no retry: a timeout means "nothing there".
    let r = t.request_once(index, 0x00, 0x01, &[0, 0, 0x5A], timeout)?;
    if r[2] != 0x5A {
        bail!("bad ping echo");
    }
    Ok((r[0], r[1]))
}

fn build_device(t: &Rc<Transport>, index: u8, protocol: (u8, u8)) -> Result<Device> {
    let to = hidpp::DEFAULT_TIMEOUT;
    // IRoot.getFeature(0x0001) -> feature set index
    let r = t.request(index, 0x00, 0x00, &FEAT_FEATURE_SET.to_be_bytes(), to)?;
    let fs_idx = r[0];
    if fs_idx == 0 {
        bail!("device has no IFeatureSet");
    }
    let count = t.request(index, fs_idx, 0x00, &[], to)?[0];
    let mut features = BTreeMap::new();
    features.insert(FEAT_ROOT, (0u8, 0u8));
    for i in 1..=count {
        let r = t.request(index, fs_idx, 0x01, &[i], to)?;
        let id = hidpp::be_u16(&r[0..2]);
        features.insert(id, (i, r[2]));
    }

    let mut dev = Device {
        transport: Rc::clone(t),
        index,
        name: String::new(),
        kind: DeviceKind::Other,
        protocol,
        features,
    };

    // 0x0005 DeviceNameAndType
    if dev.has(FEAT_DEVICE_NAME) {
        let len = dev.call(FEAT_DEVICE_NAME, 0x00, &[])?[0] as usize;
        let mut name = Vec::with_capacity(len);
        while name.len() < len {
            let chunk = dev.call(FEAT_DEVICE_NAME, 0x01, &[name.len() as u8])?;
            let take = (len - name.len()).min(16);
            name.extend_from_slice(&chunk[..take]);
            if take == 0 {
                break;
            }
        }
        dev.name = String::from_utf8_lossy(&name)
            .trim_end_matches('\0')
            .trim()
            .to_owned();
        let ty = dev.call(FEAT_DEVICE_NAME, 0x02, &[])?[0];
        dev.kind = match ty {
            0 => DeviceKind::Keyboard,
            3 => DeviceKind::Mouse,
            _ => DeviceKind::Other,
        };
    }
    if dev.name.is_empty() {
        dev.name = if t.product_string.is_empty() {
            format!("Logitech {:04X}", t.product_id)
        } else {
            t.product_string.clone()
        };
    }
    Ok(dev)
}

/// Enumerate every HID++ long-report collection of Logitech devices and return the
/// HID++ 2.0 devices found behind them (wired or paired to a receiver).
pub fn discover(log: &mut dyn FnMut(String)) -> Result<Vec<Device>> {
    let api = HidApi::new()?;
    let mut seen_paths = HashSet::new();
    let mut devices = Vec::new();

    let mut candidates: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == hidpp::VENDOR_LOGITECH && hidpp::is_hidpp_page(d.usage_page()))
        .collect();
    // Prefer the long-report collection; fall back to short-only collections.
    candidates.sort_by_key(|d| {
        if d.usage() & 0xFF == hidpp::USAGE_LONG {
            0
        } else {
            1
        }
    });

    // One transport per (interface serial/PID); several collections of the same interface
    // share the same physical endpoint, so only open the first (long) one.
    let mut opened_ifaces: HashSet<String> = HashSet::new();

    for info in candidates {
        let usage = info.usage() & 0xFF;
        if usage != hidpp::USAGE_LONG && usage != hidpp::USAGE_SHORT {
            continue;
        }
        let path = info.path();
        if !seen_paths.insert(path.to_owned()) {
            continue;
        }
        let iface_key = format!(
            "{:04X}:{}:{}",
            info.product_id(),
            info.serial_number().unwrap_or(""),
            info.interface_number()
        );
        if !opened_ifaces.insert(iface_key) {
            continue;
        }
        let product = info.product_string().unwrap_or("").to_owned();
        let t = match Transport::open(&api, path, info.product_id(), &product) {
            Ok(t) => Rc::new(t),
            Err(e) => {
                log(format!("open {:04X} failed: {e}", info.product_id()));
                continue;
            }
        };
        log(format!(
            "opened PID {:04X} usage {} \"{}\"",
            info.product_id(),
            info.usage(),
            product
        ));

        // Wired device answers at 0xFF; a receiver answers 0xFF with a HID++1.0 error
        // (which may arrive on the short collection and thus be invisible -> timeout).
        let probe_to = Duration::from_millis(300);
        match probe_protocol(&t, hidpp::DEVICE_IDX_WIRED, probe_to) {
            Ok(proto) => match build_device(&t, hidpp::DEVICE_IDX_WIRED, proto) {
                Ok(d) => {
                    log(format!("  found {}", d.describe()));
                    devices.push(d);
                }
                Err(e) => log(format!("  wired device init failed: {e}")),
            },
            Err(e) => {
                log(format!("  idx FF: {e} -> treating as receiver"));
                let mut found_any = false;
                // Quick pass over all slots; a sleeping wireless device may need longer to
                // answer, so if nothing turned up re-probe the first two slots patiently.
                let passes: [(std::ops::RangeInclusive<u8>, Duration); 2] =
                    [(1..=6, probe_to), (1..=2, Duration::from_millis(1200))];
                for (slots, to) in passes {
                    if found_any {
                        break;
                    }
                    for idx in slots {
                        match probe_protocol(&t, idx, to) {
                            Ok(proto) => match build_device(&t, idx, proto) {
                                Ok(d) => {
                                    log(format!("  found {}", d.describe()));
                                    devices.push(d);
                                    found_any = true;
                                }
                                Err(e) => log(format!("  slot {idx} init failed: {e}")),
                            },
                            Err(e) if hidpp::is_timeout(&e) => {}
                            Err(e) => log(format!("  slot {idx}: {e}")),
                        }
                    }
                }
            }
        }
    }
    Ok(devices)
}
