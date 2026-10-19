use crate::{
    graphics::{Palette, Rect, Resources, wide},
    ipc::{self, Model},
    native::{NativeEvent, Scene, State, Window},
    sensors::{Reader, Sensors},
    views::{self, Action},
};
use anyhow::{Result, ensure};
use std::{
    collections::HashMap,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender, SyncSender},
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::Threading::*,
    UI::{HiDpi::*, Shell::*, WindowsAndMessaging::*},
};

const PREFIX: &str = "Local\\GlazeWMWorkspaceToggleResident-";
fn preview_bounds(owner: &Window) -> (Rect, Rect) {
    use windows_sys::Win32::Graphics::Gdi::*;
    let mut area = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let mut main = owner.rect();
    let scale = owner.state.borrow().scale;
    let p = |n: f64| (n * scale).round_ties_even() as i32;
    let ok = unsafe {
        GetMonitorInfoW(
            MonitorFromWindow(owner.hwnd, MONITOR_DEFAULTTONEAREST),
            &mut area,
        )
    };
    let work = if ok != 0 {
        Rect::new(
            area.rcWork.left,
            area.rcWork.top,
            area.rcWork.right - area.rcWork.left,
            area.rcWork.bottom - area.rcWork.top,
        )
    } else {
        main
    };
    let width = p(360.);
    let gap = p(12.);
    if work.w - p(32.) - width - gap >= p(850.) {
        main.x = work.x + (work.w - main.w - gap - width) / 2;
    }
    let x = (main.x + main.w + gap)
        .min(work.x + work.w - width)
        .max(work.x);
    let y = main.y.min(work.y + work.h - main.h).max(work.y);
    (main, Rect::new(x, y, width, main.h))
}
enum Work {
    Refresh,
    Command(String),
    ToggleGroup,
    Resync,
    RemoteChanged,
    Stop,
}
enum Update {
    Model(Model),
    Sensors(Sensors),
    Theme(Palette),
    Stop,
    Error(String),
    WifiRows(usize, u64, Result<Vec<crate::wifi::Network>, String>),
    WifiConnected(usize, u64, Result<(), String>),
    Brightness(String, Option<u8>, u64, Option<String>),
    Weather(crate::weather::Weather),
    Control(String),
    Wallpaper(usize, u64, crate::wallpaper_worker::Item),
    Precache(usize, crate::precache::Progress),
    Online(usize, u64, crate::online_ui::Item),
    Comparison(
        usize,
        u64,
        Result<crate::style_compare_worker::Palettes, String>,
    ),
    ThemeFinished(usize, u64, bool, Result<(), String>),
}
struct Bar {
    window: Window,
    registered: bool,
}
impl Bar {
    fn appbar(&self, message: u32, r: Rect) -> Rect {
        unsafe {
            let mut data = APPBARDATA {
                cbSize: size_of::<APPBARDATA>() as u32,
                hWnd: self.window.hwnd,
                uCallbackMessage: WM_APP + 80,
                uEdge: ABE_TOP,
                rc: r.native(),
                lParam: 0,
            };
            SHAppBarMessage(message, &mut data);
            Rect::new(
                data.rc.left,
                data.rc.top,
                data.rc.right - data.rc.left,
                data.rc.bottom - data.rc.top,
            )
        }
    }
    fn position(&mut self, bounds: Rect, shadow: bool, hidden: bool) {
        if hidden || (!shadow && crate::hooks::remote_covers(bounds)) {
            if self.registered {
                self.appbar(ABM_REMOVE, Rect::default());
                self.registered = false;
            }
            unsafe {
                ShowWindow(self.window.hwnd, SW_HIDE);
            }
            return;
        }
        if shadow {
            self.window.bounds(bounds);
            self.window.show();
            return;
        }
        let was_registered = self.registered;
        if !self.registered {
            self.appbar(ABM_NEW, bounds);
            self.registered = true;
        }
        let mut r = self.appbar(ABM_QUERYPOS, bounds);
        r.h = bounds.h;
        if !was_registered || self.window.rect() != r {
            let actual = self.appbar(ABM_SETPOS, r);
            self.window.bounds(actual);
        }
        self.window.show();
    }
}
impl Drop for Bar {
    fn drop(&mut self) {
        if self.registered {
            self.appbar(ABM_REMOVE, Rect::default());
        }
    }
}
fn send(tx: &Sender<Update>, thread: u32, event: Update) {
    if tx.send(event).is_ok() {
        unsafe {
            PostThreadMessageW(thread, WM_APP + 1, 0, 0);
        }
    }
}
fn request_refresh(tx: &SyncSender<Work>, pending: &AtomicBool) {
    if !pending.swap(true, Ordering::Relaxed) && tx.try_send(Work::Refresh).is_err() {
        pending.store(false, Ordering::Relaxed);
    }
}
fn visible_sensor_key(s: &Sensors) -> String {
    format!(
        "{:.0}/{:.0}/{:.0}/{:.1}/{}/{}/{}/{}/{}/{:.1}/{:.1}",
        s.cpu,
        s.memory,
        s.percent,
        s.watts,
        s.battery,
        s.charging,
        s.discharging,
        s.known,
        s.network,
        s.full,
        s.remaining
    )
}
fn make_state(
    root: &std::path::Path,
    scale: f64,
    scene: Scene,
    passive: bool,
    events: &Sender<NativeEvent>,
) -> State {
    State {
        root: root.to_owned(),
        scale,
        palette: Palette::load(root),
        scene,
        resources: Resources::new(),
        hits: vec![],
        hover: None,
        drag: false,
        passive,
        events: events.clone(),
        wake: 0,
    }
}
fn scene(m: &ipc::Monitor, model: &Model, s: &Sensors) -> Scene {
    Scene::Bar {
        monitor: m.clone(),
        sensors: s.clone(),
        paused: model.paused,
        resize: model.resize,
        vertical: model.vertical,
        clock: views::clock(),
        brightness: None,
        weather: None,
    }
}

pub fn run(root: PathBuf, shadow_seconds: Option<u64>) -> Result<()> {
    run_with_popup(root, shadow_seconds, None)
}
pub fn run_with_popup(
    root: PathBuf,
    shadow_seconds: Option<u64>,
    mut initial: Option<Action>,
) -> Result<()> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let shadow = shadow_seconds.is_some();
    let mutex_name = if shadow {
        format!("{PREFIX}RustShadow")
    } else {
        format!("{PREFIX}Instance")
    };
    let mutex = unsafe { CreateMutexW(null(), 0, wide(&mutex_name).as_ptr()) };
    ensure!(!mutex.is_null(), "Cannot create resident mutex");
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(mutex);
        }
        return Ok(());
    }
    struct MutexGuard(HANDLE);
    impl Drop for MutexGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let _mutex = MutexGuard(mutex);
    let thread = unsafe { GetCurrentThreadId() };
    let mut init = MSG::default();
    unsafe {
        PeekMessageW(&mut init, null_mut(), 0, 0, PM_NOREMOVE);
    }
    let (tx, rx) = mpsc::channel();
    let remote = Arc::new(AtomicBool::new(false));
    let (hook_tx, hook_rx) = mpsc::channel();
    let hooks = if shadow {
        None
    } else {
        Some(crate::hooks::Hooks::install(hook_tx, remote.clone(), true)?)
    };
    let weather_worker = {
        let tx = tx.clone();
        crate::weather::Worker::spawn(move |result| match result {
            Ok(w) => send(&tx, thread, Update::Weather(w)),
            Err(e) => send(&tx, thread, Update::Error(format!("Weather: {e}"))),
        })
    };
    let mut weather: Option<crate::weather::Weather> = None;
    let brightness_worker = {
        let tx = tx.clone();
        crate::brightness::Worker::spawn(move |name, value, revision, error| {
            send(
                &tx,
                thread,
                Update::Brightness(name, value, revision, error),
            )
        })
    };
    let mut brightness_cache: HashMap<String, (Option<u8>, u64)> = HashMap::new();
    let (ntx, nrx) = mpsc::channel();
    let (work_tx, work_rx) = mpsc::sync_channel(64);
    let stop = Arc::new(AtomicBool::new(false));
    let pending = Arc::new(AtomicBool::new(false));
    let worker = {
        let tx = tx.clone();
        let root = root.clone();
        let pending = pending.clone();
        let remote = remote.clone();
        std::thread::spawn(move || {
            let mut client = ipc::Client::new();
            let mut remote_owned = false;
            while let Ok(work) = work_rx.recv() {
                match work {
                    Work::Stop => {
                        if !shadow {
                            if remote_owned
                                && client.request("query paused").is_ok_and(|v| v == true)
                            {
                                if let Err(e) = client.request("command wm-toggle-pause") {
                                    send(&tx, thread, Update::Error(e.to_string()));
                                }
                            }
                            if let Err(e) = client.restore_group(&root) {
                                send(&tx, thread, Update::Error(e.to_string()));
                            }
                        }
                        break;
                    }
                    Work::ToggleGroup => {
                        if remote.load(Ordering::Relaxed) {
                            continue;
                        }
                        if let Err(e) = client.toggle_group(&root) {
                            send(&tx, thread, Update::Error(e.to_string()));
                        }
                        match client.model() {
                            Ok(m) => send(&tx, thread, Update::Model(m)),
                            Err(e) => send(&tx, thread, Update::Error(e.to_string())),
                        }
                    }
                    Work::Resync => {
                        std::thread::sleep(Duration::from_millis(120));
                        if remote.load(Ordering::Relaxed) {
                            continue;
                        }
                        if client.request("query paused").is_ok_and(|v| v != true) {
                            unsafe {
                                let h = GetForegroundWindow();
                                if let Ok(rows) = client.request("query windows") {
                                    if GetForegroundWindow() == h
                                        && rows["windows"].as_array().is_some_and(|rows| {
                                            rows.iter().any(|v| {
                                                ipc::string(v, "handle")
                                                    == format!("{}", h as usize)
                                            })
                                        })
                                    {
                                        windows_sys::Win32::UI::Accessibility::NotifyWinEvent(
                                            3, h, 0, 0,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Work::RemoteChanged => {
                        match client.request("query paused") {
                            Ok(value) => {
                                let (toggle, owned) = crate::hooks::pause_transition(
                                    remote.load(Ordering::Relaxed),
                                    value == true,
                                    remote_owned,
                                );
                                let result = if toggle {
                                    client.request("command wm-toggle-pause").map(|_| ())
                                } else {
                                    Ok(())
                                };
                                match result {
                                    Ok(()) => remote_owned = owned,
                                    Err(e) => send(&tx, thread, Update::Error(e.to_string())),
                                }
                            }
                            Err(e) => send(&tx, thread, Update::Error(e.to_string())),
                        }
                        match client.model() {
                            Ok(model) => send(&tx, thread, Update::Model(model)),
                            Err(e) => send(&tx, thread, Update::Error(e.to_string())),
                        }
                    }
                    Work::Command(command) => {
                        if let Err(e) = client.request(&format!("command {command}")) {
                            send(&tx, thread, Update::Error(e.to_string()));
                        }
                        match client.model() {
                            Ok(model) => send(&tx, thread, Update::Model(model)),
                            Err(e) => send(&tx, thread, Update::Error(e.to_string())),
                        }
                    }
                    Work::Refresh => {
                        pending.store(false, Ordering::Relaxed);
                        if !shadow {
                            if let Err(e) = client.auto_split(
                                remote.load(Ordering::Relaxed)
                                    || root.join("autotile.disabled").exists(),
                            ) {
                                send(&tx, thread, Update::Error(e.to_string()));
                            }
                        }
                        match client.model() {
                            Ok(model) => send(&tx, thread, Update::Model(model)),
                            Err(e) => send(&tx, thread, Update::Error(e.to_string())),
                        }
                    }
                }
            }
        })
    };
    let observer = {
        if !shadow {
            let _ = work_tx.try_send(Work::RemoteChanged);
        }
        let work_tx = work_tx.clone();
        let pending = pending.clone();
        let stop = stop.clone();
        std::thread::spawn(move || ipc::subscribe(|| request_refresh(&work_tx, &pending), &stop))
    };
    let sensor_thread = {
        let tx = tx.clone();
        let stop = stop.clone();
        let theme_file = root.join("theme-state/glazebar.json");
        let theme_root = root.clone();
        std::thread::spawn(move || {
            let mut reader = Reader::new();
            let mut s = Sensors::default();
            let mut key = String::new();
            let mut count = 0;
            let signature = || {
                std::fs::metadata(&theme_file)
                    .ok()
                    .and_then(|m| m.modified().ok().map(|time| (time, m.len())))
            };
            let mut theme_signature = signature();
            while !stop.load(Ordering::Relaxed) {
                if count % 2 == 0 {
                    let current = signature();
                    if current != theme_signature {
                        theme_signature = current;
                        send(&tx, thread, Update::Theme(Palette::load(&theme_root)));
                    }
                }
                reader.read(&mut s, count % 10 == 0);
                let next = visible_sensor_key(&s);
                if key != next {
                    send(&tx, thread, Update::Sensors(s.clone()));
                    key = next;
                }
                count += 1;
                std::thread::park_timeout(Duration::from_millis(500));
            }
        })
    };
    // One blocking wait replaces one polling thread for every named event.
    let event_names = [
        "Stop",
        "Toggle",
        "Bar",
        "Sheet",
        "Wallpaper",
        "FocusResync",
        "Snapshot",
        "ClosePopups",
    ];
    let event_handles = event_names
        .iter()
        .map(|name| {
            let text = wide(&format!("{PREFIX}{name}"));
            unsafe {
                CreateEventW(
                    null(),
                    (*name == "Stop") as i32,
                    0,
                    if shadow { null() } else { text.as_ptr() },
                )
            }
        })
        .collect::<Vec<_>>();
    let stop_event = event_handles[0];
    let event_thread = {
        let handles = event_handles
            .iter()
            .map(|h| *h as usize)
            .collect::<Vec<_>>();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let handles = handles.into_iter().map(|h| h as HANDLE).collect::<Vec<_>>();
            loop {
                let index = unsafe {
                    WaitForMultipleObjects(handles.len() as u32, handles.as_ptr(), 0, INFINITE)
                };
                if index == WAIT_FAILED {
                    send(&tx, thread, Update::Error("Named event wait failed".into()));
                    send(&tx, thread, Update::Stop);
                    break;
                }
                if index == 0 {
                    send(&tx, thread, Update::Stop);
                    break;
                }
                if (index as usize) < event_names.len() {
                    send(
                        &tx,
                        thread,
                        Update::Control(event_names[index as usize].into()),
                    );
                }
            }
        })
    };
    request_refresh(&work_tx, &pending);
    let mut bars: HashMap<String, Bar> = HashMap::new();
    let mut popup: Option<Window> = None;
    let mut child_popup: Option<Window> = None;
    let mut preview_popup: Option<Window> = None;
    let mut theme_job: Option<std::thread::JoinHandle<()>> = None;
    let mut theme_operation = 0u64;
    let compare_worker = {
        let tx = tx.clone();
        crate::style_compare_worker::Worker::start(move |hwnd, revision, result| {
            send(&tx, thread, Update::Comparison(hwnd, revision, result))
        })
    };
    let mut compare_active = false;
    let mut online_worker = {
        let tx = tx.clone();
        crate::online_ui::Worker::start(move |hwnd, revision, item| {
            send(&tx, thread, Update::Online(hwnd, revision, item))
        })
    };
    let mut online_active = false;
    let mut cache_worker = {
        let tx = tx.clone();
        crate::precache::Worker::start(move |hwnd, p| send(&tx, thread, Update::Precache(hwnd, p)))
    };
    let mut cache_active = false;
    let wallpaper_worker = {
        let tx = tx.clone();
        crate::wallpaper_worker::Worker::start(move |hwnd, revision, item| {
            send(&tx, thread, Update::Wallpaper(hwnd, revision, item))
        })
    };
    let mut wifi_jobs: Vec<std::thread::JoinHandle<()>> = vec![];
    let mut battery_popup: Option<Window> = None;
    let mut model = Model::default();
    let mut sensors = Sensors::default();
    let mut hidden = !shadow && root.join("bar-hidden.txt").exists();
    let mut last_clock = views::clock();
    let start = Instant::now();
    let mut wallpaper_captured = false;
    let mut comparison_captured = None;
    let until_minute = (60000 - chrono::Utc::now().timestamp_millis().rem_euclid(60000)) as u32;
    let timer = unsafe {
        SetTimer(
            null_mut(),
            0,
            if shadow { 1000 } else { until_minute },
            None,
        )
    };
    let mut result = Ok(());
    'main: loop {
        let mut msg = MSG::default();
        let code = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if code <= 0 {
            break;
        }
        unsafe {
            let parent = GetAncestor(msg.hwnd, GA_ROOT);
            if msg.message == WM_KEYDOWN && matches!(msg.wParam, 13 | 27) && parent != msg.hwnd {
                SendMessageW(parent, msg.message, msg.wParam, msg.lParam);
            } else {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if shadow_seconds.is_some_and(|seconds| start.elapsed().as_secs() >= seconds) {
            break;
        }
        if !shadow && msg.message == WM_HOTKEY && !remote.load(Ordering::Relaxed) {
            if let Some(command) = match msg.wParam {
                2 => Some("Bar"),
                3 => Some("Wallpaper"),
                _ => None,
            } {
                send(&tx, thread, Update::Control(command.into()));
            }
        }
        for event in hook_rx.try_iter() {
            match event {
                crate::hooks::Event::Control(command) => {
                    send(&tx, thread, Update::Control(command.into()))
                }
                crate::hooks::Event::Foreground => {
                    let _ = work_tx.try_send(Work::RemoteChanged);
                    for m in &model.monitors {
                        if let Some(bar) = bars.get_mut(&m.name) {
                            let mut bounds = m.bounds;
                            bounds.h = (40. * m.scale).round_ties_even() as i32;
                            bar.position(bounds, shadow, hidden);
                        }
                    }
                }
            }
        }
        for update in rx.try_iter() {
            match update {
                Update::Theme(palette) => {
                    for bar in bars.values() {
                        bar.window.state.borrow_mut().palette = palette;
                        bar.window.invalidate();
                    }
                    for w in [&popup, &child_popup, &preview_popup, &battery_popup]
                        .into_iter()
                        .flatten()
                    {
                        w.state.borrow_mut().palette = palette;
                        w.invalidate();
                    }
                }
                Update::Control(name) => match name.as_str() {
                    "Wallpaper" => {
                        wallpaper_worker.cancel();
                        if popup
                            .as_ref()
                            .is_some_and(|w| matches!(w.state.borrow().scene, Scene::Wallpaper(_)))
                        {
                            popup = None;
                        } else {
                            match crate::wallpaper_ui::Picker::load(&root) {
                                Ok(p) => {
                                    let (bounds, scale) =
                                        crate::native::centered_bounds(1000., 590.);
                                    match Window::new(
                                        "GlazeBar · 배경화면",
                                        bounds,
                                        make_state(&root, scale, Scene::Wallpaper(p), false, &ntx),
                                        null_mut(),
                                    ) {
                                        Ok(w) => {
                                            if let Scene::Wallpaper(p) =
                                                &mut w.state.borrow_mut().scene
                                            {
                                                wallpaper_worker.request(w.hwnd as usize, p);
                                            }
                                            w.show();
                                            popup = Some(w);
                                        }
                                        Err(e) => {
                                            result = Err(e);
                                            break 'main;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = std::fs::write(
                                        root.join("rust-runtime-error.log"),
                                        e.to_string(),
                                    );
                                }
                            }
                        }
                    }
                    "Sheet" => {
                        if popup
                            .as_ref()
                            .is_some_and(|w| matches!(w.state.borrow().scene, Scene::Sheet(_)))
                        {
                            popup = None;
                        } else {
                            match std::fs::read_to_string(root.join("cheatsheet.tsv")) {
                                Ok(text) => {
                                    let (bounds, scale) =
                                        crate::native::centered_bounds(940., 850.);
                                    match Window::new(
                                        "GlazeWM - Shortcut Cheat Sheet",
                                        bounds,
                                        make_state(
                                            &root,
                                            scale,
                                            Scene::Sheet(crate::sheet::Sheet::parse(&text)),
                                            false,
                                            &ntx,
                                        ),
                                        null_mut(),
                                    ) {
                                        Ok(w) => {
                                            w.show();
                                            popup = Some(w)
                                        }
                                        Err(e) => {
                                            result = Err(e);
                                            break 'main;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = std::fs::write(
                                        root.join("rust-runtime-error.log"),
                                        e.to_string(),
                                    );
                                }
                            }
                        }
                    }
                    "Toggle" => {
                        let _ = work_tx.try_send(Work::ToggleGroup);
                    }
                    "FocusResync" => {
                        let _ = work_tx.try_send(Work::Resync);
                    }
                    "ClosePopups" => {
                        child_popup = None;
                        wallpaper_worker.cancel();
                        popup = None;
                        battery_popup = None;
                    }
                    "Snapshot" => {
                        for (i, bar) in bars.values().enumerate() {
                            if let Err(e) = bar
                                .window
                                .snapshot(&root.join(format!("rust-live-bar-{i}.png")))
                            {
                                let _ = std::fs::write(
                                    root.join("rust-runtime-error.log"),
                                    e.to_string(),
                                );
                            }
                        }
                        if let Some(w) = &popup {
                            let _ = w.snapshot(&root.join("rust-live-popup.png"));
                        }
                    }
                    "Bar" => {
                        let path = root.join("bar-hidden.txt");
                        let result = if hidden {
                            std::fs::remove_file(path)
                        } else {
                            std::fs::write(path, "'0px'")
                        };
                        match result {
                            Ok(()) => {
                                hidden = !hidden;
                                for m in &model.monitors {
                                    if let Some(bar) = bars.get_mut(&m.name) {
                                        let mut bounds = m.bounds;
                                        bounds.h = (40. * m.scale).round_ties_even() as i32;
                                        bar.position(bounds, shadow, hidden);
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = std::fs::write(
                                    root.join("rust-runtime-error.log"),
                                    e.to_string(),
                                );
                            }
                        }
                    }
                    _ => {
                        let _ = std::fs::write(
                            root.join("rust-runtime-error.log"),
                            format!("Unported control: {name}"),
                        );
                    }
                },
                Update::ThemeFinished(hwnd, operation, restored, outcome) => {
                    if let Some(job) = theme_job.take() {
                        let _ = job.join();
                    }
                    let palette = Palette::load(&root);
                    for bar in bars.values() {
                        bar.window.state.borrow_mut().palette = palette;
                        bar.window.invalidate();
                    }
                    if let Some(w) = &battery_popup {
                        w.state.borrow_mut().palette = palette;
                        w.invalidate();
                    }
                    if let Some(w) = &popup {
                        let mut state = w.state.borrow_mut();
                        state.palette = palette;
                        if let Scene::Wallpaper(p) = &mut state.scene {
                            if w.hwnd as usize == hwnd && p.operation == operation {
                                p.busy = false;
                                p.status = match &outcome {
                                    Ok(()) => {
                                        if restored {
                                            "이전 색상을 복원했습니다".into()
                                        } else {
                                            "적용했습니다".into()
                                        }
                                    }
                                    Err(e) => e.clone(),
                                };
                            }
                        }
                        w.invalidate();
                    }
                    if let Err(e) = outcome {
                        let _ = std::fs::write(root.join("rust-runtime-error.log"), e);
                    }
                }
                Update::Comparison(hwnd, revision, result) => {
                    if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                        if let Scene::Compare(v) = &mut w.state.borrow_mut().scene {
                            if v.revision == revision {
                                match result {
                                    Ok(palettes) => {
                                        v.palettes = palettes.map(Some);
                                        v.status.clear();
                                    }
                                    Err(e) => v.status = e,
                                }
                                w.invalidate();
                            }
                        }
                    }
                }
                Update::Online(hwnd, revision, item) => {
                    let mut downloaded = None;
                    if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                        if let Scene::Online(v) = &mut w.state.borrow_mut().scene {
                            if v.revision == revision {
                                match item {
                                    crate::online_ui::Item::Rows(result) => {
                                        v.busy = false;
                                        v.status.clear();
                                        match result {
                                            Ok(result) => {
                                                v.rows = result.wallpapers;
                                                v.last =
                                                    result.last_page.min(i32::MAX as u32) as i32;
                                                v.page = result.page.min(i32::MAX as u32) as i32;
                                                v.selected = 0;
                                                v.thumbs.clear();
                                            }
                                            Err(e) => v.status = e,
                                        }
                                    }
                                    crate::online_ui::Item::Thumb(id, image) => {
                                        v.thumbs.insert(id, image);
                                    }
                                    crate::online_ui::Item::Download(result) => {
                                        v.busy = false;
                                        match result {
                                            Ok(path) => downloaded = Some(path),
                                            Err(e) => v.status = e,
                                        }
                                    }
                                }
                                w.invalidate();
                            }
                        }
                    }
                    if let Some(path) = downloaded {
                        if let Some(owner) = &popup {
                            if let Scene::Wallpaper(p) = &mut owner.state.borrow_mut().scene {
                                match crate::wallpaper_ui::Picker::load(&root) {
                                    Ok(loaded) => {
                                        p.files = loaded.files;
                                        p.selected =
                                            p.files.iter().position(|f| f == &path).unwrap_or(0);
                                        p.palette = None;
                                        wallpaper_worker.request(owner.hwnd as usize, p);
                                    }
                                    Err(e) => p.status = e.to_string(),
                                }
                            }
                            owner.invalidate();
                            owner.show();
                        }
                        child_popup = None;
                    }
                }
                Update::Precache(hwnd, progress) => {
                    if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                        if let Scene::Precache(p) = &mut w.state.borrow_mut().scene {
                            if p.revision == progress.revision {
                                *p = progress;
                                w.invalidate();
                            }
                        }
                    }
                }
                Update::Wallpaper(hwnd, revision, item) => {
                    if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                        if let Scene::Wallpaper(p) = &mut w.state.borrow_mut().scene {
                            if p.revision == revision {
                                match item {
                                    crate::wallpaper_worker::Item::Image(path, im) => {
                                        p.loaded.insert(path.clone());
                                        if let Some(im) = im {
                                            p.images.insert(path, im);
                                        }
                                    }
                                    crate::wallpaper_worker::Item::Palette(result) => {
                                        match result {
                                            Ok(palette) => {
                                                p.palette = Some(palette);
                                                if !p.busy && p.status == "Aether 색상 추출 중…"
                                                {
                                                    p.status.clear();
                                                }
                                            }
                                            Err(e) => {
                                                if !p.busy {
                                                    p.status = e;
                                                }
                                            }
                                        }
                                    }
                                }
                                w.invalidate();
                            }
                        }
                    }
                }
                Update::Weather(value) => {
                    if weather.as_ref() != Some(&value) {
                        weather = Some(value.clone());
                        for bar in bars.values() {
                            if let Scene::Bar { weather, .. } =
                                &mut bar.window.state.borrow_mut().scene
                            {
                                *weather = Some((value.icon.clone(), value.text.clone()));
                            }
                            bar.window.invalidate();
                        }
                    }
                }
                Update::Brightness(name, value, revision, error) => {
                    let entry = brightness_cache.entry(name.clone()).or_insert((None, 0));
                    if revision >= entry.1 {
                        *entry = (value, revision);
                        if let Some(bar) = bars.get(&name) {
                            if let Scene::Bar { brightness, .. } =
                                &mut bar.window.state.borrow_mut().scene
                            {
                                *brightness = value;
                            }
                            bar.window.invalidate();
                        }
                        if let Some(error) = error {
                            let _ = std::fs::write(root.join("rust-brightness-error.log"), error);
                        }
                    }
                }
                Update::WifiRows(hwnd, token, rows) => {
                    if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                        let mut changed = false;
                        {
                            let mut s = w.state.borrow_mut();
                            if let Scene::Wifi(v) = &mut s.scene {
                                if v.token == token {
                                    match rows {
                                        Ok(rows) => v.update(rows),
                                        Err(e) => {
                                            v.loading = false;
                                            v.status = e;
                                        }
                                    }
                                    changed = true;
                                }
                            }
                        }
                        if changed {
                            w.wifi_layout(false);
                        }
                    }
                }
                Update::WifiConnected(hwnd, token, outcome) => {
                    if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                        let mut changed = false;
                        {
                            let mut s = w.state.borrow_mut();
                            if let Scene::Wifi(v) = &mut s.scene {
                                if v.token == token {
                                    v.connecting = false;
                                    v.status = outcome.err().unwrap_or_default();
                                    changed = true;
                                }
                            }
                        }
                        if changed {
                            w.wifi_layout(false);
                            let _ = ntx.send(NativeEvent::Action(hwnd, Action::WifiScan(false)));
                        }
                    }
                }
                Update::Stop => break 'main,
                Update::Error(e) => {
                    let _ = std::fs::write(root.join("rust-runtime-error.log"), e);
                }
                Update::Sensors(s) => {
                    sensors = s;
                    if let Some(w) = &battery_popup {
                        w.state.borrow_mut().scene = Scene::Battery(sensors.clone());
                        let mut bounds = w.rect();
                        bounds.h = ((if sensors.discharging && !sensors.charging {
                            110.
                        } else {
                            132.
                        }) * w.state.borrow().scale)
                            .round_ties_even() as i32;
                        w.bounds(bounds);
                        w.invalidate();
                    }
                    for bar in bars.values() {
                        if let Scene::Bar {
                            sensors: current, ..
                        } = &mut bar.window.state.borrow_mut().scene
                        {
                            *current = sensors.clone();
                        }
                        bar.window.invalidate();
                    }
                }
                Update::Model(next) => {
                    if next != model {
                        let old_names = model
                            .monitors
                            .iter()
                            .map(|m| m.name.clone())
                            .collect::<Vec<_>>();
                        let new_names = next
                            .monitors
                            .iter()
                            .map(|m| m.name.clone())
                            .collect::<Vec<_>>();
                        if old_names != new_names {
                            brightness_worker.devices(new_names);
                        }
                        model = next;
                        bars.retain(|name, _| model.monitors.iter().any(|m| &m.name == name));
                        for m in &model.monitors {
                            let mut bounds = m.bounds;
                            bounds.h = (40. * m.scale).round_ties_even() as i32;
                            if shadow {
                                bounds.y += bounds.h + 10;
                            }
                            if let Some(bar) = bars.get_mut(&m.name) {
                                let mut new_scene = scene(m, &model, &sensors);
                                if let Scene::Bar {
                                    weather: current, ..
                                } = &mut new_scene
                                {
                                    *current =
                                        weather.as_ref().map(|w| (w.icon.clone(), w.text.clone()));
                                }
                                if let Scene::Bar { brightness, .. } = &mut new_scene {
                                    *brightness = brightness_cache.get(&m.name).and_then(|v| v.0);
                                }
                                bar.window.state.borrow_mut().scene = new_scene;
                                bar.window.state.borrow_mut().scale = m.scale;
                                bar.position(bounds, shadow, hidden);
                            } else {
                                match Window::new(
                                    &format!("GlazeBar Rust - {}", m.name),
                                    bounds,
                                    make_state(
                                        &root,
                                        m.scale,
                                        scene(m, &model, &sensors),
                                        true,
                                        &ntx,
                                    ),
                                    null_mut(),
                                ) {
                                    Ok(window) => {
                                        if let Scene::Bar {
                                            weather: current,
                                            brightness,
                                            ..
                                        } = &mut window.state.borrow_mut().scene
                                        {
                                            *current = weather
                                                .as_ref()
                                                .map(|w| (w.icon.clone(), w.text.clone()));
                                            *brightness =
                                                brightness_cache.get(&m.name).and_then(|v| v.0);
                                        }
                                        let mut bar = Bar {
                                            window,
                                            registered: false,
                                        };
                                        bar.position(bounds, shadow, hidden);
                                        bars.insert(m.name.clone(), bar);
                                    }
                                    Err(e) => {
                                        result = Err(e);
                                        break 'main;
                                    }
                                }
                            }
                        }
                        let _=std::fs::write(root.join(if shadow{"rust-shadow-ready.json"}else{"rust-ready.json"}),serde_json::to_vec(&serde_json::json!({"pid":std::process::id(),"monitors":bars.len(),"shadow":shadow})).unwrap());
                    }
                }
            }
        }
        if !bars.is_empty() {
            if let Some(action) = initial.take() {
                let _ = ntx.send(NativeEvent::Action(
                    bars.values().next().unwrap().window.hwnd as usize,
                    action,
                ));
            }
        }
        for event in nrx.try_iter() {
            match event {
                NativeEvent::TaskbarCreated(hwnd) => {
                    if let Some((name, bar)) = bars
                        .iter_mut()
                        .find(|(_, bar)| bar.window.hwnd as usize == hwnd)
                    {
                        bar.registered = false;
                        if let Some(m) = model.monitors.iter().find(|m| &m.name == name) {
                            let mut bounds = m.bounds;
                            bounds.h = (40. * m.scale).round_ties_even() as i32;
                            bar.position(bounds, shadow, hidden);
                        }
                    }
                }
                NativeEvent::BrightnessSet(hwnd, value) => {
                    if let Some((name, bar)) = bars
                        .iter()
                        .find(|(_, bar)| bar.window.hwnd as usize == hwnd)
                    {
                        let entry = brightness_cache.entry(name.clone()).or_insert((None, 0));
                        if entry.0.is_some() && entry.0 != Some(value) {
                            entry.0 = Some(value);
                            entry.1 += 1;
                            brightness_worker.set(name.clone(), value, entry.1);
                            if let Scene::Bar { brightness, .. } =
                                &mut bar.window.state.borrow_mut().scene
                            {
                                *brightness = Some(value);
                            }
                            bar.window.invalidate();
                        }
                    }
                }
                NativeEvent::Action(hwnd, action) => match action {
                    Action::Picker(crate::wallpaper_ui::Action::Precache) => {
                        if let Some(owner) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let files = if let Scene::Wallpaper(p) = &owner.state.borrow().scene {
                                if !p.busy && !p.files.is_empty() {
                                    Some(p.files.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            if let Some(files) = files {
                                child_popup = None;
                                let (bounds, scale) = crate::native::centered_bounds(560., 310.);
                                match Window::new(
                                    "GlazeBar · 전체 캐시 생성",
                                    bounds,
                                    make_state(
                                        &root,
                                        scale,
                                        Scene::Precache(crate::precache::Progress::new(
                                            files.len(),
                                        )),
                                        false,
                                        &ntx,
                                    ),
                                    owner.hwnd,
                                ) {
                                    Ok(w) => {
                                        if let Scene::Precache(p) = &mut w.state.borrow_mut().scene
                                        {
                                            cache_worker.request(w.hwnd as usize, files, p);
                                            cache_active = true;
                                        }
                                        w.show();
                                        child_popup = Some(w);
                                    }
                                    Err(e) => {
                                        let _ = std::fs::write(
                                            root.join("rust-runtime-error.log"),
                                            e.to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Action::PrecacheStop => {
                        let mut close = false;
                        if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Precache(p) = &mut w.state.borrow_mut().scene {
                                if p.running {
                                    cache_worker.stop();
                                    p.current = "현재 사진 처리 후 중지합니다…".into();
                                    w.invalidate();
                                } else {
                                    close = true;
                                }
                            }
                        }
                        if close {
                            child_popup = None;
                            if let Some(w) = &popup {
                                w.show();
                            }
                        }
                    }
                    Action::Picker(crate::wallpaper_ui::Action::Online) => {
                        if let Some(owner) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let available = matches!(&owner.state.borrow().scene,Scene::Wallpaper(p) if !p.busy);
                            if available {
                                child_popup = None;
                                let (bounds, scale) = crate::native::centered_bounds(1000., 650.);
                                match Window::new(
                                    "GlazeBar · Wallhaven",
                                    bounds,
                                    make_state(
                                        &root,
                                        scale,
                                        Scene::Online(Default::default()),
                                        false,
                                        &ntx,
                                    ),
                                    owner.hwnd,
                                ) {
                                    Ok(w) => {
                                        w.online_layout();
                                        if let Scene::Online(v) = &mut w.state.borrow_mut().scene {
                                            online_worker.search(w.hwnd as usize, v, String::new());
                                        }
                                        online_active = true;
                                        w.show();
                                        child_popup = Some(w);
                                    }
                                    Err(e) => {
                                        let _ = std::fs::write(
                                            root.join("rust-runtime-error.log"),
                                            e.to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Action::Online(edit) => {
                        if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let query = w.edit_text(101);
                            if let Scene::Online(v) = &mut w.state.borrow_mut().scene {
                                match edit {
                                    crate::online_ui::Edit::Select(i) => {
                                        v.selected = i.min(v.rows.len().saturating_sub(1));
                                    }
                                    crate::online_ui::Edit::Search(reset) if !v.busy => {
                                        if reset {
                                            v.page = 1;
                                        }
                                        online_worker.search(hwnd, v, query);
                                    }
                                    crate::online_ui::Edit::Page(delta) if !v.busy => {
                                        let next = v.page.saturating_add(delta);
                                        if next >= 1 && next <= v.last {
                                            v.page = next;
                                            online_worker.search(hwnd, v, query);
                                        }
                                    }
                                    crate::online_ui::Edit::Download if !v.busy => {
                                        if let Some(home) = std::env::var_os("USERPROFILE") {
                                            online_worker.download(
                                                hwnd,
                                                v,
                                                crate::wallpaper::folder(
                                                    &root,
                                                    &std::path::PathBuf::from(home),
                                                ),
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            w.invalidate();
                        }
                    }
                    Action::Picker(crate::wallpaper_ui::Action::Compare) => {
                        if let Some(owner) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let comparison =
                                if let Scene::Wallpaper(p) = &owner.state.borrow().scene {
                                    if p.busy {
                                        None
                                    } else {
                                        p.files.get(p.selected).map(|image| {
                                            crate::style_compare_ui::Comparison::new(
                                                image.clone(),
                                                p.mode,
                                                p.light,
                                            )
                                        })
                                    }
                                } else {
                                    None
                                };
                            if let Some(comparison) = comparison {
                                child_popup = None;
                                let (bounds, scale) = crate::native::centered_bounds(1120., 680.);
                                match Window::new(
                                    "GlazeBar · 스타일 비교",
                                    bounds,
                                    make_state(
                                        &root,
                                        scale,
                                        Scene::Compare(comparison),
                                        false,
                                        &ntx,
                                    ),
                                    owner.hwnd,
                                ) {
                                    Ok(w) => {
                                        if let Scene::Compare(v) = &mut w.state.borrow_mut().scene {
                                            compare_worker.request(w.hwnd as usize, v);
                                            compare_active = true;
                                        }
                                        w.show();
                                        child_popup = Some(w);
                                    }
                                    Err(e) => {
                                        let _ = std::fs::write(
                                            root.join("rust-runtime-error.log"),
                                            e.to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Action::Compare(edit) => {
                        let mut choice = None;
                        if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Compare(v) = &mut w.state.borrow_mut().scene {
                                match edit {
                                    crate::style_compare_ui::Edit::Choose(index) => {
                                        if v.palettes.get(index).is_some_and(Option::is_some) {
                                            choice = Some((index, v.light));
                                        }
                                    }
                                    crate::style_compare_ui::Edit::Light(light) => {
                                        if v.light != light {
                                            v.light = light;
                                            compare_worker.request(hwnd, v);
                                        }
                                    }
                                }
                                w.invalidate();
                            }
                        }
                        if let Some((index, light)) = choice {
                            if let Some(owner) = &popup {
                                if let Scene::Wallpaper(p) = &mut owner.state.borrow_mut().scene {
                                    p.mode = index;
                                    p.light = light;
                                    wallpaper_worker.request(owner.hwnd as usize, p);
                                }
                                owner.invalidate();
                            }
                            child_popup = None;
                            if let Some(owner) = &popup {
                                owner.show();
                            }
                        }
                    }
                    Action::Picker(crate::wallpaper_ui::Action::Windows) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let settings = if let Scene::Wallpaper(p) = &w.state.borrow().scene {
                                if p.busy {
                                    None
                                } else {
                                    Some(crate::windows_options_ui::Settings {
                                        enabled: p.windows,
                                        options: p.options.clone(),
                                        light: p.light,
                                        status: String::new(),
                                    })
                                }
                            } else {
                                None
                            };
                            if let Some(settings) = settings {
                                let (bounds, scale) = crate::native::centered_bounds(510., 435.);
                                child_popup = None;
                                match Window::new(
                                    "Windows 시스템 연동",
                                    bounds,
                                    make_state(
                                        &root,
                                        scale,
                                        Scene::WindowsOptions(settings),
                                        false,
                                        &ntx,
                                    ),
                                    w.hwnd,
                                ) {
                                    Ok(child) => {
                                        child.show();
                                        child_popup = Some(child);
                                    }
                                    Err(e) => {
                                        let _ = std::fs::write(
                                            root.join("rust-runtime-error.log"),
                                            e.to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Action::WindowsOption(edit) => {
                        let mut selected = None;
                        if let Some(w) = child_popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::WindowsOptions(v) = &mut w.state.borrow_mut().scene {
                                if edit == crate::windows_options_ui::Edit::Save {
                                    match v.save(&root) {
                                        Ok(()) => selected = Some(v.clone()),
                                        Err(e) => v.status = e.to_string(),
                                    }
                                } else {
                                    v.change(&edit);
                                }
                                w.invalidate();
                            }
                        }
                        if let Some(selected) = selected {
                            if let Some(w) = &popup {
                                if let Scene::Wallpaper(p) = &mut w.state.borrow_mut().scene {
                                    p.windows = selected.enabled;
                                    p.options = selected.options;
                                }
                                w.invalidate();
                            }
                            child_popup = None;
                            if let Some(w) = &popup {
                                w.show();
                            }
                        }
                    }
                    Action::Wallpaper => send(&tx, thread, Update::Control("Wallpaper".into())),
                    Action::Picker(crate::wallpaper_ui::Action::Folder) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let allowed =
                                matches!(&w.state.borrow().scene,Scene::Wallpaper(p) if !p.busy);
                            if allowed {
                                // No RefCell borrow may span a native modal dialog's message pump.
                                let selection = crate::wallpaper::choose_folder(w.hwnd);
                                let result = selection.and_then(|path| {
                                    if let Some(path) = path {
                                        let text = path.to_str().ok_or_else(|| {
                                            anyhow::anyhow!("폴더 경로를 저장할 수 없습니다")
                                        })?;
                                        crate::theme_transaction::atomic_write(
                                            &root.join("wallpaper-folder.txt"),
                                            text.as_bytes(),
                                        )?;
                                        Ok(Some(crate::wallpaper_ui::Picker::load(&root)?))
                                    } else {
                                        Ok(None)
                                    }
                                });
                                if let Scene::Wallpaper(p) = &mut w.state.borrow_mut().scene {
                                    match result {
                                        Ok(Some(next)) => {
                                            p.files = next.files;
                                            p.selected = next.selected;
                                            p.images.clear();
                                            p.loaded.clear();
                                            wallpaper_worker.request(hwnd, p);
                                        }
                                        Ok(None) => {}
                                        Err(e) => p.status = e.to_string(),
                                    }
                                }
                                w.show();
                                w.invalidate();
                            }
                        }
                    }
                    Action::Picker(
                        action @ (crate::wallpaper_ui::Action::Apply
                        | crate::wallpaper_ui::Action::Restore),
                    ) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Wallpaper(p) = &mut w.state.borrow_mut().scene {
                                if !p.busy {
                                    if theme_job.is_some() {
                                        p.status = "다른 색상 적용 작업이 진행 중입니다".into();
                                    } else {
                                        let restored =
                                            action == crate::wallpaper_ui::Action::Restore;
                                        let prepared = if restored {
                                            Ok(None)
                                        } else {
                                            crate::wallpaper_actions::prepare(p).map(Some)
                                        };
                                        let home = std::env::var_os("USERPROFILE")
                                            .map(PathBuf::from)
                                            .ok_or_else(|| anyhow::anyhow!("USERPROFILE missing"));
                                        match prepared.and_then(|prepared| Ok((prepared, home?))) {
                                            Err(e) => p.status = e.to_string(),
                                            Ok((prepared, home)) => {
                                                p.busy = true;
                                                p.status = if restored {
                                                    "복원 중…".into()
                                                } else {
                                                    "적용 중…".into()
                                                };
                                                theme_operation += 1;
                                                p.operation = theme_operation;
                                                let operation = theme_operation;
                                                let tx = tx.clone();
                                                theme_job = Some(std::thread::spawn(move || {
                                                    let result = match prepared {
                                                        Some((path, request)) => {
                                                            crate::wallpaper_actions::apply(
                                                                &home,
                                                                &path,
                                                                request.as_ref(),
                                                            )
                                                        }
                                                        None => {
                                                            crate::wallpaper_actions::restore(&home)
                                                        }
                                                    }
                                                    .map_err(|e| e.to_string());
                                                    send(
                                                        &tx,
                                                        thread,
                                                        Update::ThemeFinished(
                                                            hwnd, operation, restored, result,
                                                        ),
                                                    );
                                                }));
                                            }
                                        }
                                    }
                                    w.invalidate();
                                }
                            }
                        }
                    }
                    Action::Picker(action) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Wallpaper(p) = &mut w.state.borrow_mut().scene {
                                if !p.busy {
                                    use crate::wallpaper_ui::Action as P;
                                    let mut extract = true;
                                    match action {
                                        P::Select(delta) => p.select(delta),
                                        P::Enabled => p.enabled = !p.enabled,
                                        P::Mode => {
                                            p.mode = (p.mode + 1) % crate::wallpaper_ui::MODES.len()
                                        }
                                        P::Light(light) => p.light = light,
                                        P::Target(i) => {
                                            if let Some(v) = p.targets.get_mut(i) {
                                                *v = !*v;
                                            }
                                            extract = false;
                                        }
                                        _ => {
                                            p.status = format!("Rust 이식 진행 중: {action:?}");
                                            extract = false;
                                        }
                                    }
                                    if extract {
                                        wallpaper_worker.request(hwnd, p);
                                    }
                                    w.invalidate();
                                }
                            }
                        }
                    }
                    Action::Sheet => send(&tx, thread, Update::Control("Sheet".into())),
                    Action::Wifi => {
                        if let Some(bar) = bars.values().find(|b| b.window.hwnd as usize == hwnd) {
                            let factor = bar.window.state.borrow().scale;
                            let br = bar.window.rect();
                            let v = crate::wifi_ui::Wifi::new();
                            let width = (420. * factor).round_ties_even() as i32;
                            let height = (v.height() * factor).round_ties_even() as i32;
                            let bounds = Rect::new(
                                br.x + br.w - width - (18. * factor).round_ties_even() as i32,
                                br.y + br.h + (8. * factor).round_ties_even() as i32,
                                width,
                                height,
                            );
                            match Window::new(
                                "GlazeBar Rust · Wi-Fi",
                                bounds,
                                make_state(&root, factor, Scene::Wifi(v), false, &ntx),
                                bar.window.hwnd,
                            ) {
                                Ok(w) => {
                                    w.wifi_layout(true);
                                    w.show();
                                    unsafe {
                                        SetTimer(w.hwnd, 2, 5000, None);
                                    }
                                    let _ = ntx.send(NativeEvent::Action(
                                        w.hwnd as usize,
                                        Action::WifiScan(true),
                                    ));
                                    popup = Some(w);
                                }
                                Err(e) => {
                                    result = Err(e);
                                    break 'main;
                                }
                            }
                        }
                    }
                    Action::WifiSelect(index) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Wifi(v) = &mut w.state.borrow_mut().scene {
                                v.select(index);
                            }
                            w.wifi_layout(true);
                        }
                    }
                    Action::WifiScroll(delta) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Wifi(v) = &mut w.state.borrow_mut().scene {
                                v.offset = if delta > 0 {
                                    v.offset.saturating_sub(3)
                                } else {
                                    (v.offset + 3).min(v.rows.len().saturating_sub(6))
                                };
                            }
                            w.invalidate();
                        }
                    }
                    Action::WifiSettings => {
                        unsafe {
                            ShellExecuteW(
                                hwnd as HWND,
                                null(),
                                wide("ms-settings:network-wifi").as_ptr(),
                                null(),
                                null(),
                                SW_SHOWNORMAL,
                            );
                        }
                        popup = None;
                    }
                    Action::WifiScan(trigger) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let token = {
                                let mut s = w.state.borrow_mut();
                                if let Scene::Wifi(v) = &mut s.scene {
                                    if v.loading || v.connecting {
                                        None
                                    } else {
                                        v.loading = true;
                                        Some(v.token)
                                    }
                                } else {
                                    None
                                }
                            };
                            if let Some(token) = token {
                                w.invalidate();
                                let tx = tx.clone();
                                let stop = stop.clone();
                                wifi_jobs.push(std::thread::spawn(move || {
                                    let rows = (|| -> Result<Vec<crate::wifi::Network>> {
                                        let rows = crate::wifi::scan(trigger)?;
                                        if trigger {
                                            for _ in 0..10 {
                                                if stop.load(Ordering::Relaxed) {
                                                    anyhow::bail!("검색 취소")
                                                }
                                                std::thread::sleep(Duration::from_millis(100));
                                            }
                                            crate::wifi::scan(false)
                                        } else {
                                            Ok(rows)
                                        }
                                    })()
                                    .map_err(|e| e.to_string());
                                    send(&tx, thread, Update::WifiRows(hwnd, token, rows));
                                }));
                            }
                        }
                    }
                    Action::WifiConnect => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            let secret = w.wifi_secret();
                            let selection = {
                                let mut s = w.state.borrow_mut();
                                if let Scene::Wifi(v) = &mut s.scene {
                                    if v.can_connect() {
                                        let n = v.rows[v.selected].clone();
                                        match crate::wifi::profile(&n, &secret) {
                                            Ok(_) => {
                                                v.connecting = true;
                                                v.status.clear();
                                                Some((v.token, n))
                                            }
                                            Err(e) => {
                                                v.status = e.to_string();
                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            };
                            w.wifi_layout(selection.is_some());
                            if let Some((token, n)) = selection {
                                let tx = tx.clone();
                                let stop = stop.clone();
                                wifi_jobs.push(std::thread::spawn(move || {
                                    let result = crate::wifi::connect(&n, &secret, &stop)
                                        .map_err(|e| e.to_string());
                                    send(&tx, thread, Update::WifiConnected(hwnd, token, result));
                                }));
                            }
                        }
                    }
                    Action::Close => {
                        if child_popup
                            .as_ref()
                            .is_some_and(|p| p.hwnd as usize == hwnd)
                        {
                            child_popup = None;
                            if let Some(w) = &popup {
                                w.show();
                            }
                        }
                        if popup.as_ref().is_some_and(|p| p.hwnd as usize == hwnd) {
                            child_popup = None;
                            wallpaper_worker.cancel();
                            popup = None;
                        }
                    }
                    Action::Command(command) => {
                        let _ = work_tx.try_send(Work::Command(command));
                    }
                    Action::Power => {
                        if let Some(bar) = bars.values().find(|b| b.window.hwnd as usize == hwnd) {
                            match crate::power::read() {
                                Ok(profiles) => {
                                    let scale = bar.window.state.borrow().scale;
                                    let br = bar.window.rect();
                                    let width = (240. * scale).round_ties_even() as i32;
                                    let height = (136. * scale).round_ties_even() as i32;
                                    let bounds = Rect::new(
                                        br.x + br.w
                                            - width
                                            - (18. * scale).round_ties_even() as i32,
                                        br.y + br.h + (8. * scale).round_ties_even() as i32,
                                        width,
                                        height,
                                    );
                                    match Window::new(
                                        "GlazeBar Rust · 전원",
                                        bounds,
                                        make_state(
                                            &root,
                                            scale,
                                            Scene::Power(profiles),
                                            false,
                                            &ntx,
                                        ),
                                        bar.window.hwnd,
                                    ) {
                                        Ok(w) => {
                                            w.show();
                                            popup = Some(w)
                                        }
                                        Err(e) => {
                                            result = Err(e);
                                            break 'main;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = std::fs::write(
                                        root.join("rust-runtime-error.log"),
                                        e.to_string(),
                                    );
                                }
                            }
                        }
                    }
                    Action::PowerSelect(id) => {
                        if let Some(w) = popup.as_ref().filter(|w| w.hwnd as usize == hwnd) {
                            if let Scene::Power(profiles) = &mut w.state.borrow_mut().scene {
                                match crate::power::apply(&id) {
                                    Ok(()) => {
                                        profiles.active = id;
                                        profiles.error.clear()
                                    }
                                    Err(e) => profiles.error = e.to_string(),
                                }
                            }
                            w.invalidate();
                        }
                    }
                    Action::Scale => {
                        if let Some(bar) = bars.values().find(|b| b.window.hwnd as usize == hwnd) {
                            let state = bar.window.state.borrow();
                            if let Scene::Bar { monitor, .. } = &state.scene {
                                match crate::display::read(&monitor.name) {
                                    Ok(scale) => {
                                        let factor = state.scale;
                                        let icon = state
                                            .hits
                                            .iter()
                                            .find(|hit| hit.action == Action::Scale)
                                            .unwrap()
                                            .rect;
                                        let br = bar.window.rect();
                                        let width = (260. * factor).round_ties_even() as i32;
                                        let height = (112. * factor).round_ties_even() as i32;
                                        let x = (br.x + icon.x + icon.w / 2 - width / 2).clamp(
                                            monitor.bounds.x,
                                            (monitor.bounds.x + monitor.bounds.w - width)
                                                .max(monitor.bounds.x),
                                        );
                                        let bounds = Rect::new(
                                            x,
                                            br.y + br.h + (6. * factor).round_ties_even() as i32,
                                            width,
                                            height,
                                        );
                                        match Window::new(
                                            "GlazeBar Rust · 배율",
                                            bounds,
                                            make_state(
                                                &root,
                                                factor,
                                                Scene::Scale {
                                                    state: scale,
                                                    selected: scale.current,
                                                    error: String::new(),
                                                },
                                                false,
                                                &ntx,
                                            ),
                                            bar.window.hwnd,
                                        ) {
                                            Ok(w) => {
                                                w.show();
                                                popup = Some(w)
                                            }
                                            Err(e) => {
                                                result = Err(e);
                                                break 'main;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = std::fs::write(
                                            root.join("rust-runtime-error.log"),
                                            e.to_string(),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        let _ = std::fs::write(
                            root.join("rust-runtime-error.log"),
                            format!("Unported action: {action:?}"),
                        );
                    }
                },
                NativeEvent::ScaleCommit(hwnd, state, selected) => {
                    if popup.as_ref().is_some_and(|p| p.hwnd as usize == hwnd) {
                        match crate::display::write(state, selected) {
                            Ok(()) => {
                                popup = None;
                                request_refresh(&work_tx, &pending);
                            }
                            Err(e) => {
                                if let Some(w) = &popup {
                                    if let Scene::Scale { error, .. } =
                                        &mut w.state.borrow_mut().scene
                                    {
                                        *error = e.to_string()
                                    }
                                    w.invalidate();
                                }
                            }
                        }
                    }
                }
                NativeEvent::Closed(hwnd) => {
                    if child_popup
                        .as_ref()
                        .is_some_and(|p| p.hwnd as usize == hwnd)
                    {
                        child_popup = None;
                    }
                    if popup.as_ref().is_some_and(|p| p.hwnd as usize == hwnd) {
                        wallpaper_worker.cancel();
                        popup = None;
                    }
                }
                NativeEvent::DisplayChanged => request_refresh(&work_tx, &pending),
                NativeEvent::HoverBattery(hwnd, over) => {
                    if !over {
                        battery_popup = None;
                    } else if let Some(bar) = bars.values().find(|b| b.window.hwnd as usize == hwnd)
                    {
                        let scale = bar.window.state.borrow().scale;
                        let br = bar.window.rect();
                        let width = (240. * scale).round_ties_even() as i32;
                        let height = ((if sensors.discharging && !sensors.charging {
                            110.
                        } else {
                            132.
                        }) * scale)
                            .round_ties_even() as i32;
                        let bounds = Rect::new(
                            br.x + br.w - width - (18. * scale).round_ties_even() as i32,
                            br.y + br.h + (8. * scale).round_ties_even() as i32,
                            width,
                            height,
                        );
                        match Window::new(
                            "GlazeBar Rust · 배터리",
                            bounds,
                            make_state(&root, scale, Scene::Battery(sensors.clone()), true, &ntx),
                            bar.window.hwnd,
                        ) {
                            Ok(w) => {
                                w.show();
                                battery_popup = Some(w)
                            }
                            Err(e) => {
                                result = Err(e);
                                break 'main;
                            }
                        }
                    }
                }
                NativeEvent::AppBar(hwnd, event, param) => {
                    if event == ABN_POSCHANGED as usize {
                        if let Some((name, bar)) = bars
                            .iter_mut()
                            .find(|(_, b)| b.window.hwnd as usize == hwnd)
                        {
                            if let Some(m) = model.monitors.iter().find(|m| &m.name == name) {
                                let mut bounds = m.bounds;
                                bounds.h = (40. * m.scale).round_ties_even() as i32;
                                bar.position(bounds, shadow, hidden);
                            }
                        }
                    } else if event == ABN_FULLSCREENAPP as usize {
                        unsafe {
                            SetWindowPos(
                                hwnd as HWND,
                                if param != 0 {
                                    HWND_NOTOPMOST
                                } else {
                                    HWND_TOPMOST
                                },
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
            }
        }
        if msg.message == WM_TIMER && msg.wParam == timer && !shadow {
            unsafe {
                SetTimer(null_mut(), timer, 60000, None);
            }
        }
        let preview_owner = popup
            .as_ref()
            .filter(|w| matches!(w.state.borrow().scene, Scene::Wallpaper(_)));
        if let Some(owner) = preview_owner {
            let (enabled, preview, scale, palette) = {
                let s = owner.state.borrow();
                let Scene::Wallpaper(p) = &s.scene else {
                    unreachable!()
                };
                (
                    p.enabled,
                    crate::theme_preview_ui::Preview {
                        light: p.light,
                        palette: p.palette.clone(),
                    },
                    s.scale,
                    s.palette,
                )
            };
            if preview_popup.as_ref().is_some_and(|w| !w.alive()) {
                preview_popup = None;
            }
            if enabled {
                if preview_popup.is_none() {
                    let (main, bounds) = preview_bounds(owner);
                    if owner.rect() != main {
                        owner.bounds(main);
                    }
                    match Window::new(
                        "GlazeBar · 테마 미리보기",
                        bounds,
                        make_state(
                            &root,
                            scale,
                            Scene::ThemePreview(preview.clone()),
                            true,
                            &ntx,
                        ),
                        owner.hwnd,
                    ) {
                        Ok(w) => {
                            w.show();
                            preview_popup = Some(w);
                        }
                        Err(e) => {
                            let _ =
                                std::fs::write(root.join("rust-runtime-error.log"), e.to_string());
                        }
                    }
                }
                if let Some(w) = &preview_popup {
                    let changed = {
                        let mut s = w.state.borrow_mut();
                        let changed = s.palette != palette
                            || !matches!(&s.scene,Scene::ThemePreview(old) if old==&preview);
                        if changed {
                            s.palette = palette;
                            s.scene = Scene::ThemePreview(preview);
                        }
                        changed
                    };
                    if changed {
                        w.invalidate();
                    }
                    if unsafe { IsWindowVisible(w.hwnd) } == 0 {
                        w.show();
                    }
                }
            } else if let Some(w) = &preview_popup {
                if unsafe { IsWindowVisible(w.hwnd) } != 0 {
                    unsafe {
                        ShowWindow(w.hwnd, SW_HIDE);
                    }
                }
            }
        } else {
            preview_popup = None;
        }
        let clock = views::clock();
        if shadow {
            if let Some(w) = &child_popup {
                if let Scene::Online(v) = &w.state.borrow().scene {
                    let _=std::fs::write(root.join("rust-shadow-online.json"),serde_json::to_vec_pretty(&serde_json::json!({"pid":std::process::id(),"rows":v.rows.len(),"thumbs":v.thumbs.len(),"page":v.page,"last":v.last,"selected":v.selected,"busy":v.busy,"status":v.status,"revision":v.revision})).unwrap());
                }
                if let Scene::Precache(p) = &w.state.borrow().scene {
                    let _=std::fs::write(root.join("rust-shadow-precache.json"),serde_json::to_vec_pretty(&serde_json::json!({"pid":std::process::id(),"running":p.running,"created":p.created,"reused":p.reused,"failed":p.failed,"current":p.current,"images":p.images})).unwrap());
                }
                let revision = if let Scene::Compare(v) = &w.state.borrow().scene {
                    let ready = v.palettes.iter().filter(|p| p.is_some()).count();
                    let _=std::fs::write(root.join("rust-shadow-comparison.json"),serde_json::to_vec_pretty(&serde_json::json!({"pid":std::process::id(),"revision":v.revision,"ready":ready,"selected":v.selected,"light":v.light,"status":v.status})).unwrap());
                    if ready == 6 { Some(v.revision) } else { None }
                } else {
                    None
                };
                if let Some(revision) = revision {
                    if comparison_captured != Some((w.hwnd as usize, revision)) {
                        if w.snapshot(&root.join("rust-live-comparison.png")).is_ok() {
                            comparison_captured = Some((w.hwnd as usize, revision));
                        }
                    }
                }
            }
            if let Some(w) = &popup {
                let ready = {
                    let state = w.state.borrow();
                    if let Scene::Wallpaper(p) = &state.scene {
                        let _=std::fs::write(root.join("rust-shadow-wallpaper.json"),serde_json::to_vec_pretty(&serde_json::json!({"pid":std::process::id(),"files":p.files.len(),"selected":p.selected,"loaded":p.loaded.len(),"paletteReady":p.palette.is_some(),"status":p.status,"hits":state.hits.len(),"mode":crate::wallpaper_ui::MODES[p.mode],"light":p.light})).unwrap());
                        p.palette.is_some()
                            && p.neighbors().iter().all(|path| p.loaded.contains(path))
                    } else {
                        false
                    }
                };
                if ready && !wallpaper_captured {
                    wallpaper_captured = w.snapshot(&root.join("rust-live-wallpaper.png")).is_ok();
                }
                if let Scene::Wifi(v) = &w.state.borrow().scene {
                    let _=std::fs::write(root.join("rust-shadow-wifi.json"),serde_json::to_vec(&serde_json::json!({"pid":std::process::id(),"rows":v.rows.len(),"loading":v.loading,"connecting":v.connecting,"status":v.status})).unwrap());
                }
            }
        }
        if child_popup.as_ref().is_some_and(|w| !w.alive()) {
            child_popup = None;
        }
        if compare_active
            && !child_popup
                .as_ref()
                .is_some_and(|w| matches!(w.state.borrow().scene, Scene::Compare(_)))
        {
            compare_worker.cancel();
            compare_active = false;
        }
        if online_active
            && !child_popup
                .as_ref()
                .is_some_and(|w| matches!(w.state.borrow().scene, Scene::Online(_)))
        {
            online_worker.cancel();
            online_active = false;
        }
        if cache_active
            && !child_popup
                .as_ref()
                .is_some_and(|w| matches!(w.state.borrow().scene, Scene::Precache(_)))
        {
            cache_worker.stop();
            cache_active = false;
        }
        let mut job_index = 0;
        while job_index < wifi_jobs.len() {
            if wifi_jobs[job_index].is_finished() {
                let job = wifi_jobs.swap_remove(job_index);
                let _ = job.join();
            } else {
                job_index += 1;
            }
        }
        if clock != last_clock {
            last_clock = clock.clone();
            for b in bars.values() {
                if let Scene::Bar { clock: label, .. } = &mut b.window.state.borrow_mut().scene {
                    *label = clock.clone();
                }
                b.window.invalidate();
            }
        }
    }
    // Stop producers before closing windows or releasing event handles.
    if shadow {
        let _=std::fs::write(root.join("rust-shadow-summary.json"),serde_json::to_vec_pretty(&serde_json::json!({"pid":std::process::id(),"brightness":brightness_cache,"monitors":bars.len()})).unwrap());
    }
    stop.store(true, Ordering::Relaxed);
    drop(hooks);
    drop(brightness_worker);
    drop(weather_worker);
    drop(wallpaper_worker);
    drop(compare_worker);
    drop(online_worker);
    drop(cache_worker);
    if let Some(job) = theme_job {
        let _ = job.join();
    }
    unsafe {
        SetEvent(stop_event);
        KillTimer(null_mut(), timer);
    }
    sensor_thread.thread().unpark();
    let _ = work_tx.send(Work::Stop);
    let _ = event_thread.join();
    let _ = sensor_thread.join();
    let _ = observer.join();
    let _ = worker.join();
    for job in wifi_jobs {
        let _ = job.join();
    }
    unsafe {
        for handle in event_handles {
            CloseHandle(handle);
        }
    }
    drop(preview_popup);
    drop(popup);
    drop(battery_popup);
    drop(bars);
    result
}

pub fn signal(root: &std::path::Path, argument: &str) -> Result<()> {
    let name = match argument {
        "--stop" => "Stop",
        "--toggle" => "Toggle",
        "--bar" => "Bar",
        "--sheet" => "Sheet",
        "--wallpaper" => "Wallpaper",
        "--resync" => "FocusResync",
        "--snapshot" => "Snapshot",
        "--close-popups" => "ClosePopups",
        _ => anyhow::bail!("Unknown resident command"),
    };
    unsafe {
        if matches!(argument, "--sheet" | "--wallpaper") {
            if let Some(pid) = std::fs::read(root.join("rust-ready.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                .and_then(|v| v["pid"].as_u64())
                .filter(|p| *p > 0 && *p <= u32::MAX as u64)
            {
                AllowSetForegroundWindow(pid as u32);
            }
        }
        let h = OpenEventW(
            EVENT_MODIFY_STATE,
            0,
            wide(&format!("{PREFIX}{name}")).as_ptr(),
        );
        ensure!(!h.is_null(), "Resident command unavailable: {name}");
        let success = SetEvent(h);
        CloseHandle(h);
        ensure!(success != 0, "Resident command failed: {name}");
    }
    Ok(())
}
