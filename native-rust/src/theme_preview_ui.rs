use crate::graphics::{Canvas, Rect, blend, parse};
#[derive(Clone, PartialEq)]
pub struct Preview {
    pub light: bool,
    pub palette: Option<crate::theme::Palette>,
}
fn line(c: &mut Canvas, text: &str, col: u32, x: f64, y: f64) {
    c.text(
        text,
        Rect::new(c.p(x), c.p(y), c.width - c.p(x + 16.), c.p(17.)),
        col,
        12.,
        false,
        0,
    );
}
fn tokens(c: &mut Canvas, a: &[u32; 16], y: f64, parts: &[(&str, usize)]) {
    let mut x = c.p(30.);
    for &(text, slot) in parts {
        let width = c.measure(text, 12., false);
        c.text(
            text,
            Rect::new(x, c.p(y), width, c.p(17.)),
            a[slot],
            12.,
            false,
            0,
        );
        x += width;
    }
}
pub fn paint(c: &mut Canvas, p: &Preview) {
    c.text(
        "테마 미리보기",
        Rect::new(c.p(24.), c.p(18.), c.p(240.), c.p(30.)),
        c.ui.text,
        17.,
        true,
        0,
    );
    c.text(
        if p.light { "라이트" } else { "다크" },
        Rect::new(c.width - c.p(84.), c.p(24.), c.p(64.), c.p(24.)),
        c.ui.muted,
        12.,
        false,
        2,
    );
    c.frame(Rect::new(0, 0, c.width, c.height), c.ui.border);
    let Some(palette) = &p.palette else {
        c.text(
            "색상 추출 중…",
            Rect::new(0, 0, c.width, c.height),
            c.ui.text,
            12.,
            false,
            1,
        );
        return;
    };
    let a: [u32; 16] = std::array::from_fn(|i| parse(&palette[i]));
    let cell = (c.width - c.p(40.)) / 8;
    for (i, col) in a.iter().enumerate() {
        c.fill(
            Rect::new(
                c.p(20.) + (i % 8) as i32 * cell,
                c.p((66 + i / 8 * 15) as f64),
                cell + 1,
                c.p(12.),
            ),
            *col,
        );
    }
    line(c, "GlazeBar", c.ui.muted, 24., 112.);
    c.fill(
        Rect::new(c.p(20.), c.p(137.), c.width - c.p(40.), c.p(30.)),
        a[0],
    );
    c.fill(
        Rect::new(c.p(28.), c.p(142.), c.p(24.), c.p(20.)),
        blend(a[0], a[4], 0.28),
    );
    for (text, slot, x) in [
        ("1", 15, 36.),
        ("2", 7, 65.),
        ("3", 7, 94.),
        ("10:30", 15, 155.),
        ("5.2 W", 2, 270.),
    ] {
        line(c, text, a[slot], x, 145.);
    }
    for (label, y, height) in [
        ("Terminal", 185., 75.),
        ("Codex CLI", 285., 125.),
        ("VS Code", 430., 118.),
    ] {
        line(c, label, c.ui.muted, 24., y);
        c.fill(
            Rect::new(
                c.p(20.),
                c.p(y + 25.),
                c.width - c.p(40.),
                c.p(height - 25.),
            ),
            a[0],
        );
    }
    line(c, "PS  ~", a[6], 30., 220.);
    line(c, "> Get-ChildItem", a[15], 30., 242.);
    tokens(
        c,
        &a,
        321.,
        &[("function ", 5), ("preview", 4), ("() {", 15)],
    );
    tokens(
        c,
        &a,
        345.,
        &[("  return ", 5), ("\"wallpaper\"", 2), (";", 15)],
    );
    line(c, "}", a[15], 30., 369.);
    line(c, "+ added line", a[2], 30., 391.);
    line(c, "− removed line", a[1], 170., 391.);
    line(c, "preview.ts", a[7], 30., 466.);
    tokens(
        c,
        &a,
        490.,
        &[
            ("const ", 5),
            ("theme", 15),
            (" = ", 15),
            ("\"aether\"", 2),
            (";", 15),
        ],
    );
    line(c, "// wallpaper palette", a[8], 30., 514.);
}
