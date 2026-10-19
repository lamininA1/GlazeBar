use crate::{
    palette_base as base,
    theme::{self, Palette},
};
use anyhow::{Result, bail};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remaining_styles_match_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            colors: Vec<String>,
            weights: Option<Vec<f64>>,
            light: bool,
            mode: String,
            expected: Palette,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/extra-styles-go.json")).unwrap();
        assert_eq!(cases.len(), 1980);
        for (i, c) in cases.into_iter().enumerate() {
            assert_eq!(
                crate::palette_mono::generate(&c.colors, c.weights.as_deref(), c.light, &c.mode)
                    .unwrap(),
                c.expected,
                "extra style case {i}, {}",
                c.mode
            );
        }
    }
}
fn dominant_hue(colors: &[String]) -> f64 {
    let mut hue = 0.;
    let mut best = 0.;
    for c in colors {
        let [_, ch, h] = theme::to_oklch(c);
        if ch > best {
            best = ch;
            hue = h;
        }
    }
    hue
}
pub fn generate(colors: &[String], light: bool, mode: &str) -> Result<Palette> {
    let h = dominant_hue(colors);
    let opposite = (h + 180.) % 360.;
    let mut p: Palette = std::array::from_fn(|_| String::new());
    if mode == "analogous" {
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for c in colors {
            let l = theme::to_oklch(c)[0];
            low = low.min(l);
            high = high.max(l);
        }
        p[0] = base::hex(if light {
            [high.max(0.92), 0.02, h]
        } else {
            [low.min(0.16), 0.03, h]
        });
        p[7] = base::hex(if light {
            [(low + 0.05).min(0.28), 0.04, h]
        } else {
            [(high - 0.05).max(0.88), 0.025, h]
        });
        let c = [0.10, 0.13, 0.09, 0.14, 0.10, 0.12];
        let offsets = [-30., -18., -6., 6., 18., 30.];
        let stagger = [-0.06, 0.04, -0.04, 0.06, -0.05, 0.05];
        let l: f64 = if light { 0.50 } else { 0.62 };
        for i in 0..6 {
            let hue = (h + offsets[i] + 360.) % 360.;
            p[i + 1] = base::hex([(l + stagger[i]).clamp(0.30, 0.85), c[i], hue]);
            p[i + 9] = base::hex([
                if light {
                    (l + stagger[i] - 0.10).clamp(0.20, 0.70)
                } else {
                    (l + stagger[i] + 0.10).clamp(0.30, 0.92)
                },
                c[i] + 0.02,
                hue,
            ]);
        }
        p[8] = base::comment(&p[0]);
        p[15] = base::hex(if light {
            [0.10, 0.04, h]
        } else {
            [0.97, 0.015, h]
        });
        return Ok(p);
    }
    let scheme = match mode {
        "complementary" => Some((
            [h - 8., opposite - 8., h, opposite, h + 8., opposite + 8.],
            [0.14, 0.14, 0.11, 0.11, 0.16, 0.16],
            [-0.06, -0.04, 0.04, 0.02, 0.08, 0.06],
        )),
        "triadic" => Some((
            [
                h,
                (h + 120.) % 360.,
                (h + 240.) % 360.,
                h,
                (h + 120.) % 360.,
                (h + 240.) % 360.,
            ],
            [0.13, 0.13, 0.13, 0.16, 0.16, 0.16],
            [0.05, 0.05, 0.05, -0.07, -0.07, -0.07],
        )),
        "split-complementary" => Some((
            [
                h,
                (h + 150.) % 360.,
                (h + 210.) % 360.,
                h,
                (h + 150.) % 360.,
                (h + 210.) % 360.,
            ],
            [0.14, 0.13, 0.13, 0.17, 0.16, 0.16],
            [0.05, 0.05, 0.05, -0.06, -0.06, -0.06],
        )),
        "tetradic" => Some((
            [
                h,
                (h + 90.) % 360.,
                opposite,
                (h + 270.) % 360.,
                h,
                opposite,
            ],
            [0.12, 0.12, 0.12, 0.12, 0.15, 0.15],
            [0.04, -0.02, 0.04, -0.02, 0.10, 0.10],
        )),
        "duotone" => Some((
            [h, opposite, h, opposite, h, opposite],
            [0.09; 6],
            [-0.10, -0.10, 0.04, 0.04, 0.14, 0.14],
        )),
        _ => None,
    };
    if let Some((hues, chromas, offsets)) = scheme {
        let l: f64 = if mode == "duotone" {
            p[0] = base::hex(if light {
                [0.97, 0.008, h]
            } else {
                [0.12, 0.018, h]
            });
            p[7] = base::hex(if light {
                [0.20, 0.025, h]
            } else {
                [0.90, 0.020, h]
            });
            if light { 0.50 } else { 0.62 }
        } else {
            p[0] = base::hex(if light {
                [0.96, 0.012, h]
            } else {
                [0.13, 0.022, h]
            });
            p[7] = base::hex(if light {
                [0.22, 0.04, h]
            } else {
                [0.88, 0.025, h]
            });
            if light { 0.48 } else { 0.65 }
        };
        for i in 0..6 {
            let hue = if mode == "duotone" {
                hues[i]
            } else {
                (hues[i] + 360.) % 360.
            };
            p[i + 1] = base::hex([(l + offsets[i]).clamp(0.30, 0.85), chromas[i], hue]);
        }
        base::finalize(&mut p);
        return Ok(p);
    }
    mood(colors, light, mode)
}
struct Mood {
    bg: [[f64; 3]; 2],
    fg: [[f64; 3]; 2],
    anchor: f64,
    tint: f64,
    chroma: [f64; 2],
    l: [f64; 2],
    offset: [f64; 6],
}
fn mood(colors: &[String], light: bool, mode: &str) -> Result<Palette> {
    let (bg, fg, anchor, tint, chroma, l, offset) = match mode {
        "fire" => (
            [[0.09, 0.045, 28.], [0.95, 0.030, 45.]],
            [[0.90, 0.040, 55.], [0.22, 0.060, 20.]],
            30.,
            0.28,
            [0.07, 0.12],
            [0.62, 0.46],
            [-0.05, 0.01, 0.07, -0.04, -0.02, 0.03],
        ),
        "ocean" => (
            [[0.10, 0.050, 240.], [0.96, 0.030, 215.]],
            [[0.90, 0.035, 205.], [0.20, 0.055, 230.]],
            220.,
            0.28,
            [0.09, 0.14],
            [0.60, 0.44],
            [-0.05, 0.02, 0.06, -0.03, -0.01, 0.03],
        ),
        "forest" => (
            [[0.13, 0.040, 145.], [0.95, 0.025, 130.]],
            [[0.88, 0.035, 125.], [0.24, 0.050, 140.]],
            145.,
            0.25,
            [0.06, 0.10],
            [0.66, 0.48],
            [-0.05, 0.02, 0.07, -0.03, -0.01, 0.04],
        ),
        "earthtone" => (
            [[0.18, 0.040, 40.], [0.93, 0.025, 60.]],
            [[0.84, 0.050, 50.], [0.26, 0.055, 35.]],
            45.,
            0.40,
            [0.05, 0.09],
            [0.58, 0.44],
            [-0.10, -0.04, 0.04, 0.10, -0.07, 0.07],
        ),
        "neon" => (
            [[0.05, 0.030, 280.], [0.97, 0.015, 280.]],
            [[0.92, 0.045, 175.], [0.18, 0.060, 290.]],
            0.,
            0.,
            [0.20, 0.26],
            [0.72, 0.50],
            [-0.03, 0.01, 0.04, -0.02, -0.01, 0.02],
        ),
        "sunset" => (
            [[0.12, 0.060, 350.], [0.96, 0.030, 30.]],
            [[0.90, 0.050, 30.], [0.24, 0.060, 350.]],
            20.,
            0.32,
            [0.12, 0.18],
            [0.70, 0.50],
            [-0.05, 0.02, 0.07, -0.04, -0.02, 0.04],
        ),
        "vaporwave" => (
            [[0.15, 0.060, 315.], [0.96, 0.025, 320.]],
            [[0.90, 0.060, 190.], [0.26, 0.060, 310.]],
            320.,
            0.28,
            [0.10, 0.16],
            [0.74, 0.52],
            [-0.04, 0.02, 0.06, -0.03, -0.01, 0.04],
        ),
        "midnight" => (
            [[0.07, 0.035, 250.], [0.95, 0.020, 240.]],
            [[0.88, 0.020, 220.], [0.22, 0.045, 250.]],
            250.,
            0.20,
            [0.07, 0.12],
            [0.62, 0.42],
            [-0.04, 0.01, 0.05, -0.03, -0.02, 0.03],
        ),
        "aurora" => (
            [[0.08, 0.045, 240.], [0.96, 0.025, 220.]],
            [[0.92, 0.040, 175.], [0.20, 0.055, 240.]],
            160.,
            0.42,
            [0.12, 0.18],
            [0.70, 0.50],
            [-0.05, 0.02, 0.06, -0.03, -0.01, 0.04],
        ),
        _ => bail!("unknown extraction mode {mode:?}"),
    };
    let spec = Mood {
        bg,
        fg,
        anchor,
        tint,
        chroma,
        l,
        offset,
    };
    let m = light as usize;
    let mut p = base::extract_hues(colors, None, light);
    p[0] = base::hex(spec.bg[m]);
    p[7] = base::hex(spec.fg[m]);
    for i in 0..6 {
        let [_, c, h] = theme::to_oklch(&p[i + 1]);
        let hue = (h + ((spec.anchor - h + 540.) % 360. - 180.) * spec.tint + 360.) % 360.;
        p[i + 1] = base::hex([
            (spec.l[m] + spec.offset[i]).clamp(0.05, 0.95),
            c.clamp(spec.chroma[0], spec.chroma[1]),
            hue,
        ]);
    }
    base::finalize(&mut p);
    Ok(p)
}
