use crate::{
    graphics::{Canvas, Rect},
    views::{self, Action, Hit},
    wallhaven,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Tab(Mode),
    Heart,
    QueryChanged,
    Refresh,
    Search(bool),
    Page(i32),
    Jump,
    Batch(i32),
    Select(usize),
    Download,
    Sort,
    Range,
    Category(usize),
    Minimum,
    Favorite,
    UseFavorite(usize),
    FavoritePage,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Search,
    Cache,
    Hearts,
}
#[derive(Clone)]
pub struct Online {
    pub mode: Mode,
    pub cache: wallhaven::CacheStats,
    pub rows: Vec<wallhaven::Wallpaper>,
    pub thumbs: HashMap<String, Arc<image::RgbaImage>>,
    pub page: i32,
    pub last: i32,
    pub selected: usize,
    pub revision: u64,
    pub busy: bool,
    pub status: String,
    pub downloading: bool,
    pub active: wallhaven::SearchOptions,
    pub filters: wallhaven::Filters,
    pub requested_page: i32,
    pub total: u64,
    pub preferences: crate::online_preferences::Preferences,
    pub favorite_page: usize,
    pub notice: String,
    pub query: String,
}
impl Default for Online {
    fn default() -> Self {
        Self {
            mode: Mode::Search,
            cache: Default::default(),
            rows: vec![],
            thumbs: HashMap::new(),
            page: 1,
            last: 1,
            selected: 0,
            revision: 0,
            busy: false,
            status: String::new(),
            downloading: false,
            active: Default::default(),
            filters: Default::default(),
            requested_page: 1,
            total: 0,
            preferences: Default::default(),
            favorite_page: 0,
            notice: String::new(),
            query: String::new(),
        }
    }
}
impl Online {
    pub fn load(root: &Path) -> Self {
        match crate::online_preferences::Preferences::load(root) {
            Ok(preferences) => Self {
                query: preferences.last_search.query.clone(),
                active: preferences.last_search.clone(),
                filters: preferences.last_search.filters.clone(),
                preferences,
                ..Default::default()
            },
            Err(e) => Self {
                notice: format!("저장한 검색 설정을 읽지 못했습니다: {e}"),
                ..Default::default()
            },
        }
    }
    pub fn draft(&self, query: &str) -> wallhaven::SearchOptions {
        wallhaven::SearchOptions {
            query: query.trim().into(),
            filters: self.filters.clone(),
            seed: None,
        }
    }
    pub fn begin_search(&mut self, query: &str) {
        self.mode = Mode::Search;
        self.query = query.into();
        self.active = self.draft(query);
        self.requested_page = 1;
    }
    pub fn persist(&mut self, root: &Path) {
        self.preferences.last_search = self.active.clone();
        self.preferences.last_search.seed = None;
        if let Err(e) = self.preferences.save(root) {
            self.notice = format!("즐겨찾기 / 검색 설정 저장 실패: {e}");
        } else {
            self.notice.clear();
        }
    }
    pub fn batch(&mut self, delta: i32) {
        let last = self.rows.len().saturating_sub(1) / 6;
        let batch = (self.selected / 6) as i32;
        self.selected = (batch.saturating_add(delta).clamp(0, last as i32) as usize * 6)
            .min(self.rows.len().saturating_sub(1));
        if self.mode != Mode::Search {
            self.page = (self.selected / 24 + 1) as i32;
        }
    }
    pub fn jump(&self, text: &str) -> anyhow::Result<i32> {
        let page = text
            .trim()
            .parse::<i32>()
            .map_err(|_| anyhow::anyhow!("이동할 페이지 번호를 입력해주세요"))?;
        anyhow::ensure!(
            (1..=self.last).contains(&page),
            "페이지는 1~{} 사이로 입력해주세요",
            self.last
        );
        Ok(page)
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
    let mut button =
        |c: &mut Canvas, text: &str, r: Rect, selected: bool, enabled: bool, edit: Edit| {
            views::button(
                c,
                &mut hits,
                text,
                r,
                selected,
                hover.is_some_and(|(x, y)| r.contains(x, y)),
                enabled,
                Action::Online(edit),
            );
        };
    button(
        c,
        "검색",
        Rect::new(c.width - s(106.), s(17.), s(84.), s(32.)),
        false,
        !v.downloading,
        Edit::Search(true),
    );
    button(
        c,
        "새로고침",
        Rect::new(c.width - s(202.), s(17.), s(88.), s(32.)),
        false,
        !v.downloading,
        Edit::Refresh,
    );
    button(
        c,
        &format!("정렬 · {}", v.filters.sorting.label()),
        Rect::new(s(20.), s(62.), s(140.), s(30.)),
        false,
        !v.downloading,
        Edit::Sort,
    );
    let toplist = v.filters.sorting == wallhaven::Sorting::Toplist
        || (v.filters.sorting == wallhaven::Sorting::Auto && v.query.trim().is_empty());
    button(
        c,
        &format!("기간 · {}", wallhaven::TOP_RANGES[v.filters.top_range]),
        Rect::new(s(168.), s(62.), s(100.), s(30.)),
        false,
        toplist && !v.downloading,
        Edit::Range,
    );
    for (i, label) in ["일반", "애니", "인물"].into_iter().enumerate() {
        button(
            c,
            label,
            Rect::new(s(280. + i as f64 * 88.), s(62.), s(80.), s(30.)),
            v.filters.categories[i],
            !v.downloading,
            Edit::Category(i),
        );
    }
    button(
        c,
        &format!(
            "최소 해상도 · {}",
            if v.filters.minimum == 0 {
                "전체"
            } else {
                wallhaven::MINIMUMS[v.filters.minimum]
            }
        ),
        Rect::new(s(552.), s(62.), s(218.), s(30.)),
        false,
        !v.downloading,
        Edit::Minimum,
    );
    c.text(
        "SFW · +태그 -제외 / id:123",
        Rect::new(s(780.), s(62.), c.width - s(800.), s(30.)),
        c.ui.muted,
        11.,
        false,
        0,
    );
    let saved = v
        .preferences
        .favorites
        .iter()
        .any(|f| crate::online_preferences::same(f, &v.draft(&v.query)));
    button(
        c,
        if saved {
            "★ 저장됨"
        } else {
            "☆ 검색 저장"
        },
        Rect::new(s(20.), s(104.), s(112.), s(30.)),
        saved,
        !v.downloading,
        Edit::Favorite,
    );
    let chip = ((c.width - s(246.)) / 4).max(s(60.));
    let pages = v.preferences.favorites.len().div_ceil(4).max(1);
    let start = v.favorite_page.min(pages - 1) * 4;
    for (i, favorite) in v
        .preferences
        .favorites
        .iter()
        .enumerate()
        .skip(start)
        .take(4)
    {
        button(
            c,
            &favorite.query,
            Rect::new(
                s(142.) + (i - start) as i32 * chip,
                s(104.),
                chip - s(6.),
                s(30.),
            ),
            crate::online_preferences::same(favorite, &v.active),
            !v.downloading,
            Edit::UseFavorite(i),
        );
    }
    button(
        c,
        &format!("{}/{} →", v.favorite_page.min(pages - 1) + 1, pages),
        Rect::new(c.width - s(94.), s(104.), s(74.), s(30.)),
        false,
        pages > 1,
        Edit::FavoritePage,
    );
    let page_y = c.height - s(97.);
    for (label, x, mode) in [
        ("검색", 20., Mode::Search),
        ("캐시 보관함", 112., Mode::Cache),
        ("♥ 보관함", 260., Mode::Hearts),
    ] {
        button(
            c,
            label,
            Rect::new(
                s(x),
                s(146.),
                s(if mode == Mode::Search { 84. } else { 140. }),
                s(30.),
            ),
            v.mode == mode,
            !v.downloading,
            Edit::Tab(mode),
        );
    }
    c.text(
        &format!(
            "캐시 {:.1} MB · {}장",
            v.cache.bytes as f64 / 1048576.,
            v.cache.previews
        ),
        Rect::new(s(416.), s(146.), c.width - s(586.), s(30.)),
        c.ui.muted,
        12.,
        false,
        0,
    );
    let heart = v.rows.get(v.selected).is_some_and(|row| {
        v.preferences
            .saved_wallpapers
            .iter()
            .any(|saved| saved.id == row.id)
    });
    button(
        c,
        if heart {
            "♥ 보관 중"
        } else {
            "♡ 사진 보관"
        },
        Rect::new(c.width - s(160.), s(146.), s(140.), s(30.)),
        heart,
        !v.busy && !v.rows.is_empty(),
        Edit::Heart,
    );
    for (text, x, edit, enabled) in [
        ("←", 20., Edit::Page(-1), !v.busy && v.page > 1),
        ("→", 68., Edit::Page(1), !v.busy && v.page < v.last),
        ("이동", 252., Edit::Jump, !v.busy),
        ("이전 6장", 350., Edit::Batch(-1), v.selected >= 6),
        (
            "다음 6장",
            446.,
            Edit::Batch(1),
            v.selected / 6 < v.rows.len().saturating_sub(1) / 6,
        ),
    ] {
        button(
            c,
            text,
            Rect::new(s(x), page_y, s(if x < 100. { 40. } else { 86. }), s(30.)),
            false,
            enabled,
            edit,
        );
    }
    c.text(
        &format!("/ {} 페이지", v.last),
        Rect::new(s(540.), page_y, c.width - s(560.), s(30.)),
        c.ui.muted,
        12.,
        false,
        0,
    );
    button(
        c,
        "사진 받기",
        Rect::new(c.width - s(138.), c.height - s(53.), s(116.), s(32.)),
        false,
        !v.busy && !v.rows.is_empty(),
        Edit::Download,
    );
    drop(button);
    let start = v.selected / 6 * 6;
    let width = (c.width - s(62.)) / 3;
    let height = ((page_y - s(212.)) / 2).max(s(40.));
    for (i, row) in v.rows.iter().enumerate().skip(start).take(6) {
        let x = s(20.) + (i - start) as i32 % 3 * (width + s(10.));
        let y = s(190.) + (i - start) as i32 / 3 * (height + s(10.));
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
            &format!("{} · {}", row.id, row.resolution),
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
    let mut status = if !v.notice.is_empty() {
        v.notice.clone()
    } else if !v.status.is_empty() {
        v.status.clone()
    } else if v.rows.is_empty() {
        match v.mode {
            Mode::Search => "검색 결과가 없습니다. 다른 태그나 필터를 사용해보세요.",
            Mode::Cache => "보관된 미리보기가 없습니다. 웹에서 사진을 검색해보세요.",
            Mode::Hearts => "♡ 사진 보관을 누르면 이곳에 미리보기를 계속 보관합니다.",
        }
        .into()
    } else {
        format!(
            "{}개 · {} / {} 페이지 · {}–{} / {}장 · 휠로 넘기기",
            v.total,
            v.page,
            v.last,
            start + 1,
            (start + 6).min(v.rows.len()),
            v.rows.len()
        )
    };
    let memory: usize = v.thumbs.values().map(|image| image.len()).sum();
    if memory >= 256 << 20 && v.status.is_empty() && v.notice.is_empty() {
        status = format!(
            "미리보기 메모리 {}MB · 이미지가 많습니다. 사용하지 않는 이미지를 정리해주세요.",
            memory >> 20
        );
    }
    c.text(
        &status,
        Rect::new(s(20.), c.height - s(49.), c.width - s(180.), s(30.)),
        if memory >= 256 << 20 {
            c.ui.warning
        } else {
            c.ui.text
        },
        12.,
        false,
        0,
    );
    hits
}
pub enum Item {
    Gallery(Result<wallhaven::Gallery, String>),
    Pinned(Result<(wallhaven::Wallpaper, Arc<image::RgbaImage>), String>),
    Stats(wallhaven::CacheStats),
    Rows(Result<wallhaven::Search, String>),
    Thumb(String, Arc<image::RgbaImage>),
    Download(Result<PathBuf, String>),
}
enum Task {
    Gallery(Vec<wallhaven::Wallpaper>, bool),
    Pin(wallhaven::Wallpaper, Vec<wallhaven::Wallpaper>, bool),
    Search(wallhaven::SearchOptions, i32, bool),
    Download(String, PathBuf),
}
struct Request {
    hwnd: usize,
    revision: u64,
    cancel: Arc<AtomicBool>,
    task: Task,
}
struct ThumbRequest {
    offline: bool,
    hwnd: usize,
    revision: u64,
    cancel: Arc<AtomicBool>,
    rows: Vec<wallhaven::Wallpaper>,
}
pub struct Worker {
    tx: Option<mpsc::Sender<Request>>,
    cancel: Arc<AtomicBool>,
    revision: u64,
    job: Option<thread::JoinHandle<()>>,
    thumb_job: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(publish: impl Fn(usize, u64, Item) + Send + Sync + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let (thumb_tx, thumb_rx) = mpsc::channel::<ThumbRequest>();
        let publish = Arc::new(publish);
        let thumb_publish = publish.clone();
        let thumb_job = thread::spawn(move || {
            while let Ok(mut r) = thumb_rx.recv() {
                while let Ok(next) = thumb_rx.try_recv() {
                    r = next;
                }
                for row in r.rows {
                    if r.cancel.load(Ordering::Acquire) {
                        break;
                    }
                    let result = if r.offline {
                        wallhaven::cached_thumbnail(&row)
                    } else {
                        wallhaven::thumbnail(&row.thumbs.small, &r.cancel)
                    };
                    if let Ok(image) = result {
                        if !r.offline {
                            wallhaven::remember_preview(&row);
                        }
                        if !r.cancel.load(Ordering::Acquire) {
                            thumb_publish(r.hwnd, r.revision, Item::Thumb(row.id, Arc::new(image)));
                        }
                    }
                }
                if !r.cancel.load(Ordering::Acquire) {
                    thumb_publish(r.hwnd, r.revision, Item::Stats(wallhaven::cache_stats()));
                }
            }
        });
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
                    Task::Gallery(saved, hearts) => {
                        let gallery = wallhaven::gallery(&saved, hearts, &r.cancel)
                            .map_err(|e| e.to_string());
                        let rows = gallery.as_ref().map(|g| g.rows.clone()).unwrap_or_default();
                        emit(Item::Gallery(gallery));
                        // Local gallery reads must not wait behind an old HTTP
                        // thumbnail request that is still unwinding cancellation.
                        for row in rows {
                            if r.cancel.load(Ordering::Acquire) {
                                break;
                            }
                            if let Ok(image) = wallhaven::cached_thumbnail(&row) {
                                emit(Item::Thumb(row.id, Arc::new(image)));
                            }
                        }
                    }
                    Task::Pin(row, rows, offline) => {
                        let result = wallhaven::thumbnail(&row.thumbs.small, &r.cancel)
                            .and_then(|image| {
                                wallhaven::cached_thumbnail(&row)?;
                                wallhaven::remember_preview(&row);
                                Ok((row, Arc::new(image)))
                            })
                            .map_err(|e| e.to_string());
                        emit(Item::Pinned(result));
                        let _ = thumb_tx.send(ThumbRequest {
                            offline,
                            hwnd: r.hwnd,
                            revision: r.revision,
                            cancel: r.cancel.clone(),
                            rows,
                        });
                    }
                    Task::Search(options, page, refresh) => {
                        let result =
                            wallhaven::search_options_fresh(&options, page, &r.cancel, refresh)
                                .map_err(|e| e.to_string());
                        let rows = result
                            .as_ref()
                            .map(|v| v.wallpapers.clone())
                            .unwrap_or_default();
                        emit(Item::Rows(result));
                        let mut missing = Vec::new();
                        for row in rows {
                            if r.cancel.load(Ordering::Acquire) {
                                break;
                            }
                            if let Ok(image) = wallhaven::cached_thumbnail(&row) {
                                emit(Item::Thumb(row.id, Arc::new(image)));
                            } else {
                                missing.push(row)
                            }
                        }
                        emit(Item::Stats(wallhaven::cache_stats()));
                        let _ = thumb_tx.send(ThumbRequest {
                            offline: false,
                            hwnd: r.hwnd,
                            revision: r.revision,
                            cancel: r.cancel.clone(),
                            rows: missing,
                        });
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
            thumb_job: Some(thumb_job),
        }
    }
    fn request(&mut self, hwnd: usize, v: &mut Online, task: Task) {
        self.cancel();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.revision += 1;
        v.revision = self.revision;
        v.busy = true;
        v.downloading = matches!(&task, Task::Download(..) | Task::Pin(..));
        v.status = match &task {
            Task::Search(..) => "검색 중…",
            Task::Download(..) => "다운로드 중…",
            Task::Gallery(..) => "보관함 불러오는 중…",
            Task::Pin(..) => "미리보기 보관 중…",
        }
        .into();
        let _ = self.tx.as_ref().unwrap().send(Request {
            hwnd,
            revision: v.revision,
            cancel: self.cancel.clone(),
            task,
        });
    }
    pub fn search(&mut self, hwnd: usize, v: &mut Online) {
        self.request(
            hwnd,
            v,
            Task::Search(v.active.clone(), v.requested_page, false),
        );
    }
    pub fn refresh(&mut self, hwnd: usize, v: &mut Online) {
        self.request(
            hwnd,
            v,
            Task::Search(v.active.clone(), v.requested_page, true),
        );
    }
    pub fn gallery(&mut self, hwnd: usize, v: &mut Online) {
        self.request(
            hwnd,
            v,
            Task::Gallery(
                v.preferences.saved_wallpapers.clone(),
                v.mode == Mode::Hearts,
            ),
        );
    }
    pub fn pin(&mut self, hwnd: usize, v: &mut Online) {
        if let Some(row) = v.rows.get(v.selected).cloned() {
            self.request(
                hwnd,
                v,
                Task::Pin(row, v.rows.clone(), v.mode != Mode::Search),
            );
        }
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
        if let Some(job) = self.thumb_job.take() {
            let _ = job.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_resets_page_and_draft_does_not_change_active_query() {
        let mut online = Online {
            page: 8,
            last: 20,
            requested_page: 8,
            ..Default::default()
        };
        online.begin_search("  id:123  ");
        assert_eq!(online.requested_page, 1);
        assert_eq!(online.active.query, "id:123");
        assert_eq!(online.page, 8);
        let draft = online.draft("+sky -people");
        assert_eq!(draft.query, "+sky -people");
        assert_eq!(online.active.query, "id:123");
        assert_eq!(online.jump("20").unwrap(), 20);
        for value in ["0", "21", "-1", "bad", ""] {
            assert!(online.jump(value).is_err());
        }
    }
    #[test]
    fn batches_cover_partial_pages_without_overrunning() {
        let mut online = Online {
            rows: (0..23).map(|_| wallhaven::Wallpaper::default()).collect(),
            ..Default::default()
        };
        for expected in [6, 12, 18, 18] {
            online.batch(1);
            assert_eq!(online.selected, expected);
        }
        for expected in [12, 6, 0, 0] {
            online.batch(-1);
            assert_eq!(online.selected, expected);
        }
        online.rows.clear();
        online.batch(1);
        assert_eq!(online.selected, 0);
    }
}
