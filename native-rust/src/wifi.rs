use crate::graphics::wide;
use anyhow::{Result, ensure};
use std::{
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use windows_sys::{
    Win32::{Foundation::HANDLE, NetworkManagement::WiFi::*},
    core::GUID,
};
#[derive(Clone)]
pub struct Network {
    pub adapter: GUID,
    pub ssid: Vec<u8>,
    pub name: String,
    pub profile: String,
    pub signal: u32,
    pub auth: u32,
    pub cipher: u32,
    pub connected: bool,
    pub secure: bool,
    pub connectable: bool,
}
struct Client(HANDLE);
impl Client {
    fn new() -> Result<Self> {
        unsafe {
            let mut h = null_mut();
            let mut version = 0;
            let code = WlanOpenHandle(2, null(), &mut version, &mut h);
            ensure!(code == 0, "Wi-Fi 오류 {code}");
            Ok(Self(h))
        }
    }
    fn adapters(&self) -> Result<Vec<GUID>> {
        unsafe {
            let mut list = null_mut();
            let code = WlanEnumInterfaces(self.0, null(), &mut list);
            ensure!(code == 0, "Wi-Fi 어댑터 조회 오류 {code}");
            let count = (*list).dwNumberOfItems.min(64) as usize;
            let out = std::slice::from_raw_parts((*list).InterfaceInfo.as_ptr(), count)
                .iter()
                .map(|v| v.InterfaceGuid)
                .collect();
            WlanFreeMemory(list.cast());
            Ok(out)
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        unsafe {
            WlanCloseHandle(self.0, null());
        }
    }
}
pub fn scan(trigger: bool) -> Result<Vec<Network>> {
    let client = Client::new()?;
    let mut out = vec![];
    for adapter in client.adapters()? {
        unsafe {
            if trigger {
                let code = WlanScan(client.0, &adapter, null(), null(), null());
                ensure!(code == 0, "Wi-Fi 검색 오류 {code}");
            }
            let mut list = null_mut();
            let code = WlanGetAvailableNetworkList(client.0, &adapter, 0, null(), &mut list);
            ensure!(code == 0, "Wi-Fi 목록 오류 {code}");
            for row in std::slice::from_raw_parts(
                (*list).Network.as_ptr(),
                (*list).dwNumberOfItems.min(4096) as usize,
            ) {
                let len = row.dot11Ssid.uSSIDLength as usize;
                if !(1..=32).contains(&len) {
                    continue;
                }
                let ssid = row.dot11Ssid.ucSSID[..len].to_vec();
                let end = row
                    .strProfileName
                    .iter()
                    .position(|&v| v == 0)
                    .unwrap_or(256);
                let n = Network {
                    adapter,
                    name: String::from_utf8_lossy(&ssid).into_owned(),
                    ssid,
                    profile: String::from_utf16_lossy(&row.strProfileName[..end]),
                    signal: row.wlanSignalQuality,
                    auth: row.dot11DefaultAuthAlgorithm as u32,
                    cipher: row.dot11DefaultCipherAlgorithm as u32,
                    connected: row.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0,
                    secure: row.bSecurityEnabled != 0,
                    connectable: row.bNetworkConnectable != 0,
                };
                out.push(n);
            }
            WlanFreeMemory(list.cast());
        }
    }
    out.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| (!b.profile.is_empty()).cmp(&(!a.profile.is_empty())))
            .then_with(|| b.signal.cmp(&a.signal))
    });
    let mut seen = std::collections::HashSet::new();
    out.retain(|n| seen.insert((guid_key(n.adapter), n.ssid.clone(), n.auth)));
    Ok(out)
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn profile(n: &Network, password: &str) -> Result<(String, String)> {
    if !n.profile.is_empty() {
        return Ok((n.profile.clone(), String::new()));
    }
    ensure!(
        !n.secure || matches!(n.auth, 4 | 7 | 9),
        "이 네트워크는 Windows Wi-Fi 설정에서 연결해 주세요"
    );
    let hex = n
        .ssid
        .iter()
        .map(|v| format!("{v:02X}"))
        .collect::<String>();
    let name = format!("GlazeBar-{hex}-{}", n.auth);
    let (mut auth, mut cipher, mut key) = ("open", "none", String::new());
    if n.secure {
        auth = match n.auth {
            4 => "WPAPSK",
            7 => "WPA2PSK",
            _ => "WPA3SAE",
        };
        cipher = if n.cipher == 2 { "TKIP" } else { "AES" };
        let typ = if password.len() == 64
            && n.auth != 9
            && password.bytes().all(|c| c.is_ascii_hexdigit())
        {
            "networkKey"
        } else {
            ensure!(
                (8..=63).contains(&password.len()),
                "비밀번호는 8–63자여야 합니다"
            );
            ensure!(
                password.bytes().all(|v| (32..=126).contains(&v)),
                "비밀번호는 ASCII 문자만 사용할 수 있습니다"
            );
            "passPhrase"
        };
        key = format!(
            "<sharedKey><keyType>{typ}</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
            escape(password)
        );
    }
    Ok((
        name.clone(),
        format!(
            "<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\"><name>{name}</name><SSIDConfig><SSID><hex>{hex}</hex></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>manual</connectionMode><MSM><security><authEncryption><authentication>{auth}</authentication><encryption>{cipher}</encryption><useOneX>false</useOneX></authEncryption>{key}</security></MSM></WLANProfile>"
        ),
    ))
}
pub fn connect(n: &Network, password: &str, cancel: &AtomicBool) -> Result<()> {
    let (name, xml) = profile(n, password)?;
    let client = Client::new()?;
    unsafe {
        if !xml.is_empty() {
            let mut reason = 0;
            let code = WlanSetProfile(
                client.0,
                &n.adapter,
                WLAN_PROFILE_USER,
                wide(&xml).as_ptr(),
                null(),
                1,
                null(),
                &mut reason,
            );
            ensure!(code == 0, "Wi-Fi 프로필 오류 {code} ({reason})");
        }
        let name = wide(&name);
        let params = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: name.as_ptr(),
            dot11BssType: dot11_BSS_type_infrastructure,
            ..Default::default()
        };
        let code = WlanConnect(client.0, &n.adapter, &params, null());
        ensure!(code == 0, "Wi-Fi 연결 오류 {code}");
    }
    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "연결 확인이 취소되었습니다"
        );
        if scan(false)?
            .iter()
            .any(|v| guid_key(v.adapter) == guid_key(n.adapter) && v.ssid == n.ssid && v.connected)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    anyhow::bail!("연결을 확인하지 못했습니다. 비밀번호와 신호를 확인해 주세요")
}
#[cfg(test)]
mod tests {
    use super::*;
    fn network() -> Network {
        Network {
            adapter: GUID::from_u128(0),
            ssid: b"a<&>".to_vec(),
            name: "a<&>".into(),
            profile: String::new(),
            signal: 75,
            auth: 7,
            cipher: 4,
            connected: false,
            secure: true,
            connectable: true,
        }
    }
    #[test]
    fn escaping() {
        let (_, xml) = profile(&network(), "abcd<&>123").unwrap();
        assert!(xml.contains("abcd&lt;&amp;&gt;123"));
        assert!(xml.contains("<hex>613C263E</hex>"));
    }
    #[test]
    fn password_validation() {
        assert!(profile(&network(), "short").is_err());
        assert!(
            profile(&network(), &"A".repeat(64))
                .unwrap()
                .1
                .contains("networkKey")
        );
        let mut n = network();
        n.auth = 9;
        assert!(profile(&n, &"A".repeat(64)).is_err());
        n.auth = 6;
        assert!(profile(&n, "abcdefgh").is_err());
        n.profile = "saved".into();
        assert_eq!(profile(&n, "").unwrap(), ("saved".into(), String::new()));
    }
}

fn guid_key(g: GUID) -> (u32, u16, u16, [u8; 8]) {
    (g.data1, g.data2, g.data3, g.data4)
}
