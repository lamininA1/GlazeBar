use crate::{
    go_sort, palette_base as base,
    theme::{self, Palette},
};
use anyhow::Result;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_and_monochromatic_match_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            colors: Vec<String>,
            weights: Option<Vec<f64>>,
            light: bool,
            mode: String,
            expected: Palette,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/mono-go.json")).unwrap();
        assert_eq!(cases.len(), 296);
        for (i, c) in cases.into_iter().enumerate() {
            assert_eq!(
                generate(&c.colors, c.weights.as_deref(), c.light, &c.mode).unwrap(),
                c.expected,
                "mono case {i}, {}",
                c.mode
            );
        }
    }
}
fn monochrome(colors: &[String], weights: Option<&[f64]>) -> bool {
    if colors.is_empty() {
        return false;
    }
    let (mut achro, mut chroma, mut sin, mut cos) = (0., 0., 0., 0.);
    let mut count = 0;
    for (i, c) in colors.iter().enumerate() {
        let [_, ch, h] = theme::to_oklch(c);
        let w = weights.map_or(1., |w| w.get(i).copied().unwrap_or(0.));
        if ch < 0.04 {
            achro += w;
            continue;
        }
        chroma += w;
        count += 1;
        let rad = h * std::f64::consts::PI / 180.;
        sin += rad.sin() * w;
        cos += rad.cos() * w;
    }
    if weights.is_none() {
        return achro / colors.len() as f64 > 0.6
            || (count >= 4 && (sin * sin + cos * cos).sqrt() / count as f64 > 0.85);
    }
    if achro + chroma == 0. {
        return false;
    }
    chroma / (achro + chroma) < 0.06 || (sin * sin + cos * cos).sqrt() / chroma > 0.85
}
fn tint(colors: &[String]) -> Option<f64> {
    let (mut sin, mut cos, mut total) = (0., 0., 0.);
    for c in colors {
        let [_, ch, h] = theme::to_oklch(c);
        if ch > 0.005 {
            let rad = h * std::f64::consts::PI / 180.;
            sin += rad.sin() * ch;
            cos += rad.cos() * ch;
            total += ch;
        }
    }
    if total / (colors.len() as f64) < 0.008 {
        return None;
    }
    Some(((sin / total).atan2(cos / total) * 180. / std::f64::consts::PI + 360.) % 360.)
}
fn readable_l(bg: &str, light: bool) -> f64 {
    let mut l = if light { 0.80 } else { 0.20 };
    while if light { l >= 0.05 } else { l <= 0.95 } {
        if theme::contrast(bg, &base::hex([l, 0., 0.])) >= 4.5 {
            return l;
        }
        l += if light { -0.01 } else { 0.01 };
    }
    if light { 0.45 } else { 0.55 }
}
fn headroom(bg: String, light: bool) -> String {
    let [_, c, h] = theme::to_oklch(&bg);
    if light && readable_l(&bg, true) < 0.42 {
        return base::hex([0.95, c.min(0.04), h]);
    }
    if !light && readable_l(&bg, false) > 0.60 {
        return base::hex([0.13, c.min(0.06), h]);
    }
    bg
}
fn ramp(bg: &str, light: bool) -> [f64; 6] {
    let start = readable_l(bg, light) + if light { -0.02 } else { 0.02 };
    let step = if light {
        -((start - 0.16) / 5.).max(0.072)
    } else {
        ((0.92 - start) / 5.).max(0.072)
    };
    let mut result = [0.; 6];
    for (pos, slot) in [3, 0, 4, 1, 5, 2].iter().enumerate() {
        result[*slot] = (start + step * pos as f64).clamp(0.06, 0.97);
    }
    result
}
pub fn mono(colors: &[String], light: bool, explicit: bool) -> Palette {
    let mut sorted: Vec<_> = colors.iter().map(|c| (c, theme::to_oklch(c)[0])).collect();
    go_sort::sort(&mut sorted, |a, b| a.1 < b.1);
    let darkest = sorted.first().unwrap();
    let lightest = sorted.last().unwrap();
    let mut p: Palette = std::array::from_fn(|_| String::new());
    if let Some(hue) = tint(colors) {
        p[0] = if light {
            lightest.0.clone()
        } else {
            darkest.0.clone()
        };
        p[7] = if light {
            darkest.0.clone()
        } else {
            lightest.0.clone()
        };
        let [l, c, h] = theme::to_oklch(&p[0]);
        if light && l < 0.75 {
            p[0] = base::hex([0.94, c.min(0.04), h]);
        }
        if !light && l > 0.35 {
            p[0] = base::hex([0.14, c.min(0.06), h]);
        }
        p[0] = headroom(p[0].clone(), light);
        if theme::contrast(&p[0], &p[7]) < 7. {
            let mut lab = theme::to_oklab(&p[7]);
            lab[0] = if theme::to_oklab(&p[0])[0] < 0.5 {
                0.97
            } else {
                0.05
            };
            p[7] = base::lab_hex(lab);
        }
        let levels = [0.09, 0.11, 0.13, 0.10, 0.12, 0.14];
        let bright = [0.11, 0.14, 0.16, 0.12, 0.15, 0.17];
        let factor = (base::salient(colors) / (levels.iter().sum::<f64>() / 6.)).clamp(0.4, 1.);
        let slots = ramp(&p[0], light);
        let arc = if explicit { 28. } else { 46. };
        for i in 0..6 {
            let h =
                (hue + (((base::HUES[i] - hue + 540.) % 360. - 180.) / 180.) * arc + 360.) % 360.;
            p[i + 1] = base::hex([slots[i], levels[i] * factor, h]);
            p[i + 9] = base::hex([
                (slots[i] + if light { -0.07 } else { 0.07 }).clamp(0.18, 0.96),
                bright[i] * factor,
                h,
            ]);
        }
        p[15] = base::hex(if light {
            [0.08, 0.03, hue]
        } else {
            [0.97, 0.015, hue]
        });
    } else {
        p[0] = base::hex([
            if light {
                lightest.1.max(0.94)
            } else {
                darkest.1.min(0.14)
            },
            0.,
            0.,
        ]);
        p[7] = base::hex([
            if light {
                darkest.1.min(0.22)
            } else {
                lightest.1.max(0.88)
            },
            0.,
            0.,
        ]);
        p[0] = headroom(p[0].clone(), light);
        let slots = ramp(&p[0], light);
        for i in 0..6 {
            p[i + 1] = base::hex([slots[i], 0., 0.]);
            p[i + 9] = base::hex([
                (slots[i] + if light { -0.06 } else { 0.06 }).clamp(0.05, 0.97),
                0.,
                0.,
            ]);
        }
        p[15] = base::hex([if light { 0.04 } else { 0.99 }, 0., 0.]);
    }
    p[8] = base::comment(&p[0]);
    p
}
pub fn generate(
    colors: &[String],
    weights: Option<&[f64]>,
    light: bool,
    mode: &str,
) -> Result<Palette> {
    base::validate(colors, weights)?;
    let mut p = match mode {
        "normal" => {
            if monochrome(colors, weights) {
                mono(colors, light, false)
            } else {
                base::chromatic(colors, weights, light)?
            }
        }
        "monochromatic" => mono(colors, light, true),
        "chromatic" => base::chromatic(colors, weights, light)?,
        _ => crate::palette_styles::generate(colors, light, mode)?,
    };
    base::normalize(&mut p);
    Ok(p)
}
