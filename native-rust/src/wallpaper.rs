use std::path::{Path, PathBuf};
use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETDESKWALLPAPER, SystemParametersInfoW};

pub fn current() -> anyhow::Result<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    if unsafe {
        SystemParametersInfoW(
            SPI_GETDESKWALLPAPER,
            buffer.len() as u32,
            buffer.as_mut_ptr().cast(),
            0,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    use std::os::windows::ffi::OsStringExt;
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Ok(std::ffi::OsString::from_wide(&buffer[..end]).into())
}

pub fn folder(root: &Path, home: &Path) -> PathBuf {
    std::fs::read_to_string(root.join("wallpaper-folder.txt"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| PathBuf::from(s.trim()))
        .unwrap_or_else(|| home.join("Pictures").join("Wallpapers"))
}
pub fn set(path: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_SETDESKWALLPAPER, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE,
    };
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            path.as_ptr() as *mut _,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        matches!(
            s.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "bmp"
        )
    })
}
pub fn choose_folder(
    owner: windows_sys::Win32::Foundation::HWND,
) -> anyhow::Result<Option<PathBuf>> {
    browse_folder(owner, None, 0)
}
pub fn validate_folder_cancel(owner: windows_sys::Win32::Foundation::HWND) -> anyhow::Result<()> {
    unsafe extern "system" fn cancel(
        hwnd: windows_sys::Win32::Foundation::HWND,
        message: u32,
        _: isize,
        data: isize,
    ) -> i32 {
        if message == windows_sys::Win32::UI::Shell::BFFM_INITIALIZED {
            unsafe {
                *(data as *mut bool) = true;
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    hwnd,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_COMMAND,
                    2,
                    0,
                );
            }
        }
        0
    }
    let mut initialized = false;
    let result = browse_folder(owner, Some(cancel), &mut initialized as *mut bool as isize)?;
    anyhow::ensure!(
        initialized && result.is_none(),
        "Native folder dialog cancel failed"
    );
    Ok(())
}
fn browse_folder(
    owner: windows_sys::Win32::Foundation::HWND,
    callback: windows_sys::Win32::UI::Shell::BFFCALLBACK,
    param: isize,
) -> anyhow::Result<Option<PathBuf>> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::*;
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let title = crate::graphics::wide("배경화면 사진 폴더");
        let mut display = vec![0u16; 32768];
        let info = BROWSEINFOW {
            hwndOwner: owner,
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: callback,
            lParam: param,
            ..Default::default()
        };
        let pidl = SHBrowseForFolderW(&info);
        if pidl.is_null() {
            return Ok(None);
        }
        let mut path = vec![0u16; 32768];
        let ok = SHGetPathFromIDListEx(pidl, path.as_mut_ptr(), path.len() as u32, 0);
        CoTaskMemFree(Some(pidl.cast()));
        anyhow::ensure!(ok != 0, "선택한 폴더 경로를 읽을 수 없습니다");
        let end = path.iter().position(|&c| c == 0).unwrap_or(path.len());
        Ok(Some(std::ffi::OsString::from_wide(&path[..end]).into()))
    }
}

fn collect(directory: &Path, recursive: bool, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        // Avoid following junctions/symlinks while walking the fallback tree.
        if kind.is_dir() && recursive && !kind.is_symlink() {
            collect(&path, true, files);
        } else if kind.is_file() && supported(&path) {
            files.push(path);
        }
    }
}

pub fn files(directory: &Path, fallback: &Path, current: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(directory, false, &mut files);
    if files.is_empty() {
        collect(fallback, true, &mut files);
    }
    let key = |p: &Path| {
        p.components()
            .collect::<PathBuf>()
            .to_string_lossy()
            .to_lowercase()
    };
    files.sort_by_cached_key(|p| key(p));
    if current.is_file() && !files.iter().any(|p| key(p) == key(current)) {
        files.insert(0, current.to_owned());
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_selection_and_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("local");
        let fallback = temp.path().join("system");
        std::fs::create_dir_all(fallback.join("nested")).unwrap();
        std::fs::create_dir_all(local.join("ignored")).unwrap();
        let system = fallback.join("nested/system.jpg");
        std::fs::write(&system, []).unwrap();
        std::fs::write(local.join("ignored/hidden.png"), []).unwrap();
        std::fs::write(local.join("unsupported.txt"), []).unwrap();
        let current = temp.path().join("TranscodedWallpaper");
        std::fs::write(&current, []).unwrap();
        assert_eq!(
            files(&local, &fallback, &current),
            vec![current.clone(), system]
        );
        let a = local.join("a.PNG");
        let b = local.join("B.jpg");
        std::fs::write(&b, []).unwrap();
        std::fs::write(&a, []).unwrap();
        assert_eq!(files(&local, &fallback, &a), vec![a.clone(), b.clone()]);
        let mixed = PathBuf::from(a.to_string_lossy().replace('\\', "/"));
        assert_eq!(files(&local, &fallback, &mixed), vec![a.clone(), b.clone()]);
        assert_eq!(files(&local, &fallback, &current), vec![current, a, b]);
    }
    #[test]
    fn folder_preferences() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(
            folder(temp.path(), temp.path()),
            temp.path().join("Pictures/Wallpapers")
        );
        std::fs::write(
            temp.path().join("wallpaper-folder.txt"),
            "  C:\\Photos\\사진\r\n",
        )
        .unwrap();
        assert_eq!(
            folder(temp.path(), temp.path()),
            PathBuf::from("C:\\Photos\\사진")
        );
    }
}
