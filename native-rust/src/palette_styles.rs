use crate::{
    palette_base as base,
    theme::{self, Palette},
};
use anyhow::Result;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn styles_match_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            colors: Vec<String>,
            weights: Option<Vec<f64>>,
            light: bool,
            mode: String,
            expected: Palette,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/styles-go.json")).unwrap();
        assert_eq!(cases.len(), 792);
        for (i, c) in cases.into_iter().enumerate() {
            assert_eq!(
                crate::palette_mono::generate(&c.colors, c.weights.as_deref(), c.light, &c.mode)
                    .unwrap(),
                c.expected,
                "style case {i}, {}",
                c.mode
            );
        }
    }
}
fn to_hsl(color: &str) -> [f64; 3] {
    let [r, g, b] = theme::rgb(color).map(|c| c / 255.);
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let l = (max + min) / 2.;
    if max == min {
        return [0., 0., l * 100.];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2. - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6. } else { 0. }) / 6.
    } else if max == g {
        ((b - r) / d + 2.) / 6.
    } else {
        ((r - g) / d + 4.) / 6.
    };
    [h * 360., s * 100., l * 100.]
}
fn hsl_hex([h, s, l]: [f64; 3]) -> String {
    let (h, s, l) = (h / 360., s / 100., l / 100.);
    let q = if l < 0.5 { l * (1. + s) } else { l + s - l * s };
    let p = 2. * l - q;
    let channel = |mut t: f64| {
        if t < 0. {
            t += 1.;
        }
        if t > 1. {
            t -= 1.;
        }
        let c = if s == 0. {
            l
        } else if t < 1. / 6. {
            p + (q - p) * 6. * t
        } else if t < 0.5 {
            q
        } else if t < 2. / 3. {
            p + (q - p) * (2. / 3. - t) * 6.
        } else {
            p
        };
        (c * 255.).round().clamp(0., 255.) as u8
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(h + 1. / 3.),
        channel(h),
        channel(h - 1. / 3.)
    )
}
fn material(colors: &[String], light: bool) -> Palette {
    let mut p: Palette = std::array::from_fn(|_| String::new());
    let mut used = vec![false; colors.len()];
    p[0] = if light { "#fafafa" } else { "#121212" }.into();
    p[7] = if light { "#212121" } else { "#ffffff" }.into();
    for i in 0..6 {
        let mut best = f64::INFINITY;
        let mut index = 0;
        for (j, c) in colors.iter().enumerate() {
            if used[j] {
                continue;
            }
            let score = base::score(theme::to_oklch(c), base::HUES[i], light);
            if score < best {
                best = score;
                index = j;
            }
        }
        let [h, s, l] = to_hsl(&colors[index]);
        p[i + 1] = hsl_hex([
            h,
            s.max(35.),
            if light {
                l.clamp(35., 60.)
            } else {
                l.clamp(45., 70.)
            },
        ]);
        used[index] = true;
    }
    p[8] = if light { "#757575" } else { "#9e9e9e" }.into();
    for i in 1..=6 {
        let [h, s, l] = to_hsl(&p[i]);
        p[i + 8] = hsl_hex([
            h,
            (s + 8.).min(100.),
            if light {
                (l - 8.).max(30.)
            } else {
                (l + 8.).min(75.)
            },
        ]);
    }
    p[15] = if light { "#000000" } else { "#ffffff" }.into();
    p
}
pub fn generate(colors: &[String], light: bool, mode: &str) -> Result<Palette> {
    if mode == "material" {
        return Ok(material(colors, light));
    }
    // [dark, light] bg/fg L,C; accent L and chroma bounds/factor; per-slot L offsets.
    let (bg, fg, ansi_l, cmin, cmax, factor, offset): (
        [[f64; 2]; 2],
        [[f64; 2]; 2],
        [f64; 2],
        [f64; 2],
        [f64; 2],
        f64,
        [f64; 6],
    ) = match mode {
        "pastel" => (
            [[0.20, 0.022], [0.96, 0.012]],
            [[0.85, 0.04], [0.32, 0.05]],
            [0.78, 0.55],
            [0.05, 0.045],
            [0.075, 0.07],
            1.,
            [-0.04, 0.02, 0.05, -0.03, -0.01, 0.03],
        ),
        "colorful" => (
            [[0.10, 0.022], [0.97, 0.012]],
            [[0.92, 0.03], [0.18, 0.04]],
            [0.65, 0.50],
            [0.14, 0.14],
            [0.20, 0.20],
            1.,
            [-0.05, 0.02, 0.06, -0.03, -0.02, 0.04],
        ),
        "muted" => (
            [[0.16, 0.018], [0.94, 0.010]],
            [[0.84, 0.035], [0.28, 0.045]],
            [0.62, 0.48],
            [0.035, 0.035],
            [0.065, 0.065],
            0.5,
            [-0.10, -0.04, 0.04, 0.10, -0.07, 0.07],
        ),
        "bright" => (
            [[0.10, 0.020], [0.98, 0.010]],
            [[0.92, 0.025], [0.15, 0.04]],
            [0.78, 0.42],
            [0.10, 0.10],
            [0.18, 0.18],
            1.,
            [-0.04, 0.01, 0.06, -0.03, -0.02, 0.03],
        ),
        "high-contrast" => (
            [[0.05, 0.010], [0.99, 0.005]],
            [[0.95, 0.015], [0.10, 0.020]],
            [0.82, 0.32],
            [0.10, 0.10],
            [0.18, 0.18],
            1.,
            [-0.03, 0.01, 0.04, -0.02, -0.01, 0.02],
        ),
        _ => return crate::palette_extra::generate(colors, light, mode),
    };
    let mut p = base::extract_hues(colors, None, light);
    let m = light as usize;
    p[0] = base::hex([bg[m][0], bg[m][1], theme::to_oklch(&p[0])[2]]);
    p[7] = base::hex([fg[m][0], fg[m][1], theme::to_oklch(&p[7])[2]]);
    for i in 0..6 {
        let [_, c, h] = theme::to_oklch(&p[i + 1]);
        p[i + 1] = base::hex([
            (ansi_l[m] + offset[i]).clamp(0.05, 0.95),
            (c * factor).clamp(cmin[m], cmax[m]),
            h,
        ]);
    }
    base::finalize(&mut p);
    if mode == "high-contrast" {
        for i in 1..=6 {
            if theme::contrast(&p[0], &p[i]) < 7. {
                p[i] = base::boost(&p[i], &p[0], 7.);
                p[i + 8] = base::bright(&p[i]);
            }
        }
    }
    Ok(p)
}
