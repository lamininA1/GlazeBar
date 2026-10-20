use crate::{
    display,
    graphics::{Canvas, Palette, Rect, Resources, wide},
    ipc::Monitor,
    sensors::Sensors,
    views::{self, Action, Hit},
};
use std::{
    cell::RefCell,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    rc::Rc,
    sync::{Once, mpsc::Sender},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

#[derive(Clone)]
pub enum Scene {
    Bar {
        monitor: Monitor,
        sensors: Sensors,
        paused: bool,
        resize: bool,
        vertical: bool,
        clock: String,
        brightness: Option<u8>,
        weather: Option<(String, String)>,
    },
    Scale {
        state: display::Scale,
        selected: usize,
        error: String,
    },
    Battery(Sensors),
    Power(crate::power::Profiles),
    Wifi(crate::wifi_ui::Wifi),
    Sheet(crate::sheet::Sheet),
    Wallpaper(crate::wallpaper_ui::Picker),
    WindowsOptions(crate::windows_options_ui::Settings),
    ThemePreview(crate::theme_preview_ui::Preview),
    Compare(crate::style_compare_ui::Comparison),
    Precache(crate::precache::Progress),
    Online(crate::online_ui::Online),
}
pub enum NativeEvent {
    Dismiss(usize),
    Action(usize, Action),
    ScaleCommit(usize, display::Scale, usize),
    Closed(usize),
    DisplayChanged,
    AppBar(usize, usize, isize),
    HoverBattery(usize, bool),
    BrightnessSet(usize, u8),
    TaskbarCreated(usize),
}
pub struct State {
    pub root: PathBuf,
    pub scale: f64,
    pub palette: Palette,
    pub scene: Scene,
    pub resources: Resources,
    pub hits: Vec<Hit>,
    pub hover: Option<(i32, i32)>,
    pub drag: bool,
    pub passive: bool,
    pub events: Sender<NativeEvent>,
    pub wake: usize,
}
impl State {
    fn send(&self, event: NativeEvent) {
        if self.events.send(event).is_ok() && self.wake != 0 {
            unsafe {
                PostMessageW(self.wake as HWND, WM_APP + 1, 0, 0);
            }
        }
    }
    fn clickable(&self, x: i32, y: i32) -> bool {
        if matches!(&self.scene, Scene::Scale { .. }) {
            let p = |n: f64| (n * self.scale).round_ties_even() as i32;
            x >= p(10.) && x < p(250.) && y >= p(45.) && y <= p(78.)
        } else {
            self.hits
                .iter()
                .any(|h| h.action != Action::Battery && h.rect.contains(x, y))
        }
    }
    fn paint(&mut self, dc: HDC, width: i32, height: i32) {
        let mut c = Canvas {
            dc,
            scale: self.scale,
            width,
            height,
            ui: self.palette,
            resources: &mut self.resources,
            root: &self.root,
        };
        c.fill(Rect::new(0, 0, width, height), c.ui.bg);
        self.hits.clear();
        match &self.scene {
            Scene::Bar {
                monitor,
                sensors,
                paused,
                resize,
                vertical,
                clock,
                brightness,
                weather,
            } => {
                self.hits = views::bar(
                    &mut c,
                    monitor,
                    sensors,
                    *paused,
                    *resize,
                    *vertical,
                    clock,
                    *brightness,
                    weather.as_ref().map(|(i, t)| (i.as_str(), t.as_str())),
                )
            }
            Scene::Scale {
                state,
                selected,
                error,
            } => views::scale(&mut c, *selected, state.recommended, state.max, error),
            Scene::Battery(s) => views::battery(&mut c, s),
            Scene::Power(p) => self.hits = views::power(&mut c, p, self.hover),
            Scene::Wifi(v) => self.hits = crate::wifi_ui::paint(&mut c, v, self.hover),
            Scene::Sheet(v) => self.hits = crate::sheet::paint(&mut c, v, self.hover),
            Scene::ThemePreview(v) => crate::theme_preview_ui::paint(&mut c, v),
            Scene::Precache(v) => self.hits = crate::precache::paint(&mut c, v, self.hover),
            Scene::Online(v) => self.hits = crate::online_ui::paint(&mut c, v, self.hover),
            Scene::Compare(v) => self.hits = crate::style_compare_ui::paint(&mut c, v, self.hover),
            Scene::WindowsOptions(v) => {
                self.hits = crate::windows_options_ui::paint(&mut c, v, self.hover)
            }
            Scene::Wallpaper(v) => {
                self.hits = crate::wallpaper_ui::paint(&mut c, v, self.hover)
                    .into_iter()
                    .map(|h| Hit {
                        rect: h.rect,
                        action: Action::Picker(h.action),
                    })
                    .collect()
            }
        }
    }
}
pub struct Window {
    pub hwnd: HWND,
    pub state: Rc<RefCell<State>>,
}
impl Window {
    pub fn new(title: &str, bounds: Rect, state: State, parent: HWND) -> anyhow::Result<Self> {
        unsafe {
            static REGISTER: Once = Once::new();
            REGISTER.call_once(|| {
                let class_name = wide("GlazeBarRustNative");
                let cls = WNDCLASSEXW {
                    cbSize: size_of::<WNDCLASSEXW>() as u32,
                    lpfnWndProc: Some(wndproc),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                    lpszClassName: class_name.as_ptr(),
                    ..Default::default()
                };
                RegisterClassExW(&cls);
            });
            let passive = state.passive;
            let state = Rc::new(RefCell::new(state));
            let raw = Rc::into_raw(state.clone());
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST | if passive { WS_EX_NOACTIVATE } else { 0 },
                wide("GlazeBarRustNative").as_ptr(),
                wide(title).as_ptr(),
                WS_POPUP | WS_CLIPCHILDREN,
                bounds.x,
                bounds.y,
                bounds.w,
                bounds.h,
                parent,
                null_mut(),
                GetModuleHandleW(null()),
                raw.cast(),
            );
            if hwnd.is_null() {
                // WM_NCCREATE takes ownership only when creation gets that far.
                anyhow::bail!("CreateWindowExW failed: {}", GetLastError())
            }
            Ok(Self { hwnd, state })
        }
    }
    pub fn show(&self) {
        let passive = self.state.borrow().passive;
        unsafe {
            ShowWindow(self.hwnd, if passive { SW_SHOWNOACTIVATE } else { SW_SHOW });
            if !passive {
                let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
                let current_thread = windows_sys::Win32::System::Threading::GetCurrentThreadId();
                let attached = foreground_thread != 0
                    && foreground_thread != current_thread
                    && windows_sys::Win32::System::Threading::AttachThreadInput(
                        current_thread,
                        foreground_thread,
                        1,
                    ) != 0;
                SetForegroundWindow(self.hwnd);
                SetFocus(self.hwnd);
                if attached {
                    windows_sys::Win32::System::Threading::AttachThreadInput(
                        current_thread,
                        foreground_thread,
                        0,
                    );
                }
            }
            InvalidateRect(self.hwnd, null(), 0);
        }
    }
    pub fn invalidate(&self) {
        unsafe {
            InvalidateRect(self.hwnd, null(), 0);
        }
    }
    pub fn owns_focus(&self, target: HWND) -> bool {
        unsafe {
            // Include this popup's controls and owned dialogs, but not its
            // owner (the bar) or sibling popups owned by the same bar.
            let mut window = GetAncestor(target, GA_ROOT);
            while !window.is_null() {
                if window == self.hwnd {
                    return true;
                }
                window = GetWindow(window, GW_OWNER);
            }
            false
        }
    }
    pub fn bounds(&self, r: Rect) {
        unsafe {
            SetWindowPos(self.hwnd, HWND_TOPMOST, r.x, r.y, r.w, r.h, SWP_NOACTIVATE);
        }
    }
    pub fn rect(&self) -> Rect {
        unsafe {
            let mut r = RECT::default();
            GetWindowRect(self.hwnd, &mut r);
            Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
        }
    }
    pub fn alive(&self) -> bool {
        unsafe { IsWindow(self.hwnd) != 0 }
    }
    pub fn snapshot(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let (root, scale, palette) = {
            let s = self.state.borrow();
            (s.root.clone(), s.scale, s.palette)
        };
        let r = self.rect();
        crate::graphics::render_png(&root, path, r.w, r.h, scale, palette, |c| {
            self.state.borrow_mut().paint(c.dc, r.w, r.h)
        })
    }
    pub fn wifi_layout(&self, clear: bool) {
        let (scale, v, font) = {
            let mut s = self.state.borrow_mut();
            let Scene::Wifi(v) = &s.scene else { return };
            let v = v.clone();
            let scale = s.scale;
            let font = s
                .resources
                .font((13. * scale).round_ties_even() as i32, false);
            (scale, v, font)
        };
        let p = |n: f64| (n * scale).round_ties_even() as i32;
        let mut bounds = self.rect();
        bounds.h = p(v.height());
        self.bounds(bounds);
        unsafe {
            let mut edit = GetDlgItem(self.hwnd, 100);
            if edit.is_null() {
                edit = CreateWindowExW(
                    WS_EX_CLIENTEDGE,
                    wide("EDIT").as_ptr(),
                    wide("").as_ptr(),
                    WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_PASSWORD as u32,
                    p(20.),
                    p(v.input_y()),
                    bounds.w - p(130.),
                    p(27.),
                    self.hwnd,
                    100usize as HMENU,
                    GetModuleHandleW(null()),
                    null(),
                );
                SendMessageW(edit, WM_SETFONT, font as usize, 0);
                SendMessageW(edit, 0x00c5 /* EM_SETLIMITTEXT */, 64, 0);
            }
            SetWindowPos(
                edit,
                null_mut(),
                p(20.),
                p(v.input_y()),
                bounds.w - p(130.),
                p(27.),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if clear {
                SetWindowTextW(edit, wide("").as_ptr());
            }
            EnableWindow(edit, (!v.connecting) as i32);
            ShowWindow(
                edit,
                if v.needs_password() {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_HIDE
                },
            );
        }
        self.invalidate();
    }
    pub fn wifi_secret(&self) -> String {
        self.edit_text(100)
    }
    pub fn online_layout(&self) {
        let mut state = self.state.borrow_mut();
        let scale = state.scale;
        let p = |n: f64| (n * scale).round_ties_even() as i32;
        let font = state.resources.font(p(12.), false);
        unsafe {
            let edit = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32,
                p(145.),
                p(18.),
                self.rect().w - p(368.),
                p(30.),
                self.hwnd,
                101usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(edit, WM_SETFONT, font as usize, 1);
            SendMessageW(edit, 0x00c5, 2048, 0);
            let page = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                wide("1").as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32 | ES_NUMBER as u32,
                p(128.),
                self.rect().h - p(97.),
                p(112.),
                p(30.),
                self.hwnd,
                102usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            SendMessageW(page, WM_SETFONT, font as usize, 1);
            SendMessageW(page, 0x00c5, 10, 0);
        }
    }
    pub fn set_edit_text(&self, id: i32, text: &str) {
        unsafe {
            SetWindowTextW(GetDlgItem(self.hwnd, id), wide(text).as_ptr());
        }
    }
    pub fn edit_text(&self, id: i32) -> String {
        unsafe {
            let edit = GetDlgItem(self.hwnd, id);
            let len = GetWindowTextLengthW(edit);
            let mut text = vec![0u16; len as usize + 1];
            GetWindowTextW(edit, text.as_mut_ptr(), text.len() as i32);
            String::from_utf16_lossy(&text[..len as usize])
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
unsafe extern "system" fn wndproc(h: HWND, m: u32, wp: usize, lp: isize) -> isize {
    unsafe {
        if m == WM_NCCREATE {
            let create = &*(lp as *const CREATESTRUCTW);
            SetWindowLongPtrW(h, GWLP_USERDATA, create.lpCreateParams as isize);
            // Default creation initializes the system-visible caption used by
            // accessibility tools, window discovery and window-manager rules.
            return DefWindowProcW(h, m, wp, lp);
        }
        let raw = GetWindowLongPtrW(h, GWLP_USERDATA) as *const RefCell<State>;
        if raw.is_null() {
            return DefWindowProcW(h, m, wp, lp);
        }
        if m == WM_NCDESTROY {
            SetWindowLongPtrW(h, GWLP_USERDATA, 0);
            let state = Rc::from_raw(raw);
            if let Ok(s) = state.try_borrow() {
                s.send(NativeEvent::Closed(h as usize));
            }
            return DefWindowProcW(h, m, wp, lp);
        }
        // Every callback owns a temporary strong reference, including reentrant callbacks.
        Rc::increment_strong_count(raw);
        let cell = Rc::from_raw(raw);
        let Ok(mut s) = cell.try_borrow_mut() else {
            return DefWindowProcW(h, m, wp, lp);
        };
        match m {
            WM_COMMAND if matches!(s.scene, Scene::Online(_)) => {
                if (wp & 0xffff) == 101 && (wp >> 16) as u32 == EN_CHANGE {
                    s.send(NativeEvent::Action(
                        h as usize,
                        Action::Online(crate::online_ui::Edit::QueryChanged),
                    ));
                }
                return 0;
            }
            WM_NCHITTEST if matches!(s.scene, Scene::Sheet(_)) => {
                let mut pos = POINT {
                    x: (lp as u16) as i16 as i32,
                    y: ((lp >> 16) as u16) as i16 as i32,
                };
                ScreenToClient(h, &mut pos);
                let mut r = RECT::default();
                GetClientRect(h, &mut r);
                if pos.y < (94. * s.scale).round_ties_even() as i32
                    && pos.x < r.right - (64. * s.scale).round_ties_even() as i32
                {
                    return HTCAPTION as isize;
                }
                return HTCLIENT as isize;
            }
            WM_MOUSEWHEEL if matches!(s.scene, Scene::Online(_)) => {
                let delta = (wp >> 16) as i16;
                if delta != 0 {
                    s.send(NativeEvent::Action(
                        h as usize,
                        Action::Online(crate::online_ui::Edit::Batch(if delta > 0 {
                            -1
                        } else {
                            1
                        })),
                    ));
                }
                return 0;
            }
            WM_MOUSEWHEEL if matches!(s.scene, Scene::Wallpaper(_)) => {
                let delta = (wp >> 16) as i16;
                if delta != 0 {
                    s.send(NativeEvent::Action(
                        h as usize,
                        Action::Picker(crate::wallpaper_ui::Action::Select(if delta > 0 {
                            -1
                        } else {
                            1
                        })),
                    ));
                }
                return 0;
            }
            WM_MOUSEWHEEL if matches!(s.scene, Scene::Sheet(_)) => {
                let scale = s.scale;
                let mut r = RECT::default();
                GetClientRect(h, &mut r);
                if let Scene::Sheet(v) = &mut s.scene {
                    let delta = if ((wp >> 16) as u16) as i16 > 0 {
                        -126.
                    } else {
                        126.
                    };
                    v.scroll((delta * scale).round_ties_even() as i32, scale, r.bottom);
                }
                InvalidateRect(h, null(), 0);
                return 0;
            }
            value if value == taskbar_message() && matches!(s.scene, Scene::Bar { .. }) => {
                s.send(NativeEvent::TaskbarCreated(h as usize));
                return 0;
            }
            WM_CAPTURECHANGED => {
                s.drag = false;
                return 0;
            }
            WM_MOUSEWHEEL if matches!(s.scene, Scene::Bar { .. }) => {
                let mut pos = POINT {
                    x: (lp as u16) as i16 as i32,
                    y: ((lp >> 16) as u16) as i16 as i32,
                };
                ScreenToClient(h, &mut pos);
                if s.hits
                    .iter()
                    .any(|hit| hit.action == Action::Brightness && hit.rect.contains(pos.x, pos.y))
                {
                    if let Scene::Bar {
                        brightness: Some(value),
                        ..
                    } = s.scene
                    {
                        let step = if ((wp >> 16) as u16) as i16 > 0 {
                            5
                        } else {
                            -5
                        };
                        s.send(NativeEvent::BrightnessSet(
                            h as usize,
                            (i32::from(value) + step).clamp(0, 100) as u8,
                        ));
                    }
                }
                return 0;
            }
            WM_CTLCOLOREDIT => {
                let ui = s.palette;
                SetTextColor(wp as HDC, ui.text);
                SetBkColor(wp as HDC, ui.surface);
                return s.resources.brush(ui.surface) as isize;
            }
            WM_TIMER if wp == 2 => {
                if matches!(s.scene, Scene::Wifi(_)) {
                    s.send(NativeEvent::Action(h as usize, Action::WifiScan(false)));
                }
                return 0;
            }
            WM_MOUSEWHEEL if matches!(s.scene, Scene::Wifi(_)) => {
                s.send(NativeEvent::Action(
                    h as usize,
                    Action::WifiScroll(((wp >> 16) as u16) as i16 as i32),
                ));
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_MOUSEACTIVATE if s.passive => return MA_NOACTIVATE as isize,
            WM_SETCURSOR if wp == h as usize && (lp as u32 & 65535) == HTCLIENT => {
                let mut pos = POINT::default();
                GetCursorPos(&mut pos);
                ScreenToClient(h, &mut pos);
                SetCursor(LoadCursorW(
                    null_mut(),
                    if s.clickable(pos.x, pos.y) {
                        IDC_HAND
                    } else {
                        IDC_ARROW
                    },
                ));
                return 1;
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(h, &mut ps);
                let mut r = RECT::default();
                GetClientRect(h, &mut r);
                let (w, ht) = (r.right - r.left, r.bottom - r.top);
                if w > 0 && ht > 0 {
                    let mem = CreateCompatibleDC(dc);
                    let bm = CreateCompatibleBitmap(dc, w, ht);
                    let old = SelectObject(mem, bm);
                    s.paint(mem, w, ht);
                    BitBlt(dc, 0, 0, w, ht, mem, 0, 0, SRCCOPY);
                    SelectObject(mem, old);
                    DeleteObject(bm);
                    DeleteDC(mem);
                }
                EndPaint(h, &ps);
                return 0;
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP => {
                if m == WM_LBUTTONDOWN && matches!(s.scene, Scene::Online(_)) {
                    SetFocus(h);
                }
                let x = (lp as u16) as i16 as i32;
                let y = ((lp >> 16) as u16) as i16 as i32;
                if m == WM_MOUSEMOVE {
                    if s.hover.is_none() {
                        let mut track = TRACKMOUSEEVENT {
                            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: h,
                            dwHoverTime: 0,
                        };
                        TrackMouseEvent(&mut track);
                    }
                    let old = s.hover;
                    s.hover = Some((x, y));
                    if s.passive {
                        let over = s
                            .hits
                            .iter()
                            .any(|hit| hit.action == Action::Battery && hit.rect.contains(x, y));
                        let was_over = old.is_some_and(|(x, y)| {
                            s.hits
                                .iter()
                                .any(|hit| hit.action == Action::Battery && hit.rect.contains(x, y))
                        });
                        if over != was_over {
                            s.send(NativeEvent::HoverBattery(h as usize, over));
                        }
                    }
                    SetCursor(LoadCursorW(
                        null_mut(),
                        if s.clickable(x, y) {
                            IDC_HAND
                        } else {
                            IDC_ARROW
                        },
                    ));
                    if old != s.hover && !s.passive {
                        InvalidateRect(h, null(), 0);
                    }
                }
                if matches!(s.scene, Scene::Bar { .. }) {
                    let rail = s
                        .hits
                        .iter()
                        .find(|hit| hit.action == Action::Brightness)
                        .map(|hit| hit.rect);
                    if let Some(rail) = rail {
                        if m == WM_LBUTTONDOWN && rail.contains(x, y) {
                            s.drag = true;
                            SetCapture(h);
                        }
                        if s.drag {
                            let start = rail.x + (25. * s.scale).round_ties_even() as i32;
                            let width = rail.w - (66. * s.scale).round_ties_even() as i32;
                            let value = ((x - start) * 100 / width.max(1)).clamp(0, 100) as u8;
                            s.send(NativeEvent::BrightnessSet(h as usize, value));
                            if m == WM_LBUTTONUP {
                                s.drag = false;
                                ReleaseCapture();
                            }
                            return 0;
                        }
                    }
                }
                if matches!(s.scene, Scene::Scale { .. }) {
                    if m == WM_LBUTTONDOWN && s.clickable(x, y) {
                        s.drag = true;
                        SetCapture(h);
                    }
                    if s.drag {
                        let scale = s.scale;
                        let mut r = RECT::default();
                        GetClientRect(h, &mut r);
                        if let Scene::Scale {
                            state, selected, ..
                        } = &mut s.scene
                        {
                            let start = (16. * scale).round_ties_even() as i32;
                            let length = r.right - (32. * scale).round_ties_even() as i32;
                            *selected = (((x - start) as f64 * state.max as f64
                                / length.max(1) as f64)
                                .round_ties_even() as i32)
                                .clamp(0, state.max as i32)
                                as usize;
                        }
                        InvalidateRect(h, null(), 0);
                        if m == WM_LBUTTONUP {
                            s.drag = false;
                            ReleaseCapture();
                            if let Scene::Scale {
                                state, selected, ..
                            } = s.scene
                            {
                                s.send(NativeEvent::ScaleCommit(h as usize, state, selected));
                            }
                        }
                    }
                } else if m == WM_LBUTTONUP {
                    if let Some(hit) = s.hits.iter().find(|hit| hit.rect.contains(x, y)) {
                        s.send(NativeEvent::Action(h as usize, hit.action.clone()));
                    }
                }
                return 0;
            }
            0x02a3 => {
                if s.passive {
                    s.send(NativeEvent::HoverBattery(h as usize, false));
                }
                s.hover = None;
                InvalidateRect(h, null(), 0);
                return 0;
            }
            WM_KEYDOWN => {
                if wp == VK_ESCAPE as usize {
                    s.send(NativeEvent::Action(h as usize, Action::Close));
                    return 0;
                }
                if let Scene::Scale {
                    state, selected, ..
                } = &mut s.scene
                {
                    if wp == VK_LEFT as usize {
                        *selected = selected.saturating_sub(1)
                    }
                    if wp == VK_RIGHT as usize {
                        *selected = (*selected + 1).min(state.max)
                    }
                    if wp == VK_RETURN as usize {
                        let (state, selected) = (*state, *selected);
                        s.send(NativeEvent::ScaleCommit(h as usize, state, selected));
                    }
                    InvalidateRect(h, null(), 0);
                }
                if let Scene::Wifi(v) = &s.scene {
                    if wp == VK_RETURN as usize {
                        s.send(NativeEvent::Action(h as usize, Action::WifiConnect));
                    } else if wp == VK_UP as usize || wp == VK_DOWN as usize {
                        let index = if wp == VK_UP as usize {
                            v.selected.saturating_sub(1)
                        } else {
                            v.selected + 1
                        };
                        s.send(NativeEvent::Action(h as usize, Action::WifiSelect(index)));
                    }
                }
                if let Scene::Online(v) = &s.scene {
                    let editing =
                        GetFocus() == GetDlgItem(h, 101) || GetFocus() == GetDlgItem(h, 102);
                    let edit = match wp as u16 {
                        VK_RETURN if GetFocus() == GetDlgItem(h, 102) => {
                            Some(crate::online_ui::Edit::Jump)
                        }
                        VK_RETURN if GetFocus() == GetDlgItem(h, 101) => {
                            Some(crate::online_ui::Edit::Search(true))
                        }
                        VK_LEFT if GetAsyncKeyState(VK_CONTROL as i32) < 0 => {
                            Some(crate::online_ui::Edit::Page(-1))
                        }
                        VK_RIGHT if GetAsyncKeyState(VK_CONTROL as i32) < 0 => {
                            Some(crate::online_ui::Edit::Page(1))
                        }
                        VK_PRIOR if !editing => Some(crate::online_ui::Edit::Batch(-1)),
                        VK_NEXT if !editing => Some(crate::online_ui::Edit::Batch(1)),
                        VK_LEFT if !editing => {
                            Some(crate::online_ui::Edit::Select(v.selected.saturating_sub(1)))
                        }
                        VK_RIGHT if !editing => Some(crate::online_ui::Edit::Select(
                            (v.selected + 1).min(v.rows.len().saturating_sub(1)),
                        )),
                        VK_UP if !editing => {
                            Some(crate::online_ui::Edit::Select(v.selected.saturating_sub(3)))
                        }
                        VK_DOWN if !editing => Some(crate::online_ui::Edit::Select(
                            (v.selected + 3).min(v.rows.len().saturating_sub(1)),
                        )),
                        _ => None,
                    };
                    if let Some(edit) = edit {
                        s.send(NativeEvent::Action(h as usize, Action::Online(edit)));
                    }
                }
                if matches!(s.scene, Scene::Wallpaper(_)) {
                    if wp == VK_RETURN as usize {
                        s.send(NativeEvent::Action(
                            h as usize,
                            Action::Picker(crate::wallpaper_ui::Action::Apply),
                        ));
                    }
                    if wp == 0x4f && GetAsyncKeyState(VK_CONTROL as i32) < 0 {
                        s.send(NativeEvent::Action(
                            h as usize,
                            Action::Picker(crate::wallpaper_ui::Action::Folder),
                        ));
                    }
                    let delta = match wp as u16 {
                        VK_LEFT | 0x48 => Some(-1),
                        VK_RIGHT | 0x4c => Some(1),
                        VK_TAB => Some(if GetAsyncKeyState(VK_SHIFT as i32) < 0 {
                            -1
                        } else {
                            1
                        }),
                        _ => None,
                    };
                    if let Some(delta) = delta {
                        s.send(NativeEvent::Action(
                            h as usize,
                            Action::Picker(crate::wallpaper_ui::Action::Select(delta)),
                        ));
                    }
                }
                if matches!(s.scene, Scene::WindowsOptions(_)) && wp == VK_RETURN as usize {
                    s.send(NativeEvent::Action(
                        h as usize,
                        Action::WindowsOption(crate::windows_options_ui::Edit::Save),
                    ));
                }
                let mut compare_choice = None;
                if let Scene::Compare(v) = &mut s.scene {
                    if (0x31..=0x36).contains(&wp) {
                        compare_choice = Some(wp - 0x31);
                    } else if wp == VK_RETURN as usize {
                        compare_choice = Some(v.selected);
                    } else {
                        v.navigate(match wp as u16 {
                            VK_LEFT => -1,
                            VK_RIGHT => 1,
                            VK_UP => -3,
                            VK_DOWN => 3,
                            _ => 0,
                        });
                        InvalidateRect(h, null(), 0);
                    }
                }
                if let Some(index) = compare_choice {
                    s.send(NativeEvent::Action(
                        h as usize,
                        Action::Compare(crate::style_compare_ui::Edit::Choose(index)),
                    ));
                }
                let scale = s.scale;
                let mut rect = RECT::default();
                GetClientRect(h, &mut rect);
                if let Scene::Sheet(v) = &mut s.scene {
                    let delta = match wp as u16 {
                        VK_UP => -42.,
                        VK_DOWN => 42.,
                        VK_PRIOR => -420.,
                        VK_NEXT => 420.,
                        VK_HOME => -100000.,
                        VK_END => 100000.,
                        _ => 0.,
                    };
                    v.scroll((delta * scale).round_ties_even() as i32, scale, rect.bottom);
                    InvalidateRect(h, null(), 0);
                }
                return 0;
            }
            WM_CLOSE => {
                s.send(NativeEvent::Action(h as usize, Action::Close));
                return 0;
            }
            WM_ACTIVATE if !s.passive && (wp & 65535) == WA_INACTIVE as usize => {
                // Keep the parent alive while an owned popup or native dialog is active.
                let next = lp as HWND;
                if next.is_null() || GetAncestor(next, GA_ROOTOWNER) != GetAncestor(h, GA_ROOTOWNER)
                {
                    s.send(NativeEvent::Dismiss(h as usize));
                }
                return 0;
            }
            WM_DPICHANGED | WM_DISPLAYCHANGE => {
                s.send(NativeEvent::DisplayChanged);
                return 0;
            }
            v if v == WM_APP + 80 => {
                s.send(NativeEvent::AppBar(h as usize, wp, lp));
                return 0;
            }
            _ => (),
        }
        drop(s);
        DefWindowProcW(h, m, wp, lp)
    }
}
fn taskbar_message() -> u32 {
    static MESSAGE: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) })
}
pub fn centered_bounds(width: f64, height: f64) -> (Rect, f64) {
    unsafe {
        let mut pos = POINT::default();
        GetCursorPos(&mut pos);
        let monitor = MonitorFromPoint(pos, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info);
        let (mut x, mut y) = (96, 96);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y);
        let scale = x as f64 / 96.;
        let ww =
            ((width * scale).round_ties_even() as i32).min(info.rcWork.right - info.rcWork.left);
        let hh =
            ((height * scale).round_ties_even() as i32).min(info.rcWork.bottom - info.rcWork.top);
        (
            Rect::new(
                info.rcWork.left + (info.rcWork.right - info.rcWork.left - ww) / 2,
                info.rcWork.top + (info.rcWork.bottom - info.rcWork.top - hh) / 2,
                ww,
                hh,
            ),
            scale,
        )
    }
}
pub fn preview(root: PathBuf, seconds: u64) -> anyhow::Result<()> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let model = crate::ipc::Client::new().model()?;
    let monitor = model
        .monitors
        .first()
        .ok_or_else(|| anyhow::anyhow!("No monitor"))?;
    let scale = display::read(&monitor.name)?;
    let (tx, rx) = std::sync::mpsc::channel();
    let state = State {
        palette: Palette::load(&root),
        root,
        scale: monitor.scale,
        scene: Scene::Scale {
            state: scale,
            selected: scale.current,
            error: String::new(),
        },
        resources: Resources::new(),
        hits: vec![],
        hover: None,
        drag: false,
        passive: false,
        events: tx,
        wake: 0,
    };
    let w = (260. * monitor.scale).round_ties_even() as i32;
    let ht = (112. * monitor.scale).round_ties_even() as i32;
    let window = Window::new(
        "GlazeBar Rust · 배율",
        Rect::new(
            monitor.bounds.x + (monitor.bounds.w - w) / 2,
            monitor.bounds.y + 100,
            w,
            ht,
        ),
        state,
        null_mut(),
    )?;
    window.show();
    unsafe {
        SetTimer(window.hwnd, 1, 100, None);
    }
    let started = std::time::Instant::now();
    loop {
        let mut msg = MSG::default();
        let result = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for e in rx.try_iter() {
            match e {
                NativeEvent::Action(_, Action::Close)
                | NativeEvent::Dismiss(_)
                | NativeEvent::Closed(_) => return Ok(()),
                NativeEvent::ScaleCommit(_, state, selected) => {
                    match display::write(state, selected) {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            if let Scene::Scale { error, .. } = &mut window.state.borrow_mut().scene
                            {
                                *error = e.to_string()
                            }
                            window.invalidate();
                        }
                    }
                }
                _ => (),
            }
        }
        if seconds > 0 && started.elapsed().as_secs() >= seconds {
            break;
        }
    }
    Ok(())
}

pub fn validate(root: PathBuf) -> anyhow::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let scale = display::Scale {
        source: Default::default(),
        recommended: 4,
        current: 3,
        max: 7,
    };
    let ui = Palette::load(&root);
    let window = Window::new(
        "GlazeBar Rust validation",
        Rect::new(-32000, -32000, 260, 112),
        State {
            root: root.clone(),
            scale: 1.,
            palette: ui,
            scene: Scene::Scale {
                state: scale,
                selected: 3,
                error: String::new(),
            },
            resources: Resources::new(),
            hits: vec![],
            hover: None,
            drag: false,
            passive: false,
            events: tx,
            wake: 0,
        },
        null_mut(),
    )?;
    let handle = window.hwnd;
    anyhow::ensure!(window.alive(), "Native window was not created");
    unsafe {
        let mut caption = [0u16; 128];
        let length = GetWindowTextW(handle, caption.as_mut_ptr(), caption.len() as i32);
        anyhow::ensure!(
            String::from_utf16_lossy(&caption[..length as usize]) == "GlazeBar Rust validation",
            "Native window title is missing"
        );
    }
    unsafe {
        let previous = GetCursor();
        SendMessageW(handle, WM_MOUSEMOVE, 0, ((60isize) << 16) | 60);
        let hand = GetCursor() == LoadCursorW(null_mut(), IDC_HAND);
        SendMessageW(handle, WM_MOUSEMOVE, 0, ((10isize) << 16) | 10);
        let arrow = GetCursor() == LoadCursorW(null_mut(), IDC_ARROW);
        SetCursor(previous);
        anyhow::ensure!(hand && arrow, "Native cursor feedback failed");
        SendMessageW(handle, WM_KEYDOWN, VK_LEFT as usize, 0);
        SendMessageW(handle, WM_KEYDOWN, VK_RETURN as usize, 0);
    }
    let selected = match &window.state.borrow().scene {
        Scene::Scale { selected, .. } => *selected,
        _ => usize::MAX,
    };
    anyhow::ensure!(selected == 2, "Left arrow did not select adjacent scale");
    anyhow::ensure!(
        rx.try_iter()
            .any(|e| matches!(e, NativeEvent::ScaleCommit(_, _, 2))),
        "Enter did not emit scale commit"
    );
    {
        let mut s = window.state.borrow_mut();
        s.scene = Scene::Bar {
            monitor: Monitor::default(),
            sensors: Sensors::default(),
            paused: false,
            resize: false,
            vertical: false,
            clock: String::new(),
            brightness: Some(37),
            weather: None,
        };
        s.hits = vec![Hit {
            rect: Rect::new(10, 0, 150, 40),
            action: Action::Brightness,
        }];
    }
    unsafe {
        SendMessageW(handle, WM_LBUTTONDOWN, 0, (20isize << 16) | 77);
        SendMessageW(handle, WM_LBUTTONUP, 0, (20isize << 16) | 77);
    }
    anyhow::ensure!(
        rx.try_iter()
            .any(|e| matches!(e, NativeEvent::BrightnessSet(_, 50))),
        "Brightness drag did not map to 50 percent"
    );
    window.state.borrow_mut().scene = Scene::Wifi(crate::wifi_ui::fixture());
    window.bounds(Rect::new(-32000, -32000, 420, 414));
    window.wifi_layout(true);
    unsafe {
        let edit = GetDlgItem(handle, 100);
        anyhow::ensure!(!edit.is_null(), "Wi-Fi native edit was not created");
        let mut bounds = RECT::default();
        GetWindowRect(edit, &mut bounds);
        anyhow::ensure!(
            bounds.right - bounds.left == 290,
            "Wi-Fi password width changed"
        );
        anyhow::ensure!(
            GetWindowLongPtrW(edit, GWL_STYLE) & ES_PASSWORD as isize != 0,
            "Wi-Fi edit does not mask password"
        );
        SetWindowTextW(edit, wide("test<&>123").as_ptr());
        anyhow::ensure!(
            window.wifi_secret() == "test<&>123",
            "Wi-Fi native edit changed input"
        );
        SendMessageW(handle, WM_KEYDOWN, VK_RETURN as usize, 0);
    }
    anyhow::ensure!(
        rx.try_iter()
            .any(|e| matches!(e, NativeEvent::Action(_, Action::WifiConnect))),
        "Wi-Fi Enter did not emit connect"
    );
    window.wifi_layout(true);
    anyhow::ensure!(window.wifi_secret().is_empty(), "Password was not cleared");
    window.state.borrow_mut().scene = Scene::Wallpaper(crate::wallpaper_ui::Picker::load(&root)?);
    window.bounds(Rect::new(-32000, -32000, 1000, 590));
    unsafe {
        let dc = GetDC(handle);
        window.state.borrow_mut().paint(dc, 1000, 590);
        ReleaseDC(handle, dc);
        let previous = GetCursor();
        SendMessageW(handle, WM_MOUSEMOVE, 0, (30isize << 16) | 160);
        anyhow::ensure!(
            GetCursor() == LoadCursorW(null_mut(), IDC_HAND),
            "Picker toggle does not show hand cursor"
        );
        SetCursor(previous);
        SendMessageW(handle, WM_LBUTTONUP, 0, (30isize << 16) | 160);
        SendMessageW(handle, WM_KEYDOWN, VK_RIGHT as usize, 0);
        SendMessageW(handle, WM_MOUSEWHEEL, (120usize) << 16, 0);
        SendMessageW(handle, WM_KEYDOWN, VK_RETURN as usize, 0);
    }
    let events: Vec<_> = rx.try_iter().collect();
    for action in [
        crate::wallpaper_ui::Action::Enabled,
        crate::wallpaper_ui::Action::Select(1),
        crate::wallpaper_ui::Action::Select(-1),
        crate::wallpaper_ui::Action::Apply,
    ] {
        anyhow::ensure!(
            events
                .iter()
                .any(|e| matches!(e,NativeEvent::Action(_,Action::Picker(a)) if *a==action)),
            "Missing picker input event: {action:?}"
        );
    }
    let events = window.state.borrow().events.clone();
    let options = Window::new(
        "Rust options validation",
        Rect::new(-32000, -32000, 510, 435),
        State {
            root: root.clone(),
            scale: 1.,
            palette: ui,
            scene: Scene::WindowsOptions(crate::windows_options_ui::Settings {
                enabled: true,
                options: Default::default(),
                light: false,
                status: String::new(),
            }),
            resources: Resources::new(),
            hits: vec![],
            hover: None,
            drag: false,
            passive: false,
            events,
            wake: 0,
        },
        handle,
    )?;
    unsafe {
        anyhow::ensure!(
            window.owns_focus(options.hwnd),
            "Owned popup is outside its family"
        );
        anyhow::ensure!(options.owns_focus(options.hwnd), "Popup excludes itself");
        anyhow::ensure!(
            !options.owns_focus(handle),
            "Popup includes its owner bar in its click area"
        );
        anyhow::ensure!(
            !window.owns_focus(GetDesktopWindow()),
            "Desktop belongs to popup family"
        );
        // Neither window has been shown or clicked: dismissal must not need
        // an earlier activation or pointer event inside the popup.
        SendMessageW(
            options.hwnd,
            WM_ACTIVATE,
            WA_INACTIVE as usize,
            GetDesktopWindow() as isize,
        );
        anyhow::ensure!(
            rx.try_iter()
                .any(|e| matches!(e, NativeEvent::Dismiss(h) if h == options.hwnd as usize)),
            "Unclicked popup did not dismiss on outside activation"
        );
        SendMessageW(
            handle,
            WM_ACTIVATE,
            WA_INACTIVE as usize,
            options.hwnd as isize,
        );
        anyhow::ensure!(
            !rx.try_iter().any(|e| matches!(
                e,
                NativeEvent::Action(_, Action::Close) | NativeEvent::Dismiss(_)
            )),
            "Owned dialog closes its picker"
        );
        let dc = GetDC(options.hwnd);
        options.state.borrow_mut().paint(dc, 510, 435);
        ReleaseDC(options.hwnd, dc);
        anyhow::ensure!(
            options.state.borrow().hits.len() == 8,
            "Windows options controls missing"
        );
        SendMessageW(options.hwnd, WM_LBUTTONUP, 0, (80isize << 16) | 40);
        SendMessageW(options.hwnd, WM_KEYDOWN, VK_RETURN as usize, 0);
    }
    let events: Vec<_> = rx.try_iter().collect();
    for edit in [
        crate::windows_options_ui::Edit::Enabled,
        crate::windows_options_ui::Edit::Save,
    ] {
        anyhow::ensure!(
            events
                .iter()
                .any(|e| matches!(e,NativeEvent::Action(_,Action::WindowsOption(v)) if *v==edit)),
            "Missing options event {edit:?}"
        );
    }
    drop(options);
    anyhow::ensure!(window.alive(), "Closing options destroyed picker");
    crate::wallpaper::validate_folder_cancel(handle)?;
    let mut comparison = crate::style_compare_ui::Comparison::new(Default::default(), 0, false);
    comparison.palettes = std::array::from_fn(|_| Some(std::array::from_fn(|_| "#abcdef".into())));
    window.state.borrow_mut().scene = Scene::Compare(comparison);
    window.bounds(Rect::new(-32000, -32000, 1120, 680));
    unsafe {
        let dc = GetDC(handle);
        window.state.borrow_mut().paint(dc, 1120, 680);
        ReleaseDC(handle, dc);
        anyhow::ensure!(
            window.state.borrow().hits.len() == 9,
            "Style comparison hit areas missing"
        );
        SendMessageW(handle, WM_KEYDOWN, VK_RIGHT as usize, 0);
        SendMessageW(handle, WM_KEYDOWN, VK_DOWN as usize, 0);
        SendMessageW(handle, WM_KEYDOWN, VK_RETURN as usize, 0);
        SendMessageW(handle, WM_KEYDOWN, 0x31, 0);
    }
    let events: Vec<_> = rx.try_iter().collect();
    for index in [4, 0] {
        anyhow::ensure!(events.iter().any(|e|matches!(e,NativeEvent::Action(_,Action::Compare(crate::style_compare_ui::Edit::Choose(i))) if *i==index)),"Style comparison keyboard selection failed");
    }
    window.state.borrow_mut().scene = Scene::Online(Default::default());
    window.bounds(Rect::new(-32000, -32000, 1000, 650));
    window.online_layout();
    unsafe {
        let edit = GetDlgItem(handle, 101);
        anyhow::ensure!(!edit.is_null(), "Online native edit missing");
        anyhow::ensure!(window.owns_focus(edit), "Popup excludes its input control");
        SetWindowTextW(edit, wide("landscape 한글").as_ptr());
        anyhow::ensure!(
            window.edit_text(101) == "landscape 한글",
            "Online query text roundtrip failed"
        );
        anyhow::ensure!(
            GetWindowLongW(edit, GWL_STYLE) & ES_PASSWORD as i32 == 0,
            "Online query is masked"
        );
        let dc = GetDC(handle);
        window.state.borrow_mut().paint(dc, 1000, 650);
        ReleaseDC(handle, dc);
        anyhow::ensure!(
            [
                crate::online_ui::Edit::Sort,
                crate::online_ui::Edit::Favorite,
                crate::online_ui::Edit::Jump
            ]
            .iter()
            .all(|edit| window
                .state
                .borrow()
                .hits
                .iter()
                .any(|hit| hit.action == Action::Online(edit.clone()))),
            "Online empty controls missing"
        );
        SendMessageW(handle, WM_LBUTTONUP, 0, ((30 << 16) | 920) as isize);
        let page = GetDlgItem(handle, 102);
        anyhow::ensure!(
            !page.is_null() && window.owns_focus(page),
            "Page jump native input missing"
        );
        SetWindowTextW(page, wide("12").as_ptr());
        anyhow::ensure!(
            window.edit_text(102) == "12",
            "Page number roundtrip failed"
        );
        SendMessageW(handle, WM_MOUSEWHEEL, (-120i16 as u16 as usize) << 16, 0);
        SendMessageW(handle, WM_LBUTTONUP, 0, ((568 << 16) | 280) as isize);
    }
    let online_events: Vec<_> = rx.try_iter().collect();
    anyhow::ensure!(
        online_events.iter().any(|e| matches!(
            e,
            NativeEvent::Action(_, Action::Online(crate::online_ui::Edit::Search(true)))
        )),
        "Online search click missing"
    );
    for expected in [
        crate::online_ui::Edit::Batch(1),
        crate::online_ui::Edit::Jump,
    ] {
        anyhow::ensure!(online_events.iter().any(|event|matches!(event,NativeEvent::Action(_,Action::Online(edit)) if *edit==expected)),"Missing online input: {expected:?}");
    }
    drop(window);
    anyhow::ensure!(
        unsafe { IsWindow(handle) } == 0,
        "Native window was not destroyed"
    );
    std::fs::write(
        root.join("ui-reference/rust/native-validation.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"handCursor":true,"arrowCursor":true,"keyboardScaleSelection":true,"enterEmitsCommit":true,"brightnessDragEvent":true,"windowDestroyed":true,"wifiNativePassword":true,"wifiPasswordCleared":true,"wifiEnterEvent":true,"wallpaperInputEvents":true,"optionsOwnedWindow":true,"optionsInputEvents":true,"folderDialogCancelled":true,"hardwareSettingsChanged":false,"themeSettingsChanged":false}),
        )?,
    )?;
    println!("Native cursor, keyboard, event dispatch and destruction checks passed.");
    Ok(())
}
