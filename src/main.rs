#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod device;
mod features;
mod hidpp;
mod i18n;
mod trayicon;
mod winutil;

use anyhow::{Context, Result, anyhow, bail};
use app::{APP_NAME, App};
use config::Config;
use features::{dpi, misc, rgb};
use tray_icon::menu::MenuEvent;

const USAGE: &str = "\
LogiTray - Logitech HID++ tray control (DPI / lighting / report rate / battery)

Usage:
  logitray                    start the tray app
  logitray probe              list devices, features, DPI, lighting zones
  logitray dpi <value>        set mouse DPI (for example 1600)
  logitray color <#RRGGBB>    fixed color on every lighting zone
  logitray breathe <#RRGGBB>  breathing effect
  logitray cycle              color cycle
  logitray off                turn lighting off
  logitray effect <id> [hex]  on-device effect (4=color wave, 5=starlight, 0x0B=ripple); optional 10 parameter bytes
  logitray release            hand lighting control back to the device profile (0x8071)
  logitray rate <Hz>          set report rate (1000/500/250/125; host mode required)
  logitray autostart [on|off] show or set start-with-Windows

Options: --device <fragment>  only devices whose name contains the fragment
         --persist             also write lighting to device memory

Config: %APPDATA%\\LogiTray\\config.json    Log: %APPDATA%\\LogiTray\\logitray.log
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        if let Err(e) = run_tray() {
            winutil::message_box(APP_NAME, &format!("{e:#}"), true);
            std::process::exit(1);
        }
        return;
    }
    winutil::attach_parent_console();
    if let Err(e) = run_cli(&args) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run_tray() -> Result<()> {
    // Per-monitor DPI awareness so the shell reports the real small-icon size for badge rendering.
    winutil::enable_dpi_awareness();
    let cfg = Config::load();
    i18n::apply(cfg.language);
    if !winutil::acquire_single_instance("Local\\LogiTray.SingleInstance") {
        winutil::message_box(
            APP_NAME,
            i18n::t("LogiTray is already running (see the notification area icon)."),
            false,
        );
        return Ok(());
    }
    app::log_line(&format!(
        "--- {APP_NAME} v{} starting (UI {} -> {})",
        env!("CARGO_PKG_VERSION"),
        cfg.language.code(),
        i18n::lang().code()
    ));
    let mut app = App::new(cfg).context("failed to create the tray icon")?;
    if let Some(text) = app.take_ghub_notice() {
        winutil::message_box(APP_NAME, &text, false);
    }
    let full_timer = winutil::set_timer(1, app.refresh_interval_ms());
    // Faster, lightweight timer that only re-reads the metric shown in the tray icon.
    let mut badge_timer = if app.badge_active() {
        winutil::set_timer(2, app::BADGE_REFRESH_MS)
    } else {
        0
    };
    let rx = MenuEvent::receiver();
    loop {
        match winutil::pump() {
            winutil::Pump::Quit => break,
            winutil::Pump::Timer(id) if id == full_timer => app.on_timer(),
            winutil::Pump::Timer(id) if id == badge_timer && id != 0 => app.on_badge_timer(),
            winutil::Pump::Timer(_) | winutil::Pump::Message => {}
        }
        while let Ok(ev) = rx.try_recv() {
            if !app.on_menu(&ev.id) {
                app::log_line("quit requested from menu");
                winutil::post_quit();
            }
        }
        if let Some(text) = app.take_ghub_notice() {
            winutil::message_box(APP_NAME, &text, false);
        }
        // Keep the badge timer in sync with whether a metric is selected.
        match (app.badge_active(), badge_timer != 0) {
            (true, false) => badge_timer = winutil::set_timer(2, app::BADGE_REFRESH_MS),
            (false, true) => {
                winutil::kill_timer(badge_timer);
                badge_timer = 0;
            }
            _ => {}
        }
    }
    let _ = app.cfg.save();
    app::log_line("exited");
    Ok(())
}

// ------------------------------------------------------------------ CLI mode

struct Cli {
    cmd: String,
    value: Option<String>,
    extra: Option<String>,
    device: Option<String>,
    persist: bool,
}

fn parse_cli(args: &[String]) -> Result<Cli> {
    let mut cli = Cli {
        cmd: String::new(),
        value: None,
        extra: None,
        device: None,
        persist: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--device" | "-d" => {
                cli.device = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--device needs a value"))?
                        .clone(),
                )
            }
            "--persist" => cli.persist = true,
            "-h" | "--help" | "help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            s if cli.cmd.is_empty() => cli.cmd = s.to_lowercase(),
            s if cli.value.is_none() => cli.value = Some(s.to_owned()),
            s if cli.extra.is_none() => cli.extra = Some(s.to_owned()),
            s => bail!("unexpected argument: {s}"),
        }
    }
    Ok(cli)
}

fn parse_u16(s: &str) -> Result<u16> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Ok(u16::from_str_radix(h, 16)?)
    } else {
        Ok(s.parse()?)
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        bail!("hex length must be even");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| Ok(u8::from_str_radix(&s[i..i + 2], 16)?))
        .collect()
}

fn run_cli(args: &[String]) -> Result<()> {
    let cli = parse_cli(args)?;
    if cli.cmd == "autostart" {
        match cli.value.as_deref() {
            Some("on") => winutil::set_autostart(true)?,
            Some("off") => winutil::set_autostart(false)?,
            _ => {}
        }
        println!(
            "start with Windows: {}",
            if winutil::autostart_enabled() {
                "on"
            } else {
                "off"
            }
        );
        return Ok(());
    }
    let mut log = |s: String| eprintln!("[scan] {s}");
    let devices = device::discover(&mut log)?;
    let selected: Vec<_> = devices
        .iter()
        .filter(|d| match &cli.device {
            Some(f) => d.name.to_lowercase().contains(&f.to_lowercase()),
            None => true,
        })
        .collect();
    if selected.is_empty() {
        bail!("no matching device");
    }
    if cli.cmd != "probe" && cli.cmd != "list" && cli.cmd != "info" && winutil::ghub_running() {
        bail!(
            "G HUB is running and has taken over the devices; close G HUB before changing settings"
        );
    }

    match cli.cmd.as_str() {
        "probe" | "list" | "info" => {
            for d in &selected {
                probe(d);
            }
        }
        "dpi" => {
            let want: u16 = cli
                .value
                .as_deref()
                .ok_or_else(|| anyhow!("missing DPI value"))?
                .parse()?;
            let mut done = false;
            for d in &selected {
                if let Some(info) = dpi::read(d)? {
                    let v = info.snap(want);
                    dpi::set(d, &info, v)?;
                    let after = dpi::read(d)?.map(|i| i.current).unwrap_or(v);
                    println!("{}: DPI {} -> {}", d.name, info.current, after);
                    done = true;
                }
            }
            if !done {
                bail!("no device supports DPI");
            }
        }
        "color" | "breathe" | "cycle" | "off" => {
            let color = match cli.cmd.as_str() {
                "color" | "breathe" => {
                    let s = cli
                        .value
                        .as_deref()
                        .ok_or_else(|| anyhow!("missing color (#RRGGBB)"))?;
                    Some(rgb::Rgb::parse(s).ok_or_else(|| anyhow!("color must be #RRGGBB"))?)
                }
                _ => None,
            };
            let effect = match cli.cmd.as_str() {
                "color" => rgb::Effect::Fixed {
                    color: color.unwrap(),
                },
                "breathe" => rgb::Effect::Breathing {
                    color: color.unwrap(),
                    period_ms: 3000,
                    brightness: 100,
                },
                "cycle" => rgb::Effect::Cycle {
                    period_ms: 3000,
                    brightness: 100,
                },
                _ => rgb::Effect::Off,
            };
            let mut done = false;
            for d in &selected {
                if let Some(info) = rgb::read(d)? {
                    for z in &info.zones {
                        match rgb::set(d, &info, z.index, &effect, cli.persist) {
                            Ok(()) => println!("{}: {} -> {}", d.name, z.name(), effect.label()),
                            Err(e) => println!("{}: {} failed: {e}", d.name, z.name()),
                        }
                    }
                    done = true;
                }
            }
            if !done {
                bail!("no device supports lighting");
            }
        }
        "effect" => {
            // Raw on-device effect: id in hex/decimal, optional 10 parameter bytes as hex.
            let id_s = cli
                .value
                .as_deref()
                .ok_or_else(|| anyhow!("missing effect id (for example 4 or 0x0B)"))?;
            let id = parse_u16(id_s)?;
            let params = match cli.extra.as_deref() {
                Some(h) => parse_hex(h)?,
                None => Vec::new(),
            };
            let effect = rgb::Effect::Other { id, params };
            for d in &selected {
                if let Some(info) = rgb::read(d)? {
                    for z in &info.zones {
                        match rgb::set(d, &info, z.index, &effect, cli.persist) {
                            Ok(()) => println!("{}: {} -> {}", d.name, z.name(), effect.label()),
                            Err(e) => println!("{}: {} failed: {e}", d.name, z.name()),
                        }
                    }
                }
            }
        }
        "swctl" => {
            // Debug: raw manageSwControl set, e.g. `swctl 010304`
            let h = cli
                .value
                .as_deref()
                .ok_or_else(|| anyhow!("missing hex argument"))?;
            let p = parse_hex(h)?;
            for d in &selected {
                if d.has(rgb::FEAT_RGB_EFFECTS) {
                    match d.call(rgb::FEAT_RGB_EFFECTS, 0x05, &p) {
                        Ok(r) => println!("{}: swctl -> {:02X?}", d.name, &r[..4]),
                        Err(e) => println!("{}: swctl failed: {e}", d.name),
                    }
                }
            }
        }
        "release" => {
            for d in &selected {
                if let Some(info) = rgb::read(d)?
                    && info.feature == rgb::FEAT_RGB_EFFECTS
                {
                    rgb::release_sw_control(d, &info)?;
                    println!("{}: lighting control returned to the device", d.name);
                }
            }
        }
        "rate" => {
            let hz: u32 = cli
                .value
                .as_deref()
                .ok_or_else(|| anyhow!("missing report rate (Hz)"))?
                .parse()?;
            let ms = (1000 / hz.max(1)).clamp(1, 8) as u8;
            for d in &selected {
                if let Some(r) = misc::report_rate(d)? {
                    if !r.supported.contains(&ms) {
                        println!(
                            "{}: {hz} Hz is not supported (supported {:?} ms)",
                            d.name, r.supported
                        );
                        continue;
                    }
                    if misc::onboard_mode(d)? == Some(misc::OnboardMode::Onboard) {
                        println!(
                            "{name}: onboard profile is active; report rate is fixed until host mode",
                            name = d.name
                        );
                        continue;
                    }
                    misc::set_report_rate(d, ms)?;
                    println!("{}: report rate -> {} Hz", d.name, 1000 / ms as u32);
                }
            }
        }
        other => bail!("unknown command {other}\n\n{USAGE}"),
    }
    Ok(())
}

fn probe(d: &device::Device) {
    println!("== {}", d.describe());
    let ids: Vec<String> = d.features.keys().map(|k| format!("{k:04X}")).collect();
    println!("   features: {}", ids.join(" "));
    match dpi::read(d) {
        Ok(Some(i)) => println!(
            "   DPI (0x{:04X}): current {} default {} range {}..{} ({} steps) y={:?} lod={:?}",
            i.feature,
            i.current,
            i.default,
            i.min(),
            i.max(),
            i.supported.len(),
            i.current_y,
            i.lod
        ),
        Ok(None) => {}
        Err(e) => println!("   DPI read failed: {e}"),
    }
    match rgb::read(d) {
        Ok(Some(i)) => {
            println!(
                "   lighting (0x{:04X}): nv_caps {:#06x} ext_caps {:#06x}",
                i.feature, i.nv_caps, i.ext_caps
            );
            if let Ok(Some((m, f))) = rgb::sw_control(d, &i) {
                println!(
                    "   LED control: {} (mode {m}, flags {f:#04x})",
                    if m == 0 { "device profile" } else { "software" }
                );
            }
            for z in &i.zones {
                let effs: Vec<String> = z
                    .effects
                    .iter()
                    .map(|e| format!("{}(0x{:02X})", rgb::effect_name(e.effect_id), e.effect_id))
                    .collect();
                println!(
                    "     zone {} [{}] persist {:#04x}: {}",
                    z.index,
                    z.name(),
                    z.persistency_caps,
                    effs.join(", ")
                );
                match rgb::get_zone_effect(d, &i, z) {
                    Ok(Some(fx)) => println!("       current: {}", fx.label()),
                    Ok(None) => {}
                    Err(e) => println!("       current effect read failed: {e}"),
                }
            }
        }
        Ok(None) => {}
        Err(e) => println!("   lighting read failed: {e}"),
    }
    match misc::battery(d) {
        Ok(Some(b)) => println!("   {}", b.label_en()),
        Ok(None) => {}
        Err(e) => println!("   battery read failed: {e}"),
    }
    match misc::onboard_mode(d) {
        Ok(Some(m)) => println!("   profile mode: {m:?}"),
        Ok(None) => {}
        Err(e) => println!("   profile mode read failed: {e}"),
    }
    match misc::report_rate(d) {
        Ok(Some(r)) => println!(
            "   report rate: {} ms (supported {:?} ms)",
            r.current_ms, r.supported
        ),
        Ok(None) => {}
        Err(e) => println!("   report rate read failed: {e}"),
    }
    match misc::brightness(d) {
        Ok(Some(b)) => println!("   backlight: {}/{}", b.current, b.max),
        Ok(None) => {}
        Err(e) => println!("   brightness read failed: {e}"),
    }
}
