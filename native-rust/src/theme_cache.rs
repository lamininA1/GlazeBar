use crate::{graphics::wide, theme::Palette, theme_transaction::atomic_write};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    ptr::null_mut,
    time::UNIX_EPOCH,
};
use windows_sys::Win32::Security::Cryptography::*;
pub const MODES: [&str; 23] = [
    "normal",
    "monochromatic",
    "analogous",
    "pastel",
    "material",
    "colorful",
    "muted",
    "bright",
    "complementary",
    "triadic",
    "split-complementary",
    "tetradic",
    "fire",
    "ocean",
    "forest",
    "earthtone",
    "neon",
    "sunset",
    "vaporwave",
    "midnight",
    "aurora",
    "high-contrast",
    "duotone",
];
pub fn validate_mode(mode: &str) -> Result<()> {
    ensure!(MODES.contains(&mode), "unknown extraction mode {mode:?}");
    Ok(())
}
fn valid(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn digest(data: &[u8]) -> Result<String> {
    hash("MD5", 16, data)
}
pub fn hash(algorithm: &str, size: usize, data: &[u8]) -> Result<String> {
    unsafe {
        let mut handle = null_mut();
        let status =
            BCryptOpenAlgorithmProvider(&mut handle, wide(algorithm).as_ptr(), std::ptr::null(), 0);
        ensure!(status >= 0, "Cannot open cache-key provider: {status}");
        struct Algorithm(BCRYPT_ALG_HANDLE);
        impl Drop for Algorithm {
            fn drop(&mut self) {
                unsafe {
                    BCryptCloseAlgorithmProvider(self.0, 0);
                }
            }
        }
        let handle = Algorithm(handle);
        let mut hash = vec![0u8; size];
        let status = BCryptHash(
            handle.0,
            std::ptr::null(),
            0,
            data.as_ptr(),
            data.len() as u32,
            hash.as_mut_ptr(),
            size as u32,
        );
        ensure!(status >= 0, "Cannot hash cache key: {status}");
        Ok(hash.iter().map(|b| format!("{b:02x}")).collect())
    }
}
pub fn key_at(image: &str, nanoseconds: i128, light: bool, mode: &str) -> Result<String> {
    validate_mode(mode)?;
    let base = digest(
        format!(
            "{image}-{nanoseconds}-{}",
            if light { "light" } else { "dark" }
        )
        .as_bytes(),
    )?;
    Ok(match mode {
        "normal" => format!("{base}_v6"),
        "monochromatic" => format!("{base}_monochromatic_v5"),
        _ => format!("{base}_{mode}"),
    })
}
pub fn key(path: &Path, light: bool, mode: &str) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let n = match modified.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    };
    key_at(&path.to_string_lossy(), n, light, mode).ok()
}
pub fn directory() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|p| PathBuf::from(p).join(".cache/aether/color-cache"))
}
#[derive(Serialize, Deserialize)]
struct Data {
    palette: Palette,
    #[serde(default)]
    timestamp: i64,
    version: u32,
}
pub fn load(dir: &Path, key: &str) -> Option<Palette> {
    if !valid(key) {
        return None;
    }
    let data: Data =
        serde_json::from_slice(&std::fs::read(dir.join(format!("{key}.json"))).ok()?).ok()?;
    if data.version != 12
        || data.palette.iter().any(|c| {
            c.len() != 7
                || !c.starts_with('#')
                || !c.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        })
    {
        return None;
    }
    Some(data.palette)
}
pub fn save(dir: &Path, key: &str, palette: &Palette) -> Result<()> {
    ensure!(valid(key), "invalid cache key");
    let data = Data {
        palette: palette.clone(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        version: 12,
    };
    atomic_write(
        &dir.join(format!("{key}.json")),
        &serde_json::to_vec_pretty(&data)?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_and_cache_bytes_match_go() {
        #[derive(Deserialize)]
        struct Case {
            image: String,
            nanoseconds: i128,
            mode: String,
            light: bool,
            key: String,
            content: String,
            palette: Palette,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/cache-go.json")).unwrap();
        assert_eq!(cases.len(), 46);
        let temp = tempfile::tempdir().unwrap();
        for c in cases {
            assert_eq!(
                key_at(&c.image, c.nanoseconds, c.light, &c.mode).unwrap(),
                c.key
            );
            std::fs::write(temp.path().join(format!("{}.json", c.key)), c.content).unwrap();
            assert_eq!(load(temp.path(), &c.key), Some(c.palette));
        }
    }
    #[test]
    fn md5_and_mode_versions() {
        assert_eq!(digest(b"abc").unwrap(), "900150983cd24fb0d6963f7d28e17f72");
        assert!(
            key_at("image", 123, false, "normal")
                .unwrap()
                .ends_with("_v6")
        );
        assert!(
            key_at("image", 123, false, "monochromatic")
                .unwrap()
                .ends_with("_monochromatic_v5")
        );
        assert_ne!(
            key_at("image", 123, false, "normal").unwrap(),
            key_at("image", 124, false, "normal").unwrap()
        );
    }
    #[test]
    fn cache_roundtrip_and_invalid_entries() {
        let temp = tempfile::tempdir().unwrap();
        let p = std::array::from_fn(|_| "#123456".into());
        save(temp.path(), "sample_v6", &p).unwrap();
        assert_eq!(load(temp.path(), "sample_v6"), Some(p));
        for key in ["../outside", "", "a/b", "a\\b"] {
            assert!(load(temp.path(), key).is_none());
            assert!(save(temp.path(), key, &std::array::from_fn(|_| "#123456".into())).is_err());
        }
        std::fs::write(
            temp.path().join("sample_v6.json"),
            br##"{"palette":["#123456"],"version":12}"##,
        )
        .unwrap();
        assert!(load(temp.path(), "sample_v6").is_none());
        for mode in ["", "Normal", "../normal", "unknown"] {
            assert!(key_at("image", 0, false, mode).is_err());
        }
    }
}
