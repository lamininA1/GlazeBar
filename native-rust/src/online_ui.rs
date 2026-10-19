use crate::{
    graphics::{Canvas, Rect},
    views::{self, Action, Hit},
    wallhaven,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Search(bool),
    Page(i32),
    Select(usize),
    Download,
}
#[derive(Clone)]
pub struct Online {
    pub rows: Vec<wallhaven::Wallpaper>,
    pub thumbs: HashMap<String, Arc<image::RgbaImage>>,
    pub page: i32,
    pub last: i32,
    pub selected: usize,
    pub revision: u64,
    pub busy: bool,
    pub status: String,
}
impl Default for Online {
    fn default() -> Self {
        Self {
            rows: vec![],
            thumbs: HashMap::new(),
            page: 1,
            last: 1,
            selected: 0,
            revision: 0,
            busy: false,
            status: String::new(),
        }
    }
}
pub fn paint(c: &mut Canvas, v: &Online, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let mut hits = vec![];
    let scale = c.scale;
    let s = |n: f64| (n * scale).round_ties_even() as i32;
    c.text(
        "Wallhaven",
        Rect::new(s(26.), s(19.), s(112.), s(30.)),
        c.ui.text,
        12.,
        false,
        0,
    );
    for (text, r, edit) in [
        (
            "검색",
            Rect::new(c.width - s(106.), s(17.), s(84.), s(32.)),
            Edit::Search(true),
        ),
        (
            "←",
            Rect::new(s(20.), c.height - s(51.), s(40.), s(30.)),
            Edit::Page(-1),
        ),
        (
            "→",
            Rect::new(s(68.), c.height - s(51.), s(40.), s(30.)),
            Edit::Page(1),
        ),
        (
            "사진 받기",
            Rect::new(c.width - s(138.), c.height - s(53.), s(116.), s(32.)),
            Edit::Download,
        ),
    ] {
        views::button(
            c,
            &mut hits,
            text,
            r,
            false,
            hover.is_some_and(|(x, y)| r.contains(x, y)),
            true,
            Action::Online(edit),
        );
    }
    let start = v.selected / 6 * 6;
    for (i, row) in v.rows.iter().enumerate().skip(start).take(6) {
        let width = (c.width - s(74.) - 17) / 3;
        let height = (width as f64 * 0.64) as i32 + s(28.);
        let x = s(21.) + (i - start) as i32 % 3 * (width + s(10.));
        let y = s(71.) + (i - start) as i32 / 3 * (height + s(10.));
        let r = Rect::new(x, y, width, height);
        c.fill(
            r,
            if i == v.selected {
                c.ui.selected
            } else {
                c.ui.surface
            },
        );
        if let Some(im) = v.thumbs.get(&row.id) {
            let area = Rect::new(x + s(2.), y + s(2.), width - s(4.), height - s(30.));
            let ratio = (area.w as f64 / im.width() as f64).min(area.h as f64 / im.height() as f64);
            let w = (im.width() as f64 * ratio) as i32;
            let h = (im.height() as f64 * ratio) as i32;
            let bgra: Vec<u8> = im.pixels().flat_map(|p| [p[2], p[1], p[0], p[3]]).collect();
            unsafe {
                windows_sys::Win32::Graphics::Gdi::SetStretchBltMode(
                    c.dc,
                    windows_sys::Win32::Graphics::Gdi::HALFTONE,
                );
            }
            c.bitmap(
                Rect::new(area.x + (area.w - w) / 2, area.y + (area.h - h) / 2, w, h),
                im.width() as i32,
                im.height() as i32,
                &bgra,
            );
        }
        c.text(
            &row.resolution,
            Rect::new(x + s(2.), y + height - s(28.), width - s(4.), s(26.)),
            c.ui.text,
            12.,
            false,
            1,
        );
        hits.push(Hit {
            rect: r,
            action: Action::Online(Edit::Select(i)),
        });
    }
    let status = if v.status.is_empty() && !v.rows.is_empty() {
        format!("{}장 · {} / {} · Wallhaven", v.rows.len(), v.page, v.last)
    } else {
        v.status.clone()
    };
    c.text(
        &status,
        Rect::new(s(126.), c.height - s(49.), c.width - s(282.), s(30.)),
        c.ui.text,
        12.,
        false,
        0,
    );
    hits
}
pub enum Item {
    Rows(Result<wallhaven::Search, String>),
    Thumb(String, Arc<image::RgbaImage>),
    Download(Result<PathBuf, String>),
}
enum Task {
    Search(String, i32),
    Download(String, PathBuf),
}
struct Request {
    hwnd: usize,
    revision: u64,
    cancel: Arc<AtomicBool>,
    task: Task,
}
pub struct Worker {
    tx: Option<mpsc::Sender<Request>>,
    cancel: Arc<AtomicBool>,
    revision: u64,
    job: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(publish: impl Fn(usize, u64, Item) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let job = thread::spawn(move || {
            while let Ok(r) = rx.recv() {
                if r.cancel.load(Ordering::Acquire) {
                    continue;
                }
                let emit = |item| {
                    if !r.cancel.load(Ordering::Acquire) {
                        publish(r.hwnd, r.revision, item)
                    }
                };
                match r.task {
                    Task::Search(query, page) => {
                        let result =
                            wallhaven::search(&query, page, &r.cancel).map_err(|e| e.to_string());
                        let rows = result
                            .as_ref()
                            .map(|v| v.wallpapers.clone())
                            .unwrap_or_default();
                        emit(Item::Rows(result));
                        for row in rows {
                            if r.cancel.load(Ordering::Acquire) {
                                break;
                            }
                            if let Ok(image) = wallhaven::thumbnail(&row.thumbs.small, &r.cancel) {
                                emit(Item::Thumb(row.id, Arc::new(image)));
                            }
                        }
                    }
                    Task::Download(id, folder) => emit(Item::Download(
                        wallhaven::download(&id, &folder, &r.cancel).map_err(|e| e.to_string()),
                    )),
                }
            }
        });
        Self {
            tx: Some(tx),
            cancel: Arc::new(AtomicBool::new(false)),
            revision: 0,
            job: Some(job),
        }
    }
    fn request(&mut self, hwnd: usize, v: &mut Online, task: Task) {
        self.cancel();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.revision += 1;
        v.revision = self.revision;
        v.busy = true;
        v.status = match &task {
            Task::Search(..) => "검색 중…",
            Task::Download(..) => "다운로드 중…",
        }
        .into();
        let _ = self.tx.as_ref().unwrap().send(Request {
            hwnd,
            revision: v.revision,
            cancel: self.cancel.clone(),
            task,
        });
    }
    pub fn search(&mut self, hwnd: usize, v: &mut Online, query: String) {
        self.request(hwnd, v, Task::Search(query, v.page));
    }
    pub fn download(&mut self, hwnd: usize, v: &mut Online, folder: PathBuf) {
        if let Some(row) = v.rows.get(v.selected) {
            self.request(hwnd, v, Task::Download(row.id.clone(), folder));
        }
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel();
        self.tx.take();
        if let Some(job) = self.job.take() {
            let _ = job.join();
        }
    }
}
