use crate::graphics::{Canvas, Rect};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const MODES: [&str; 6] = [
    "normal",
    "muted",
    "pastel",
    "material",
    "colorful",
    "high-contrast",
];
pub const PRESET_MODES: [&str; 6] = [
    "catppuccin",
    "everforest",
    "rose-pine",
    "monokai",
    "dracula",
    "tokyo-night",
];
pub const ALL_MODES: [&str; 12] = [
    "normal",
    "muted",
    "pastel",
    "material",
    "colorful",
    "high-contrast",
    "catppuccin",
    "everforest",
    "rose-pine",
    "monokai",
    "dracula",
    "tokyo-night",
];
pub const NAMES: [&str; 12] = [
    "기본",
    "차분하게",
    "파스텔",
    "머티리얼",
    "선명하게",
    "고대비",
    "Catppuccin",
    "Everforest",
    "Rosé Pine",
    "Monokai",
    "Dracula",
    "Tokyo Night",
];
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Select(i32),
    Enabled,
    Mode,
    Light(bool),
    Online,
    Folder,
    Target(usize),
    Windows,
    Precache,
    Compare,
    Restore,
    Apply,
}
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}
#[derive(Clone)]
pub struct Picker {
    pub operation: u64,
    pub revision: u64,
    pub loaded: std::collections::HashSet<PathBuf>,
    pub files: Vec<PathBuf>,
    pub selected: usize,
    pub images: HashMap<PathBuf, Arc<image::RgbaImage>>,
    pub mode: usize,
    pub light: bool,
    pub enabled: bool,
    pub targets: [bool; 4],
    pub extra_targets: [bool; 8],
    pub windows: bool,
    pub options: crate::theme_windows::Options,
    pub busy: bool,
    pub palette: Option<crate::theme::Palette>,
    pub status: String,
}
impl Picker {
    pub fn load(root: &Path) -> anyhow::Result<Self> {
        let home = PathBuf::from(
            std::env::var_os("USERPROFILE")
                .ok_or_else(|| anyhow::anyhow!("USERPROFILE missing"))?,
        );
        let system = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or_else(|| anyhow::anyhow!("SystemRoot missing"))?,
        );
        let current = crate::wallpaper::current()?;
        let files = crate::wallpaper::files(
            &crate::wallpaper::folder(root, &home),
            &system.join("Web").join("Wallpaper"),
            &current,
        );
        let key = |p: &Path| {
            p.components()
                .collect::<PathBuf>()
                .to_string_lossy()
                .to_lowercase()
        };
        let selected = files
            .iter()
            .position(|p| key(p) == key(&current))
            .unwrap_or(0);
        let preferences: serde_json::Value =
            std::fs::read(root.join("theme-state/windows-preferences.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
        let bar_settings = crate::settings_ui::BarSettings::load(root);
        let targets = [
            bar_settings.targets[0],
            bar_settings.targets[1],
            bar_settings.targets[2],
            bar_settings.targets[3],
        ];
        let extra_targets = [
            bar_settings.targets[4],
            bar_settings.targets[5],
            bar_settings.targets[6],
            bar_settings.targets[7],
            bar_settings.targets[8],
            bar_settings.targets[9],
            bar_settings.targets[10],
            bar_settings.targets[11],
        ];
        Ok(Self {
            operation: 0,
            revision: 0,
            loaded: Default::default(),
            files,
            selected,
            images: HashMap::new(),
            mode: 0,
            light: false,
            enabled: true,
            targets,
            extra_targets,
            windows: preferences["enabled"].as_bool().unwrap_or(false),
            options: serde_json::from_value(preferences["options"].clone()).unwrap_or(
                crate::theme_windows::Options {
                    mode: true,
                    accent: true,
                    ..Default::default()
                },
            ),
            busy: false,
            palette: None,
            status: String::new(),
        })
    }
    pub fn select(&mut self, delta: i32) {
        if !self.busy && !self.files.is_empty() {
            self.selected = (self.selected as i64 + i64::from(delta))
                .rem_euclid(self.files.len() as i64) as usize;
            if self.mode < MODES.len() {
                self.palette = None;
            }
            self.status.clear();
        }
    }
    pub fn neighbors(&self) -> Vec<PathBuf> {
        if self.files.is_empty() {
            return vec![];
        }
        let mut paths = vec![];
        for d in [0, -1, 1] {
            let i = (self.selected as i64 + d).rem_euclid(self.files.len() as i64) as usize;
            if !paths.contains(&self.files[i]) {
                paths.push(self.files[i].clone());
            }
        }
        paths
    }
}

pub fn thumbnail(path: &Path) -> anyhow::Result<image::RgbaImage> {
    thumbnail_in(path, crate::ui_cache::directory("thumbnail").as_deref())
}
pub(crate) fn thumbnail_in(path: &Path, dir: Option<&Path>) -> anyhow::Result<image::RgbaImage> {
    let key = crate::ui_cache::thumbnail_key(path);
    let cache = dir
        .zip(key.as_deref())
        .map(|(dir, key)| dir.join(format!("{key}.jpg")));
    if let Some(cache) = &cache {
        if let Ok(image) = read_thumbnail(&cache, true) {
            return Ok(image);
        }
    }
    let image = read_thumbnail(path, false)?;
    if key == crate::ui_cache::thumbnail_key(path) {
        if let Some(cache) = cache {
            let save = || -> anyhow::Result<()> {
                let rgb: Vec<u8> = image
                    .pixels()
                    .flat_map(|p| {
                        [p[0], p[1], p[2]].map(|v| (u16::from(v) * u16::from(p[3]) / 255) as u8)
                    })
                    .collect();
                let mut bytes = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 90).encode(
                    &rgb,
                    image.width(),
                    image.height(),
                    image::ExtendedColorType::Rgb8,
                )?;
                crate::theme_transaction::atomic_write(&cache, &bytes)
            };
            if let Err(e) = save() {
                eprintln!("[thumbnail cache] write failed: {e}");
            }
        }
    }
    Ok(image)
}
fn read_thumbnail(path: &Path, cached: bool) -> anyhow::Result<image::RgbaImage> {
    use image::ImageDecoder;
    let decoder = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let (w, h) = decoder.dimensions();
    anyhow::ensure!(
        !cached || (w <= 640 && h <= 640),
        "Invalid thumbnail cache dimensions"
    );
    anyhow::ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 80_000_000,
        "Image too large"
    );
    let im = image::DynamicImage::from_decoder(decoder)?.into_rgba8();
    let ratio = (640. / f64::from(w.max(h))).min(1.);
    Ok(image::imageops::resize(
        &im,
        (f64::from(w) * ratio).max(1.) as u32,
        (f64::from(h) * ratio).max(1.) as u32,
        image::imageops::FilterType::CatmullRom,
    ))
}

fn crop(c: &Canvas, im: &image::RgbaImage, r: Rect, dim: bool) {
    let (w, h) = im.dimensions();
    let ratio = (f64::from(r.w) / f64::from(w)).max(f64::from(r.h) / f64::from(h));
    let cw = (f64::from(r.w) / ratio).max(1.) as u32;
    let ch = (f64::from(r.h) / ratio).max(1.) as u32;
    let mut bgra = Vec::with_capacity((cw * ch * 4) as usize);
    for y in (h - ch) / 2..(h - ch) / 2 + ch {
        for x in (w - cw) / 2..(w - cw) / 2 + cw {
            let p = im.get_pixel(x, y).0;
            for k in [2, 1, 0] {
                let v = u32::from(p[k]) * u32::from(p[3]) / 255;
                let v = if dim { v * 155 / 255 } else { v };
                let background = (c.ui.bg >> (k * 8)) & 255;
                bgra.push((v + background * (255 - u32::from(p[3])) / 255) as u8);
            }
            bgra.push(p[3]);
        }
    }
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::{HALFTONE, SetStretchBltMode};
        let previous = SetStretchBltMode(c.dc, HALFTONE);
        c.bitmap(r, cw as i32, ch as i32, &bgra);
        SetStretchBltMode(c.dc, previous);
    }
}
fn button(
    c: &mut Canvas,
    hits: &mut Vec<Hit>,
    text: &str,
    r: Rect,
    selected: bool,
    action: Action,
    hover: Option<(i32, i32)>,
    busy: bool,
) {
    let over = hover.is_some_and(|(x, y)| r.contains(x, y));
    // Go keeps the normal appearance while a picker operation is busy.
    crate::views::button(
        c,
        &mut vec![],
        text,
        r,
        selected,
        over,
        true,
        crate::views::Action::Close,
    );
    if !busy {
        hits.push(Hit { rect: r, action });
    }
}
fn toggle(
    c: &mut Canvas,
    hits: &mut Vec<Hit>,
    label: &str,
    r: Rect,
    checked: bool,
    action: Action,
    busy: bool,
) {
    let side = c.p(10.);
    let y = r.y + (r.h - side) / 2;
    let col = if checked { c.ui.accent } else { c.ui.border };
    c.frame(Rect::new(r.x, y, side, side), col);
    if checked {
        let inset = c.p(3.);
        c.fill(
            Rect::new(r.x + inset, y + inset, side - 2 * inset, side - 2 * inset),
            col,
        );
    }
    c.text(
        label,
        Rect::new(r.x + side + c.p(8.), r.y, r.w - side - c.p(8.), r.h),
        c.ui.text,
        12.,
        false,
        0,
    );
    if !busy {
        hits.push(Hit { rect: r, action });
    }
}
pub fn paint(c: &mut Canvas, p: &Picker, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let mut hits = vec![];
    let scale = c.scale;
    let s = |n: f64| (n * scale).round_ties_even() as i32;
    let w = c.width;
    let h = c.height;
    c.text(
        "배경화면",
        Rect::new(s(28.), s(19.), s(115.), s(28.)),
        c.ui.text,
        17.,
        true,
        0,
    );
    toggle(
        c,
        &mut hits,
        "색상 함께 적용",
        Rect::new(s(145.), s(18.), s(140.), s(30.)),
        p.enabled,
        Action::Enabled,
        p.busy,
    );
    let mode_prefix = if p.mode >= MODES.len() {
        "테마"
    } else {
        "스타일"
    };
    button(
        c,
        &mut hits,
        &format!("{mode_prefix} · {}", NAMES[p.mode.min(NAMES.len() - 1)]),
        Rect::new(s(295.), s(18.), s(140.), s(30.)),
        p.mode >= MODES.len(),
        Action::Mode,
        hover,
        p.busy,
    );
    for (label, x, light) in [("다크", 450., false), ("라이트", 530., true)] {
        button(
            c,
            &mut hits,
            label,
            Rect::new(s(x), s(18.), s(72.), s(30.)),
            p.light == light,
            Action::Light(light),
            hover,
            p.busy,
        );
    }
    button(
        c,
        &mut hits,
        "온라인 찾기",
        Rect::new(w - s(228.), s(17.), s(100.), s(30.)),
        false,
        Action::Online,
        hover,
        p.busy,
    );
    button(
        c,
        &mut hits,
        "폴더 선택",
        Rect::new(w - s(116.), s(17.), s(96.), s(30.)),
        false,
        Action::Folder,
        hover,
        p.busy,
    );
    let mw = (f64::from(w) * 0.68) as i32;
    let mh = h - s(240.);
    let y = s(66.);
    let side = s(30.).max((w - mw) / 2 - s(30.));
    let center = Rect::new((w - mw) / 2, y, mw, mh);
    let cards = [
        Rect::new(s(14.), y + s(12.), side, mh - s(24.)),
        center,
        Rect::new(w - side - s(14.), y + s(12.), side, mh - s(24.)),
    ];
    if p.files.is_empty() {
        c.text("사진 폴더를 선택해주세요", center, c.ui.text, 12., false, 1);
    } else {
        for (i, r) in cards.into_iter().enumerate() {
            if p.files.len() == 1 && i != 1 {
                continue;
            }
            let index = (p.selected + i + p.files.len() - 1) % p.files.len();
            c.fill(r, c.ui.surface);
            if let Some(im) = p.images.get(&p.files[index]) {
                crop(c, im, r, i != 1);
            }
            let col = if i == 1 { c.ui.accent } else { c.ui.border };
            let width = if i == 1 { s(3.) } else { 1 };
            for inset in 0..width {
                c.frame(
                    Rect::new(r.x + inset, r.y + inset, r.w - 2 * inset, r.h - 2 * inset),
                    col,
                );
            }
            if i != 1 && !p.busy {
                hits.push(Hit {
                    rect: r,
                    action: Action::Select(i as i32 - 1),
                });
            }
        }
        let label = p.files[p.selected]
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        c.text(
            if label == "TranscodedWallpaper" {
                "현재 배경화면"
            } else {
                &label
            },
            Rect::new(s(24.), h - s(165.), w - s(48.), s(24.)),
            c.ui.text,
            12.,
            false,
            1,
        );
    }
    if !p.enabled {
        c.text(
            "배경화면만 적용",
            Rect::new(s(24.), h - s(126.), w - s(48.), s(18.)),
            c.ui.text,
            12.,
            false,
            1,
        );
    } else if let Some(palette) = &p.palette {
        let cell = (w - s(48.)) / 16;
        for (i, col) in palette.iter().enumerate() {
            c.fill(
                Rect::new(s(24.) + i as i32 * cell, h - s(126.), cell + 1, s(18.)),
                crate::graphics::parse(col),
            );
        }
    }
    for (i, label) in ["GlazeBar", "Terminal", "Codex CLI", "VS Code"]
        .iter()
        .enumerate()
    {
        toggle(
            c,
            &mut hits,
            label,
            Rect::new(s((24 + i * 88) as f64), h - s(84.), s(86.), s(30.)),
            p.targets[i],
            Action::Target(i),
            p.busy,
        );
    }
    button(
        c,
        &mut hits,
        if p.windows {
            "Windows · 켬"
        } else {
            "Windows · 끔"
        },
        Rect::new(s(382.), h - s(84.), s(94.), s(30.)),
        p.windows,
        Action::Windows,
        hover,
        p.busy,
    );
    for (label, right, width, action) in [
        ("전체 캐시 생성", 364., 124., Action::Precache),
        ("스타일 비교", 228., 100., Action::Compare),
        ("색상 복원", 116., 96., Action::Restore),
    ] {
        button(
            c,
            &mut hits,
            label,
            Rect::new(w - s(right), h - s(84.), s(width), s(30.)),
            false,
            action,
            hover,
            p.busy,
        );
    }
    let mut footer = p.status.clone();
    if footer.is_empty() {
        footer = format!(
            "휠 / ← → 선택   ·   Enter 적용   ·   Esc 닫기    {} / {}",
            p.selected + 1,
            p.files.len()
        );
        if p.loaded.len() < p.files.len() {
            footer += &format!(" · 미리보기 {}/{}", p.loaded.len(), p.files.len());
        }
    }
    let memory: usize = p.images.values().map(|im| im.len()).sum();
    let col = if memory >= 256 << 20 {
        footer = format!(
            "미리보기 메모리 {}MB · 이미지가 많습니다. 사용하지 않는 이미지를 정리해주세요.",
            memory >> 20
        );
        c.ui.warning
    } else {
        c.ui.muted
    };
    c.text(
        &footer,
        Rect::new(s(24.), h - s(43.), w - s(48.), s(24.)),
        col,
        12.,
        false,
        1,
    );
    c.frame(Rect::new(0, 0, w, h), c.ui.border);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_cache_is_written_and_reused() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.png");
        image::RgbaImage::from_pixel(800, 400, image::Rgba([100, 80, 60, 128]))
            .save(&source)
            .unwrap();
        let fresh = thumbnail_in(&source, Some(dir.path())).unwrap();
        assert_eq!(fresh.dimensions(), (640, 320));
        let cache = dir.path().join(format!(
            "{}.jpg",
            crate::ui_cache::thumbnail_key(&source).unwrap()
        ));
        let bytes = std::fs::read(&cache).unwrap();
        assert_eq!(&bytes[..2], &[255, 216]);
        let stored = image::open(&cache).unwrap().into_rgba8();
        assert_eq!(thumbnail_in(&source, Some(dir.path())).unwrap(), stored);
        assert_eq!(std::fs::read(cache).unwrap(), bytes);
        let pixel = stored.get_pixel(100, 100);
        assert!(
            pixel[0].abs_diff(50) <= 3 && pixel[1].abs_diff(40) <= 3 && pixel[2].abs_diff(30) <= 3
        );
    }
    #[test]
    fn navigation_wraps_and_busy_preserves_selection() {
        let mut p = Picker {
            operation: 0,
            revision: 0,
            loaded: Default::default(),
            files: vec!["a".into(), "b".into(), "c".into()],
            selected: 0,
            images: Default::default(),
            mode: 0,
            light: false,
            enabled: true,
            targets: [true; 4],
            extra_targets: [true; 8],
            windows: false,
            options: Default::default(),
            busy: false,
            palette: None,
            status: String::new(),
        };
        p.select(-1);
        assert_eq!(p.selected, 2);
        assert_eq!(
            p.neighbors(),
            vec![PathBuf::from("c"), "b".into(), "a".into()]
        );
        p.select(4);
        assert_eq!(p.selected, 0);
        p.busy = true;
        p.select(1);
        assert_eq!(p.selected, 0);
        p.busy = false;
        p.files.truncate(1);
        assert_eq!(p.neighbors().len(), 1);
        p.files.clear();
        p.select(-1);
        assert!(p.neighbors().is_empty());
    }
}
