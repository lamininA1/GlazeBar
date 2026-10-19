use crate::{
    graphics::wide,
    theme::{self, Palette},
    theme_transaction::Change,
};
use anyhow::{Result, bail, ensure};
use serde::Deserialize;
use std::{
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{Foundation::*, System::Registry::*, UI::WindowsAndMessaging::*};
pub const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
pub const DWM: &str = r"Software\Microsoft\Windows\DWM";
pub const ACCENT: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent";
const DESKTOP: &str = r"Control Panel\Desktop";
#[derive(Default, Clone, Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Options {
    pub mode: bool,
    pub accent: bool,
    pub titlebars: Option<bool>,
    pub taskbar: Option<bool>,
    pub transparency: Option<bool>,
}
pub fn valid(key: &str, name: &str) -> bool {
    [
        (
            PERSONALIZE,
            &[
                "AppsUseLightTheme",
                "SystemUsesLightTheme",
                "ColorPrevalence",
                "EnableTransparency",
            ][..],
        ),
        (
            DWM,
            &[
                "AccentColor",
                "ColorizationColor",
                "ColorizationAfterglow",
                "ColorPrevalence",
            ][..],
        ),
        (
            ACCENT,
            &["AccentColorMenu", "StartColorMenu", "AccentPalette"][..],
        ),
        (DESKTOP, &["AutoColorization"][..]),
    ]
    .iter()
    .any(|(k, n)| k.eq_ignore_ascii_case(key) && n.iter().any(|n| n.eq_ignore_ascii_case(name)))
}
fn data(typ: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = typ.to_le_bytes().to_vec();
    b.extend_from_slice(payload);
    b
}
fn integer(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        8 if bytes[..4] == REG_DWORD.to_le_bytes() => {
            Some(u32::from_le_bytes(bytes[4..].try_into().ok()?) as u64)
        }
        12 if bytes[..4] == REG_QWORD.to_le_bytes() => {
            Some(u64::from_le_bytes(bytes[4..].try_into().ok()?))
        }
        _ => None,
    }
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
fn read_raw(key: &str, name: &str) -> Result<Option<Vec<u8>>> {
    unsafe {
        let mut handle = null_mut();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut handle,
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        ensure!(status == ERROR_SUCCESS, "Registry open {key}: {status}");
        let handle = Key(handle);
        let name = wide(name);
        for _ in 0..4 {
            let mut len = 0;
            let mut typ = 0;
            let status = RegQueryValueExW(
                handle.0,
                name.as_ptr(),
                null(),
                &mut typ,
                null_mut(),
                &mut len,
            );
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            ensure!(status == ERROR_SUCCESS, "Registry size query: {status}");
            ensure!(len <= 1024 * 1024, "Registry value too large");
            let mut b = vec![0; len as usize];
            let status = RegQueryValueExW(
                handle.0,
                name.as_ptr(),
                null(),
                &mut typ,
                b.as_mut_ptr(),
                &mut len,
            );
            if status == ERROR_MORE_DATA {
                continue;
            }
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            ensure!(status == ERROR_SUCCESS, "Registry read: {status}");
            b.truncate(len as usize);
            return Ok(Some(data(typ, &b)));
        }
        bail!("Registry value changed repeatedly during read")
    }
}
pub fn read(key: &str, name: &str) -> Result<Option<Vec<u8>>> {
    ensure!(
        valid(key, name),
        "unrecognized registry value: {key}\\{name}"
    );
    read_raw(key, name)
}
fn write_raw(key: &str, name: &str, bytes: Option<&[u8]>) -> Result<()> {
    if let Some(b) = bytes {
        ensure!(b.len() >= 4, "invalid registry backup");
    }
    unsafe {
        let mut handle = null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null(),
            &mut handle,
            null_mut(),
        );
        ensure!(status == ERROR_SUCCESS, "Registry create/open: {status}");
        let handle = Key(handle);
        let name = wide(name);
        let status = if let Some(b) = bytes {
            RegSetValueExW(
                handle.0,
                name.as_ptr(),
                0,
                u32::from_le_bytes(b[..4].try_into().unwrap()),
                b[4..].as_ptr(),
                (b.len() - 4) as u32,
            )
        } else {
            RegDeleteValueW(handle.0, name.as_ptr())
        };
        ensure!(
            status == ERROR_SUCCESS || (bytes.is_none() && status == ERROR_FILE_NOT_FOUND),
            "Registry write: {status}"
        );
        Ok(())
    }
}
pub fn write(key: &str, name: &str, bytes: Option<&[u8]>) -> Result<()> {
    ensure!(
        valid(key, name),
        "unrecognized registry value: {key}\\{name}"
    );
    write_raw(key, name, bytes)
}
pub fn plan(p: &Palette, light: bool, options: &Options) -> Result<Vec<Change>> {
    plan_with(p, light, options, read)
}
fn plan_with(
    p: &Palette,
    light: bool,
    o: &Options,
    reader: impl Fn(&str, &str) -> Result<Option<Vec<u8>>>,
) -> Result<Vec<Change>> {
    let mut values: Vec<(&str, &str, Vec<u8>)> = vec![];
    let dword = |n: u32| data(REG_DWORD, &n.to_le_bytes());
    if o.mode {
        for name in ["AppsUseLightTheme", "SystemUsesLightTheme"] {
            values.push((PERSONALIZE, name, dword(light as u32)));
        }
    }
    if o.accent {
        let mut c = theme::to_oklch(&p[4]);
        c[0] = if light { 0.52 } else { 0.62 };
        c[1] = c[1].min(0.16);
        let [r, g, b] = theme::from_oklch(c).map(u32::from);
        let mut opacity = 0xc4000000;
        if let Some(old) = reader(DWM, "ColorizationColor")? {
            if old.len() == 8 {
                opacity = u32::from_le_bytes(old[4..].try_into().unwrap()) & 0xff000000;
            }
        }
        for (key, name, value) in [
            (DWM, "AccentColor", 0xff000000 | b << 16 | g << 8 | r),
            (DWM, "ColorizationColor", opacity | r << 16 | g << 8 | b),
            (DWM, "ColorizationAfterglow", opacity | r << 16 | g << 8 | b),
            (ACCENT, "AccentColorMenu", 0xff000000 | b << 16 | g << 8 | r),
        ] {
            values.push((key, name, dword(value)));
        }
        let mut palette = [0u8; 32];
        if let Some(old) = reader(ACCENT, "AccentPalette")? {
            if old.len() == 36 && old[..4] == REG_BINARY.to_le_bytes() {
                palette.copy_from_slice(&old[4..]);
            }
        }
        for (i, l) in [0.90, 0.80, 0.70, c[0], 0.45, 0.34, 0.24]
            .iter()
            .enumerate()
        {
            let mut shade = c;
            shade[0] = *l;
            palette[i * 4..i * 4 + 3].copy_from_slice(&theme::from_oklch(shade));
        }
        values.push((ACCENT, "AccentPalette", data(REG_BINARY, &palette)));
        values.push((
            ACCENT,
            "StartColorMenu",
            dword(
                0xff000000
                    | (palette[18] as u32) << 16
                    | (palette[17] as u32) << 8
                    | palette[16] as u32,
            ),
        ));
        values.push((DESKTOP, "AutoColorization", dword(0)));
    }
    if let Some(v) = o.titlebars {
        values.push((DWM, "ColorPrevalence", dword(v as u32)));
    }
    if let Some(v) = o.taskbar {
        if v {
            ensure!(
                if o.mode {
                    !light
                } else {
                    reader(PERSONALIZE, "SystemUsesLightTheme")?
                        .as_deref()
                        .and_then(integer)
                        == Some(0)
                },
                "Start/taskbar accent requires Windows dark mode"
            );
        }
        values.push((PERSONALIZE, "ColorPrevalence", dword(v as u32)));
    }
    if let Some(v) = o.transparency {
        values.push((PERSONALIZE, "EnableTransparency", dword(v as u32)));
    }
    ensure!(!values.is_empty(), "select at least one Windows option");
    values
        .into_iter()
        .map(|(key, name, after)| {
            let before = reader(key, name)?;
            Ok(Change {
                registry_key: key.into(),
                registry_name: name.into(),
                path: PathBuf::from(format!("HKCU\\{key}\\{name}")),
                existed: before.is_some(),
                before: before.unwrap_or_default(),
                after,
            })
        })
        .collect()
}
pub fn notify(changes: &[Change]) {
    if !changes.iter().any(|c| !c.registry_key.is_empty()) {
        return;
    }
    unsafe {
        let mut result = 0;
        for name in ["ImmersiveColorSet", "WindowsThemeElement", "TraySettings"] {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                wide(name).as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                1000,
                &mut result,
            );
        }
        if let Ok(Some(b)) = read(DWM, "ColorizationColor") {
            if let Some(n) = integer(&b) {
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    0x320,
                    n as usize,
                    1,
                    SMTO_ABORTIFHUNG,
                    1000,
                    &mut result,
                );
            }
        }
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_THEMECHANGED,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_match_go_reference() {
        #[derive(Deserialize)]
        struct Case {
            palette: Palette,
            light: bool,
            options: Options,
            changes: Vec<Change>,
        }
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../fixtures/theme-windows-go.json")).unwrap();
        assert_eq!(cases.len(), 20);
        for case in cases {
            let actual = plan_with(&case.palette, case.light, &case.options, |key, name| {
                let old = case
                    .changes
                    .iter()
                    .find(|c| c.registry_key == key && c.registry_name == name)
                    .expect("fixture includes queried registry value");
                Ok(if old.existed {
                    Some(old.before.clone())
                } else {
                    None
                })
            })
            .unwrap();
            assert_eq!(actual, case.changes);
        }
    }
    fn palette() -> Palette {
        theme::readable(&vec!["#7392aa".into(); 16]).unwrap()
    }
    #[test]
    fn options_and_restore_allowlist() {
        let options = Options {
            mode: true,
            accent: true,
            titlebars: Some(true),
            taskbar: Some(false),
            transparency: Some(true),
        };
        for light in [false, true] {
            let changes = plan_with(&palette(), light, &options, |_, _| Ok(None)).unwrap();
            assert_eq!(changes.len(), 12);
            assert!(
                changes
                    .iter()
                    .all(|c| valid(&c.registry_key, &c.registry_name))
            );
        }
        assert!(
            plan_with(
                &palette(),
                true,
                &Options {
                    mode: true,
                    taskbar: Some(true),
                    ..Default::default()
                },
                |_, _| Ok(None)
            )
            .is_err()
        );
        assert!(plan_with(&palette(), false, &Options::default(), |_, _| Ok(None)).is_err());
        assert!(!valid(PERSONALIZE, "Unrelated"));
    }
    #[test]
    fn taskbar_requires_known_dark_mode_unless_mode_is_selected() {
        let options = Options {
            taskbar: Some(true),
            ..Default::default()
        };
        for existing in [
            None,
            Some(data(REG_DWORD, &1u32.to_le_bytes())),
            Some(data(REG_BINARY, &0u32.to_le_bytes())),
        ] {
            assert!(plan_with(&palette(), false, &options, |_, _| Ok(existing.clone())).is_err());
        }
        for existing in [
            data(REG_DWORD, &0u32.to_le_bytes()),
            data(REG_QWORD, &0u64.to_le_bytes()),
        ] {
            let changes = plan_with(&palette(), false, &options, |_, _| {
                Ok(Some(existing.clone()))
            })
            .unwrap();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].registry_name, "ColorPrevalence");
        }
    }
    #[test]
    fn opacity_and_eighth_palette_color_are_preserved() {
        let old_palette = data(REG_BINARY, &[7; 32]);
        let changes = plan_with(
            &palette(),
            false,
            &Options {
                accent: true,
                ..Default::default()
            },
            |_, name| {
                Ok(match name {
                    "ColorizationColor" => Some(data(REG_DWORD, &0xab123456u32.to_le_bytes())),
                    "AccentPalette" => Some(old_palette.clone()),
                    _ => None,
                })
            },
        )
        .unwrap();
        let color = changes
            .iter()
            .find(|c| c.registry_name == "ColorizationColor")
            .unwrap();
        assert_eq!(color.after[7], 0xab);
        let palette = changes
            .iter()
            .find(|c| c.registry_name == "AccentPalette")
            .unwrap();
        assert_eq!(&palette.after[32..], &[7; 4]);
    }
    #[test]
    fn native_registry_roundtrip_in_isolated_key() {
        let temp = tempfile::tempdir().unwrap();
        let key = format!(
            "Software\\GlazeBar\\RustTests\\{}",
            temp.path().file_name().unwrap().to_string_lossy()
        );
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                unsafe {
                    RegDeleteKeyW(HKEY_CURRENT_USER, wide(&self.0).as_ptr());
                }
            }
        }
        let _cleanup = Cleanup(key.clone());
        assert_eq!(read_raw(&key, "Value").unwrap(), None);
        for bytes in [
            data(REG_BINARY, &[0, 1, 255, 0]),
            data(REG_DWORD, &[1, 2, 3, 255]),
            data(REG_SZ, &[b'a', 0, 0, 0]),
        ] {
            write_raw(&key, "Value", Some(&bytes)).unwrap();
            assert_eq!(read_raw(&key, "Value").unwrap(), Some(bytes));
        }
        write_raw(&key, "Value", None).unwrap();
        assert_eq!(read_raw(&key, "Value").unwrap(), None);
    }
}
