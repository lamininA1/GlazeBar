use crate::{
    graphics::{Canvas, Rect},
    views::{self, Action, Hit},
    weather::CITIES,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const APP_LABELS: [&str; 12] = [
    "GlazeBar",
    "Terminal",
    "Codex CLI",
    "VS Code",
    "Rio Terminal",
    "Zed Editor",
    "Antigravity",
    "Claude",
    "OpenCode",
    "Firefox",
    "Chrome",
    "Edge",
];

pub const TERMINALS: [(&str, &str); 5] = [
    ("Windows Terminal (wt)", "shell-exec wt"),
    ("Rio Terminal (rio)", "shell-exec rio"),
    ("PowerShell (pwsh)", "shell-exec pwsh"),
    ("Alacritty", "shell-exec alacritty"),
    ("WezTerm", "shell-exec wezterm-gui"),
];

pub const BROWSERS: [(&str, &str); 4] = [
    (
        "기본 브라우저 (Google)",
        "shell-exec cmd /c start \"\" \"https://www.google.com\"",
    ),
    (
        "Chrome",
        "shell-exec cmd /c start chrome \"https://www.google.com\"",
    ),
    (
        "Edge",
        "shell-exec cmd /c start msedge \"https://www.google.com\"",
    ),
    (
        "Firefox",
        "shell-exec cmd /c start firefox \"https://www.google.com\"",
    ),
];

pub const GAPS: [(&str, &str); 4] = [
    ("없음 (0px)", "0px"),
    ("좁게 (8px)", "8px"),
    ("기본 (16px)", "16px"),
    ("넓게 (24px)", "24px"),
];

fn default_true() -> bool {
    true
}
fn default_targets() -> [bool; 12] {
    [true; 12]
}
fn default_gap_idx() -> usize {
    2
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarSettings {
    #[serde(default = "default_true")]
    pub show_weather: bool,
    #[serde(default = "default_true")]
    pub show_cpu: bool,
    #[serde(default = "default_true")]
    pub show_memory: bool,
    #[serde(default = "default_true")]
    pub show_battery: bool,
    #[serde(default = "default_true")]
    pub show_wifi: bool,
    #[serde(default = "default_true")]
    pub show_brightness: bool,
    #[serde(default = "default_true")]
    pub show_scale: bool,
    #[serde(default = "default_true")]
    pub show_quick_buttons: bool,
    #[serde(default)]
    pub weather_city: usize,
    #[serde(default = "default_true")]
    pub auto_split: bool,
    #[serde(default)]
    pub terminal_idx: usize,
    #[serde(default)]
    pub browser_idx: usize,
    #[serde(default = "default_gap_idx")]
    pub gap_idx: usize,
    #[serde(default = "default_targets")]
    pub targets: [bool; 12],
    #[serde(skip)]
    pub status: String,
}

impl Default for BarSettings {
    fn default() -> Self {
        Self {
            show_weather: true,
            show_cpu: true,
            show_memory: true,
            show_battery: true,
            show_wifi: true,
            show_brightness: true,
            show_scale: true,
            show_quick_buttons: true,
            weather_city: 0,
            auto_split: true,
            terminal_idx: 0,
            browser_idx: 0,
            gap_idx: 2,
            targets: [true; 12],
            status: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    ToggleWeather,
    ToggleCpu,
    ToggleMemory,
    ToggleBattery,
    ToggleWifi,
    ToggleBrightness,
    ToggleScale,
    ToggleQuickButtons,
    CycleCity,
    ToggleAutoSplit,
    CycleTerminal,
    CycleBrowser,
    CycleGap,
    ToggleTarget(usize),
    SelectAllTargets(bool),
    OpenConfig,
    ReloadGlazeWM,
    OpenWallpaper,
    OpenSheet,
    Save,
}

impl BarSettings {
    pub fn load(root: &Path) -> Self {
        let path = root.join("theme-state/glazebar-settings.json");
        let mut s: Self = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        s.weather_city = s.weather_city.min(CITIES.len().saturating_sub(1));
        s.terminal_idx = s.terminal_idx.min(TERMINALS.len().saturating_sub(1));
        s.browser_idx = s.browser_idx.min(BROWSERS.len().saturating_sub(1));
        s.gap_idx = s.gap_idx.min(GAPS.len().saturating_sub(1));
        s.auto_split = !root.join("autotile.disabled").exists();
        s
    }

    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        crate::theme_transaction::atomic_write(
            &root.join("theme-state/glazebar-settings.json"),
            &serde_json::to_vec_pretty(self)?,
        )?;
        let autotile_flag = root.join("autotile.disabled");
        if self.auto_split {
            if autotile_flag.exists() {
                let _ = std::fs::remove_file(autotile_flag);
            }
        } else {
            let _ = std::fs::write(autotile_flag, "disabled");
        }
        let config_path = root.join("config.yaml");
        if let Ok(source) = std::fs::read_to_string(&config_path) {
            let mut updated = source.clone();
            if let Some((_, cmd)) = TERMINALS.get(self.terminal_idx) {
                updated = crate::theme_text::glazewm_set_binding_command(&updated, "alt+enter", cmd);
            }
            if let Some((_, cmd)) = BROWSERS.get(self.browser_idx) {
                updated = crate::theme_text::glazewm_set_binding_command(
                    &updated,
                    "alt+shift+enter",
                    cmd,
                );
            }
            if let Some((_, gap)) = GAPS.get(self.gap_idx) {
                updated = crate::theme_text::glazewm_set_scalar(
                    &updated,
                    &["gaps"],
                    "inner_gap",
                    &format!("'{gap}'"),
                );
            }
            if updated != source {
                let _ = crate::theme_transaction::atomic_write(&config_path, updated.as_bytes());
            }
        }
        Ok(())
    }

    pub fn apply_edit(&mut self, edit: &Edit) {
        match edit {
            Edit::ToggleWeather => self.show_weather = !self.show_weather,
            Edit::ToggleCpu => self.show_cpu = !self.show_cpu,
            Edit::ToggleMemory => self.show_memory = !self.show_memory,
            Edit::ToggleBattery => self.show_battery = !self.show_battery,
            Edit::ToggleWifi => self.show_wifi = !self.show_wifi,
            Edit::ToggleBrightness => self.show_brightness = !self.show_brightness,
            Edit::ToggleScale => self.show_scale = !self.show_scale,
            Edit::ToggleQuickButtons => self.show_quick_buttons = !self.show_quick_buttons,
            Edit::CycleCity => self.weather_city = (self.weather_city + 1) % CITIES.len(),
            Edit::ToggleAutoSplit => self.auto_split = !self.auto_split,
            Edit::CycleTerminal => self.terminal_idx = (self.terminal_idx + 1) % TERMINALS.len(),
            Edit::CycleBrowser => self.browser_idx = (self.browser_idx + 1) % BROWSERS.len(),
            Edit::CycleGap => self.gap_idx = (self.gap_idx + 1) % GAPS.len(),
            Edit::ToggleTarget(i) => {
                if let Some(t) = self.targets.get_mut(*i) {
                    *t = !*t;
                }
            }
            Edit::SelectAllTargets(val) => {
                self.targets = [*val; 12];
            }
            _ => {}
        }
        self.status.clear();
    }
}

fn toggle(
    c: &mut Canvas,
    hits: &mut Vec<Hit>,
    label: &str,
    r: Rect,
    checked: bool,
    edit: Edit,
) {
    let side = c.p(11.);
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
    hits.push(Hit {
        rect: r,
        action: Action::Settings(edit),
    });
}

pub fn paint(c: &mut Canvas, v: &BarSettings, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let mut hits = vec![];
    let scale = c.scale;
    let s = |n: f64| (n * scale).round_ties_even() as i32;
    let w = c.width;
    let h = c.height;

    c.text(
        "GlazeBar 환경 설정",
        Rect::new(s(26.), s(16.), s(320.), s(30.)),
        c.ui.text,
        18.,
        true,
        0,
    );
    c.text(
        "상단 바 표시 위젯, 날씨 지역, 기본 실행 단축키, 테마 적용 대상 프로그램을 설정합니다",
        Rect::new(s(26.), s(46.), w - s(140.), s(22.)),
        c.ui.muted,
        11.5,
        false,
        0,
    );

    let close_rect = Rect::new(w - s(98.), s(18.), s(72.), s(30.));
    views::button(
        c,
        &mut hits,
        "닫기",
        close_rect,
        false,
        hover.is_some_and(|(x, y)| close_rect.contains(x, y)),
        true,
        Action::Close,
    );

    // Section 1: 상단 바 표시 위젯 & 날씨
    let sec1 = Rect::new(s(24.), s(76.), w - s(48.), s(118.));
    c.round_rect(sec1, c.ui.surface, s(8.));
    c.text(
        "상단 바 위젯 표시 및 날씨 설정",
        Rect::new(s(38.), s(84.), s(300.), s(22.)),
        c.ui.accent,
        12.,
        true,
        0,
    );

    let city_name = CITIES.get(v.weather_city).map(|c| c.name).unwrap_or("서울");
    let city_btn = Rect::new(w - s(224.), s(82.), s(186.), s(26.));
    views::button(
        c,
        &mut hits,
        &format!("날씨 지역 · {city_name} (변경)"),
        city_btn,
        false,
        hover.is_some_and(|(x, y)| city_btn.contains(x, y)),
        true,
        Action::Settings(Edit::CycleCity),
    );

    let col_w = (w - s(76.)) / 4;
    for (i, (label, checked, edit)) in [
        ("날씨 아이콘/온도", v.show_weather, Edit::ToggleWeather),
        ("CPU 사용률", v.show_cpu, Edit::ToggleCpu),
        ("메모리 사용률", v.show_memory, Edit::ToggleMemory),
        ("배터리 / 전력", v.show_battery, Edit::ToggleBattery),
        ("Wi-Fi 네트워크", v.show_wifi, Edit::ToggleWifi),
        ("화면 밝기 슬라이더", v.show_brightness, Edit::ToggleBrightness),
        ("디스플레이 배율", v.show_scale, Edit::ToggleScale),
        ("빠른 실행 버튼", v.show_quick_buttons, Edit::ToggleQuickButtons),
    ]
    .into_iter()
    .enumerate()
    {
        let row = i / 4;
        let col = i % 4;
        toggle(
            c,
            &mut hits,
            label,
            Rect::new(
                s(38.) + col as i32 * col_w,
                s(114. + row as f64 * 34.),
                col_w - s(8.),
                s(28.),
            ),
            checked,
            edit,
        );
    }

    // Section 2: GlazeWM 동작 및 기본 실행 앱
    let sec2 = Rect::new(s(24.), s(204.), w - s(48.), s(122.));
    c.round_rect(sec2, c.ui.surface, s(8.));
    c.text(
        "창 관리 및 단축키 기본 실행 프로그램",
        Rect::new(s(38.), s(212.), s(360.), s(22.)),
        c.ui.accent,
        12.,
        true,
        0,
    );

    toggle(
        c,
        &mut hits,
        "창 비율에 따라 가로/세로 분할 방향 자동 전환 (Auto-Split)",
        Rect::new(s(38.), s(238.), w - s(80.), s(28.)),
        v.auto_split,
        Edit::ToggleAutoSplit,
    );

    let term_label = TERMINALS
        .get(v.terminal_idx)
        .map(|t| t.0)
        .unwrap_or(TERMINALS[0].0);
    let brow_label = BROWSERS
        .get(v.browser_idx)
        .map(|b| b.0)
        .unwrap_or(BROWSERS[0].0);
    let gap_label = GAPS.get(v.gap_idx).map(|g| g.0).unwrap_or(GAPS[2].0);

    let r_term = Rect::new(s(38.), s(276.), s(280.), s(32.));
    views::button(
        c,
        &mut hits,
        &format!("터미널: {term_label}"),
        r_term,
        false,
        hover.is_some_and(|(x, y)| r_term.contains(x, y)),
        true,
        Action::Settings(Edit::CycleTerminal),
    );

    let r_brow = Rect::new(s(328.), s(276.), s(280.), s(32.));
    views::button(
        c,
        &mut hits,
        &format!("브라우저: {brow_label}"),
        r_brow,
        false,
        hover.is_some_and(|(x, y)| r_brow.contains(x, y)),
        true,
        Action::Settings(Edit::CycleBrowser),
    );

    let r_gap = Rect::new(s(618.), s(276.), w - s(656.), s(32.));
    views::button(
        c,
        &mut hits,
        &format!("창 간격: {gap_label}"),
        r_gap,
        false,
        hover.is_some_and(|(x, y)| r_gap.contains(x, y)),
        true,
        Action::Settings(Edit::CycleGap),
    );

    // Section 3: 테마 자동 적용 대상 프로그램
    let sec3 = Rect::new(s(24.), s(336.), w - s(48.), s(156.));
    c.round_rect(sec3, c.ui.surface, s(8.));
    c.text(
        "테마 자동 적용 대상 프로그램 (설치되지 않은 앱은 오류 없이 자동 건너뜀)",
        Rect::new(s(38.), s(344.), s(540.), s(22.)),
        c.ui.accent,
        12.,
        true,
        0,
    );

    let all_on = v.targets.iter().all(|&b| b);
    let btn_all = Rect::new(w - s(224.), s(342.), s(186.), s(26.));
    views::button(
        c,
        &mut hits,
        if all_on {
            "전체 선택 해제"
        } else {
            "전체 프로그램 선택"
        },
        btn_all,
        false,
        hover.is_some_and(|(x, y)| btn_all.contains(x, y)),
        true,
        Action::Settings(Edit::SelectAllTargets(!all_on)),
    );

    for (i, label) in APP_LABELS.iter().enumerate() {
        let row = i / 4;
        let col = i % 4;
        toggle(
            c,
            &mut hits,
            label,
            Rect::new(
                s(38.) + col as i32 * col_w,
                s(376. + row as f64 * 34.),
                col_w - s(8.),
                s(28.),
            ),
            v.targets[i],
            Edit::ToggleTarget(i),
        );
    }

    // Bottom quick actions
    for (label, x, width, selected, edit) in [
        ("테마 · 배경화면 열기", 24., 160., false, Edit::OpenWallpaper),
        ("단축키 안내 보기", 194., 145., false, Edit::OpenSheet),
        ("config.yaml 직접 편집", 349., 165., false, Edit::OpenConfig),
        ("GlazeWM 재적용", 524., 135., false, Edit::ReloadGlazeWM),
        ("저장 완료", 686., 150., true, Edit::Save),
    ] {
        let r = Rect::new(s(x), s(510.), s(width), s(34.));
        views::button(
            c,
            &mut hits,
            label,
            r,
            selected,
            hover.is_some_and(|(x, y)| r.contains(x, y)),
            true,
            Action::Settings(edit),
        );
    }

    let footer = if v.status.is_empty() {
        "변경 사항은 즉시 반영되며 [저장 완료] 또는 Enter 키를 누르면 설정 파일에 저장됩니다."
    } else {
        &v.status
    };
    c.text(
        footer,
        Rect::new(s(24.), h - s(40.), w - s(48.), s(24.)),
        c.ui.muted,
        11.5,
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
    fn settings_roundtrip_and_config_update() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.yaml"),
            "gaps:\n  inner_gap: '16px'\nkeybindings:\n  - commands: ['shell-exec wt']\n    bindings: ['alt+enter', 'win+enter']\n  - commands: ['shell-exec cmd /c start \"\" \"https://www.google.com\"']\n    bindings: ['alt+shift+enter']\n",
        )
        .unwrap();
        let mut s = BarSettings::load(dir.path());
        assert!(s.show_weather);
        assert_eq!(s.targets, [true; 12]);
        s.apply_edit(&Edit::CycleCity);
        s.apply_edit(&Edit::CycleTerminal);
        s.apply_edit(&Edit::ToggleAutoSplit);
        s.apply_edit(&Edit::ToggleTarget(4));
        s.save(dir.path()).unwrap();

        let loaded = BarSettings::load(dir.path());
        assert_eq!(loaded.weather_city, 1);
        assert_eq!(loaded.terminal_idx, 1);
        assert!(!loaded.auto_split);
        assert!(!loaded.targets[4]);
        assert!(dir.path().join("autotile.disabled").exists());
        let cfg = std::fs::read_to_string(dir.path().join("config.yaml")).unwrap();
        assert!(cfg.contains("shell-exec rio"));
    }
}
