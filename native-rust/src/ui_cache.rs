use crate::theme::Palette;
use anyhow::Result;
use std::path::{Path, PathBuf};
pub fn directory(kind: &str) -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("GlazeBar")
            .join(format!("{kind}-cache")),
    )
}
pub fn identity(path: &Path) -> Option<String> {
    use std::os::windows::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    let full = std::path::absolute(path)
        .ok()?
        .components()
        .collect::<PathBuf>();
    Some(identity_at(
        &full.to_string_lossy(),
        meta.len(),
        u128::from(meta.last_write_time()) + 504911232000000000u128,
    ))
}
fn identity_at(path: &str, size: u64, ticks: u128) -> String {
    let upper: String = path
        .chars()
        .map(|ch| {
            let code = ch as u32;
            let index = crate::go_upper_table::UPPER.partition_point(|&(_, end, _)| end < code);
            if let Some(&(start, _, delta)) = crate::go_upper_table::UPPER.get(index) {
                if code >= start {
                    return char::from_u32((i64::from(code) + i64::from(delta)) as u32).unwrap();
                }
            }
            ch
        })
        .collect();
    format!("{upper}|{size}|{ticks}")
}
fn palette_key_at(identity: &str, mode: &str, light: bool) -> Option<String> {
    crate::theme_cache::hash(
        "SHA256",
        32,
        format!(
            "aether-29d07b1-normalize-v2|{identity}|{mode}|{}",
            if light { "True" } else { "False" }
        )
        .as_bytes(),
    )
    .ok()
}
pub fn palette_key(path: &Path, mode: &str, light: bool) -> Option<String> {
    let identity = identity(path)?;
    palette_key_at(&identity, mode, light)
}
pub fn thumbnail_key(path: &Path) -> Option<String> {
    crate::theme_cache::hash(
        "SHA256",
        32,
        format!("thumbnail-v1-640|{}", identity(path)?).as_bytes(),
    )
    .ok()
}
pub fn load(dir: &Path, path: &Path, mode: &str, light: bool) -> Option<Palette> {
    let key = palette_key(path, mode, light)?;
    let p: Palette =
        serde_json::from_slice(&std::fs::read(dir.join(format!("{key}.json"))).ok()?).ok()?;
    if p.iter().all(|c| {
        c.len() == 7 && c.starts_with('#') && c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    }) {
        Some(p)
    } else {
        None
    }
}
pub fn extract(path: &Path, light: bool, mode: &str) -> Result<Palette> {
    Ok(extract_modes(
        path,
        light,
        &[mode],
        directory("palette").as_deref(),
        crate::theme_cache::directory().as_deref(),
    )?
    .remove(0))
}
pub fn extract_modes(
    path: &Path,
    light: bool,
    modes: &[&str],
    ui_dir: Option<&Path>,
    engine_dir: Option<&Path>,
) -> Result<Vec<Palette>> {
    for mode in modes {
        crate::theme_cache::validate_mode(mode)?;
    }
    let keys: Vec<_> = modes
        .iter()
        .map(|mode| palette_key(path, mode, light))
        .collect();
    let mut values: Vec<_> = modes
        .iter()
        .map(|mode| ui_dir.and_then(|dir| load(dir, path, mode, light)))
        .collect();
    let missing: Vec<_> = modes
        .iter()
        .enumerate()
        .filter(|(i, _)| values[*i].is_none())
        .map(|(i, mode)| (i, *mode))
        .collect();
    if !missing.is_empty() {
        let generated = crate::theme_extract::extract_modes(
            path,
            light,
            &missing.iter().map(|(_, mode)| *mode).collect::<Vec<_>>(),
            engine_dir,
        )?;
        for ((index, mode), p) in missing.into_iter().zip(generated) {
            if keys[index] == palette_key(path, mode, light) {
                if let Some((dir, key)) = ui_dir.zip(keys[index].as_deref()) {
                    if let Err(e) = crate::theme_transaction::atomic_write(
                        &dir.join(format!("{key}.json")),
                        &serde_json::to_vec(&p)?,
                    ) {
                        eprintln!("[UI cache] write failed: {e}");
                    }
                }
            }
            values[index] = Some(p);
        }
    }
    Ok(values.into_iter().map(Option::unwrap).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cross_language_cache_roundtrip() {
        let Some(root) = std::env::var_os("GLAZEBAR_UI_CACHE_ROUNDTRIP") else {
            return;
        };
        let root = PathBuf::from(root);
        let image = root.join("image.png");
        let palettes = root.join("GlazeBar/palette-cache");
        if std::env::var("GLAZEBAR_UI_CACHE_PHASE").as_deref() == Ok("read") {
            assert_eq!(
                load(&palettes, &image, "normal", false).unwrap()[1],
                "#123456"
            );
            return;
        }
        std::fs::create_dir_all(&root).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sampler-1.png"),
            &image,
        )
        .unwrap();
        let mut expected = serde_json::Map::new();
        for light in [false, true] {
            let generated = extract_modes(
                &image,
                light,
                &crate::wallpaper_ui::MODES,
                Some(&palettes),
                Some(&root.join("engine")),
            )
            .unwrap();
            for (mode, palette) in crate::wallpaper_ui::MODES.into_iter().zip(generated) {
                expected.insert(format!("{mode}-{light}"), serde_json::json!(palette));
            }
        }
        crate::wallpaper_ui::thumbnail_in(&image, Some(&root.join("GlazeBar/thumbnail-cache")))
            .unwrap();
        std::fs::write(
            root.join("expected.json"),
            serde_json::to_vec_pretty(&expected).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn identities_match_go_and_palette_keys_invalidate_old_normalization() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../fixtures/ui-cache-go.json")).unwrap();
        for c in cases {
            let id = identity_at(
                c["path"].as_str().unwrap(),
                c["size"].as_u64().unwrap(),
                u128::from(c["ticks"].as_u64().unwrap()),
            );
            assert_eq!(id, c["identity"]);
            assert_ne!(
                palette_key_at(
                    &id,
                    c["mode"].as_str().unwrap(),
                    c["light"].as_bool().unwrap()
                )
                .unwrap(),
                c["paletteKey"]
            );
            assert_eq!(
                crate::theme_cache::hash("SHA256", 32, format!("thumbnail-v1-640|{id}").as_bytes())
                    .unwrap(),
                c["thumbnailKey"]
            );
        }
    }
    #[test]
    fn old_final_palette_is_regenerated_with_readable_comments() {
        let dir = tempfile::tempdir().unwrap();
        let image = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/sampler-0.png");
        let id = identity(&image).unwrap();
        let old_key = crate::theme_cache::hash(
            "SHA256",
            32,
            format!("aether-29d07b1-normalize-v1|{id}|normal|False").as_bytes(),
        )
        .unwrap();
        let old: Palette = std::array::from_fn(|_| "#fdf3eb".into());
        let old_path = dir.path().join(format!("{old_key}.json"));
        std::fs::write(&old_path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(load(dir.path(), &image, "normal", false).is_none());
        let generated = extract_modes(&image, false, &["normal"], Some(dir.path()), None).unwrap();
        let p = &generated[0];
        assert_ne!(p[8], p[15]);
        assert!(crate::theme::contrast(&p[0], &p[8]) >= 4.5);
        assert_eq!(load(dir.path(), &image, "normal", false).as_ref(), Some(p));
        assert!(old_path.exists());
    }
    #[test]
    fn final_palette_hit_preserves_bytes_and_skips_decoder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.png");
        std::fs::write(&path, b"not an image").unwrap();
        let palette: Palette = std::array::from_fn(|_| "#aBcDEF".into());
        let key = palette_key(&path, "normal", false).unwrap();
        std::fs::write(
            dir.path().join(format!("{key}.json")),
            serde_json::to_vec(&palette).unwrap(),
        )
        .unwrap();
        assert_eq!(
            extract_modes(&path, false, &["normal"], Some(dir.path()), None).unwrap(),
            vec![palette]
        );
        assert!(extract_modes(&path, true, &["normal"], Some(dir.path()), None).is_err());
        std::fs::write(&path, b"changed size").unwrap();
        assert!(load(dir.path(), &path, "normal", false).is_none());
    }
}
