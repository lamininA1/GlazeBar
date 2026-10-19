//! Pinned Aether palette selection/normalization; see licenses/AETHER-LICENSE.txt.
use crate::{
    go_sort,
    theme::{self, Palette},
};
use anyhow::{Result, ensure};
pub const HUES: [f64; 6] = [29.2, 139.1, 111.3, 266.7, 326.4, 194.8];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chromatic_palettes_match_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            colors: Vec<String>,
            weights: Option<Vec<f64>>,
            light: bool,
            expected: Palette,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/chromatic-go.json")).unwrap();
        assert_eq!(cases.len(), 132);
        for (i, c) in cases.into_iter().enumerate() {
            assert_eq!(
                chromatic(&c.colors, c.weights.as_deref(), c.light).unwrap(),
                c.expected,
                "chromatic case {i}"
            );
        }
    }
}
pub fn hex(lch: [f64; 3]) -> String {
    let [r, g, b] = theme::from_oklch(lch);
    format!("#{r:02x}{g:02x}{b:02x}")
}
pub fn lab_hex(lab: [f64; 3]) -> String {
    let [r, g, b] = theme::oklab_to_rgb(lab);
    format!("#{r:02x}{g:02x}{b:02x}")
}
pub fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    if d > 180. { 360. - d } else { d }
}
pub fn salient(colors: &[String]) -> f64 {
    if colors.is_empty() {
        return 0.;
    }
    let mut values: Vec<_> = colors.iter().map(|c| theme::to_oklch(c)[1]).collect();
    values.sort_by(f64::total_cmp);
    values[(0.9 * (values.len() - 1) as f64).round() as usize]
}
fn extremity(colors: &[String], weights: Option<&[f64]>, lightest: bool, used: &[bool]) -> usize {
    let mut index = 0;
    let mut best = f64::NEG_INFINITY;
    for (i, c) in colors.iter().enumerate() {
        if used.get(i) == Some(&true) {
            continue;
        }
        let l = theme::to_oklab(c)[0];
        let mut score = if lightest { l } else { 1. - l };
        if let Some(w) = weights.and_then(|w| w.get(i)) {
            score += 0.6 * w;
        }
        if score > best {
            best = score;
            index = i;
        }
    }
    index
}
pub fn foreground(
    colors: &[String],
    weights: Option<&[f64]>,
    light: bool,
    bg: &str,
    used: &[bool],
) -> (String, Option<usize>) {
    let index = extremity(colors, weights, !light, used);
    let fg = &colors[index];
    let contrast = theme::contrast(bg, fg);
    if contrast >= 7. {
        return (fg.clone(), Some(index));
    }
    let mut candidates: Vec<_> = colors
        .iter()
        .enumerate()
        .filter(|(i, _)| used.get(*i) != Some(&true))
        .map(|(i, c)| (i, theme::contrast(bg, c)))
        .filter(|(_, c)| *c > contrast)
        .collect();
    go_sort::sort(&mut candidates, |a, b| a.1 > b.1);
    if let Some(&(i, c)) = candidates.first() {
        if c >= 4.5 {
            return (colors[i].clone(), Some(i));
        }
    }
    let mut lab = theme::to_oklab(fg);
    lab[0] = if theme::to_oklab(bg)[0] < 0.5 {
        0.97
    } else {
        0.05
    };
    (lab_hex(lab), None)
}
pub fn score([l, c, h]: [f64; 3], hue: f64, light: bool) -> f64 {
    let hue_score = hue_distance(h, hue) * 2.;
    let chroma = if c < 0.035 {
        80.
    } else if c < 0.05 {
        40.
    } else if c < 0.06 {
        15.
    } else if c <= 0.20 {
        0.
    } else {
        (c - 0.20) * 50.
    };
    let ideal = if light { 0.45 } else { 0.60 };
    let luminance = if l < 0.25 {
        (0.25 - l) * 200.
    } else if l > 0.87 {
        (l - 0.87) * 150.
    } else {
        (l - ideal).abs() * 20.
    };
    hue_score + chroma + luminance
}
pub fn extract_hues(colors: &[String], weights: Option<&[f64]>, light: bool) -> Palette {
    let top = colors.len().min(12);
    let bg_index = extremity(&colors[..top], weights, light, &[]);
    let mut bg = colors[bg_index].clone();
    let lch = theme::to_oklch(&bg);
    if light && lch[0] < 0.85 {
        bg = hex([0.94, lch[1].min(0.04), lch[2]]);
    }
    if !light && lch[0] > 0.20 {
        bg = hex([0.12, lch[1].min(0.05), lch[2]]);
    }
    let mut used = vec![false; colors.len()];
    used[bg_index] = true;
    let (fg, index) = foreground(colors, weights, light, &bg, &used);
    if let Some(i) = index {
        used[i] = true;
    }
    let mut palette: Palette = std::array::from_fn(|_| String::new());
    palette[0] = bg;
    palette[7] = fg;
    let scores: Vec<Vec<_>> = HUES
        .iter()
        .map(|h| {
            let mut entries: Vec<_> = colors
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    (
                        i,
                        if used[i] {
                            f64::INFINITY
                        } else {
                            score(theme::to_oklch(c), *h, light)
                        },
                    )
                })
                .collect();
            go_sort::sort(&mut entries, |a, b| a.1 < b.1);
            entries
        })
        .collect();
    let mut assignments = [None; 6];
    for _ in 0..6 {
        let mut best = f64::INFINITY;
        let mut choice = None;
        for a in 0..6 {
            if assignments[a].is_some() {
                continue;
            }
            for &(i, s) in &scores[a] {
                if used[i] {
                    continue;
                }
                if s < best {
                    best = s;
                    choice = Some((a, i));
                }
                break;
            }
        }
        let Some((a, i)) = choice else {
            break;
        };
        assignments[a] = Some(i);
        used[i] = true;
    }
    for i in 0..6 {
        palette[i + 1] =
            colors[assignments[i].expect("at least eight validated dominant colors")].clone();
    }
    palette
}
pub fn bright(color: &str) -> String {
    let [l, c, h] = theme::to_oklch(color);
    if l >= 0.78 {
        return hex([(l + 0.04).min(0.94), (c + 0.04).max(c * 1.3).min(0.32), h]);
    }
    let boost = ((0.92 - l) * 0.6).min(0.12).max(0.04);
    hex([(l + boost).min(0.92), (c * 1.1).min(0.30), h])
}
pub fn boost(color: &str, bg: &str, ratio: f64) -> String {
    let mut c = theme::to_oklch(color);
    let dark = theme::to_oklab(bg)[0] < 0.5;
    while if dark { c[0] < 0.95 } else { c[0] > 0.05 } {
        c[0] += if dark { 0.03 } else { -0.03 };
        let candidate = hex(c);
        if theme::contrast(bg, &candidate) >= ratio {
            return candidate;
        }
    }
    hex(c)
}
pub fn comment(bg: &str) -> String {
    let [l, _, h] = theme::to_oklch(bg);
    let dark = l < 0.5;
    let mut target = if dark {
        (l + 0.30).min(1.)
    } else {
        (l - 0.30).max(0.)
    };
    let mut color = hex([target, 0.01, h]);
    while theme::contrast(bg, &color) < 3. && if dark { target < 0.95 } else { target > 0.05 } {
        target += if dark { 0.05 } else { -0.05 };
        color = hex([target, 0.01, h]);
    }
    color
}
pub fn normalize(p: &mut Palette) {
    for i in 1..=6 {
        if theme::contrast(&p[0], &p[i]) < 4.5 {
            p[i] = boost(&p[i], &p[0], 4.5);
            p[i + 8] = bright(&p[i]);
        }
    }
}
pub fn finalize(p: &mut Palette) {
    p[8] = comment(&p[0]);
    for i in 1..=6 {
        p[i + 8] = bright(&p[i]);
    }
    p[15] = bright(&p[7]);
    normalize(p);
}
fn clusters(colors: &[String], weights: Option<&[f64]>) -> Vec<f64> {
    let mut bins = [[0.; 3]; 12];
    for (i, c) in colors.iter().enumerate() {
        let [_, chroma, h] = theme::to_oklch(c);
        if chroma < 0.04 {
            continue;
        }
        let weight = weights
            .and_then(|w| w.get(i))
            .copied()
            .unwrap_or(1. / colors.len() as f64);
        let b = (h / 30.) as usize % 12;
        let rad = h * std::f64::consts::PI / 180.;
        bins[b][0] += weight;
        bins[b][1] += rad.sin() * weight;
        bins[b][2] += rad.cos() * weight;
    }
    bins.iter()
        .filter(|b| b[0] >= 0.025)
        .map(|b| (b[1].atan2(b[2]) * 180. / std::f64::consts::PI + 360.) % 360.)
        .collect()
}
pub fn chromatic(colors: &[String], weights: Option<&[f64]>, light: bool) -> Result<Palette> {
    validate(colors, weights)?;
    Ok(chromatic_validated(colors, weights, light))
}
pub fn validate(colors: &[String], weights: Option<&[f64]>) -> Result<()> {
    ensure!(colors.len() >= 8, "not enough colors extracted from image");
    ensure!(
        colors.iter().all(|c| c.len() == 7
            && c.starts_with('#')
            && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)),
        "invalid dominant color"
    );
    ensure!(
        weights.is_none_or(|w| w.iter().all(|v| v.is_finite() && *v >= 0.)),
        "invalid coverage weights"
    );
    Ok(())
}
fn chromatic_validated(colors: &[String], weights: Option<&[f64]>, light: bool) -> Palette {
    let mut p = extract_hues(colors, weights, light);
    let target = (salient(colors) * 0.85).clamp(0.05, 0.16);
    let clusters = clusters(colors, weights);
    for i in 1..=6 {
        let mut lch = theme::to_oklch(&p[i]);
        let gray = lch[1] < 0.03;
        if gray || !clusters.iter().any(|h| hue_distance(lch[2], *h) <= 30.) {
            let mut best = f64::INFINITY;
            let mut nearest = None;
            for h in &clusters {
                let d = hue_distance(HUES[i - 1], *h);
                if d < best {
                    best = d;
                    nearest = Some(*h);
                }
            }
            if let Some(h) = nearest {
                lch[2] = h;
            } else if gray {
                lch[1] = 0.;
            }
        }
        if lch[1] > 0. && lch[1] < target {
            lch[1] = target;
        }
        p[i] = hex(lch);
    }
    for i in 1..=6 {
        for j in i + 1..=6 {
            let a = theme::to_oklab(&p[i]);
            let b = theme::to_oklab(&p[j]);
            let dl = a[0] - b[0];
            let da = a[1] - b[1];
            let db = a[2] - b[2];
            if (dl * dl + da * da + db * db).sqrt() < 0.05 {
                let mut lch = theme::to_oklch(&p[j]);
                lch[0] = if light {
                    (lch[0] - 0.10).clamp(0.20, 0.85)
                } else {
                    (lch[0] + 0.10).clamp(0.30, 0.92)
                };
                p[j] = hex(lch);
            }
        }
    }
    finalize(&mut p);
    p
}
