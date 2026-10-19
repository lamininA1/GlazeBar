//! Catmull-Rom sampling compatible with Go x/image/draw's separable kernel.
//! Kernel accumulation/rounding adapted under licenses/GO-LICENSE.txt.
use anyhow::{Result, ensure};
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::path::Path;
struct Distribution {
    weights: Vec<(usize, f64)>,
    inverse: f64,
}
fn distribution(dst: usize, src: usize) -> Vec<Distribution> {
    let scale = src as f64 / dst as f64;
    let half = 2. * scale.max(1.);
    let argument = if scale > 1. { 1. / scale } else { 1. };
    (0..dst)
        .map(|x| {
            let center = (x as f64 + 0.5) * scale - 0.5;
            let start = (center - half).floor().max(0.) as usize;
            let end = (center + half).ceil().min(src as f64) as usize;
            let mut weights = vec![];
            let mut total = 0.;
            for coord in start..end {
                let t = ((center - coord as f64) * argument).abs();
                if t >= 2. {
                    continue;
                }
                let weight = if t < 1. {
                    (1.5 * t - 2.5) * t * t + 1.
                } else {
                    ((-0.5 * t + 2.5) * t - 4.) * t + 2.
                };
                if weight != 0. {
                    total += weight;
                    weights.push((coord, weight));
                }
            }
            Distribution {
                weights,
                inverse: 1. / total,
            }
        })
        .collect()
}
fn sample(w: usize, h: usize, pixel: impl Fn(usize, usize) -> [u16; 4]) -> Vec<[f64; 3]> {
    let (dw, dh) = if w >= h {
        (400, (h as f64 * 400. / w as f64).round().max(1.) as usize)
    } else {
        ((w as f64 * 400. / h as f64).round().max(1.) as usize, 400)
    };
    let horizontal = distribution(dw, w);
    let vertical = distribution(dh, h);
    let mut tmp = vec![[0.; 4]; dw * h];
    for y in 0..h {
        for (x, d) in horizontal.iter().enumerate() {
            let mut sum = [0.; 4];
            for &(sx, weight) in &d.weights {
                let p = pixel(sx, y).map(u32::from);
                let p = [
                    p[0] * p[3] / 65535,
                    p[1] * p[3] / 65535,
                    p[2] * p[3] / 65535,
                    p[3],
                ];
                for i in 0..4 {
                    sum[i] += p[i] as f64 * weight;
                }
            }
            tmp[y * dw + x] = sum.map(|v| v * (d.inverse / 65535.));
        }
    }
    let step = (dw * dh / 50000).max(1);
    let mut colors = vec![];
    for y in (0..dh).step_by(step) {
        let d = &vertical[y];
        for x in (0..dw).step_by(step) {
            let mut sum = [0.; 4];
            for &(sy, weight) in &d.weights {
                for (i, v) in sum.iter_mut().enumerate() {
                    *v += tmp[sy * dw + x][i] * weight;
                }
            }
            for i in 0..3 {
                sum[i] = sum[i].min(sum[3]);
            }
            let mut p =
                sum.map(|v| ((65535. * (v * d.inverse) + 0.5) as i32).clamp(0, 65535) as u32);
            if p[3] >> 8 < 128 {
                continue;
            }
            if p[3] != 65535 {
                for i in 0..3 {
                    p[i] = p[i] * 65535 / p[3];
                }
            }
            colors.push([(p[0] >> 8) as f64, (p[1] >> 8) as f64, (p[2] >> 8) as f64]);
        }
    }
    colors
}
pub fn load(path: &Path) -> Result<Vec<[f64; 3]>> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    // Go image/gif.Decode returns the first frame's rectangle, not a composited
    // logical screen. Padding it changes the resize ratio and sampled colors.
    if reader.format() == Some(image::ImageFormat::Gif) {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        options.check_frame_consistency(true);
        let mut decoder = options.read_info(reader.into_inner())?;
        dimensions(u32::from(decoder.width()), u32::from(decoder.height()))?;
        let frame = decoder
            .next_frame_info()?
            .ok_or_else(|| anyhow::anyhow!("GIF has no frames"))?;
        let (w, h) = (usize::from(frame.width), usize::from(frame.height));
        dimensions(w as u32, h as u32)?;
        let mut pixels = vec![0u8; w * h * 4];
        decoder.read_into_buffer(&mut pixels)?;
        return Ok(sample(w, h, |x, y| {
            let offset = (y * w + x) * 4;
            std::array::from_fn(|i| u16::from(pixels[offset + i]) * 257)
        }));
    }
    let decoder = reader.into_decoder()?;
    let (w, h) = decoder.dimensions();
    dimensions(w, h)?;
    let image = DynamicImage::from_decoder(decoder)?;
    let colors = if matches!(
        image,
        DynamicImage::ImageLuma16(_)
            | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageRgb16(_)
            | DynamicImage::ImageRgba16(_)
    ) {
        let image = image.into_rgba16();
        sample(w as usize, h as usize, |x, y| {
            image.get_pixel(x as u32, y as u32).0
        })
    } else {
        let image = image.into_rgba8();
        sample(w as usize, h as usize, |x, y| {
            image
                .get_pixel(x as u32, y as u32)
                .0
                .map(|v| v as u16 * 257)
        })
    };
    Ok(colors)
}
fn dimensions(w: u32, h: u32) -> Result<()> {
    ensure!(
        w > 0 && h > 0 && w <= 16384 && h <= 16384 && u64::from(w) * u64::from(h) <= 40_000_000,
        "unsafe image dimensions {w}x{h}"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    #[test]
    fn format_decode_comparison() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../fixtures/formats-go.json")).unwrap();
        let mut results = vec![];
        for case in cases {
            let name = case["image"].as_str().unwrap();
            let expected = STANDARD.decode(case["pixels"].as_str().unwrap()).unwrap();
            let pixels = load(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("fixtures")
                    .join(name),
            )
            .unwrap();
            let actual: Vec<u8> = pixels.iter().flatten().map(|v| *v as u8).collect();
            let deltas: Vec<u8> = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| a.abs_diff(*b))
                .collect();
            results.push(serde_json::json!({"image":name,"actualSamples":actual.len()/3,"expectedSamples":expected.len()/3,"differentChannels":deltas.iter().filter(|d|**d!=0).count(),"maxError":deltas.iter().max(),"meanError":deltas.iter().map(|v|*v as f64).sum::<f64>()/deltas.len() as f64}));
        }
        println!("{}", serde_json::to_string_pretty(&results).unwrap());
        if let Some(path) = std::env::var_os("GLAZEBAR_RUST_FORMATS_REPORT") {
            std::fs::write(path, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
        }
        assert!(
            results
                .iter()
                .all(|v| v["actualSamples"] == v["expectedSamples"]),
            "format frame dimensions/alpha mismatch"
        );
        for result in &results {
            let name = result["image"].as_str().unwrap();
            if name.ends_with(".bmp") || name.ends_with(".gif") {
                assert_eq!(
                    result["differentChannels"], 0,
                    "{name} must match Go exactly"
                );
            }
        }
    }
    #[test]
    fn gif_header_limits_precede_pixel_allocation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut data = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/format-paletted.gif"),
        )
        .unwrap();
        data[6..8].copy_from_slice(&16385u16.to_le_bytes());
        std::fs::write(file.path(), data).unwrap();
        assert!(
            load(file.path())
                .unwrap_err()
                .to_string()
                .contains("unsafe image dimensions")
        );
    }
    #[test]
    fn sampled_png_pixels_match_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            image: String,
            pixels: String,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/sampler-go.json")).unwrap();
        for case in cases {
            let pixels = load(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("fixtures")
                    .join(&case.image),
            )
            .unwrap();
            let actual: Vec<u8> = pixels.iter().flatten().map(|v| *v as u8).collect();
            let expected = STANDARD.decode(case.pixels).unwrap();
            assert_eq!(actual.len(), expected.len(), "{} sample length", case.image);
            let differences: Vec<_> = actual
                .iter()
                .zip(&expected)
                .enumerate()
                .filter(|(_, (a, b))| a != b)
                .take(10)
                .collect();
            assert!(
                differences.is_empty(),
                "{} pixel differences {differences:?}",
                case.image
            );
        }
    }
}
