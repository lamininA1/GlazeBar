use crate::{
    graphics::{Canvas, Rect, parse},
    theme::Palette,
    views::{self, Action, Hit},
};
use std::path::PathBuf;
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Choose(usize),
    Light(bool),
}
#[derive(Clone)]
pub struct Comparison {
    pub image: PathBuf,
    pub palettes: [Option<Palette>; 6],
    pub selected: usize,
    pub light: bool,
    pub revision: u64,
    pub status: String,
}
impl Comparison {
    pub fn new(image: PathBuf, selected: usize, light: bool) -> Self {
        Self {
            image,
            palettes: Default::default(),
            selected: selected.min(5),
            light,
            revision: 0,
            status: String::new(),
        }
    }
    pub fn navigate(&mut self, delta: i32) {
        self.selected = (self.selected as i64 + i64::from(delta)).rem_euclid(6) as usize;
    }
}
const DESCRIPTIONS: [&str; 6] = [
    "사진 색감 우선 · 배경과 통일감",
    "낮은 채도 · 절제된 코드 강조",
    "부드러운 강조색 · 은은한 구문 구분",
    "중립 배경 · 일상 코딩 추천",
    "높은 채도 · 또렷한 구문 강조",
    "강한 명암 대비 · 글자 가독성 우선",
];
pub fn paint(c: &mut Canvas, v: &Comparison, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let mut hits = vec![];
    let scale = c.scale;
    let s = |n: f64| (n * scale).round_ties_even() as i32;
    c.text(
        "스타일 비교",
        Rect::new(s(26.), s(18.), s(300.), s(30.)),
        c.ui.text,
        17.,
        true,
        0,
    );
    c.text(
        "같은 사진의 코드 색상을 비교하고 선택하세요",
        Rect::new(s(26.), s(56.), c.width - s(52.), s(24.)),
        c.ui.muted,
        12.,
        false,
        0,
    );
    for (label, right, selected, action) in [
        ("다크", 260., !v.light, Action::Compare(Edit::Light(false))),
        ("라이트", 180., v.light, Action::Compare(Edit::Light(true))),
        ("닫기", 92., false, Action::Close),
    ] {
        let r = Rect::new(c.width - s(right), s(18.), s(72.), s(30.));
        views::button(
            c,
            &mut hits,
            label,
            r,
            selected,
            hover.is_some_and(|(x, y)| r.contains(x, y)),
            true,
            action,
        );
    }
    let gap = s(14.);
    let width = (c.width - s(44.) - 2 * gap) / 3;
    let height = (c.height - s(155.) - gap) / 2;
    for i in 0..6 {
        let r = Rect::new(
            s(22.) + (i % 3) as i32 * (width + gap),
            s(100.) + (i / 3) as i32 * (height + gap),
            width,
            height,
        );
        c.fill(r, c.ui.surface);
        c.text(
            &format!("{}  {}", i + 1, crate::wallpaper_ui::NAMES[i]),
            Rect::new(r.x + s(18.), r.y + s(9.), width - s(32.), s(24.)),
            c.ui.text,
            12.,
            false,
            0,
        );
        c.text(
            DESCRIPTIONS[i],
            Rect::new(r.x + s(18.), r.y + s(35.), width - s(32.), s(36.)),
            c.ui.muted,
            12.,
            false,
            0,
        );
        if let Some(a) = &v.palettes[i] {
            let code = Rect::new(r.x + s(12.), r.y + s(84.), width - s(24.), height - s(98.));
            c.fill(code, parse(&a[0]));
            let cell = code.w / 16;
            for (j, col) in a.iter().enumerate() {
                c.fill(
                    Rect::new(code.x + j as i32 * cell, code.y, cell + 1, s(9.)),
                    parse(col),
                );
            }
            for (y, parts) in [
                (25., vec![("function ", 5), ("preview", 4), ("() {", 15)]),
                (49., vec![("  return ", 5), ("\"wallpaper\"", 2), (";", 15)]),
                (73., vec![("// comment", 8)]),
                (97., vec![("+ added  ", 2), ("− removed", 1)]),
            ] {
                let mut x = r.x + s(16.);
                for (text, slot) in parts {
                    let w = c.measure(text, 12., false);
                    c.text(
                        text,
                        Rect::new(x, code.y + s(y), w, s(18.)),
                        parse(&a[slot]),
                        12.,
                        false,
                        0,
                    );
                    x += w;
                }
            }
            hits.push(Hit {
                rect: r,
                action: Action::Compare(Edit::Choose(i)),
            });
        } else {
            c.text(
                "계산 중…",
                Rect::new(r.x, r.y + s(90.), width, height - s(100.)),
                c.ui.text,
                12.,
                false,
                1,
            );
        }
        let (border, thickness) = if i == v.selected {
            (c.ui.accent, s(2.))
        } else {
            (c.ui.border, 1)
        };
        for n in 0..thickness {
            c.frame(
                Rect::new(r.x + n, r.y + n, r.w - 2 * n, r.h - 2 * n),
                border,
            );
        }
    }
    c.text(
        if v.status.is_empty() {
            "클릭 또는 1–6 선택   ·   방향키 이동 / Enter 선택   ·   Esc 취소"
        } else {
            &v.status
        },
        Rect::new(s(22.), c.height - s(39.), c.width - s(44.), s(24.)),
        c.ui.muted,
        12.,
        false,
        1,
    );
    c.frame(Rect::new(0, 0, c.width, c.height), c.ui.border);
    hits
}
