use serde::Serialize;
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::{
    Win32::{
        Devices::DeviceAndDriverInstallation::*,
        Foundation::*,
        NetworkManagement::WiFi::*,
        Storage::FileSystem::*,
        System::{IO::DeviceIoControl, Power::*, SystemInformation::*, Threading::GetSystemTimes},
    },
    core::GUID,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Sensors {
    pub cpu: f64,
    pub memory: f64,
    pub percent: f64,
    pub full: f64,
    pub remaining: f64,
    pub watts: f64,
    pub battery: bool,
    pub charging: bool,
    pub discharging: bool,
    pub known: bool,
    pub network: String,
    pub signal: u32,
    pub wifi_available: bool,
}
struct Device {
    handle: HANDLE,
    tag: u32,
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}
pub struct Reader {
    devices: Vec<Device>,
    last_times: Option<[u64; 3]>,
    last_discovery: std::time::Instant,
}
impl Reader {
    pub fn new() -> Self {
        Self {
            devices: vec![],
            last_times: None,
            last_discovery: std::time::Instant::now() - std::time::Duration::from_secs(10),
        }
    }
    fn discover(&mut self) {
        unsafe {
            self.devices.clear();
            self.last_discovery = std::time::Instant::now();
            let guid = GUID::from_u128(0x72631e54_78a4_11d0_bcf7_00aa00b7b32a);
            let set = SetupDiGetClassDevsW(
                &guid,
                null(),
                null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            );
            if set == -1 {
                return;
            }
            for i in 0..16 {
                let mut info = SP_DEVICE_INTERFACE_DATA {
                    cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                    ..Default::default()
                };
                if SetupDiEnumDeviceInterfaces(set, null(), &guid, i, &mut info) == 0 {
                    break;
                }
                let mut size = 0;
                SetupDiGetDeviceInterfaceDetailW(set, &info, null_mut(), 0, &mut size, null_mut());
                if !(8..=65536).contains(&size) {
                    continue;
                }
                // Pointer-aligned storage is required by SP_DEVICE_INTERFACE_DETAIL_DATA_W.
                let mut data = vec![0usize; (size as usize).div_ceil(size_of::<usize>())];
                let detail = data
                    .as_mut_ptr()
                    .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
                (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(set, &info, detail, size, &mut size, null_mut())
                    == 0
                {
                    continue;
                }
                let h = CreateFileW(
                    (*detail).DevicePath.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    null(),
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                );
                if h == INVALID_HANDLE_VALUE {
                    continue;
                }
                let mut device = Device { handle: h, tag: 0 };
                let mut tag = [0u8; 4];
                if !ioctl(h, 0x294040, &[0; 4], &mut tag) {
                    continue;
                }
                device.tag = u32::from_le_bytes(tag);
                if device.tag == 0 {
                    continue;
                }
                let mut input = [0u8; 12];
                input[..4].copy_from_slice(&tag);
                let mut info = [0u8; 36];
                if !ioctl(h, 0x294044, &input, &mut info) {
                    continue;
                }
                let caps = u32::from_le_bytes(info[..4].try_into().unwrap());
                if caps & 0x80000000 != 0 && caps & 0x20000000 == 0 {
                    self.devices.push(device)
                }
            }
            SetupDiDestroyDeviceInfoList(set);
        }
    }
    fn rate(&mut self) -> Option<(i32, u32)> {
        if self.devices.is_empty() && self.last_discovery.elapsed().as_secs() >= 5 {
            self.discover()
        }
        if self.devices.is_empty() {
            return None;
        }
        let mut rate = 0i64;
        let mut flags = 0;
        for d in &self.devices {
            let mut input = [0u8; 20];
            input[..4].copy_from_slice(&d.tag.to_le_bytes());
            let mut output = [0u8; 16];
            if !ioctl(d.handle, 0x29404c, &input, &mut output) {
                self.devices.clear();
                return None;
            }
            let r = i32::from_le_bytes(output[12..16].try_into().unwrap());
            if r == i32::MIN {
                return None;
            }
            rate += i64::from(r);
            flags |= u32::from_le_bytes(output[..4].try_into().unwrap());
        }
        Some((rate as i32, flags))
    }
    pub fn read(&mut self, s: &mut Sensors, slow: bool) {
        unsafe {
            let mut b = SYSTEM_BATTERY_STATE::default();
            if CallNtPowerInformation(
                SystemBatteryState,
                null(),
                0,
                (&mut b as *mut SYSTEM_BATTERY_STATE).cast(),
                size_of::<SYSTEM_BATTERY_STATE>() as u32,
            ) == 0
            {
                if let Some((rate, flags)) = self.rate() {
                    b.Rate = rate as u32;
                    b.Charging = flags & 4 != 0;
                    b.Discharging = flags & 2 != 0;
                }
                s.battery = b.BatteryPresent;
                s.charging = b.Charging;
                s.discharging = b.Discharging;
                s.known = b.Rate != 0x80000000 && b.Rate != u32::MAX;
                s.full = b.MaxCapacity as f64 / 1000.;
                s.remaining = b.RemainingCapacity as f64 / 1000.;
                s.percent = if b.MaxCapacity > 0 {
                    (b.RemainingCapacity as f64 * 100. / b.MaxCapacity as f64).min(100.)
                } else {
                    0.
                };
                s.watts = (b.Rate as i32 as f64).abs() / 1000.;
            }
            if slow {
                let (mut idle, mut kernel, mut user) = (
                    FILETIME::default(),
                    FILETIME::default(),
                    FILETIME::default(),
                );
                if GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0 {
                    let t = [idle, kernel, user]
                        .map(|v| u64::from(v.dwHighDateTime) << 32 | u64::from(v.dwLowDateTime));
                    if let Some(last) = self.last_times {
                        let total = t[1].saturating_sub(last[1]) + t[2].saturating_sub(last[2]);
                        if total > 0 {
                            s.cpu = (100.
                                * (1. - t[0].saturating_sub(last[0]) as f64 / total as f64))
                                .clamp(0., 100.)
                        }
                    }
                    self.last_times = Some(t);
                }
                let mut m = MEMORYSTATUSEX {
                    dwLength: size_of::<MEMORYSTATUSEX>() as u32,
                    ..Default::default()
                };
                if GlobalMemoryStatusEx(&mut m) != 0 {
                    s.memory = m.dwMemoryLoad as f64;
                }
                (s.wifi_available, s.network, s.signal) = connected_network();
            }
        }
    }
}
fn ioctl(h: HANDLE, code: u32, input: &[u8], output: &mut [u8]) -> bool {
    unsafe {
        let mut count = 0;
        DeviceIoControl(
            h,
            code,
            input.as_ptr().cast(),
            input.len() as u32,
            output.as_mut_ptr().cast(),
            output.len() as u32,
            &mut count,
            null_mut(),
        ) != 0
    }
}
pub fn connected_network() -> (bool, String, u32) {
    unsafe {
        let mut h = null_mut();
        let mut version = 0;
        if WlanOpenHandle(2, null(), &mut version, &mut h) != 0 {
            return Default::default();
        }
        let mut list = null_mut();
        let mut result: (bool, String, u32) = Default::default();
        if WlanEnumInterfaces(h, null(), &mut list) == 0 && !list.is_null() {
            // A disconnected (or radio-off) adapter still supports Wi-Fi.
            // Only missing interfaces or an unavailable WLAN service hide it.
            result.0 = (*list).dwNumberOfItems > 0;
            for interface in std::slice::from_raw_parts(
                (*list).InterfaceInfo.as_ptr(),
                (*list).dwNumberOfItems as usize,
            ) {
                if interface.isState != wlan_interface_state_connected {
                    continue;
                }
                let mut data = null_mut();
                let mut size = 0;
                let mut ty = 0;
                if WlanQueryInterface(
                    h,
                    &interface.InterfaceGuid,
                    wlan_intf_opcode_current_connection,
                    null(),
                    &mut size,
                    &mut data,
                    &mut ty,
                ) == 0
                {
                    if size as usize >= size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
                        let a = &(*(data as *const WLAN_CONNECTION_ATTRIBUTES))
                            .wlanAssociationAttributes;
                        result = (
                            true,
                            String::from_utf8_lossy(
                                &a.dot11Ssid.ucSSID[..(a.dot11Ssid.uSSIDLength as usize).min(32)],
                            )
                            .to_string(),
                            a.wlanSignalQuality,
                        );
                    }
                    WlanFreeMemory(data);
                }
                break;
            }
            WlanFreeMemory(list.cast());
        }
        WlanCloseHandle(h, null());
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi() {
        assert_eq!(size_of::<SYSTEM_BATTERY_STATE>(), 32);
        assert_eq!(size_of::<MEMORYSTATUSEX>(), 64);
    }
}
