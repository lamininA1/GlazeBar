use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};
pub type Palettes = [crate::theme::Palette; 6];
struct Request {
    hwnd: usize,
    revision: u64,
    image: PathBuf,
    light: bool,
    presets: bool,
}
pub struct Worker {
    tx: Option<mpsc::Sender<Request>>,
    latest: Arc<AtomicU64>,
    job: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(publish: impl Fn(usize, u64, Result<Palettes, String>) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Request>();
        let latest = Arc::new(AtomicU64::new(0));
        let active = latest.clone();
        let job = thread::spawn(move || {
            while let Ok(mut r) = rx.recv() {
                while let Ok(next) = rx.try_recv() {
                    r = next;
                }
                if active.load(Ordering::Acquire) != r.revision {
                    continue;
                }
                let result = if r.presets {
                    Ok(crate::color_schemes::all_palettes(r.light))
                } else {
                    crate::ui_cache::extract_modes(
                        &r.image,
                        r.light,
                        &crate::wallpaper_ui::MODES,
                        crate::ui_cache::directory("palette").as_deref(),
                        crate::theme_cache::directory().as_deref(),
                    )
                    .and_then(|v| {
                        v.try_into()
                            .map_err(|_| anyhow::anyhow!("Incomplete comparison"))
                    })
                    .map_err(|e| e.to_string())
                };
                if active.load(Ordering::Acquire) == r.revision {
                    publish(r.hwnd, r.revision, result);
                }
            }
        });
        Self {
            tx: Some(tx),
            latest,
            job: Some(job),
        }
    }
    pub fn request(&self, hwnd: usize, v: &mut crate::style_compare_ui::Comparison) {
        v.revision = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        if v.presets {
            v.fill_presets();
        } else {
            v.palettes = Default::default();
            v.status = "스타일 계산 중…".into();
        }
        let _ = self.tx.as_ref().unwrap().send(Request {
            hwnd,
            revision: v.revision,
            image: v.image.clone(),
            light: v.light,
            presets: v.presets,
        });
    }
    pub fn cancel(&self) {
        self.latest.fetch_add(1, Ordering::AcqRel);
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
