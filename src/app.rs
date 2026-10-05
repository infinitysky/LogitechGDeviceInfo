//! Tray application state, menu construction and action handling.

use crate::config::{Config, TrayDisplay, TrayMetric};
use crate::device::{self, Device, DeviceKind};
use crate::features::dpi::{self, DpiInfo};
use crate::features::misc::{self, Battery, Brightness, OnboardMode, ReportRate};
use crate::features::rgb::{self, Effect, RgbInfo};
use crate::i18n::{self, t};
use crate::{trayicon, winutil};
use anyhow::{Result, anyhow};
use std::collections::{BTreeMap, HashMap};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub const APP_NAME: &str = "LogiTray";

const DEFAULT_PERIOD_MS: u16 = 3000;
const PERIODS: [(&str, u16); 3] = [("Slow", 6000), ("Medium", 3000), ("Fast", 1200)];
const BRIGHTNESS_STEPS: [u8; 4] = [25, 50, 75, 100];

pub struct Entry {
    pub dev: Device,
    pub dpi: Option<DpiInfo>,
    pub rgb: Option<RgbInfo>,
    pub battery: Option<Battery>,
    pub onboard: Option<OnboardMode>,
    pub rate: Option<ReportRate>,
    pub brightness: Option<Brightness>,
    /// 0x8071: (mode, flags) from manageSwControl.
    pub sw_control: Option<(u8, u8)>,
    /// Current effect per zone: device readback (0x8070) or last applied value.
    pub zone_effects: BTreeMap<u8, Effect>,
    /// Raw parameters observed for device-native effects (effect id -> 10 bytes),
    /// so switching back to e.g. "wave" reproduces the device's own configuration.
    pub other_params: HashMap<u16, Vec<u8>>,
    pub lost: bool,
}

impl Entry {
    fn new(dev: Device, cfg: &Config, log: &mut dyn FnMut(String)) -> Entry {
        let mut e = Entry {
            dev,
            dpi: None,
            rgb: None,
            battery: None,
            onboard: None,
            rate: None,
            brightness: None,
            sw_control: None,
            zone_effects: BTreeMap::new(),
            other_params: HashMap::new(),
            lost: false,
        };
        let name = e.dev.name.clone();
        let mut warn = |what: &str, r: Result<()>| {
            if let Err(err) = r {
                log(format!("{name}: {what}: {err}"));
            }
        };
        warn("dpi", dpi::read(&e.dev).map(|v| e.dpi = v));
        warn("rgb", rgb::read(&e.dev).map(|v| e.rgb = v));
        warn("battery", misc::battery(&e.dev).map(|v| e.battery = v));
        warn("onboard", misc::onboard_mode(&e.dev).map(|v| e.onboard = v));
        warn("report rate", misc::report_rate(&e.dev).map(|v| e.rate = v));
        warn(
            "brightness",
            misc::brightness(&e.dev).map(|v| e.brightness = v),
        );
        if let Some(info) = e.rgb.clone() {
            if let Ok(v) = rgb::sw_control(&e.dev, &info) {
                e.sw_control = v;
            }
            // Seed zone state from the config, then prefer a live readback where available.
            if let Some(saved) = cfg.devices.get(&e.dev.key()) {
                e.zone_effects = saved.zones.clone();
                for fx in saved.zones.values() {
                    e.remember_params(fx);
                }
            }
            for z in &info.zones {
                if let Ok(Some(fx)) = rgb::get_zone_effect(&e.dev, &info, z) {
                    e.remember_params(&fx);
                    e.zone_effects.insert(z.index, fx);
                }
            }
        }
        e
    }

    fn remember_params(&mut self, fx: &Effect) {
        if let Effect::Other { id, params } = fx
            && !params.is_empty()
        {
            self.other_params.insert(*id, params.clone());
        }
    }

    fn label(&self) -> String {
        let prefix = match self.dev.kind {
            DeviceKind::Keyboard => t("Keyboard"),
            DeviceKind::Mouse => t("Mouse"),
            DeviceKind::Other => t("Device"),
        };
        format!("{prefix}: {}", self.dev.name)
    }

    /// Short status for the tooltip.
    fn status_line(&self) -> String {
        let mut parts = vec![self.dev.name.clone()];
        if let Some(d) = &self.dpi {
            parts.push(format!("DPI {}", d.current));
        }
        if let Some(b) = &self.battery
            && let Some(p) = b.percent
        {
            parts.push(format!("{}%{}", p, if b.charging { "⚡" } else { "" }));
        }
        if let Some(fx) = self.zone_effects.values().next() {
            parts.push(i18n::effect_label(fx));
        }
        parts.join(" · ")
    }

    /// Re-read volatile state. Returns Err if the device stopped responding.
    fn refresh(&mut self) -> Result<()> {
        if let Some(d) = &mut self.dpi
            && let Some(fresh) = dpi::read(&self.dev)?
        {
            *d = fresh;
        }
        self.battery = misc::battery(&self.dev)?;
        self.onboard = misc::onboard_mode(&self.dev)?;
        if let Some(info) = &self.rgb {
            self.sw_control = rgb::sw_control(&self.dev, info)?;
            let mut fresh = Vec::new();
            for z in &info.zones {
                if let Some(fx) = rgb::get_zone_effect(&self.dev, info, z)? {
                    fresh.push((z.index, fx));
                }
            }
            for (zi, fx) in fresh {
                self.remember_params(&fx);
                self.zone_effects.insert(zi, fx);
            }
        }
        Ok(())
    }

    /// Re-read only the value shown in the tray icon (cheap: one or two HID++ requests).
    fn refresh_metric(&mut self, metric: TrayMetric) -> Result<()> {
        match metric {
            TrayMetric::Dpi => {
                if let Some(d) = &mut self.dpi
                    && let Some(fresh) = dpi::read(&self.dev)?
                {
                    *d = fresh;
                }
            }
            TrayMetric::Battery => self.battery = misc::battery(&self.dev)?,
            TrayMetric::Color => {
                if let Some(info) = &self.rgb
                    && let Some(z) = info.zones.first()
                    && let Some(fx) = rgb::get_zone_effect(&self.dev, info, z)?
                {
                    let zi = z.index;
                    self.remember_params(&fx);
                    self.zone_effects.insert(zi, fx);
                }
            }
        }
        Ok(())
    }
}

/// Interval of the lightweight timer that keeps the tray badge current.
pub const BADGE_REFRESH_MS: u32 = 5_000;

#[derive(Clone, Debug)]
enum Action {
    SetDpi {
        dev: usize,
        dpi: u16,
    },
    DpiStep {
        dev: usize,
        delta: i32,
    },
    SetEffect {
        dev: usize,
        zone: u8,
        effect: Effect,
    },
    SetPeriod {
        dev: usize,
        zone: u8,
        period_ms: u16,
    },
    SetZoneBrightness {
        dev: usize,
        zone: u8,
        pct: u8,
    },
    ReleaseSw {
        dev: usize,
    },
    SetOnboard {
        dev: usize,
        mode: OnboardMode,
    },
    SetRate {
        dev: usize,
        ms: u8,
    },
    SetBrightness {
        dev: usize,
        value: u16,
    },
    SetTrayDisplay(Option<TrayDisplay>),
    TogglePersist,
    ToggleRestore,
    ToggleAutostart,
    OpenConfig,
    Refresh,
    Rescan,
    About,
    Quit,
}

pub struct App {
    pub cfg: Config,
    entries: Vec<Entry>,
    tray: TrayIcon,
    actions: HashMap<MenuId, Action>,
    log: Vec<String>,
    needs_rescan: bool,
    /// Logitech G HUB (or its agent) is running and owns the devices.
    ghub: bool,
    /// One-shot dialog the main loop should show.
    ghub_notice: bool,
}

impl App {
    pub fn new(cfg: Config) -> Result<App> {
        let tray = TrayIconBuilder::new()
            .with_icon(trayicon::default_icon())
            .with_tooltip(APP_NAME)
            .with_menu_on_left_click(true)
            .build()?;
        let mut app = App {
            cfg,
            entries: Vec::new(),
            tray,
            actions: HashMap::new(),
            log: Vec::new(),
            needs_rescan: false,
            ghub: false,
            ghub_notice: false,
        };
        app.rescan();
        Ok(app)
    }

    /// Track G HUB. Logs in English. Returns true when the state changed.
    fn note_ghub(&mut self) -> bool {
        let now = winutil::ghub_running();
        if now == self.ghub {
            return false;
        }
        self.ghub = now;
        if now {
            self.ghub_notice = true;
            self.log(
                "G HUB is running; device configuration disabled (battery and tray icon stay available)"
                    .into(),
            );
        } else {
            self.log("G HUB is no longer running; device configuration enabled".into());
        }
        true
    }

    /// Localized notice, once each time G HUB starts (including when this app starts after it).
    pub fn take_ghub_notice(&mut self) -> Option<String> {
        if self.ghub_notice {
            self.ghub_notice = false;
            Some(t("G HUB is running and has taken over the devices, so settings are locked. You can still view battery and change the tray icon.").into())
        } else {
            None
        }
    }

    fn log(&mut self, s: String) {
        log_line(&s);
        self.log.push(s);
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    pub fn refresh_interval_ms(&self) -> u32 {
        (self.cfg.refresh_secs.clamp(10, 3600) * 1000) as u32
    }

    // ---------------------------------------------------------------- devices

    pub fn rescan(&mut self) {
        self.entries.clear(); // drop HID handles before re-enumerating
        let mut lines = Vec::new();
        let devices = match device::discover(&mut |s| lines.push(s)) {
            Ok(d) => d,
            Err(e) => {
                lines.push(format!("discover failed: {e}"));
                Vec::new()
            }
        };
        for l in lines {
            self.log(l);
        }
        let mut init_log = Vec::new();
        let cfg = &self.cfg;
        self.entries = devices
            .into_iter()
            .map(|d| Entry::new(d, cfg, &mut |s| init_log.push(s)))
            .collect();
        for l in init_log {
            self.log(l);
        }
        self.needs_rescan = false;
        self.note_ghub();
        // Don't push saved settings while G HUB owns the devices.
        if self.cfg.restore_on_start && !self.ghub {
            self.restore_saved();
        }
        self.rebuild_menu();
    }

    /// Re-apply the last saved settings to every present device.
    fn restore_saved(&mut self) {
        let mut errors = Vec::new();
        for i in 0..self.entries.len() {
            let key = self.entries[i].dev.key();
            let Some(saved) = self.cfg.devices.get(&key).cloned() else {
                continue;
            };
            let e = &mut self.entries[i];
            if let (Some(want), Some(info)) = (saved.dpi, &e.dpi)
                && info.current != want
            {
                match dpi::set(&e.dev, info, info.snap(want)) {
                    Ok(()) => {
                        if let Ok(Some(fresh)) = dpi::read(&e.dev) {
                            e.dpi = Some(fresh);
                        }
                    }
                    Err(err) => errors.push(format!("{key}: restore DPI: {err}")),
                }
            }
            if let (Some(ms), Some(rate)) = (saved.report_rate_ms, &e.rate)
                && rate.current_ms != ms
                && rate.supported.contains(&ms)
            {
                if let Err(err) = misc::set_report_rate(&e.dev, ms) {
                    errors.push(format!("{key}: restore report rate: {err}"));
                } else if let Ok(r) = misc::report_rate(&e.dev) {
                    e.rate = r;
                }
            }
            if let (Some(b), Some(cur)) = (saved.brightness, &e.brightness)
                && cur.current != b
            {
                if let Err(err) = misc::set_brightness(&e.dev, b) {
                    errors.push(format!("{key}: restore brightness: {err}"));
                } else if let Ok(v) = misc::brightness(&e.dev) {
                    e.brightness = v;
                }
            }
            if let Some(info) = &e.rgb {
                for (zone, fx) in &saved.zones {
                    if e.zone_effects.get(zone) == Some(fx)
                        && info.feature == rgb::FEAT_COLOR_LED_EFFECTS
                    {
                        continue; // readback says it's already there
                    }
                    match rgb::set(&e.dev, info, *zone, fx, self.cfg.persist_lighting) {
                        Ok(()) => {
                            e.zone_effects.insert(*zone, fx.clone());
                        }
                        Err(err) => {
                            errors.push(format!("{key}: restore lighting zone {zone}: {err}"))
                        }
                    }
                }
                if let Ok(v) = rgb::sw_control(&e.dev, info) {
                    e.sw_control = v;
                }
            }
        }
        for l in errors {
            self.log(l);
        }
    }

    /// Periodic tick: refresh state, rescan if something went missing.
    /// True when the tray icon shows a device metric (so it needs periodic refreshing).
    pub fn badge_active(&self) -> bool {
        self.cfg.tray_display.is_some()
    }

    /// Lightweight tick: re-read just the displayed metric and redraw the icon if it changed.
    pub fn on_badge_timer(&mut self) {
        let ghub_changed = self.note_ghub();
        let Some(td) = self.cfg.tray_display.clone() else {
            if ghub_changed {
                self.rebuild_menu();
            }
            return;
        };
        let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.dev.key() == td.device && !e.lost)
        else {
            if ghub_changed {
                self.rebuild_menu();
            }
            return;
        };
        let before = (
            e.dpi.as_ref().map(|d| d.current),
            e.battery.clone(),
            e.zone_effects.values().next().cloned(),
        );
        if let Err(err) = e.refresh_metric(td.metric) {
            e.lost = true;
            let name = e.dev.name.clone();
            self.log(format!("{name}: lost ({err})"));
            self.needs_rescan = true;
            let _ = self.tray.set_icon(Some(self.current_icon()));
            return;
        }
        let after = (
            e.dpi.as_ref().map(|d| d.current),
            e.battery.clone(),
            e.zone_effects.values().next().cloned(),
        );
        if before != after || ghub_changed {
            self.rebuild_menu();
        }
    }

    pub fn on_timer(&mut self) {
        self.note_ghub();
        if self.needs_rescan || self.entries.is_empty() {
            self.rescan();
            return;
        }
        let mut lost = Vec::new();
        for (i, e) in self.entries.iter_mut().enumerate() {
            if let Err(err) = e.refresh() {
                e.lost = true;
                lost.push(format!("{}: lost ({err})", e.dev.name));
                let _ = i;
            }
        }
        for l in lost {
            self.log(l);
            self.needs_rescan = true;
        }
        self.rebuild_menu();
    }

    // ------------------------------------------------------------------- menu

    pub fn rebuild_menu(&mut self) {
        let (menu, actions) = self.build_menu();
        self.actions = actions;
        self.tray.set_menu(Some(Box::new(menu)));
        let _ = self.tray.set_icon(Some(self.current_icon()));
        let mut tip = String::from(APP_NAME);
        if self.ghub {
            tip.push_str(" — ");
            tip.push_str(t("G HUB is running — settings locked"));
        }
        for e in &self.entries {
            tip.push('\n');
            tip.push_str(&e.status_line());
        }
        if tip.chars().count() > 120 {
            tip = tip.chars().take(119).collect::<String>() + "…";
        }
        let _ = self.tray.set_tooltip(Some(tip));
    }

    /// The icon reflecting `cfg.tray_display`, or the default glyph when nothing is selected
    /// or the chosen device/metric is unavailable.
    fn current_icon(&self) -> Icon {
        let Some(td) = &self.cfg.tray_display else {
            return trayicon::default_icon();
        };
        let Some(e) = self
            .entries
            .iter()
            .find(|e| e.dev.key() == td.device && !e.lost)
        else {
            return trayicon::default_icon();
        };
        match td.metric {
            TrayMetric::Dpi => match &e.dpi {
                Some(d) => {
                    let rows = trayicon::dpi_rows(d.current);
                    let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
                    trayicon::text_badge(&refs, None)
                }
                None => trayicon::default_icon(),
            },
            TrayMetric::Battery => match &e.battery {
                Some(b) => {
                    let pct = b.percent.unwrap_or(0);
                    let text = pct.to_string();
                    trayicon::text_badge(
                        &[&text],
                        Some((pct as f32 / 100.0, trayicon::battery_color(pct, b.charging))),
                    )
                }
                None => trayicon::default_icon(),
            },
            TrayMetric::Color => trayicon::color_badge(e.zone_effects.values().next()),
        }
    }

    /// Metrics a device can show in the tray icon.
    fn available_metrics(e: &Entry) -> Vec<TrayMetric> {
        let mut v = Vec::new();
        if e.dpi.is_some() {
            v.push(TrayMetric::Dpi);
        }
        if e.battery.is_some() {
            v.push(TrayMetric::Battery);
        }
        if e.rgb.is_some() {
            v.push(TrayMetric::Color);
        }
        v
    }

    fn build_menu(&self) -> (Menu, HashMap<MenuId, Action>) {
        let mut actions: HashMap<MenuId, Action> = HashMap::new();
        let menu = Menu::new();
        let palette = self.cfg.palette();
        // Device writes are locked while G HUB owns the hardware. Battery and the tray icon stay.
        let write = !self.ghub;
        if self.ghub {
            let _ = menu.append(&MenuItem::new(
                t("G HUB is running — settings locked"),
                false,
                None,
            ));
            let _ = menu.append(&PredefinedMenuItem::separator());
        }

        for (di, e) in self.entries.iter().enumerate() {
            let header = MenuItem::new(e.label(), false, None);
            let _ = menu.append(&header);
            if e.lost {
                let _ = menu.append(&MenuItem::new(
                    format!("   {}", t("(not responding, waiting to reconnect)")),
                    false,
                    None,
                ));
                let _ = menu.append(&PredefinedMenuItem::separator());
                continue;
            }

            // ---- DPI
            if let Some(info) = &e.dpi {
                let sub = Submenu::new(format!("DPI: {}", info.current), write);
                let mut values: Vec<u16> =
                    self.cfg.dpi_presets.iter().map(|v| info.snap(*v)).collect();
                values.push(info.current);
                values.sort_unstable();
                values.dedup();
                for v in values {
                    let it = CheckMenuItem::new(format!("{v}"), write, v == info.current, None);
                    actions.insert(it.id().clone(), Action::SetDpi { dev: di, dpi: v });
                    let _ = sub.append(&it);
                }
                let _ = sub.append(&PredefinedMenuItem::separator());
                let up = MenuItem::new("DPI +100", write && info.current < info.max(), None);
                actions.insert(
                    up.id().clone(),
                    Action::DpiStep {
                        dev: di,
                        delta: 100,
                    },
                );
                let down = MenuItem::new("DPI -100", write && info.current > info.min(), None);
                actions.insert(
                    down.id().clone(),
                    Action::DpiStep {
                        dev: di,
                        delta: -100,
                    },
                );
                let _ = sub.append_items(&[&up, &down]);
                let range = MenuItem::new(
                    format!(
                        "{} {}–{}  ({} {})",
                        t("Range"),
                        info.min(),
                        info.max(),
                        t("default"),
                        info.default
                    ),
                    false,
                    None,
                );
                let _ = sub.append(&range);
                let _ = menu.append(&sub);
            }

            // ---- Lighting
            if let Some(info) = &e.rgb {
                for z in &info.zones {
                    let current = e.zone_effects.get(&z.index);
                    let title = if info.zones.len() > 1 {
                        format!("{} - {}", t("Lighting"), i18n::location_name(z.location))
                    } else {
                        t("Lighting").to_owned()
                    };
                    let title = match current {
                        Some(fx) => format!("{title}: {}", i18n::effect_label(fx)),
                        None => title,
                    };
                    let sub = Submenu::new(title, write);
                    let period = self.zone_period(e, z.index);
                    let bright = self.zone_brightness(e, z.index);

                    if z.supports(rgb::EFFECT_OFF) {
                        let it = CheckMenuItem::new(
                            t("Off"),
                            write,
                            matches!(current, Some(Effect::Off)),
                            None,
                        );
                        actions.insert(
                            it.id().clone(),
                            Action::SetEffect {
                                dev: di,
                                zone: z.index,
                                effect: Effect::Off,
                            },
                        );
                        let _ = sub.append(&it);
                    }
                    if z.supports(rgb::EFFECT_FIXED) {
                        let fixed = Submenu::new(t("Fixed color"), write);
                        for (name, rgbv) in &palette {
                            let checked =
                                matches!(current, Some(Effect::Fixed { color }) if color == rgbv);
                            let it = CheckMenuItem::new(
                                format!("{}  {}", i18n::color_name(name), rgbv.hex()),
                                write,
                                checked,
                                None,
                            );
                            actions.insert(
                                it.id().clone(),
                                Action::SetEffect {
                                    dev: di,
                                    zone: z.index,
                                    effect: Effect::Fixed { color: *rgbv },
                                },
                            );
                            let _ = fixed.append(&it);
                        }
                        let _ = sub.append(&fixed);
                    }
                    if z.supports(rgb::EFFECT_BREATHING) {
                        let br = Submenu::new(t("Breathing"), write);
                        for (name, rgbv) in &palette {
                            let checked = matches!(current, Some(Effect::Breathing { color, .. }) if color == rgbv);
                            let it = CheckMenuItem::new(
                                format!("{}  {}", i18n::color_name(name), rgbv.hex()),
                                write,
                                checked,
                                None,
                            );
                            actions.insert(
                                it.id().clone(),
                                Action::SetEffect {
                                    dev: di,
                                    zone: z.index,
                                    effect: Effect::Breathing {
                                        color: *rgbv,
                                        period_ms: period,
                                        brightness: bright,
                                    },
                                },
                            );
                            let _ = br.append(&it);
                        }
                        let _ = sub.append(&br);
                    }
                    if z.supports(rgb::EFFECT_CYCLE) {
                        let it = CheckMenuItem::new(
                            t("Color cycle"),
                            write,
                            matches!(current, Some(Effect::Cycle { .. })),
                            None,
                        );
                        actions.insert(
                            it.id().clone(),
                            Action::SetEffect {
                                dev: di,
                                zone: z.index,
                                effect: Effect::Cycle {
                                    period_ms: period,
                                    brightness: bright,
                                },
                            },
                        );
                        let _ = sub.append(&it);
                    }
                    for slot in &z.effects {
                        let id = slot.effect_id;
                        if matches!(
                            id,
                            rgb::EFFECT_OFF
                                | rgb::EFFECT_FIXED
                                | rgb::EFFECT_BREATHING
                                | rgb::EFFECT_CYCLE
                        ) {
                            continue;
                        }
                        let checked =
                            matches!(current, Some(Effect::Other { id: cur, .. }) if *cur == id);
                        let it = CheckMenuItem::new(i18n::effect_name(id), write, checked, None);
                        let params = e.other_params.get(&id).cloned().unwrap_or_default();
                        actions.insert(
                            it.id().clone(),
                            Action::SetEffect {
                                dev: di,
                                zone: z.index,
                                effect: Effect::Other { id, params },
                            },
                        );
                        let _ = sub.append(&it);
                    }

                    let _ = sub.append(&PredefinedMenuItem::separator());
                    let speed = Submenu::new(format!("{} ({period} ms)", t("Speed")), write);
                    for (name, ms) in PERIODS {
                        let it = CheckMenuItem::new(
                            format!("{}  {ms} ms", t(name)),
                            write,
                            ms == period,
                            None,
                        );
                        actions.insert(
                            it.id().clone(),
                            Action::SetPeriod {
                                dev: di,
                                zone: z.index,
                                period_ms: ms,
                            },
                        );
                        let _ = speed.append(&it);
                    }
                    let _ = sub.append(&speed);
                    let bsub = Submenu::new(format!("{} ({bright}%)", t("Brightness")), write);
                    for pct in BRIGHTNESS_STEPS {
                        let it = CheckMenuItem::new(format!("{pct}%"), write, pct == bright, None);
                        actions.insert(
                            it.id().clone(),
                            Action::SetZoneBrightness {
                                dev: di,
                                zone: z.index,
                                pct,
                            },
                        );
                        let _ = bsub.append(&it);
                    }
                    let _ = sub.append(&bsub);
                    let _ = menu.append(&sub);
                }

                if info.feature == rgb::FEAT_RGB_EFFECTS {
                    let sw = e.sw_control.map(|(m, _)| m != 0).unwrap_or(false);
                    let text = if sw {
                        t("Hand lighting back to the device (currently: software)")
                    } else {
                        t("Lighting is controlled by the device profile")
                    };
                    let it = MenuItem::new(text, write && sw, None);
                    actions.insert(it.id().clone(), Action::ReleaseSw { dev: di });
                    let _ = menu.append(&it);
                }
            }

            // ---- Device brightness (0x8040)
            if let Some(b) = &e.brightness {
                let pct = if b.max > 0 {
                    (b.current as u32 * 100 / b.max as u32) as u16
                } else {
                    0
                };
                let sub = Submenu::new(format!("{}: {pct}%", t("Backlight")), write);
                for p in [0u16, 25, 50, 75, 100] {
                    let value = (b.max as u32 * p as u32 / 100) as u16;
                    let it = CheckMenuItem::new(format!("{p}%"), write, value == b.current, None);
                    actions.insert(it.id().clone(), Action::SetBrightness { dev: di, value });
                    let _ = sub.append(&it);
                }
                let _ = menu.append(&sub);
            }

            // ---- Report rate (read-only while the mouse runs its onboard profile)
            if let Some(r) = &e.rate {
                let hz = |ms: u8| if ms == 0 { 0 } else { 1000 / ms as u32 };
                let locked = e.onboard == Some(OnboardMode::Onboard);
                let sub = Submenu::new(
                    format!("{}: {} Hz", t("Report rate"), hz(r.current_ms)),
                    write,
                );
                for ms in &r.supported {
                    let it = CheckMenuItem::new(
                        format!("{} Hz", hz(*ms)),
                        write && !locked,
                        *ms == r.current_ms,
                        None,
                    );
                    actions.insert(it.id().clone(), Action::SetRate { dev: di, ms: *ms });
                    let _ = sub.append(&it);
                }
                if locked {
                    let _ = sub.append(&PredefinedMenuItem::separator());
                    let _ = sub.append(&MenuItem::new(
                        t("Set by the onboard profile; switch to host mode to change it"),
                        false,
                        None,
                    ));
                }
                let _ = menu.append(&sub);
            }

            // ---- Onboard profiles
            if let Some(m) = e.onboard {
                let name = match m {
                    OnboardMode::Onboard => t("Onboard"),
                    OnboardMode::Host => t("Host"),
                };
                let sub = Submenu::new(format!("{}: {name}", t("Profile")), write);
                let a = CheckMenuItem::new(
                    t("Onboard mode (buttons and the DPI button are handled by the mouse)"),
                    write,
                    m == OnboardMode::Onboard,
                    None,
                );
                actions.insert(
                    a.id().clone(),
                    Action::SetOnboard {
                        dev: di,
                        mode: OnboardMode::Onboard,
                    },
                );
                let b = CheckMenuItem::new(
                    t("Host mode (button events need driver software)"),
                    write,
                    m == OnboardMode::Host,
                    None,
                );
                actions.insert(
                    b.id().clone(),
                    Action::SetOnboard {
                        dev: di,
                        mode: OnboardMode::Host,
                    },
                );
                let _ = sub.append_items(&[&a, &b]);
                let _ = menu.append(&sub);
            }

            // ---- Battery
            if let Some(b) = &e.battery {
                // Kept enabled on purpose: this is a readout, and it stays available while G HUB
                // locks every setting above it.
                let _ = menu.append(&MenuItem::new(i18n::battery_label(b), true, None));
            }
            let _ = menu.append(&PredefinedMenuItem::separator());
        }

        if self.entries.is_empty() {
            let _ = menu.append(&MenuItem::new(
                t("No Logitech HID++ device found"),
                false,
                None,
            ));
            let _ = menu.append(&PredefinedMenuItem::separator());
        }

        // ---- Tray icon content
        let td_title = match &self.cfg.tray_display {
            Some(td) => format!(
                "{}: {} · {}",
                t("Tray icon"),
                short_name(&td.device),
                t(td.metric.label())
            ),
            None => format!("{}: {}", t("Tray icon"), t("Default icon")),
        };
        let td_sub = Submenu::new(td_title, true);
        let none_item = CheckMenuItem::new(
            t("Default icon"),
            true,
            self.cfg.tray_display.is_none(),
            None,
        );
        actions.insert(none_item.id().clone(), Action::SetTrayDisplay(None));
        let _ = td_sub.append(&none_item);
        for e in &self.entries {
            let metrics = Self::available_metrics(e);
            if metrics.is_empty() {
                continue;
            }
            let _ = td_sub.append(&PredefinedMenuItem::separator());
            let _ = td_sub.append(&MenuItem::new(e.dev.name.clone(), false, None));
            for m in metrics {
                let td = TrayDisplay {
                    device: e.dev.key(),
                    metric: m,
                };
                let checked = self.cfg.tray_display.as_ref() == Some(&td);
                let it = CheckMenuItem::new(format!("    {}", t(m.label())), true, checked, None);
                actions.insert(it.id().clone(), Action::SetTrayDisplay(Some(td)));
                let _ = td_sub.append(&it);
            }
        }
        if let Some(td) = &self.cfg.tray_display
            && !self.entries.iter().any(|e| e.dev.key() == td.device)
        {
            let _ = td_sub.append(&PredefinedMenuItem::separator());
            let _ = td_sub.append(&MenuItem::new(
                format!(
                    "({} · {}, {})",
                    short_name(&td.device),
                    t(td.metric.label()),
                    t("device offline")
                ),
                false,
                None,
            ));
        }
        let _ = menu.append(&td_sub);
        let _ = menu.append(&PredefinedMenuItem::separator());

        // ---- Global options
        let persist = CheckMenuItem::new(
            t("Save lighting to device memory"),
            write,
            self.cfg.persist_lighting,
            None,
        );
        actions.insert(persist.id().clone(), Action::TogglePersist);
        let restore = CheckMenuItem::new(
            t("Restore last settings on startup"),
            write,
            self.cfg.restore_on_start,
            None,
        );
        actions.insert(restore.id().clone(), Action::ToggleRestore);
        let autostart = CheckMenuItem::new(
            t("Start with Windows"),
            true,
            winutil::autostart_enabled(),
            None,
        );
        actions.insert(autostart.id().clone(), Action::ToggleAutostart);
        let open_cfg = MenuItem::new(t("Open config file (DPI presets / palette)"), true, None);
        actions.insert(open_cfg.id().clone(), Action::OpenConfig);
        let refresh = MenuItem::new(t("Refresh"), true, None);
        actions.insert(refresh.id().clone(), Action::Refresh);
        let rescan = MenuItem::new(t("Rescan devices"), true, None);
        actions.insert(rescan.id().clone(), Action::Rescan);
        let about = MenuItem::new(t("About"), true, None);
        actions.insert(about.id().clone(), Action::About);
        let quit = MenuItem::new(t("Quit"), true, None);
        actions.insert(quit.id().clone(), Action::Quit);
        let items: [&dyn IsMenuItem; 10] = [
            &persist,
            &restore,
            &autostart,
            &PredefinedMenuItem::separator(),
            &open_cfg,
            &refresh,
            &rescan,
            &about,
            &PredefinedMenuItem::separator(),
            &quit,
        ];
        let _ = menu.append_items(&items);
        (menu, actions)
    }

    fn zone_period(&self, e: &Entry, zone: u8) -> u16 {
        match e.zone_effects.get(&zone) {
            Some(Effect::Breathing { period_ms, .. }) | Some(Effect::Cycle { period_ms, .. })
                if *period_ms > 0 =>
            {
                *period_ms
            }
            _ => self
                .cfg
                .devices
                .get(&e.dev.key())
                .and_then(|d| d.zone_period_ms.get(&zone).copied())
                .unwrap_or(DEFAULT_PERIOD_MS),
        }
    }

    fn zone_brightness(&self, e: &Entry, zone: u8) -> u8 {
        match e.zone_effects.get(&zone) {
            Some(Effect::Breathing { brightness, .. }) | Some(Effect::Cycle { brightness, .. }) => {
                *brightness
            }
            _ => self
                .cfg
                .devices
                .get(&e.dev.key())
                .and_then(|d| d.zone_brightness.get(&zone).copied())
                .unwrap_or(100),
        }
    }

    // ---------------------------------------------------------------- actions

    /// Returns false when the app should exit.
    pub fn on_menu(&mut self, id: &MenuId) -> bool {
        let Some(action) = self.actions.get(id).cloned() else {
            return true;
        };
        self.log(format!("menu: {action:?}"));
        if matches!(action, Action::Quit) {
            return false;
        }
        let result = self.perform(action.clone());
        if let Err(e) = result {
            let msg = format!("{e:#}");
            self.log(format!("action {action:?} failed: {msg}"));
            winutil::message_box(APP_NAME, &msg, true);
            // A failing device is probably gone; re-check on next tick.
            self.needs_rescan = true;
        }
        if let Err(e) = self.cfg.save() {
            self.log(format!("config save failed: {e}"));
        }
        self.rebuild_menu();
        true
    }

    fn entry(&mut self, i: usize) -> Result<&mut Entry> {
        self.entries
            .get_mut(i)
            .ok_or_else(|| anyhow!("device disconnected"))
    }

    fn perform(&mut self, action: Action) -> Result<()> {
        if self.ghub && is_device_config(&action) {
            self.log("ignored: G HUB is running and owns device control".into());
            return Ok(());
        }
        match action {
            Action::SetDpi { dev, dpi: want } => {
                let persist_key;
                {
                    let e = self.entry(dev)?;
                    let info = e.dpi.clone().ok_or_else(|| anyhow!("no DPI feature"))?;
                    let v = info.snap(want);
                    dpi::set(&e.dev, &info, v)?;
                    e.dpi = dpi::read(&e.dev)?;
                    persist_key = (e.dev.key(), e.dpi.as_ref().map(|d| d.current).unwrap_or(v));
                }
                self.cfg.device_mut(&persist_key.0).dpi = Some(persist_key.1);
            }
            Action::DpiStep { dev, delta } => {
                let e = self.entry(dev)?;
                let info = e.dpi.clone().ok_or_else(|| anyhow!("no DPI feature"))?;
                let target = (info.current as i32 + delta)
                    .clamp(info.min() as i32, info.max() as i32)
                    as u16;
                let v = info.snap(target);
                return self.perform(Action::SetDpi { dev, dpi: v });
            }
            Action::SetEffect { dev, zone, effect } => {
                let persist = self.cfg.persist_lighting;
                let key;
                {
                    let e = self.entry(dev)?;
                    let info = e
                        .rgb
                        .clone()
                        .ok_or_else(|| anyhow!("no lighting feature"))?;
                    rgb::set(&e.dev, &info, zone, &effect, persist)?;
                    e.zone_effects.insert(zone, effect.clone());
                    if let Ok(v) = rgb::sw_control(&e.dev, &info) {
                        e.sw_control = v;
                    }
                    key = e.dev.key();
                }
                let st = self.cfg.device_mut(&key);
                st.zones.insert(zone, effect.clone());
                match &effect {
                    Effect::Breathing {
                        period_ms,
                        brightness,
                        ..
                    }
                    | Effect::Cycle {
                        period_ms,
                        brightness,
                    } => {
                        st.zone_period_ms.insert(zone, *period_ms);
                        st.zone_brightness.insert(zone, *brightness);
                    }
                    _ => {}
                }
            }
            Action::SetPeriod {
                dev,
                zone,
                period_ms,
            } => {
                let key = self.entry(dev)?.dev.key();
                self.cfg
                    .device_mut(&key)
                    .zone_period_ms
                    .insert(zone, period_ms);
                let current = self.entries[dev].zone_effects.get(&zone).cloned();
                let updated = match current {
                    Some(Effect::Breathing {
                        color, brightness, ..
                    }) => Some(Effect::Breathing {
                        color,
                        period_ms,
                        brightness,
                    }),
                    Some(Effect::Cycle { brightness, .. }) => Some(Effect::Cycle {
                        period_ms,
                        brightness,
                    }),
                    _ => None,
                };
                if let Some(fx) = updated {
                    return self.perform(Action::SetEffect {
                        dev,
                        zone,
                        effect: fx,
                    });
                }
            }
            Action::SetZoneBrightness { dev, zone, pct } => {
                let key = self.entry(dev)?.dev.key();
                self.cfg.device_mut(&key).zone_brightness.insert(zone, pct);
                let current = self.entries[dev].zone_effects.get(&zone).cloned();
                let updated = match current {
                    Some(Effect::Breathing {
                        color, period_ms, ..
                    }) => Some(Effect::Breathing {
                        color,
                        period_ms,
                        brightness: pct,
                    }),
                    Some(Effect::Cycle { period_ms, .. }) => Some(Effect::Cycle {
                        period_ms,
                        brightness: pct,
                    }),
                    _ => None,
                };
                if let Some(fx) = updated {
                    return self.perform(Action::SetEffect {
                        dev,
                        zone,
                        effect: fx,
                    });
                }
            }
            Action::ReleaseSw { dev } => {
                let key;
                {
                    let e = self.entry(dev)?;
                    let info = e
                        .rgb
                        .clone()
                        .ok_or_else(|| anyhow!("no lighting feature"))?;
                    rgb::release_sw_control(&e.dev, &info)?;
                    e.sw_control = rgb::sw_control(&e.dev, &info)?;
                    e.zone_effects.clear();
                    key = e.dev.key();
                }
                // Forget saved zones so restore-on-start doesn't take control back.
                self.cfg.device_mut(&key).zones.clear();
            }
            Action::SetOnboard { dev, mode } => {
                let e = self.entry(dev)?;
                misc::set_onboard_mode(&e.dev, mode)?;
                e.onboard = misc::onboard_mode(&e.dev)?;
            }
            Action::SetRate { dev, ms } => {
                let key;
                {
                    let e = self.entry(dev)?;
                    misc::set_report_rate(&e.dev, ms)?;
                    e.rate = misc::report_rate(&e.dev)?;
                    key = e.dev.key();
                }
                self.cfg.device_mut(&key).report_rate_ms = Some(ms);
            }
            Action::SetBrightness { dev, value } => {
                let key;
                {
                    let e = self.entry(dev)?;
                    misc::set_brightness(&e.dev, value)?;
                    e.brightness = misc::brightness(&e.dev)?;
                    key = e.dev.key();
                }
                self.cfg.device_mut(&key).brightness = Some(value);
            }
            Action::SetTrayDisplay(td) => self.cfg.tray_display = td,
            Action::TogglePersist => self.cfg.persist_lighting = !self.cfg.persist_lighting,
            Action::ToggleRestore => self.cfg.restore_on_start = !self.cfg.restore_on_start,
            Action::ToggleAutostart => winutil::set_autostart(!winutil::autostart_enabled())?,
            Action::OpenConfig => {
                self.cfg.save()?;
                winutil::shell_open(&Config::path().display().to_string());
            }
            Action::Refresh => {
                // Reload config (user may have edited presets) then refresh devices.
                self.cfg = Config::load();
                self.on_timer();
            }
            Action::Rescan => {
                self.cfg = Config::load();
                self.rescan();
            }
            Action::About => {
                let mut text = format!(
                    "{APP_NAME} v{}\n{}\n{}: {}\n\n",
                    env!("CARGO_PKG_VERSION"),
                    t("Logitech HID++ tray control (DPI / lighting / report rate / battery)"),
                    t("Config"),
                    Config::path().display()
                );
                for e in &self.entries {
                    text.push_str(&e.dev.describe());
                    text.push('\n');
                    let ids: Vec<String> =
                        e.dev.features.keys().map(|k| format!("{k:04X}")).collect();
                    text.push_str(&format!("  features: {}\n", ids.join(" ")));
                    if let Some(info) = &e.rgb {
                        for z in &info.zones {
                            let names: Vec<String> = z
                                .effects
                                .iter()
                                .map(|s| i18n::effect_name(s.effect_id))
                                .collect();
                            text.push_str(&format!(
                                "  {} {} ({}): {}\n",
                                t("Zone"),
                                z.index,
                                i18n::location_name(z.location),
                                names.join(", ")
                            ));
                        }
                        if let Some((m, f)) = e.sw_control {
                            text.push_str(&format!(
                                "  {}: mode {m} flags {f:#04x}\n",
                                t("LED control")
                            ));
                        }
                    }
                    if let Some(d) = &e.dpi {
                        text.push_str(&format!("  DPI {} ({}–{})\n", d.current, d.min(), d.max()));
                    }
                }
                if !self.log.is_empty() {
                    text.push_str(&format!("\n{}:\n", t("Recent log")));
                    for l in self
                        .log
                        .iter()
                        .rev()
                        .take(8)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                    {
                        text.push_str(l);
                        text.push('\n');
                    }
                }
                winutil::message_box(APP_NAME, &text, false);
            }
            Action::Quit => {}
        }
        Ok(())
    }
}

/// Compact device label for menu titles ("Logitech G502 X" -> "G502 X").
fn short_name(name: &str) -> &str {
    let n = name.trim();
    n.strip_prefix("Logitech ").unwrap_or(n)
}

fn is_device_config(action: &Action) -> bool {
    !matches!(
        action,
        Action::SetTrayDisplay(_)
            | Action::ToggleAutostart
            | Action::OpenConfig
            | Action::Refresh
            | Action::Rescan
            | Action::About
            | Action::Quit
    )
}

/// Append one line to %APPDATA%\LogiTray\logitray.log. Always English.
pub fn log_line(s: &str) {
    use std::io::Write;
    eprintln!("{s}");
    let path = Config::path().with_file_name("logitray.log");
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Keep the log from growing without bound.
    if std::fs::metadata(&path)
        .map(|m| m.len() > 512 * 1024)
        .unwrap_or(false)
    {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "[{}] {s}", winutil::timestamp());
    }
}
