//! Tray icon rendering: the default app glyph, or a small badge showing a live value
//! (DPI digits, battery percentage + charge bar, or a lighting colour swatch).
//! Text is rasterised with GDI into a 32-bit DIB at the shell's small-icon size, then
//! converted to RGBA for `tray_icon::Icon`.

use crate::features::rgb::{Effect, Rgb};
use crate::winutil::wide;
use std::ffi::c_void;
use std::ptr::null_mut;
use tray_icon::Icon;
use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CLIP_DEFAULT_PRECIS,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
    DIB_RGB_COLORS, DeleteDC, DeleteObject, FF_SWISS, FW_BOLD, GdiFlush, GetDC,
    GetTextExtentPoint32W, HFONT, OUT_TT_PRECIS, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT, TextOutW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSMICON};

const BADGE_BG: Rgb = Rgb(0x1C, 0x24, 0x33);
const BADGE_FG: Rgb = Rgb(0xFF, 0xFF, 0xFF);
const ACCENT: Rgb = Rgb(0x00, 0xB8, 0xFC);

/// Pixel size the shell draws small (tray) icons at for the current DPI.
pub fn icon_size() -> u32 {
    let s = unsafe { GetSystemMetrics(SM_CXSMICON) };
    (s.max(16) as u32).min(64)
}

/// Default app icon: dark disc with a bright LED ring.
pub fn default_icon() -> Icon {
    let n = icon_size().max(32); // render larger; the shell downsamples a plain shape fine
    let mut c = Canvas::new(n, n);
    let mid = (n as f32 - 1.0) / 2.0;
    let r_out = n as f32 / 2.0 - 0.5;
    let ring_w = n as f32 * 0.14;
    for y in 0..n {
        for x in 0..n {
            let d = ((x as f32 - mid).powi(2) + (y as f32 - mid).powi(2)).sqrt();
            let (col, a) = if d > r_out {
                (ACCENT, 0.0)
            } else if d > r_out - 1.0 {
                (ACCENT, r_out - d)
            } else if d > r_out - ring_w {
                (ACCENT, 1.0)
            } else if d > r_out - ring_w - 1.0 {
                let t = r_out - ring_w - d; // 0..1 blend from accent to bg
                (lerp(ACCENT, BADGE_BG, t), 1.0)
            } else {
                (BADGE_BG, 1.0)
            };
            c.put(x, y, col, (a * 255.0) as u8);
        }
    }
    c.into_icon()
}

/// Badge with up to two rows of text and an optional bottom bar (fraction 0..1, colour).
pub fn text_badge(lines: &[&str], bar: Option<(f32, Rgb)>) -> Icon {
    let n = icon_size();
    let mut c = Canvas::new(n, n);
    c.fill_rounded(BADGE_BG, (n as f32 * 0.22) as u32);
    let bar_h = if bar.is_some() {
        (n as f32 * 0.16).round().max(2.0) as u32
    } else {
        0
    };
    let text_h = n - bar_h;
    if lines.len() == 2 {
        // Two rows: emphasise the leading digits, keep the trailing ones smaller and dimmer.
        let h1 = (text_h as f32 * 0.58).round() as u32;
        c.draw_text_rows(&lines[..1], 0, h1, BADGE_FG);
        c.draw_text_rows(&lines[1..], h1, text_h - h1, lerp(BADGE_FG, BADGE_BG, 0.3));
    } else {
        c.draw_text_rows(lines, 0, text_h, BADGE_FG);
    }
    if let Some((frac, col)) = bar {
        let w = ((n as f32 - 2.0) * frac.clamp(0.0, 1.0)).round() as u32;
        c.fill_rect(
            1,
            n - bar_h,
            n - 2,
            bar_h - 1,
            lerp(BADGE_BG, Rgb(0x60, 0x60, 0x60), 0.5),
        );
        c.fill_rect(1, n - bar_h, w, bar_h - 1, col);
        c.apply_rounded_mask((n as f32 * 0.22) as u32);
    }
    c.into_icon()
}

/// Colour swatch for the current lighting effect.
pub fn color_badge(effect: Option<&Effect>) -> Icon {
    let n = icon_size();
    let mut c = Canvas::new(n, n);
    let r = (n as f32 * 0.22) as u32;
    match effect {
        Some(Effect::Fixed { color }) | Some(Effect::Breathing { color, .. }) => {
            c.fill_rounded(*color, r);
        }
        Some(Effect::Cycle { .. }) => {
            // Horizontal rainbow.
            for x in 0..n {
                let h = x as f32 / n as f32;
                let col = hsv(h, 1.0, 1.0);
                c.fill_rect(x, 0, 1, n, col);
            }
            c.apply_rounded_mask(r);
        }
        Some(Effect::Off) => {
            c.fill_rounded(Rgb(0x30, 0x30, 0x30), r);
            c.draw_text_rows(&["off"], 0, n, Rgb(0xA0, 0xA0, 0xA0));
        }
        _ => {
            // Device-native effect (wave, starlight …) or unknown: ring + "fx".
            c.fill_rounded(BADGE_BG, r);
            c.draw_text_rows(&["fx"], 0, n, ACCENT);
        }
    }
    c.into_icon()
}

/// Split a DPI value into display rows: "800" / ["16","00"] / ["128","00"].
pub fn dpi_rows(dpi: u16) -> Vec<String> {
    let s = dpi.to_string();
    if s.len() <= 3 {
        vec![s]
    } else {
        let split = s.len() - 2;
        vec![s[..split].to_owned(), s[split..].to_owned()]
    }
}

pub fn battery_color(percent: u8, charging: bool) -> Rgb {
    if charging {
        ACCENT
    } else if percent > 40 {
        Rgb(0x3C, 0xD0, 0x6A)
    } else if percent > 15 {
        Rgb(0xFF, 0xB0, 0x20)
    } else {
        Rgb(0xFF, 0x40, 0x40)
    }
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn hsv(h: f32, s: f32, v: f32) -> Rgb {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

// ------------------------------------------------------------------ GDI canvas

/// Top-down 32bpp DIB (BGRA) we can both draw GDI text on and poke pixels into.
struct Canvas {
    w: u32,
    h: u32,
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    hbm: windows_sys::Win32::Graphics::Gdi::HBITMAP,
    old: windows_sys::Win32::Graphics::Gdi::HGDIOBJ,
    bits: *mut u8,
    screen: windows_sys::Win32::Graphics::Gdi::HDC,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Canvas {
        unsafe {
            let screen = GetDC(null_mut());
            let hdc = CreateCompatibleDC(screen);
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits: *mut c_void = null_mut();
            let hbm = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            let old = SelectObject(hdc, hbm as _);
            let c = Canvas {
                w,
                h,
                hdc,
                hbm,
                old,
                bits: bits as *mut u8,
                screen,
            };
            std::ptr::write_bytes(c.bits, 0, (w * h * 4) as usize);
            c
        }
    }

    #[inline]
    fn px(&mut self, x: u32, y: u32) -> &mut [u8] {
        let off = ((y * self.w + x) * 4) as usize;
        unsafe { std::slice::from_raw_parts_mut(self.bits.add(off), 4) }
    }

    fn put(&mut self, x: u32, y: u32, c: Rgb, a: u8) {
        let p = self.px(x, y);
        p[0] = c.2;
        p[1] = c.1;
        p[2] = c.0;
        p[3] = a;
    }

    fn fill_rect(&mut self, x0: u32, y0: u32, w: u32, h: u32, c: Rgb) {
        for y in y0..(y0 + h).min(self.h) {
            for x in x0..(x0 + w).min(self.w) {
                self.put(x, y, c, 255);
            }
        }
    }

    fn fill_rounded(&mut self, c: Rgb, radius: u32) {
        let (w, h) = (self.w, self.h);
        self.fill_rect(0, 0, w, h, c);
        self.apply_rounded_mask(radius);
    }

    /// Zero the alpha outside a rounded rectangle (with 1px anti-aliased edge).
    fn apply_rounded_mask(&mut self, radius: u32) {
        let r = radius as f32;
        if r < 1.0 {
            return;
        }
        let (w, h) = (self.w as f32, self.h as f32);
        for y in 0..self.h {
            for x in 0..self.w {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let cx = fx.clamp(r, w - r);
                let cy = fy.clamp(r, h - r);
                let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt();
                let cov = (r - d + 0.5).clamp(0.0, 1.0);
                let p = self.px(x, y);
                p[3] = (p[3] as f32 * cov) as u8;
            }
        }
    }

    /// Draw `lines` stacked and centred inside rows [y0, y0+h). GDI writes 0 into the
    /// alpha byte of touched pixels, so alpha is restored to opaque afterwards.
    fn draw_text_rows(&mut self, lines: &[&str], y0: u32, h: u32, color: Rgb) {
        if lines.is_empty() || h == 0 {
            return;
        }
        let rows = lines.len() as u32;
        let row_h = h / rows;
        // GDI clears the alpha byte of every pixel it touches; snapshot alpha to restore it.
        let alpha_before: Vec<u8> = (0..self.w * self.h)
            .map(|i| self.px(i % self.w, i / self.w)[3])
            .collect();
        unsafe {
            GdiFlush();
            SetBkMode(self.hdc, TRANSPARENT as i32);
            SetTextColor(
                self.hdc,
                (color.0 as u32) | ((color.1 as u32) << 8) | ((color.2 as u32) << 16),
            );
            for (i, line) in lines.iter().enumerate() {
                let text = wide(line);
                let n = (text.len() - 1) as i32;
                // Start from a font that fills the row, shrink until the text fits the width.
                let mut height = (row_h as f32 * 1.25).round() as i32;
                let mut font: HFONT = null_mut();
                let mut size = SIZE { cx: 0, cy: 0 };
                while height >= 6 {
                    if !font.is_null() {
                        DeleteObject(font as _);
                    }
                    font = CreateFontW(
                        -height,
                        0,
                        0,
                        0,
                        FW_BOLD as i32,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET as u32,
                        OUT_TT_PRECIS as u32,
                        CLIP_DEFAULT_PRECIS as u32,
                        ANTIALIASED_QUALITY as u32,
                        (FF_SWISS | DEFAULT_PITCH) as u32,
                        wide("Segoe UI").as_ptr(),
                    );
                    SelectObject(self.hdc, font as _);
                    GetTextExtentPoint32W(self.hdc, text.as_ptr(), n, &mut size);
                    if size.cx as u32 <= self.w.saturating_sub(1) && size.cy as u32 <= row_h + 2 {
                        break;
                    }
                    height -= 1;
                }
                let x = ((self.w as i32 - size.cx) / 2).max(0);
                let y = y0 as i32 + (i as u32 * row_h) as i32 + ((row_h as i32 - size.cy) / 2);
                TextOutW(self.hdc, x, y, text.as_ptr(), n);
                GdiFlush();
                if !font.is_null() {
                    DeleteObject(font as _);
                }
            }
        }
        for (i, a) in alpha_before.into_iter().enumerate() {
            let (x, y) = (i as u32 % self.w, i as u32 / self.w);
            self.px(x, y)[3] = a;
        }
    }

    fn into_icon(self) -> Icon {
        let (w, h) = (self.w, self.h);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        unsafe {
            GdiFlush();
            let src = std::slice::from_raw_parts(self.bits, (w * h * 4) as usize);
            for p in src.as_chunks::<4>().0 {
                rgba.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
            }
        }
        Icon::from_rgba(rgba, w, h).expect("icon rgba")
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.old);
            DeleteObject(self.hbm as _);
            DeleteDC(self.hdc);
            ReleaseDC(null_mut(), self.screen);
        }
    }
}
