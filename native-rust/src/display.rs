use anyhow::{Result, bail, ensure};
use std::{mem::size_of, ptr::null_mut};
use windows_sys::Win32::Devices::Display::*;
#[repr(C)]
struct GetScale {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    min: i32,
    current: i32,
    max: i32,
}
#[repr(C)]
struct SetScale {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: i32,
}
#[derive(Clone, Copy)]
pub struct Scale {
    pub source: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    pub recommended: usize,
    pub current: usize,
    pub max: usize,
}
pub fn read(device: &str) -> Result<Scale> {
    unsafe {
        for _ in 0..3 {
            let (mut paths, mut modes) = (0, 0);
            let code = GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes);
            ensure!(code == 0, "Display buffer query failed: {code}");
            let mut p = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
            let mut m = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
            let code = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut paths,
                p.as_mut_ptr(),
                &mut modes,
                m.as_mut_ptr(),
                null_mut(),
            );
            if code == 122 {
                continue;
            }
            ensure!(code == 0, "Display query failed: {code}");
            for path in p.iter().take(paths as usize) {
                let mut name = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                        size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                        adapterId: path.sourceInfo.adapterId,
                        id: path.sourceInfo.id,
                    },
                    ..Default::default()
                };
                if DisplayConfigGetDeviceInfo(&mut name.header) != 0 {
                    continue;
                }
                let name_text = String::from_utf16_lossy(
                    &name.viewGdiDeviceName[..name
                        .viewGdiDeviceName
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(32)],
                );
                if !name_text.eq_ignore_ascii_case(device) {
                    continue;
                }
                let mut packet = GetScale {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: -3,
                        size: size_of::<GetScale>() as u32,
                        ..name.header
                    },
                    min: 0,
                    current: 0,
                    max: 0,
                };
                let code = DisplayConfigGetDeviceInfo(&mut packet.header);
                ensure!(code == 0, "Scale query failed: {code}");
                let recommended = -packet.min;
                let current = recommended + packet.current;
                let max = recommended + packet.max;
                ensure!(
                    recommended >= 0
                        && current >= 0
                        && max >= current
                        && max < crate::views::SCALES.len() as i32
                        && recommended <= max,
                    "Unsupported scale range"
                );
                return Ok(Scale {
                    source: name.header,
                    recommended: recommended as usize,
                    current: current as usize,
                    max: max as usize,
                });
            }
        }
        bail!("Display not found: {device}")
    }
}
pub fn write(s: Scale, index: usize) -> Result<()> {
    ensure!(index <= s.max, "Unsupported scale");
    let packet = SetScale {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: -4,
            size: size_of::<SetScale>() as u32,
            ..s.source
        },
        value: index as i32 - s.recommended as i32,
    };
    let code = unsafe { DisplayConfigSetDeviceInfo(&packet.header) };
    ensure!(code == 0, "Scale update failed: {code}");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi() {
        assert_eq!(size_of::<GetScale>(), 32);
        assert_eq!(size_of::<SetScale>(), 24);
    }
}
