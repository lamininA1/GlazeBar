use crate::{
    palette_mono, pixel_sampler, quantize,
    theme::{self, Palette},
};
use anyhow::{Result, ensure};
use std::path::Path;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn style_comparison_matches_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            image: String,
            light: bool,
            both: bool,
            expected: serde_json::Value,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/compare-go.json")).unwrap();
        for c in cases {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(c.image);
            assert_eq!(compare(&path, c.light, c.both, &[]).unwrap(), c.expected);
            let cache = tempfile::tempdir().unwrap();
            let modes = crate::wallpaper_ui::MODES;
            for _ in 0..2 {
                let actual = extract_modes(
                    &path,
                    if c.both { false } else { c.light },
                    &modes,
                    Some(cache.path()),
                )
                .unwrap();
                for (mode, palette) in modes.into_iter().zip(actual) {
                    assert_eq!(serde_json::json!(palette), c.expected["palettes"][mode]);
                }
            }
        }
    }
    #[test]
    fn cache_hit_skips_decode_and_unknown_modes_are_rejected_first() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("not-an-image.png");
        std::fs::write(&source, b"not an image").unwrap();
        let p = std::array::from_fn(|_| "#abcdef".into());
        for mode in crate::theme_cache::MODES {
            let key = crate::theme_cache::key(&source, false, mode).unwrap();
            crate::theme_cache::save(temp.path(), &key, &p).unwrap();
            assert_eq!(
                extract_cached(&source, false, mode, Some(temp.path())).unwrap(),
                p
            );
        }
        for mode in ["../normal", "unknown", "Normal", ""] {
            let e = extract_cached(&source, false, mode, Some(temp.path())).unwrap_err();
            assert!(e.to_string().contains("unknown extraction mode"));
        }
        assert_eq!(
            extract_modes(
                &source,
                false,
                &crate::wallpaper_ui::MODES,
                Some(temp.path())
            )
            .unwrap(),
            vec![theme::readable(&p).unwrap(); 6]
        );
        assert!(extract_modes(&source, false, &["normal", "unknown"], Some(temp.path())).is_err());
        let key = crate::theme_cache::key(&source, false, "normal").unwrap();
        std::fs::write(temp.path().join(format!("{key}.json")), b"broken").unwrap();
        assert!(extract_cached(&source, false, "normal", Some(temp.path())).is_err());
    }
    #[test]
    fn png_to_final_palette_matches_go() {
        #[derive(serde::Deserialize)]
        struct Case {
            image: String,
            light: bool,
            expected: Palette,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/extract-go.json")).unwrap();
        assert_eq!(cases.len(), 10);
        for c in cases {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(&c.image);
            assert_eq!(
                theme::readable(&extract_raw(&path, c.light, "normal").unwrap()).unwrap(),
                c.expected,
                "{} light={}",
                c.image,
                c.light
            );
        }
    }
}
pub fn extract(path: &Path, light: bool, mode: &str) -> Result<Palette> {
    let mode = if mode.is_empty() { "normal" } else { mode };
    let p = extract_cached(
        path,
        light,
        mode,
        crate::theme_cache::directory().as_deref(),
    )?;
    theme::readable(&p)
}
pub fn extract_modes(
    path: &Path,
    light: bool,
    modes: &[&str],
    dir: Option<&Path>,
) -> Result<Vec<Palette>> {
    for mode in modes {
        crate::theme_cache::validate_mode(mode)?;
    }
    let keys: Vec<_> = modes
        .iter()
        .map(|mode| crate::theme_cache::key(path, light, mode))
        .collect();
    let mut raw: Vec<_> = keys
        .iter()
        .map(|key| {
            dir.zip(key.as_deref())
                .and_then(|(dir, key)| crate::theme_cache::load(dir, key))
        })
        .collect();
    if raw.iter().any(Option::is_none) {
        let (colors, weights) = dominant(path)?;
        for (i, mode) in modes.iter().enumerate() {
            if raw[i].is_some() {
                continue;
            }
            let p = palette_mono::generate(&colors, Some(&weights), light, mode)?;
            if keys[i] == crate::theme_cache::key(path, light, mode) {
                if let Some((dir, key)) = dir.zip(keys[i].as_deref()) {
                    if let Err(e) = crate::theme_cache::save(dir, key, &p) {
                        eprintln!("[cache] write failed: {e}");
                    }
                }
            }
            raw[i] = Some(p);
        }
    }
    raw.into_iter()
        .map(|p| theme::readable(&p.unwrap()))
        .collect()
}
fn dominant(path: &Path) -> Result<(Vec<String>, Vec<f64>)> {
    let pixels = pixel_sampler::load(path)?;
    let entries = quantize::dominant(&pixels, 48)?;
    let total = entries.iter().map(|e| e.count).sum::<usize>() as f64;
    let colors: Vec<_> = entries.iter().map(|e| e.hex.clone()).collect();
    let weights: Vec<_> = entries.iter().map(|e| e.count as f64 / total).collect();
    Ok((colors, weights))
}
pub fn extract_raw(path: &Path, light: bool, mode: &str) -> Result<Palette> {
    crate::theme_cache::validate_mode(mode)?;
    let (colors, weights) = dominant(path)?;
    palette_mono::generate(&colors, Some(&weights), light, mode)
}
fn extract_cached(path: &Path, light: bool, mode: &str, dir: Option<&Path>) -> Result<Palette> {
    crate::theme_cache::validate_mode(mode)?;
    let key = crate::theme_cache::key(path, light, mode);
    if let (Some(dir), Some(key)) = (dir, key.as_deref()) {
        if let Some(p) = crate::theme_cache::load(dir, key) {
            return Ok(p);
        }
    }
    let p = extract_raw(path, light, mode)?;
    if key == crate::theme_cache::key(path, light, mode) {
        if let (Some(dir), Some(key)) = (dir, key.as_deref()) {
            if let Err(e) = crate::theme_cache::save(dir, key, &p) {
                eprintln!("[cache] write failed: {e}");
            }
        }
    }
    Ok(p)
}
pub fn compare(
    path: &Path,
    light: bool,
    both: bool,
    modes: &[String],
) -> Result<serde_json::Value> {
    let allowed = [
        "normal",
        "muted",
        "pastel",
        "material",
        "colorful",
        "high-contrast",
    ];
    let modes: Vec<&str> = if modes.is_empty() {
        allowed.to_vec()
    } else {
        modes.iter().map(String::as_str).collect()
    };
    for mode in &modes {
        ensure!(allowed.contains(mode), "unsupported comparison mode {mode}");
    }
    let (colors, weights) = dominant(path)?;
    let mut result = serde_json::json!({"palettes":{}});
    if both {
        result["lightPalettes"] = serde_json::json!({});
    }
    for mode in modes {
        let p = palette_mono::generate(
            &colors,
            Some(&weights),
            if both { false } else { light },
            mode,
        )?;
        result["palettes"][mode] = serde_json::json!(theme::readable(&p)?);
        if both {
            let p = palette_mono::generate(&colors, Some(&weights), true, mode)?;
            result["lightPalettes"][mode] = serde_json::json!(theme::readable(&p)?);
        }
    }
    Ok(result)
}
