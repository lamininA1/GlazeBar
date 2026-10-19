use crate::{
    graphics::{Canvas, Rect},
    views::{Action, Hit},
};
#[derive(Clone)]
pub struct Sheet {
    pub rows: Vec<[String; 3]>,
    pub offset: i32,
}
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
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                p.try_into().ok()
            })
            .collect();
        Self { rows, offset: 0 }
    }
    pub fn height(&self, scale: f64) -> i32 {
        let p = |n: f64| (n * scale).round_ties_even() as i32;
        let mut group = "";
        let mut h = 0;
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
    c.text(
        "키보드 단축키",
        Rect::new(c.p(26.), c.p(18.), c.width - c.p(110.), c.p(64.)),
        ui.text,
        28.,
        true,
        0,
    );
    let close = Rect::new(c.width - c.p(62.), c.p(18.), c.p(42.), c.p(64.));
    if hover.is_some_and(|(x, y)| close.contains(x, y)) {
        c.fill(close, ui.hover)
    }
    c.text("×", close, ui.muted, 26.66, false, 1);
    c.text(
        "GLAZEWM                                          Win + F1  /  Esc  닫기",
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
    vec![Hit {
        rect: close,
        action: Action::Close,
    }]
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
    }
}
