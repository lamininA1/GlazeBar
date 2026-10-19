use crate::{
    graphics::{self, Palette, Rect},
    ipc::{Monitor, Workspace},
    sensors::Sensors,
    views,
};
use std::path::Path;
pub fn render(root: &Path) -> anyhow::Result<()> {
    let output = root.join("ui-reference/rust");
    std::fs::create_dir_all(&output)?;
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("theme-state/glazebar.json"))?)?;
    let palette: Vec<String> = serde_json::from_value(raw["palette"].clone())?;
    for factor in [1., 1.75] {
        for light in [false, true] {
            let mut colors = palette.clone();
            if light {
                colors[0] = "#f8f8f8".into();
                colors[15] = "#202020".into();
            }
            let ui = Palette::from_colors(&colors);
            let prefix = format!(
                "{}-{}-",
                if light { "light" } else { "current" },
                (factor * 100.) as i32
            );
            let save = |name: &str, w: f64, h: f64, f: &mut dyn FnMut(&mut graphics::Canvas)| {
                graphics::render_png(
                    root,
                    &output.join(format!("{prefix}{name}.png")),
                    (w * factor).round_ties_even() as i32,
                    (h * factor).round_ties_even() as i32,
                    factor,
                    ui,
                    f,
                )
            };
            let monitor = Monitor {
                name: "reference".into(),
                bounds: Rect::new(0, 0, 1645, 40),
                scale: factor,
                workspaces: (1..=3)
                    .map(|n| Workspace {
                        name: n.to_string(),
                        display: n.to_string(),
                        focused: n == 1,
                        displayed: n == 1,
                    })
                    .collect(),
            };
            let mut sensors = Sensors {
                cpu: 12.,
                memory: 37.,
                percent: 41.,
                full: 48.2,
                remaining: 29.8,
                watts: 12.4,
                battery: true,
                discharging: true,
                known: true,
                network: "Sample Wi-Fi".into(),
                signal: 70,
                ..Default::default()
            };
            save("bar", 1645., 40., &mut |c| {
                views::bar(
                    c,
                    &monitor,
                    &sensors,
                    false,
                    false,
                    false,
                    &views::clock(),
                    Some(22),
                    Some(("sun", "17°C")),
                );
            })?;
            save(
                "brightness",
                150.,
                ((40.0 * factor).round_ties_even() - 1.) / factor,
                &mut |c| views::paint_brightness(c, Rect::new(0, 0, c.width, c.height), Some(22)),
            )?;
            save("scale", 260., 112., &mut |c| views::scale(c, 3, 4, 7, ""))?;
            let wifi = crate::wifi_ui::fixture();
            let mut comparison =
                crate::style_compare_ui::Comparison::new(Default::default(), 0, light);
            let comparison_palette: crate::theme::Palette = colors
                .clone()
                .try_into()
                .map_err(|_| anyhow::anyhow!("Invalid palette"))?;
            comparison.palettes = std::array::from_fn(|_| Some(comparison_palette.clone()));
            save("style-compare", 1120., 680., &mut |c| {
                crate::style_compare_ui::paint(c, &comparison, None);
            })?;
            let preview = crate::theme_preview_ui::Preview {
                light,
                palette: Some(
                    colors
                        .clone()
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Invalid palette"))?,
                ),
            };
            save("theme-preview", 360., 590., &mut |c| {
                crate::theme_preview_ui::paint(c, &preview)
            })?;
            let options = crate::windows_options_ui::Settings {
                enabled: true,
                options: crate::theme_windows::Options {
                    mode: true,
                    accent: true,
                    titlebars: Some(true),
                    taskbar: Some(true),
                    transparency: Some(true),
                },
                light,
                status: String::new(),
            };
            save("windows-options", 510., 435., &mut |c| {
                crate::windows_options_ui::paint(c, &options, None);
            })?;
            let mut picker = crate::wallpaper_ui::Picker::load(root)?;
            picker.light = light;
            picker.windows = true;
            picker.palette = Some(
                colors
                    .clone()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Invalid palette"))?,
            );
            picker.status = format!(
                "휠 / ← → 선택   ·   Enter 적용   ·   Esc 닫기    {} / {}",
                picker.selected + 1,
                picker.files.len()
            );
            let progress = crate::precache::Progress::new(picker.files.len());
            let online = crate::online_ui::Online::default();
            let (events, _) = std::sync::mpsc::channel();
            let online_window = crate::native::Window::new(
                "Rust offline Wallhaven",
                Rect::new(
                    -32000,
                    -32000,
                    (1000. * factor) as i32,
                    (650. * factor).round_ties_even() as i32,
                ),
                crate::native::State {
                    root: root.to_path_buf(),
                    scale: factor,
                    palette: ui,
                    scene: crate::native::Scene::Online(online.clone()),
                    resources: graphics::Resources::new(),
                    hits: vec![],
                    hover: None,
                    drag: false,
                    passive: false,
                    events,
                    wake: 0,
                },
                std::ptr::null_mut(),
            )?;
            online_window.online_layout();
            save("wallhaven-empty", 1000., 650., &mut |c| {
                crate::online_ui::paint(c, &online, None);
                unsafe {
                    use windows_sys::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::*};
                    let saved = SaveDC(c.dc);
                    SetViewportOrgEx(c.dc, c.p(145.), c.p(18.), std::ptr::null_mut());
                    windows_sys::Win32::Storage::Xps::PrintWindow(
                        GetDlgItem(online_window.hwnd, 101),
                        c.dc,
                        0,
                    );
                    RestoreDC(c.dc, saved);
                }
            })?;
            drop(online_window);
            save("precache", 560., 310., &mut |c| {
                crate::precache::paint(c, &progress, None);
            })?;
            for path in picker.neighbors() {
                if let Ok(im) = crate::wallpaper_ui::thumbnail(&path) {
                    picker.images.insert(path, std::sync::Arc::new(im));
                }
            }
            let mut picker_hits = vec![];
            save("wallpaper", 1000., 590., &mut |c| {
                picker_hits=crate::wallpaper_ui::paint(c, &picker, None).into_iter().map(|h| serde_json::json!({"bounds":{"L":h.rect.x,"T":h.rect.y,"R":h.rect.x+h.rect.w,"B":h.rect.y+h.rect.h},"cursor":"Hand"})).collect::<Vec<_>>();
            })?;
            let controls: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(
                root.join("ui-reference/go/controls.json"),
            )?)?;
            let expected = controls
                .iter()
                .find(|v| v["name"] == format!("{prefix}wallpaper"))
                .ok_or_else(|| anyhow::anyhow!("Missing Go picker hit reference"))?;
            anyhow::ensure!(
                expected["clickable"] == serde_json::json!(picker_hits),
                "Picker clickable bounds differ from Go at {prefix}"
            );
            std::fs::write(
                output.join(format!("{prefix}wallpaper-controls.json")),
                serde_json::to_vec_pretty(&picker_hits)?,
            )?;
            let sheet =
                crate::sheet::Sheet::parse(&std::fs::read_to_string(root.join("cheatsheet.tsv"))?);
            save("cheatsheet", 940., 850., &mut |c| {
                crate::sheet::paint(c, &sheet, None);
            })?;
            save("wifi", 420., wifi.height(), &mut |c| {
                crate::wifi_ui::paint(c, &wifi, None);
            })?;
            let mut profiles = crate::power::read()?;
            profiles.active = profiles.ids[1].clone();
            save("power", 240., 136., &mut |c| {
                views::power(c, &profiles, None);
            })?;
            for index in 0..3 {
                for hover in [false, true] {
                    save(
                        &format!(
                            "power-{}-{}",
                            ["performance", "balance", "save"][index],
                            if hover { "hover" } else { "normal" }
                        ),
                        224.,
                        40.,
                        &mut |c| {
                            views::power_button(
                                c,
                                Rect::new(0, 0, c.width, c.height),
                                index,
                                index == 1,
                                hover,
                            )
                        },
                    )?;
                }
            }
            for state in ["discharging", "charging", "unknown"] {
                sensors.known = state != "unknown";
                sensors.charging = state == "charging";
                sensors.discharging = state != "charging";
                save(
                    &format!("battery-{state}"),
                    240.,
                    if sensors.discharging { 110. } else { 132. },
                    &mut |c| views::battery(c, &sensors),
                )?;
            }
            for state in ["normal", "hover", "disabled"] {
                save(&format!("button-{state}"), 140., 30., &mut |c| {
                    views::button(
                        c,
                        &mut vec![],
                        "스타일 · 기본",
                        Rect::new(0, 0, c.width, c.height),
                        false,
                        state == "hover",
                        state != "disabled",
                        views::Action::Close,
                    )
                })?;
            }
        }
    }
    Ok(())
}

pub fn compare(root: &Path) -> anyhow::Result<()> {
    let output = root.join("ui-reference/rust");
    let mut rows = vec![];
    for entry in std::fs::read_dir(&output)? {
        let entry = entry?;
        if entry.path().extension().and_then(|v| v.to_str()) != Some("png") {
            continue;
        }
        let name = entry.file_name();
        let baseline = root.join("ui-reference/go").join(&name);
        if !baseline.exists() {
            continue;
        }
        let old = image::open(baseline)?.to_rgba8();
        let new = image::open(entry.path())?.to_rgba8();
        let same_size = old.dimensions() == new.dimensions();
        let mut error = 0u64;
        let mut changed = 0u64;
        let wallpaper = name.to_string_lossy().ends_with("-wallpaper.png");
        let factor = new.width() as f64 / 1000.;
        let mut changed_controls = 0u64;
        if same_size {
            for (index, (a, b)) in old.pixels().zip(new.pixels()).enumerate() {
                let delta: u64 = (0..3).map(|i| a[i].abs_diff(b[i]) as u64).sum();
                error += delta;
                if delta != 0 {
                    changed += 1;
                    let y = (index / new.width() as usize) as i32;
                    if wallpaper
                        && (y < (66. * factor).round_ties_even() as i32
                            || y >= new.height() as i32 - (174. * factor).round_ties_even() as i32)
                    {
                        changed_controls += 1;
                    }
                }
            }
        }
        rows.push(serde_json::json!({"name":name.to_string_lossy(),"sameSize":same_size,"width":new.width(),"height":new.height(),"changedPixels":changed,"meanChannelError":if same_size{Some(error as f64/(old.width()as f64*old.height()as f64*3.))}else{None}}));
        if wallpaper {
            rows.last_mut().unwrap()["changedPixelsOutsideImageBand"] =
                serde_json::json!(changed_controls);
        }
    }
    rows.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_owned());
    std::fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&rows)?,
    )?;
    println!(
        "Compared {} Rust/Go image pairs (clock text is not masked).",
        rows.len()
    );
    Ok(())
}
