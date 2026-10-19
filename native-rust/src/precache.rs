use crate::{
    graphics::{Canvas, Rect},
    views::{self, Action, Hit},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};
#[derive(Clone)]
pub struct Progress {
    pub revision: u64,
    pub images: usize,
    pub created: usize,
    pub reused: usize,
    pub failed: usize,
    pub current: String,
    pub remaining: String,
    pub failure: String,
    pub running: bool,
}
impl Progress {
    pub fn new(images: usize) -> Self {
        Self {
            revision: 0,
            images,
            created: 0,
            reused: 0,
            failed: 0,
            current: "캐시 확인 중…".into(),
            remaining: String::new(),
            failure: String::new(),
            running: true,
        }
    }
}
pub fn paint(c: &mut Canvas, p: &Progress, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let total = p.images * 12;
    c.text(
        "전체 캐시 생성",
        Rect::new(c.p(28.), c.p(18.), c.width - c.p(56.), c.p(30.)),
        c.ui.text,
        17.,
        true,
        0,
    );
    c.text(
        &format!("{}장 × 6가지 스타일 × 다크·라이트 = {}개", p.images, total),
        Rect::new(c.p(28.), c.p(57.), c.width - c.p(56.), c.p(24.)),
        c.ui.muted,
        12.,
        false,
        0,
    );
    let track = Rect::new(c.p(24.), c.p(104.), c.width - c.p(48.), c.p(12.));
    c.fill(track, c.ui.hover);
    let processed = p.created + p.reused + p.failed;
    let fill = if total > 0 {
        (track.w as usize * processed / total).min(track.w as usize) as i32
    } else {
        track.w
    };
    if fill > 0 {
        c.fill(Rect::new(track.x, track.y, fill, track.h), c.ui.accent);
    }
    c.text(
        &format!(
            "{} / {} 준비 · 새로 생성 {} · 기존 {} · 실패 {}",
            p.created + p.reused,
            total,
            p.created,
            p.reused,
            p.failed
        ),
        Rect::new(c.p(28.), c.p(127.), c.width - c.p(56.), c.p(24.)),
        c.ui.text,
        12.,
        false,
        0,
    );
    for (text, y, col) in [
        (&p.current, 161., c.ui.text),
        (&p.remaining, 193., c.ui.muted),
        (&p.failure, 223., c.ui.warning),
    ] {
        c.text(
            text,
            Rect::new(c.p(28.), c.p(y), c.width - c.p(56.), c.p(24.)),
            col,
            12.,
            false,
            0,
        );
    }
    let r = Rect::new(c.width - c.p(112.), c.height - c.p(52.), c.p(88.), c.p(30.));
    let mut hits = vec![];
    views::button(
        c,
        &mut hits,
        if p.running { "중지" } else { "닫기" },
        r,
        false,
        hover.is_some_and(|(x, y)| r.contains(x, y)),
        true,
        Action::PrecacheStop,
    );
    c.frame(Rect::new(0, 0, c.width, c.height), c.ui.border);
    hits
}
fn cached(dir: &Path, path: &Path, light: bool, mode: &str) -> bool {
    crate::ui_cache::load(dir, path, mode, light).is_some()
}
fn run(
    files: &[PathBuf],
    dir: &Path,
    engine_dir: Option<&Path>,
    revision: u64,
    cancel: &AtomicBool,
    publish: impl Fn(Progress),
) {
    let mut p = Progress::new(files.len());
    p.revision = revision;
    let start = Instant::now();
    'files: for path in files {
        for light in [false, true] {
            if cancel.load(Ordering::Acquire) {
                break 'files;
            }
            let modes = crate::wallpaper_ui::MODES;
            let previous = modes.map(|mode| cached(dir, path, light, mode));
            let result = crate::ui_cache::extract_modes(path, light, &modes, Some(dir), engine_dir);
            for (index, mode) in modes.iter().enumerate() {
                if result.is_ok() && cached(dir, path, light, mode) {
                    if previous[index] {
                        p.reused += 1;
                    } else {
                        p.created += 1;
                    }
                } else {
                    p.failed += 1;
                    p.failure = match &result {
                        Err(e) => e.to_string(),
                        Ok(_) => "캐시 파일을 저장하지 못했습니다".into(),
                    };
                }
            }
            p.current = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let processed = p.created + p.reused + p.failed;
            let seconds = (start.elapsed().as_secs_f64() / processed as f64
                * (files.len() * 12 - processed) as f64) as usize;
            p.remaining = if seconds > 0 {
                format!("예상 남은 시간 · 약 {}분 {}초", seconds / 60, seconds % 60)
            } else {
                String::new()
            };
            publish(p.clone());
        }
    }
    p.running = false;
    p.remaining.clear();
    p.current = if cancel.load(Ordering::Acquire) {
        "중지했습니다. 생성된 캐시는 유지됩니다."
    } else if p.failed > 0 {
        "캐시 생성을 마쳤습니다. 실패 항목을 확인해주세요."
    } else {
        "모든 조합의 캐시가 준비되었습니다."
    }
    .into();
    publish(p);
}
struct Request {
    hwnd: usize,
    revision: u64,
    files: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
}
pub struct Worker {
    tx: Option<mpsc::Sender<Request>>,
    job: Option<thread::JoinHandle<()>>,
    cancel: Option<Arc<AtomicBool>>,
    revision: u64,
}
impl Worker {
    pub fn start(publish: impl Fn(usize, Progress) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let job = thread::spawn(move || {
            while let Ok(r) = rx.recv() {
                if let Some(dir) = crate::ui_cache::directory("palette") {
                    run(
                        &r.files,
                        &dir,
                        crate::theme_cache::directory().as_deref(),
                        r.revision,
                        &r.cancel,
                        |p| publish(r.hwnd, p),
                    );
                } else {
                    let mut p = Progress::new(r.files.len());
                    p.revision = r.revision;
                    p.failed = r.files.len() * 12;
                    p.running = false;
                    p.current = "캐시 폴더를 찾을 수 없습니다".into();
                    publish(r.hwnd, p);
                }
            }
        });
        Self {
            tx: Some(tx),
            job: Some(job),
            cancel: None,
            revision: 0,
        }
    }
    pub fn request(&mut self, hwnd: usize, files: Vec<PathBuf>, p: &mut Progress) {
        self.stop();
        self.revision += 1;
        p.revision = self.revision;
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let _ = self.tx.as_ref().unwrap().send(Request {
            hwnd,
            revision: p.revision,
            files,
            cancel,
        });
    }
    pub fn stop(&self) {
        if let Some(c) = &self.cancel {
            c.store(true, Ordering::Release);
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop();
        self.tx.take();
        if let Some(job) = self.job.take() {
            let _ = job.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn creation_reuse_failure_and_cancel_preserve_completed_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sampler-1.png");
        let cancel = AtomicBool::new(false);
        let output = RefCell::new(None);
        run(
            &[path.clone()],
            dir.path(),
            Some(dir.path()),
            1,
            &cancel,
            |p| {
                *output.borrow_mut() = Some(p);
            },
        );
        assert_eq!(output.borrow().as_ref().unwrap().created, 12);
        run(
            &[path.clone()],
            dir.path(),
            Some(dir.path()),
            2,
            &cancel,
            |p| {
                *output.borrow_mut() = Some(p);
            },
        );
        assert_eq!(output.borrow().as_ref().unwrap().reused, 12);
        run(
            &[path.clone(), dir.path().join("missing.png")],
            dir.path(),
            Some(dir.path()),
            3,
            &cancel,
            |p| {
                *output.borrow_mut() = Some(p);
            },
        );
        assert_eq!(output.borrow().as_ref().unwrap().failed, 12);
        assert_eq!(output.borrow().as_ref().unwrap().reused, 12);
        run(&[path], dir.path(), Some(dir.path()), 4, &cancel, |p| {
            if p.running {
                cancel.store(true, Ordering::Release);
            }
            *output.borrow_mut() = Some(p);
        });
        let p = output.borrow();
        let p = p.as_ref().unwrap();
        assert!(!p.running);
        assert_eq!(p.reused, 6);
        assert!(p.current.starts_with("중지했습니다"));
    }
}
