use crate::{
    graphics::{Canvas, Rect},
    ipc::Monitor,
    sensors::Sensors,
};
use chrono::{Datelike, Timelike};
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Command(String),
    Scale,
    Power,
    PowerSelect(String),
    Wifi,
    WifiScan(bool),
    WifiSelect(usize),
    WifiConnect,
    WifiSettings,
    WifiScroll(i32),
    Brightness,
    Battery,
    Close,
    Sheet,
    Wallpaper,
    PrecacheStop,
    Online(crate::online_ui::Edit),
    Compare(crate::style_compare_ui::Edit),
    WindowsOption(crate::windows_options_ui::Edit),
    Picker(crate::wallpaper_ui::Action),
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}
fn hit(hits: &mut Vec<Hit>, rect: Rect, action: Action) {
    hits.push(Hit { rect, action });
}
fn metric(
    c: &mut Canvas,
    right: &mut i32,
    hits: &mut Vec<Hit>,
    icon: &str,
    text: &str,
    color: u32,
    action: Option<Action>,
) -> Rect {
    let width = c.p(24.) + c.measure(text, 12.5, false);
    let r = Rect::new(*right - width, 0, width, c.height);
    c.icon(
        icon,
        Rect::new(r.x, (c.height - c.p(17.)) / 2, c.p(17.), c.p(17.)),
        color,
    );
    c.text(
        text,
        Rect::new(r.x + c.p(24.), 0, width - c.p(24.), c.height),
        color,
        12.5,
        false,
        2,
    );
    if let Some(action) = action {
        hit(hits, r, action)
    }
    *right = r.x - c.p(18.);
    r
}
pub fn bar(
    c: &mut Canvas,
    m: &Monitor,
    s: &Sensors,
    paused: bool,
    resize: bool,
    vertical: bool,
    clock: &str,
    brightness: Option<u8>,
    weather: Option<(&str, &str)>,
) -> Vec<Hit> {
    let mut hits = vec![];
    let ui = c.ui;
    let h = c.height;
    let mut left = c.p(18.);
    for x in 0..2 {
        for y in 0..2 {
            c.fill(
                Rect::new(left + x * c.p(8.), c.p(13.) + y * c.p(8.), c.p(6.), c.p(6.)),
                ui.muted,
            )
        }
    }
    left += c.p(34.);
    for ws in &m.workspaces {
        let width = c.p(28.).max(c.measure(&ws.display, 12.5, true) + c.p(16.));
        let r = Rect::new(left, c.p(8.), width, h - c.p(16.));
        c.fill(
            r,
            if ws.focused || ws.displayed {
                ui.selected
            } else {
                ui.surface
            },
        );
        c.text(&ws.display, r, ui.text, 12.5, true, 1);
        hit(
            &mut hits,
            r,
            Action::Command(format!("focus --workspace {}", ws.name)),
        );
        left += width + c.p(4.);
    }
    let mut right = c.width - c.p(18.);
    if let Some((icon, text)) = weather {
        metric(c, &mut right, &mut hits, icon, text, ui.muted, None);
    }
    if s.battery {
        let mut vw = 0;
        for d in 0..10 {
            for sign in ["+", "−"] {
                vw = vw.max(c.measure(&format!("{sign}{d}{d}.{d}"), 12.5, true));
            }
        }
        let uw = c.measure("W", 12.5, false);
        let r = Rect::new(right - c.p(26.) - vw - uw, 0, c.p(26.) + vw + uw, h);
        let color = if s.charging {
            ui.success
        } else if s.discharging {
            ui.warning
        } else {
            ui.muted
        };
        let value = if s.known {
            format!(
                "{}{:.1}",
                if s.charging {
                    "+"
                } else if s.discharging {
                    "−"
                } else {
                    ""
                },
                s.watts
            )
        } else {
            "—".into()
        };
        c.icon(
            "zap",
            Rect::new(r.x, (h - c.p(15.)) / 2, c.p(15.), c.p(15.)),
            color,
        );
        c.text(
            &value,
            Rect::new(r.x + c.p(22.), 0, vw, h),
            color,
            12.5,
            true,
            2,
        );
        c.text(
            "W",
            Rect::new(r.x + c.p(26.) + vw, 0, uw, h),
            color,
            12.5,
            false,
            2,
        );
        hit(&mut hits, r, Action::Battery);
        right = r.x - c.p(18.);
        let icon = if s.percent >= 65. {
            "battery-full"
        } else if s.percent >= 30. {
            "battery-medium"
        } else if s.percent > 15. {
            "battery-low"
        } else {
            "battery"
        };
        metric(
            c,
            &mut right,
            &mut hits,
            icon,
            &format!("{:.0}%", s.percent),
            ui.text,
            Some(Action::Power),
        );
    }
    metric(
        c,
        &mut right,
        &mut hits,
        "cpu",
        &format!("{:.0}%", s.cpu),
        ui.text,
        None,
    );
    metric(
        c,
        &mut right,
        &mut hits,
        "memory-stick",
        &format!("{:.0}%", s.memory),
        ui.text,
        None,
    );
    let network = if s.network.is_empty() {
        "Wi-Fi".to_string()
    } else if s.network.chars().count() > 24 {
        format!("{}…", s.network.chars().take(23).collect::<String>())
    } else {
        s.network.clone()
    };
    metric(
        c,
        &mut right,
        &mut hits,
        "wifi",
        &network,
        ui.muted,
        Some(Action::Wifi),
    );
    let r = Rect::new(right - c.p(30.), c.p(8.), c.p(30.), h - c.p(16.));
    c.fill(r, ui.hover);
    c.icon(
        if vertical {
            "arrow-up-down"
        } else {
            "arrow-left-right"
        },
        Rect::new(r.x + c.p(7.), r.y + c.p(4.), c.p(16.), c.p(16.)),
        ui.text,
    );
    hit(
        &mut hits,
        r,
        Action::Command("toggle-tiling-direction".into()),
    );
    right = r.x - c.p(12.);
    if paused {
        let r = Rect::new(right - c.p(78.), c.p(8.), c.p(78.), h - c.p(16.));
        button(
            c,
            &mut hits,
            "일시 중지",
            r,
            true,
            false,
            true,
            Action::Command("wm-toggle-pause".into()),
        );
        right -= c.p(90.);
    }
    if resize {
        button(
            c,
            &mut hits,
            "크기 조절 중",
            Rect::new(right - c.p(100.), c.p(8.), c.p(100.), h - c.p(16.)),
            true,
            false,
            true,
            Action::Command("wm-disable-binding-mode --name resize".into()),
        );
    }
    let dw = c.measure(clock, 12.5, true);
    let dx = (c.width - dw) / 2;
    c.text(clock, Rect::new(dx, 0, dw, h), ui.text, 12.5, true, 0);
    let sr = Rect::new(dx - c.p(40.), (h - c.p(28.)) / 2, c.p(28.), c.p(28.));
    if sr.x > left {
        c.fill(sr, ui.surface);
        c.icon(
            "scaling",
            Rect::new(sr.x + c.p(6.), sr.y + c.p(6.), c.p(16.), c.p(16.)),
            ui.muted,
        );
        hit(&mut hits, sr, Action::Scale);
    }
    let br = Rect::new(sr.x - c.p(162.), 0, c.p(150.), h);
    if br.x > left {
        paint_brightness(c, br, brightness);
        if brightness.is_some() {
            hit(&mut hits, br, Action::Brightness);
        }
    }
    c.fill(Rect::new(0, h - 1, c.width, 1), ui.border);
    hits
}
pub fn clock() -> String {
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap());
    let day =
        ["일", "월", "화", "수", "목", "금", "토"][now.weekday().num_days_from_sunday() as usize];
    format!(
        "{}년 {}월 {}일 ({}) {:02}:{:02}",
        now.year(),
        now.month(),
        now.day(),
        day,
        now.hour(),
        now.minute()
    )
}
pub fn button(
    c: &mut Canvas,
    hits: &mut Vec<Hit>,
    text: &str,
    r: Rect,
    selected: bool,
    hover: bool,
    enabled: bool,
    action: Action,
) {
    let ui = c.ui;
    c.fill(
        r,
        if selected {
            ui.selected
        } else if hover && enabled {
            ui.hover
        } else {
            ui.surface
        },
    );
    c.frame(
        r,
        if selected || (hover && enabled) {
            ui.accent
        } else {
            ui.border
        },
    );
    c.text(
        text,
        r,
        if enabled {
            ui.text
        } else {
            crate::graphics::blend(ui.bg, ui.text, 0.38)
        },
        12.,
        false,
        1,
    );
    if enabled {
        hit(hits, r, action)
    }
}
pub fn paint_brightness(c: &mut Canvas, r: Rect, value: Option<u8>) {
    let ui = c.ui;
    let cy = r.y + r.h / 2;
    let start = r.x + c.p(25.);
    let width = r.w - c.p(66.);
    c.icon(
        "sun",
        Rect::new(r.x, r.y + (r.h - c.p(16.)) / 2, c.p(16.), c.p(16.)),
        if value.is_some() { ui.muted } else { 0x696969 },
    );
    c.fill(Rect::new(start, cy - c.p(1.), width, c.p(2.)), ui.border);
    if let Some(v) = value {
        c.fill(
            Rect::new(start, cy - c.p(1.), width * i32::from(v) / 100, c.p(2.)),
            ui.accent,
        );
    }
    for i in 0..=10 {
        c.fill(
            Rect::new(
                start + width * i / 10 - c.p(1.),
                cy + c.p(6.),
                c.p(2.),
                c.p(2.),
            ),
            ui.border,
        )
    }
    if let Some(v) = value {
        let x = start + width * i32::from(v) / 100;
        c.fill(
            Rect::new(x - c.p(3.), cy - c.p(5.), c.p(6.), c.p(10.)),
            ui.text,
        );
    }
    c.text(
        &value.map(|v| format!("{v}%")).unwrap_or("—".into()),
        Rect::new(r.x + r.w - c.p(35.), r.y, c.p(35.), r.h),
        ui.muted,
        12.,
        false,
        2,
    );
}
pub const SCALES: [i32; 12] = [100, 125, 150, 175, 200, 225, 250, 300, 350, 400, 450, 500];
pub fn scale(c: &mut Canvas, current: usize, recommended: usize, max: usize, error: &str) {
    let ui = c.ui;
    let r = |c: &Canvas, x: f64, y: f64, w: f64, h: f64| Rect::new(c.p(x), c.p(y), c.p(w), c.p(h));
    c.text(
        "디스플레이 배율",
        r(c, 16., 12., 160., 24.),
        ui.text,
        13.,
        true,
        0,
    );
    c.text(
        &format!("{}%", SCALES[current]),
        Rect::new(c.width - c.p(80.), c.p(12.), c.p(64.), c.p(24.)),
        ui.accent,
        13.,
        true,
        2,
    );
    let left = c.p(16.);
    let width = c.width - c.p(32.);
    let cy = c.p(60.) + c.p(4.) / 2;
    let x = left + width * current as i32 / max.max(1) as i32;
    c.fill(Rect::new(left, cy - c.p(2.), width, c.p(4.)), ui.border);
    c.fill(Rect::new(left, cy - c.p(2.), x - left, c.p(4.)), ui.accent);
    for i in 0..=max {
        let d = left + width * i as i32 / max.max(1) as i32;
        c.ellipse(
            Rect::new(d - c.p(2.), cy - c.p(2.), c.p(4.), c.p(4.)),
            if i <= current { ui.accent } else { ui.border },
        );
    }
    c.ellipse(
        Rect::new(x - c.p(5.), cy - c.p(5.), c.p(10.), c.p(10.)),
        ui.text,
    );
    let label = Rect::new(c.p(16.), c.p(82.), c.width - c.p(32.), c.p(18.));
    if error.is_empty() {
        c.text("100%", label, ui.muted, 11., false, 0);
        c.text(
            &format!("권장 {}%", SCALES[recommended]),
            label,
            ui.muted,
            11.,
            false,
            1,
        );
        c.text(&format!("{}%", SCALES[max]), label, ui.muted, 11., false, 2);
    } else {
        c.text(error, label, ui.error, 11., false, 0);
    }
    c.frame(Rect::new(0, 0, c.width, c.height), ui.border);
}
pub fn battery(c: &mut Canvas, s: &Sensors) {
    let ui = c.ui;
    let compact = if s.discharging && !s.charging {
        22.
    } else {
        0.
    };
    let col = if s.charging {
        ui.success
    } else if s.discharging {
        ui.warning
    } else {
        ui.muted
    };
    let value = if s.known {
        format!(
            "{}{:.1} W",
            if s.charging {
                "+"
            } else if s.discharging {
                "−"
            } else {
                ""
            },
            s.watts
        )
    } else {
        "— W".into()
    };
    c.icon(
        "zap",
        Rect::new(c.p(16.), c.p(21.), c.p(18.), c.p(18.)),
        col,
    );
    c.text(
        &value,
        Rect::new(c.p(44.), c.p(12.), c.width - c.p(60.), c.p(36.)),
        col,
        25.,
        false,
        0,
    );
    let label = if s.charging {
        "충전 중"
    } else if s.discharging {
        ""
    } else {
        "대기 중"
    };
    c.text(
        label,
        Rect::new(c.p(46.), c.p(49.), c.width - c.p(60.), c.p(18.)),
        ui.muted,
        12.,
        false,
        0,
    );
    c.fill(
        Rect::new(c.p(16.), c.p(73. - compact), c.width - c.p(32.), 1),
        ui.border,
    );
    let remaining = if s.known && s.watts > 0. && (s.charging || s.discharging) {
        let hours = if s.charging {
            (s.full - s.remaining).max(0.) / s.watts
        } else {
            s.remaining / s.watts
        };
        if (0.0..1000.0).contains(&hours) {
            let mins = (hours * 60.).round_ties_even() as i32;
            format!("약 {}시간 {}분", mins / 60, mins % 60)
        } else {
            "—".into()
        }
    } else {
        "—".into()
    };
    for (label, text, y) in [
        (
            "배터리",
            format!("{:.1} / {:.1} Wh", s.remaining, s.full),
            82. - compact,
        ),
        (
            if s.charging {
                "완충까지"
            } else {
                "남은 시간"
            },
            remaining,
            105. - compact,
        ),
    ] {
        c.text(
            label,
            Rect::new(c.p(16.), c.p(y), c.p(72.), c.p(18.)),
            ui.muted,
            12.,
            false,
            0,
        );
        c.text(
            &text,
            Rect::new(c.p(66.), c.p(y), c.width - c.p(82.), c.p(18.)),
            ui.text,
            12.,
            false,
            2,
        );
    }
    c.frame(Rect::new(0, 0, c.width, c.height), ui.border);
}
pub fn power_button(c: &mut Canvas, r: Rect, index: usize, selected: bool, hover: bool) {
    let ui = c.ui;
    c.fill(
        r,
        if selected {
            ui.selected
        } else if hover {
            ui.hover
        } else {
            ui.bg
        },
    );
    let col = [ui.warning, ui.accent, ui.success][index];
    if selected {
        c.fill(Rect::new(r.x, r.y + c.p(10.), c.p(2.), r.h - c.p(20.)), col);
    }
    c.icon(
        ["zap", "arrow-left-right", "battery-full"][index],
        Rect::new(
            r.x + c.p(14.),
            r.y + (r.h - c.p(17.)) / 2,
            c.p(17.),
            c.p(17.),
        ),
        col,
    );
    c.text(
        ["고성능", "균형", "절전"][index],
        Rect::new(r.x + c.p(44.), r.y, r.w - c.p(76.), r.h),
        ui.text,
        14.,
        false,
        0,
    );
    if selected || hover {
        c.text(
            if selected { "✓" } else { ">" },
            Rect::new(r.x + r.w - c.p(32.), r.y, c.p(24.), r.h),
            col,
            if selected { 18. } else { 14. },
            false,
            1,
        );
    }
}
pub fn power(c: &mut Canvas, p: &crate::power::Profiles, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let mut hits = vec![];
    for (i, id) in p.ids.iter().enumerate() {
        let r = Rect::new(c.p(8.), c.p(8. + i as f64 * 40.), c.p(224.), c.p(40.));
        power_button(
            c,
            r,
            i,
            id == &p.active,
            !id.is_empty() && hover.is_some_and(|(x, y)| r.contains(x, y)),
        );
        if id.is_empty() {
            c.text(
                "—",
                Rect::new(r.x + r.w - c.p(32.), r.y, c.p(24.), r.h),
                c.ui.border,
                14.,
                false,
                1,
            );
        } else {
            hit(&mut hits, r, Action::PowerSelect(id.clone()));
        }
    }
    if !p.error.is_empty() {
        c.text(
            &p.error,
            Rect::new(c.p(8.), c.height - c.p(23.), c.width - c.p(16.), c.p(20.)),
            c.ui.error,
            10.,
            false,
            0,
        );
    }
    c.frame(Rect::new(0, 0, c.width, c.height), c.ui.border);
    hits
}
