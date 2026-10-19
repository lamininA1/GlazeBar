use std::{collections::HashMap, path::Path};
use windows_sys::Win32::{Foundation::RECT, Graphics::Gdi::*};

pub type Color = u32;
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn native(self) -> RECT {
        RECT {
            left: self.x,
            top: self.y,
            right: self.x + self.w,
            bottom: self.y + self.h,
        }
    }
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}
pub fn parse(s: &str) -> Color {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    ((n >> 16) & 255) | (n & 0xff00) | ((n & 255) << 16)
}
pub fn blend(a: Color, b: Color, t: f64) -> Color {
    (0..3).fold(0, |v, i| {
        v | ((((a >> (i * 8) & 255) as f64 * (1. - t) + (b >> (i * 8) & 255) as f64 * t).round()
            as u32)
            << (i * 8))
    })
}
fn luminance(c: Color) -> f64 {
    [0.2126, 0.7152, 0.0722]
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let x = (c >> (i * 8) & 255) as f64 / 255.;
            w * if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        })
        .sum()
}
pub fn contrast(a: Color, b: Color) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn readable(bg: Color, fg: Color, n: f64) -> Color {
    if contrast(bg, fg) >= n {
        return fg;
    }
    let end = if contrast(bg, 0) > contrast(bg, 0xffffff) {
        0
    } else {
        0xffffff
    };
    (1..=100)
        .map(|i| blend(fg, end, i as f64 / 100.))
        .find(|&c| contrast(bg, c) >= n)
        .unwrap_or(end)
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub bg: Color,
    pub text: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub surface: Color,
    pub hover: Color,
    pub selected: Color,
    pub border: Color,
    pub muted: Color,
}
impl Palette {
    pub fn from_colors(p: &[String]) -> Self {
        let bg = parse(&p[0]);
        let text = readable(bg, parse(&p[15]), 7.);
        let accent = readable(bg, parse(&p[4]), 4.5);
        Self {
            bg,
            text,
            accent,
            success: readable(bg, parse(&p[2]), 4.5),
            warning: readable(bg, parse(&p[3]), 4.5),
            error: readable(bg, parse(&p[1]), 4.5),
            surface: blend(bg, text, 0.06),
            hover: blend(bg, text, 0.1),
            selected: blend(bg, accent, 0.28),
            border: blend(bg, text, 0.25),
            muted: readable(bg, blend(bg, text, 0.65), 4.5),
        }
    }
    pub fn load(root: &Path) -> Self {
        let colors = std::fs::read(root.join("theme-state/glazebar.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .and_then(|v| serde_json::from_value::<Vec<String>>(v["palette"].clone()).ok())
            .filter(|p| {
                p.len() == 16
                    && p.iter().all(|s| {
                        s.len() == 7
                            && s.starts_with('#')
                            && u32::from_str_radix(&s[1..], 16).is_ok()
                    })
            })
            .unwrap_or_else(|| {
                [
                    "#0e131c", "#fb923c", "#9ece6a", "#e0af68", "#7aa2f7", "#000000", "#000000",
                    "#000000", "#000000", "#000000", "#000000", "#000000", "#000000", "#000000",
                    "#000000", "#e2e8f2",
                ]
                .map(str::to_owned)
                .to_vec()
            });
        Self::from_colors(&colors)
    }
}
pub struct Resources {
    fonts: HashMap<(i32, bool), HFONT>,
    brushes: HashMap<Color, HBRUSH>,
    icons: HashMap<(String, i32, Color), resvg::tiny_skia::Pixmap>,
}
impl Resources {
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            brushes: HashMap::new(),
            icons: HashMap::new(),
        }
    }
    pub(crate) fn font(&mut self, px: i32, bold: bool) -> HFONT {
        *self.fonts.entry((px, bold)).or_insert_with(|| unsafe {
            CreateFontW(
                -px,
                0,
                0,
                0,
                if bold { 700 } else { 400 },
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                wide("Pretendard").as_ptr(),
            )
        })
    }
    pub(crate) fn brush(&mut self, color: Color) -> HBRUSH {
        *self
            .brushes
            .entry(color)
            .or_insert_with(|| unsafe { CreateSolidBrush(color) })
    }
}
impl Drop for Resources {
    fn drop(&mut self) {
        for b in self.brushes.values() {
            unsafe {
                DeleteObject(*b);
            }
        }
        for f in self.fonts.values() {
            unsafe {
                DeleteObject(*f);
            }
        }
    }
}
pub struct Canvas<'a> {
    pub dc: HDC,
    pub scale: f64,
    pub width: i32,
    pub height: i32,
    pub ui: Palette,
    pub resources: &'a mut Resources,
    pub root: &'a Path,
}
impl Canvas<'_> {
    pub fn round_rect(&self, r: Rect, col: Color, diameter: i32) {
        unsafe {
            let brush = CreateSolidBrush(col);
            let old = SelectObject(self.dc, brush);
            let pen = SelectObject(self.dc, GetStockObject(NULL_PEN));
            RoundRect(self.dc, r.x, r.y, r.x + r.w, r.y + r.h, diameter, diameter);
            SelectObject(self.dc, pen);
            SelectObject(self.dc, old);
            DeleteObject(brush);
        }
    }
    pub fn p(&self, n: f64) -> i32 {
        (n * self.scale).round_ties_even() as i32
    }
    pub fn fill(&self, r: Rect, col: Color) {
        unsafe {
            let b = CreateSolidBrush(col);
            FillRect(self.dc, &r.native(), b);
            DeleteObject(b);
        }
    }
    pub fn frame(&self, r: Rect, col: Color) {
        self.fill(Rect::new(r.x, r.y, r.w, 1), col);
        self.fill(Rect::new(r.x, r.y + r.h - 1, r.w, 1), col);
        self.fill(Rect::new(r.x, r.y, 1, r.h), col);
        self.fill(Rect::new(r.x + r.w - 1, r.y, 1, r.h), col);
    }
    pub fn text(&mut self, s: &str, r: Rect, col: Color, size: f64, bold: bool, align: u32) {
        let font = self.resources.font(self.p(size), bold);
        unsafe {
            let old = SelectObject(self.dc, font);
            SetBkMode(self.dc, TRANSPARENT as i32);
            SetTextColor(self.dc, col);
            DrawTextW(
                self.dc,
                wide(s).as_ptr(),
                -1,
                &mut r.native(),
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | align,
            );
            SelectObject(self.dc, old);
        }
    }
    pub fn measure(&mut self, s: &str, size: f64, bold: bool) -> i32 {
        let font = self.resources.font(self.p(size), bold);
        unsafe {
            let old = SelectObject(self.dc, font);
            let mut r = RECT::default();
            DrawTextW(
                self.dc,
                wide(s).as_ptr(),
                -1,
                &mut r,
                DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
            );
            SelectObject(self.dc, old);
            r.right - r.left
        }
    }
    pub fn ellipse(&self, r: Rect, col: Color) {
        unsafe {
            let b = CreateSolidBrush(col);
            let ob = SelectObject(self.dc, b);
            let op = SelectObject(self.dc, GetStockObject(NULL_PEN));
            Ellipse(self.dc, r.x, r.y, r.x + r.w, r.y + r.h);
            SelectObject(self.dc, op);
            SelectObject(self.dc, ob);
            DeleteObject(b);
        }
    }
    pub fn icon(&mut self, name: &str, r: Rect, col: Color) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        let key = (name.to_string(), r.w, col);
        if !self.resources.icons.contains_key(&key) {
            let source = if name.starts_with("wifi-level-") {
                "wifi"
            } else {
                name
            };
            let Ok(mut svg) = std::fs::read_to_string(
                self.root.join("icons/lucide").join(format!("{source}.svg")),
            ) else {
                return;
            };
            if name.starts_with("wifi-level-") {
                svg = svg
                    .lines()
                    .filter(|line| {
                        !line.contains("M2 8.82")
                            && (name == "wifi-level-2" || !line.contains("M5 12.859"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
            }
            let css = format!(
                "#{:02x}{:02x}{:02x}",
                col & 255,
                (col >> 8) & 255,
                (col >> 16) & 255
            );
            let svg = svg.replace("currentColor", &css);
            let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
            else {
                return;
            };
            let Some(mut pixels) = resvg::tiny_skia::Pixmap::new(r.w as u32, r.h as u32) else {
                return;
            };
            resvg::render(
                &tree,
                resvg::tiny_skia::Transform::from_scale(
                    r.w as f32 / tree.size().width(),
                    r.h as f32 / tree.size().height(),
                ),
                &mut pixels.as_mut(),
            );
            self.resources.icons.insert(key.clone(), pixels);
        }
        let pixels = &self.resources.icons[&key];
        unsafe {
            let bg = GetPixel(self.dc, r.x, r.y);
            let mut data = Vec::with_capacity(pixels.data().len());
            for p in pixels.data().chunks_exact(4) {
                let a = p[3] as u32;
                for (channel, shift) in [(2, 16), (1, 8), (0, 0)] {
                    data.push(
                        (p[channel] as u32 + ((bg >> shift) & 255) * (255 - a) / 255).min(255)
                            as u8,
                    );
                }
                data.push(255);
            }
            self.bitmap(r, r.w, r.h, &data);
        }
    }
    pub fn bitmap(&self, r: Rect, w: i32, h: i32, bgra: &[u8]) {
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        };
        unsafe {
            StretchDIBits(
                self.dc,
                r.x,
                r.y,
                r.w,
                r.h,
                0,
                0,
                w,
                h,
                bgra.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
        }
    }
}
pub fn render_png(
    root: &Path,
    path: &Path,
    width: i32,
    height: i32,
    scale: f64,
    ui: Palette,
    paint: impl FnOnce(&mut Canvas),
) -> anyhow::Result<()> {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(dc);
        let bm = CreateCompatibleBitmap(dc, width, height);
        let old = SelectObject(mem, bm);
        let mut resources = Resources::new();
        {
            let mut c = Canvas {
                dc: mem,
                scale,
                width,
                height,
                ui,
                resources: &mut resources,
                root,
            };
            c.fill(Rect::new(0, 0, width, height), ui.bg);
            paint(&mut c);
        }
        SelectObject(mem, old);
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let ok = GetDIBits(
            dc,
            bm,
            0,
            height as u32,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );
        DeleteObject(bm);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), dc);
        anyhow::ensure!(ok != 0, "GetDIBits failed");
        for p in pixels.chunks_exact_mut(4) {
            p.swap(0, 2);
            p[3] = 255;
        }
        image::save_buffer(
            path,
            &pixels,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_bounds() {
        let r = Rect::new(10, 20, 30, 40);
        assert!(r.contains(10, 20));
        assert!(r.contains(39, 59));
        assert!(!r.contains(40, 59));
        assert!(!r.contains(39, 60));
    }
    #[test]
    fn colors() {
        assert_eq!(parse("#123456"), 0x563412);
        assert!(contrast(0, readable(0, 0x555555, 7.)) >= 7.);
    }
}
