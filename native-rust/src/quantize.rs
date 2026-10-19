//! OKLab median-cut, matching the pinned Aether extraction pipeline.
use crate::{go_sort, theme};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
type Lab = [f64; 3];
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub hex: String,
    pub count: usize,
}
struct Bucket {
    colors: Vec<Lab>,
    ranges: [[f64; 2]; 3],
}
impl Bucket {
    fn new(colors: Vec<Lab>) -> Self {
        let first = colors.first().copied().unwrap_or([0.; 3]);
        let mut ranges = first.map(|v| [v, v]);
        for c in &colors {
            for ch in 0..3 {
                ranges[ch][0] = ranges[ch][0].min(c[ch]);
                ranges[ch][1] = ranges[ch][1].max(c[ch]);
            }
        }
        Self { colors, ranges }
    }
    fn spans(&self) -> [f64; 3] {
        self.ranges.map(|r| r[1] - r[0])
    }
    fn volume(&self) -> f64 {
        let [l, a, b] = self.spans();
        l * a * b * self.colors.len() as f64
    }
    fn split(mut self) -> (Self, Self) {
        let [l, a, b] = self.spans();
        let l = l * 1.2;
        let axis = if l >= a && l >= b {
            0
        } else if a >= b {
            1
        } else {
            2
        };
        go_sort::sort(&mut self.colors, |a, b| a[axis] < b[axis]);
        let right = self.colors.split_off(self.colors.len() / 2);
        (Self::new(self.colors), Self::new(right))
    }
    fn average(&self) -> Entry {
        let mut sum = [0.; 3];
        for c in &self.colors {
            for i in 0..3 {
                sum[i] += c[i];
            }
        }
        let count = self.colors.len();
        Entry {
            hex: hex(sum.map(|v| v / count as f64)),
            count,
        }
    }
}
fn hex(lab: Lab) -> String {
    let [r, g, b] = theme::oklab_to_rgb(lab);
    format!("#{r:02X}{g:02X}{b:02X}")
}
fn boost(pixels: &[Lab]) -> Vec<Lab> {
    let mut result = Vec::with_capacity(pixels.len() * 2);
    for &p in pixels {
        result.push(p);
        let chroma = (p[1] * p[1] + p[2] * p[2]).sqrt();
        if chroma > 0.04 {
            let extra = (((chroma - 0.04) / 0.16 * 3.).round() as usize).clamp(1, 3);
            for _ in 0..extra {
                result.push(p);
            }
        }
    }
    result
}
fn median(colors: Vec<Lab>, num: usize) -> Vec<Entry> {
    if colors.is_empty() {
        return vec![];
    }
    if colors.len() <= num {
        let mut unique: Vec<Entry> = vec![];
        for c in colors {
            if !unique.iter().any(|u| {
                let n = u32::from_str_radix(&u.hex[1..], 16).unwrap();
                let lab = theme::rgb_to_oklab([
                    ((n >> 16) & 255) as f64,
                    ((n >> 8) & 255) as f64,
                    (n & 255) as f64,
                ]);
                ((c[0] - lab[0]).powi(2) + (c[1] - lab[1]).powi(2) + (c[2] - lab[2]).powi(2)).sqrt()
                    < 0.01
            }) {
                unique.push(Entry {
                    hex: hex(c),
                    count: 1,
                });
            }
        }
        return unique;
    }
    let mut buckets = vec![Bucket::new(colors)];
    while buckets.len() < num {
        let mut best = None;
        let mut volume = 0.;
        for (i, b) in buckets.iter().enumerate() {
            if b.colors.len() > 1 && b.volume() > volume {
                volume = b.volume();
                best = Some(i);
            }
        }
        let Some(index) = best else {
            break;
        };
        let (left, right) = buckets.remove(index).split();
        buckets.insert(index, left);
        buckets.insert(index + 1, right);
    }
    buckets
        .iter()
        .filter(|b| !b.colors.is_empty())
        .map(Bucket::average)
        .collect()
}
pub fn dominant(pixels: &[[f64; 3]], num: usize) -> Result<Vec<Entry>> {
    ensure!(pixels.len() >= 100, "not enough pixels to extract colors");
    ensure!((1..=256).contains(&num), "invalid dominant-color count");
    ensure!(
        pixels
            .iter()
            .flatten()
            .all(|v| v.is_finite() && (0. ..=255.).contains(v)),
        "invalid RGB pixel"
    );
    let labs: Vec<_> = pixels.iter().copied().map(theme::rgb_to_oklab).collect();
    let mut result = median(boost(&labs), num);
    ensure!(!result.is_empty(), "no colors extracted from image");
    go_sort::sort(&mut result, |a, b| a.count > b.count);
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dominant_colors_and_counts_match_go() {
        #[derive(Deserialize)]
        struct Case {
            pixels: Vec<[f64; 3]>,
            entries: Vec<Entry>,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/quantize-go.json")).unwrap();
        assert_eq!(cases.len(), 8);
        for (i, case) in cases.into_iter().enumerate() {
            assert_eq!(
                dominant(&case.pixels, 48).unwrap(),
                case.entries,
                "quantization case {i}"
            );
        }
    }
    #[test]
    fn insufficient_and_invalid_pixels_are_rejected() {
        assert!(dominant(&[[0.; 3]; 99], 48).is_err());
        assert!(dominant(&[[f64::NAN; 3]; 100], 48).is_err());
        assert!(dominant(&[[0.; 3]; 100], 0).is_err());
        assert_eq!(
            dominant(&[[0.; 3]; 100], 48).unwrap(),
            vec![Entry {
                hex: "#000000".into(),
                count: 100
            }]
        );
    }
}
