use crate::{
    graphics::{Canvas, Rect},
    views::{self, Action, Hit},
};
#[derive(Clone)]
pub struct Sheet {
    pub rows: Vec<[String; 3]>,
    pub offset: i32,
}
const QUICK_START: [(&str, &str); 6] = [
    ("Win + Enter", "기본 터미널 열기"),
    ("Alt + 1 ~ 9", "작업공간 전환 (Shift: 창 이동)"),
    ("Alt + 방향키", "창 포커스 이동 (Win+방향키: 타일 이동)"),
    ("Win+Ctrl+Alt+Space", "배경화면 · 유명 테마 변경"),
    ("Alt + F / Alt + W", "전체 화면 전환 / 현재 창 닫기"),
    ("Win + F1", "단축키 안내 언제든 다시 열기"),
];
impl Sheet {
    pub fn parse(text: &str) -> Self {
        let rows = text
            .trim_start_matches('\u{feff}')
            .lines()
            .skip(1)
            .filter_map(|line| {
                let p = line
                    .trim()
                    .split('\t')
                    .map(|s| s.trim().to_owned())
                    .collect::<Vec<_>>();
                p.try_into().ok()
            })
            .collect();
        Self { rows, offset: 0 }
    }
    pub fn show_quick_start(&self) -> bool {
        self.rows.len() > 4
    }
    pub fn height(&self, scale: f64) -> i32 {
        let p = |n: f64| (n * scale).round_ties_even() as i32;
        let mut group = "";
        let mut h = if self.show_quick_start() { p(156.) } else { 0 };
        for row in &self.rows {
            if group != row[0] {
                group = &row[0];
                h += p(48.);
            }
            h += p(42.);
        }
        h + p(20.)
    }
    pub fn scroll(&mut self, delta: i32, scale: f64, height: i32) {
        let visible = height - (138. * scale).round_ties_even() as i32;
        self.offset = (self.offset + delta).clamp(0, (self.height(scale) - visible).max(0));
    }
}
pub fn paint(c: &mut Canvas, s: &Sheet, hover: Option<(i32, i32)>) -> Vec<Hit> {
    use windows_sys::Win32::Graphics::Gdi::*;
    let ui = c.ui;
    let mut hits = vec![];
    c.text(
        "키보드 단축키 & 빠른 시작 안내",
        Rect::new(c.p(26.), c.p(14.), c.width - c.p(320.), c.p(40.)),
        ui.text,
        24.,
        true,
        0,
    );
    c.text(
        "처음이신가요? 상단 바의 [단축키] 버튼이나 Win + F1 키로 언제든 이 안내 창을 다시 열 수 있습니다.",
        Rect::new(c.p(26.), c.p(54.), c.width - c.p(320.), c.p(26.)),
        ui.muted,
        12.,
        false,
        0,
    );
    let btn_theme = Rect::new(c.width - c.p(268.), c.p(24.), c.p(96.), c.p(32.));
    views::button(
        c,
        &mut hits,
        "테마 변경",
        btn_theme,
        false,
        hover.is_some_and(|(x, y)| btn_theme.contains(x, y)),
        true,
        Action::Wallpaper,
    );
    let btn_settings = Rect::new(c.width - c.p(164.), c.p(24.), c.p(94.), c.p(32.));
    views::button(
        c,
        &mut hits,
        "환경 설정",
        btn_settings,
        false,
        hover.is_some_and(|(x, y)| btn_settings.contains(x, y)),
        true,
        Action::OpenSettings,
    );
    let close = Rect::new(c.width - c.p(62.), c.p(18.), c.p(42.), c.p(44.));
    if hover.is_some_and(|(x, y)| close.contains(x, y)) {
        c.fill(close, ui.hover)
    }
    c.text("×", close, ui.muted, 26.66, false, 1);
    hits.push(Hit {
        rect: close,
        action: Action::Close,
    });
    c.text(
        "GLAZEWM  ·  마우스 휠 또는 방향키로 스크롤                 Win + F1  /  Esc  닫기",
        Rect::new(0, c.height - c.p(44.), c.width, c.p(44.)),
        ui.muted,
        13.333,
        false,
        1,
    );
    unsafe {
        let saved = SaveDC(c.dc);
        IntersectClipRect(c.dc, 0, c.p(94.), c.width, c.height - c.p(44.));
        let mut y = c.p(94.) - s.offset;
        if s.show_quick_start() {
            let banner = Rect::new(c.p(28.), y + c.p(6.), c.width - c.p(56.), c.p(142.));
            c.round_rect(banner, ui.surface, c.p(10.));
            c.frame(banner, ui.accent);
            c.text(
                "핵심 단축키 요약 (Quick Start)",
                Rect::new(banner.x + c.p(16.), banner.y + c.p(8.), banner.w - c.p(32.), c.p(24.)),
                ui.accent,
                12.5,
                true,
                0,
            );
            let col_w = (banner.w - c.p(32.)) / 2;
            for (i, (key, desc)) in QUICK_START.iter().enumerate() {
                let row = (i / 2) as i32;
                let col = (i % 2) as i32;
                let rx = banner.x + c.p(16.) + col * col_w;
                let ry = banner.y + c.p(36.) + row * c.p(32.);
                let key_rect = Rect::new(rx, ry, c.p(165.), c.p(26.));
                c.round_rect(key_rect, ui.selected, c.p(6.));
                c.text(key, key_rect, ui.text, 11.5, true, 1);
                c.text(
                    desc,
                    Rect::new(
                        rx + c.p(175.),
                        ry,
                        col_w - c.p(182.),
                        c.p(26.),
                    ),
                    ui.text,
                    12.,
                    false,
                    0,
                );
            }
            y += c.p(156.);
        }
        let mut group = "";
        for row in &s.rows {
            if group != row[0] {
                group = &row[0];
                y += c.p(14.);
                c.text(
                    group,
                    Rect::new(c.p(28.), y, c.width - c.p(56.), c.p(28.)),
                    ui.accent,
                    12.,
                    true,
                    0,
                );
                y += c.p(34.);
            }
            let cap = Rect::new(c.p(28.), y + c.p(4.), c.p(285.), c.p(32.));
            c.round_rect(cap, ui.surface, c.p(8.));
            c.text(&row[1], cap, ui.text, 12.5, false, 1);
            c.text(
                &row[2],
                Rect::new(
                    cap.x + cap.w + c.p(20.),
                    y,
                    c.width - cap.x - cap.w - c.p(48.),
                    c.p(42.),
                ),
                ui.text,
                13.,
                false,
                0,
            );
            y += c.p(42.);
        }
        RestoreDC(c.dc, saved);
    }
    let total = s.height(c.scale);
    let visible = c.height - c.p(138.);
    if total > visible {
        let track = Rect::new(c.width - c.p(9.), c.p(94.), c.p(5.), visible);
        c.fill(track, ui.surface);
        let thumb = c.p(24.).max(visible * visible / total);
        c.fill(
            Rect::new(
                track.x,
                track.y + s.offset * (visible - thumb) / (total - visible).max(1),
                track.w,
                thumb,
            ),
            ui.border,
        );
    }
    c.frame(Rect::new(0, 0, c.width, c.height), ui.border);
    hits
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_scroll() {
        let mut s = Sheet::parse("\u{feff}분류\t키\t동작\n실행\tA\tB\ninvalid\n창\tC\tD\n");
        assert_eq!(s.rows.len(), 2);
        assert_eq!(s.height(1.), 200);
        s.scroll(10000, 1., 250);
        assert_eq!(s.offset, 88);
        s.scroll(-10000, 1., 250);
        assert_eq!(s.offset, 0);

        let template = include_str!("../../distribution/templates/cheatsheet.tsv");
        let parsed = Sheet::parse(template);
        let expected_rows = template.lines().skip(1).filter(|l| !l.trim().is_empty()).count();
        assert_eq!(parsed.rows.len(), expected_rows);
    }
}
