use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};
pub enum Item {
    Image(PathBuf, Option<Arc<image::RgbaImage>>),
    Palette(Result<crate::theme::Palette, String>),
}
struct Request {
    revision: u64,
    hwnd: usize,
    files: Vec<PathBuf>,
    image: Option<PathBuf>,
    mode: &'static str,
    light: bool,
}
pub struct Worker {
    tx: Option<mpsc::Sender<Request>>,
    latest: Arc<AtomicU64>,
    job: Option<thread::JoinHandle<()>>,
}
impl Worker {
    pub fn start(publish: impl Fn(usize, u64, Item) + Send + 'static) -> Self {
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
                if let Some(path) = &r.image {
                    let result =
                        crate::ui_cache::extract(path, r.light, r.mode).map_err(|e| e.to_string());
                    if active.load(Ordering::Acquire) != r.revision {
                        continue;
                    }
                    publish(r.hwnd, r.revision, Item::Palette(result));
                }
                for path in r.files {
                    if active.load(Ordering::Acquire) != r.revision {
                        break;
                    }
                    let image = crate::wallpaper_ui::thumbnail(&path).ok().map(Arc::new);
                    if active.load(Ordering::Acquire) != r.revision {
                        break;
                    }
                    publish(r.hwnd, r.revision, Item::Image(path, image));
                }
            }
        });
        Self {
            tx: Some(tx),
            latest,
            job: Some(job),
        }
    }
    pub fn request(&self, hwnd: usize, p: &mut crate::wallpaper_ui::Picker) {
        let revision = self.latest.fetch_add(1, Ordering::AcqRel) + 1;
        p.revision = revision;
        p.palette = None;
        let image = if p.enabled {
            p.files.get(p.selected).cloned()
        } else {
            None
        };
        p.status = if image.is_some() {
            "Aether 색상 추출 중…".into()
        } else {
            String::new()
        };
        let mut files = p.neighbors();
        for path in &p.files {
            if !files.contains(path) {
                files.push(path.clone());
            }
        }
        files.retain(|path| !p.loaded.contains(path));
        let _ = self.tx.as_ref().unwrap().send(Request {
            revision,
            hwnd,
            files,
            image,
            mode: crate::wallpaper_ui::MODES[p.mode],
            light: p.light,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn superseding_request_discards_remaining_old_work() {
        use std::time::Duration;
        let (sent, received) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let worker = Worker::start(move |_, revision, item| {
            let Item::Image(path, image) = item else {
                panic!("Unexpected palette")
            };
            sent.send((revision, path, image.is_some())).unwrap();
            if revision == 1 {
                let _ = gate.recv_timeout(Duration::from_secs(5));
            }
        });
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/sampler-1.png");
        let old_tail = path.with_file_name("must-not-be-processed.png");
        worker.latest.store(1, Ordering::Release);
        worker
            .tx
            .as_ref()
            .unwrap()
            .send(Request {
                revision: 1,
                hwnd: 1,
                files: vec![path.clone(), old_tail],
                image: None,
                mode: "normal",
                light: false,
            })
            .unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            (1, path.clone(), true)
        );
        worker.latest.store(2, Ordering::Release);
        worker
            .tx
            .as_ref()
            .unwrap()
            .send(Request {
                revision: 2,
                hwnd: 1,
                files: vec![path.clone()],
                image: None,
                mode: "normal",
                light: false,
            })
            .unwrap();
        release.send(()).unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            (2, path, true)
        );
        drop(worker);
        assert!(received.try_recv().is_err());
    }
}
