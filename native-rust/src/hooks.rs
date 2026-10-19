//! UI-thread hooks only enqueue work. No IPC or waiting inside callbacks.
use anyhow::{Result, ensure};
use std::{
    cell::RefCell,
    collections::HashSet,
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
};
use windows_sys::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::*},
    UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

#[derive(Default, Clone, Copy)]
struct Modifiers {
    win: bool,
    alt: bool,
    ctrl: bool,
    shift: bool,
}
#[derive(Default)]
struct Keyboard {
    swallowed: HashSet<u32>,
    alt_tab: bool,
}
#[derive(Default, Debug, PartialEq)]
struct Decision {
    swallow: bool,
    command: Option<&'static str>,
    neutralize: bool,
}
impl Keyboard {
    fn key(&mut self, vk: u32, down: bool, m: Modifiers, remote: bool) -> Decision {
        if !down && self.swallowed.remove(&vk) {
            return Decision {
                swallow: true,
                ..Default::default()
            };
        }
        let mut command = None;
        if remote {
            self.alt_tab = false;
        } else {
            if down && vk == 9 && m.alt && !m.ctrl {
                self.alt_tab = true;
            }
            if !down && matches!(vk, 0x12 | 0xa4 | 0xa5) && self.alt_tab {
                self.alt_tab = false;
                command = Some("FocusResync");
            }
        }
        let win_m = vk == 0x4d && m.win && !m.alt && !m.ctrl && !m.shift;
        let alt_m = !remote && vk == 0x4d && m.alt && !m.win && !m.ctrl && !m.shift;
        let sheet = !remote && vk == 0x70 && m.win && !m.alt && !m.ctrl && !m.shift;
        if down && (win_m || alt_m || sheet) {
            let first = self.swallowed.insert(vk);
            return Decision {
                swallow: true,
                neutralize: first,
                command: if first && alt_m {
                    Some("Toggle")
                } else if first && sheet {
                    Some("Sheet")
                } else {
                    None
                },
            };
        }
        Decision {
            command,
            ..Default::default()
        }
    }
}
pub enum Event {
    Control(&'static str),
    Foreground,
}
struct Context {
    tx: Sender<Event>,
    thread: u32,
    remote: Arc<AtomicBool>,
    keys: Keyboard,
    intercept: bool,
}
thread_local! { static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) }; }
impl Context {
    fn emit(&self, event: Event) {
        if self.tx.send(event).is_ok() {
            unsafe {
                PostThreadMessageW(self.thread, WM_APP + 2, 0, 0);
            }
        }
    }
}
unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(null_mut(), code, wp, lp) };
    }
    let decision = CONTEXT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(c) = slot.as_mut().filter(|c| c.intercept) else {
            return Decision::default();
        };
        let down = wp == WM_KEYDOWN as usize || wp == WM_SYSKEYDOWN as usize;
        let pressed = |key| unsafe { GetAsyncKeyState(key) < 0 };
        let m = Modifiers {
            win: pressed(0x5b) || pressed(0x5c),
            alt: pressed(0x12),
            ctrl: pressed(0x11),
            shift: pressed(0x10),
        };
        let d = c.keys.key(
            unsafe { (*(lp as *const KBDLLHOOKSTRUCT)).vkCode },
            down,
            m,
            c.remote.load(Ordering::Relaxed),
        );
        if let Some(cmd) = d.command {
            c.emit(Event::Control(cmd));
        }
        d
    });
    // Release the RefCell borrow before injecting, since hooks can reenter.
    if decision.neutralize {
        unsafe {
            keybd_event(0xe8, 0, 0, 0);
            keybd_event(0xe8, 0, KEYEVENTF_KEYUP, 0);
        }
    }
    if decision.swallow {
        1
    } else {
        unsafe { CallNextHookEx(null_mut(), code, wp, lp) }
    }
}
unsafe extern "system" fn foreground(
    _: HWINEVENTHOOK,
    _: u32,
    hwnd: HWND,
    _: i32,
    _: i32,
    _: u32,
    _: u32,
) {
    CONTEXT.with(|slot| {
        if let Some(c) = slot.borrow().as_ref() {
            c.remote.store(is_remote(hwnd), Ordering::Relaxed);
            c.emit(Event::Foreground);
        }
    });
}
fn remote_name(path: &str) -> bool {
    matches!(
        path.rsplit(['\\', '/'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "mstsc.exe" | "msrdc.exe" | "rdclientwpf.exe"
    )
}
fn is_remote(hwnd: HWND) -> bool {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return false;
        }
        let mut buf = [0u16; 32768];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        ok != 0 && remote_name(&String::from_utf16_lossy(&buf[..len as usize]))
    }
}
pub struct Hooks {
    keyboard: HHOOK,
    foreground: HWINEVENTHOOK,
    hotkeys: bool,
}
impl Hooks {
    pub fn install(tx: Sender<Event>, remote: Arc<AtomicBool>, intercept: bool) -> Result<Self> {
        ensure!(
            CONTEXT.with(|c| c.borrow().is_none()),
            "Hooks already installed on this thread"
        );
        unsafe {
            remote.store(is_remote(GetForegroundWindow()), Ordering::Relaxed);
            CONTEXT.with(|slot| {
                *slot.borrow_mut() = Some(Context {
                    tx,
                    thread: GetCurrentThreadId(),
                    remote,
                    keys: Keyboard::default(),
                    intercept,
                })
            });
            let mut hooks = Self {
                keyboard: null_mut(),
                foreground: null_mut(),
                hotkeys: intercept,
            };
            hooks.keyboard = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard),
                GetModuleHandleW(std::ptr::null()),
                0,
            );
            ensure!(
                !hooks.keyboard.is_null(),
                "Cannot install keyboard hook: {}",
                GetLastError()
            );
            hooks.foreground = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                null_mut(),
                Some(foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            ensure!(
                !hooks.foreground.is_null(),
                "Cannot install foreground hook: {}",
                GetLastError()
            );
            if intercept {
                ensure!(
                    RegisterHotKey(
                        null_mut(),
                        2,
                        MOD_NOREPEAT | MOD_WIN | MOD_ALT | MOD_SHIFT,
                        0x42
                    ) != 0,
                    "Cannot register Win+Alt+Shift+B bar hotkey: {}",
                    GetLastError()
                );
                ensure!(
                    RegisterHotKey(
                        null_mut(),
                        3,
                        MOD_NOREPEAT | MOD_WIN | MOD_CONTROL | MOD_ALT,
                        0x20
                    ) != 0,
                    "Cannot register wallpaper hotkey"
                );
            }
            Ok(hooks)
        }
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            if !self.keyboard.is_null() {
                UnhookWindowsHookEx(self.keyboard);
            }
            if !self.foreground.is_null() {
                UnhookWinEvent(self.foreground);
            }
            if self.hotkeys {
                UnregisterHotKey(null_mut(), 2);
                UnregisterHotKey(null_mut(), 3);
            }
            CONTEXT.with(|slot| *slot.borrow_mut() = None);
        }
    }
}
pub fn pause_transition(remote: bool, paused: bool, owned: bool) -> (bool, bool) {
    if remote && !paused {
        (true, true)
    } else if !remote && owned {
        (paused, false)
    } else {
        (false, owned)
    }
}
pub fn remote_covers(bounds: crate::graphics::Rect) -> bool {
    if !CONTEXT.with(|c| {
        c.borrow()
            .as_ref()
            .is_some_and(|c| c.remote.load(Ordering::Relaxed))
    }) {
        return false;
    }
    unsafe {
        let mut r = RECT::default();
        GetWindowRect(GetForegroundWindow(), &mut r) != 0
            && r.left <= bounds.x + 8
            && r.right >= bounds.x + bounds.w - 8
            && r.top <= bounds.y + 8
            && r.bottom >= bounds.y + bounds.h
    }
}
pub fn validate() -> Result<()> {
    let (tx, _rx) = std::sync::mpsc::channel();
    drop(Hooks::install(tx, Arc::new(AtomicBool::new(false)), false)?);
    ensure!(
        CONTEXT.with(|c| c.borrow().is_none()),
        "Hook context leaked"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_keys_only_act_once_and_release_is_swallowed() {
        let mut k = Keyboard::default();
        let m = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert_eq!(
            k.key(0x4d, true, m, false),
            Decision {
                swallow: true,
                command: Some("Toggle"),
                neutralize: true
            }
        );
        assert_eq!(
            k.key(0x4d, true, m, false),
            Decision {
                swallow: true,
                ..Default::default()
            }
        );
        assert!(k.key(0x4d, false, Modifiers::default(), true).swallow);
        assert!(!k.key(0x4d, false, m, false).swallow);
    }
    #[test]
    fn remote_passes_local_shortcuts_but_preserves_win_m_rule() {
        let mut k = Keyboard::default();
        assert!(
            !k.key(
                0x4d,
                true,
                Modifiers {
                    alt: true,
                    ..Default::default()
                },
                true
            )
            .swallow
        );
        assert!(
            !k.key(
                0x70,
                true,
                Modifiers {
                    win: true,
                    ..Default::default()
                },
                true
            )
            .swallow
        );
        assert!(
            k.key(
                0x4d,
                true,
                Modifiers {
                    win: true,
                    ..Default::default()
                },
                true
            )
            .swallow
        );
    }
    #[test]
    fn alt_tab_resyncs_once_on_alt_release() {
        let mut k = Keyboard::default();
        k.key(
            9,
            true,
            Modifiers {
                alt: true,
                ..Default::default()
            },
            false,
        );
        assert_eq!(
            k.key(0xa4, false, Modifiers::default(), false).command,
            Some("FocusResync")
        );
        assert_eq!(
            k.key(0xa4, false, Modifiers::default(), false).command,
            None
        );
    }
    #[test]
    fn only_undo_pause_owned_by_us() {
        assert_eq!(pause_transition(true, false, false), (true, true));
        assert_eq!(pause_transition(true, true, false), (false, false));
        assert_eq!(pause_transition(false, true, false), (false, false));
        assert_eq!(pause_transition(false, true, true), (true, false));
        assert_eq!(pause_transition(false, false, true), (false, false));
    }
    #[test]
    fn remote_process_names_are_exact() {
        assert!(remote_name("C:\\Windows\\System32\\MSTSC.EXE"));
        assert!(!remote_name("C:\\mstsc.exe.backup"));
    }
}
