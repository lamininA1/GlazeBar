use anyhow::{Result, ensure};
use std::{collections::HashMap, path::Path, ptr::null_mut};
use windows_sys::{
    Win32::{
        Foundation::LocalFree,
        System::Power::{
            ACCESS_SCHEME, PowerEnumerate, PowerGetActiveScheme, PowerSetActiveScheme,
        },
    },
    core::GUID,
};
#[derive(Clone)]
pub struct Profiles {
    pub ids: [String; 3],
    pub active: String,
    pub error: String,
}
pub fn normalize(s: &str) -> String {
    s.trim_matches(['{', '}']).to_ascii_lowercase()
}
pub fn parse_guid(s: &str) -> Result<GUID> {
    let text = normalize(s).replace('-', "");
    ensure!(text.len() == 32, "Invalid power profile GUID");
    Ok(GUID::from_u128(u128::from_str_radix(&text, 16)?))
}
pub fn active() -> Result<String> {
    unsafe {
        let mut guid = null_mut();
        let code = PowerGetActiveScheme(null_mut(), &mut guid);
        ensure!(
            code == 0 && !guid.is_null(),
            "Power scheme query failed: {code}"
        );
        let g = *guid;
        LocalFree(guid.cast());
        Ok(format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            g.data1,
            g.data2,
            g.data3,
            g.data4[0],
            g.data4[1],
            g.data4[2],
            g.data4[3],
            g.data4[4],
            g.data4[5],
            g.data4[6],
            g.data4[7]
        ))
    }
}
pub fn read() -> Result<Profiles> {
    let home =
        std::env::var_os("USERPROFILE").ok_or_else(|| anyhow::anyhow!("USERPROFILE missing"))?;
    let bytes = match std::fs::read(Path::new(&home).join("PowerProfiles/profiles.json")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return defaults();
        }
        Err(e) => return Err(e.into()),
    };
    let data: HashMap<String, String> =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))?;
    let ids = ["performance", "balance", "save"]
        .map(|key| data.get(key).map(|v| normalize(v)).unwrap_or_default());
    for id in &ids {
        parse_guid(id)?;
    }
    Ok(Profiles {
        ids,
        active: active()?,
        error: String::new(),
    })
}
fn defaults() -> Result<Profiles> {
    let mut ids = [
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".to_owned(),
        "381b4222-f694-41f0-9685-ff5bb260df2e".to_owned(),
        "a1841308-3541-4fab-bc81-f71556f20b4a".to_owned(),
    ];
    let mut available = Vec::new();
    for index in 0..256 {
        let mut guid = GUID::default();
        let mut size = std::mem::size_of::<GUID>() as u32;
        let code = unsafe {
            PowerEnumerate(
                null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                ACCESS_SCHEME,
                index,
                (&mut guid as *mut GUID).cast(),
                &mut size,
            )
        };
        if code == 259 {
            break;
        }
        ensure!(code == 0, "Power scheme enumeration failed: {code}");
        available.push(guid);
    }
    for id in &mut ids {
        let wanted = parse_guid(id)?;
        if !available.iter().any(|g| {
            g.data1 == wanted.data1
                && g.data2 == wanted.data2
                && g.data3 == wanted.data3
                && g.data4 == wanted.data4
        }) {
            id.clear();
        }
    }
    Ok(Profiles {
        ids,
        active: active()?,
        error: String::new(),
    })
}
pub fn apply(id: &str) -> Result<()> {
    let guid = parse_guid(id)?;
    let code = unsafe { PowerSetActiveScheme(null_mut(), &guid) };
    ensure!(code == 0, "Power scheme update failed: {code}");
    ensure!(
        active()? == normalize(id),
        "Power scheme change could not be confirmed"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_profiles_need_no_personal_configuration() {
        let profiles = defaults().unwrap();
        parse_guid(&profiles.active).unwrap();
        for id in profiles.ids.iter().filter(|id| !id.is_empty()) {
            parse_guid(id).unwrap();
        }
    }
    #[test]
    fn profile_guid() {
        let g = parse_guid("{381b4222-f694-41f0-9685-ff5bb260df2e}").unwrap();
        assert_eq!(g.data1, 0x381b4222);
        assert_eq!(g.data4, [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e]);
        assert!(parse_guid("invalid").is_err());
    }
}
