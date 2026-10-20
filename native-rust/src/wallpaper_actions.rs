use crate::theme_transaction::{self, Request};
use anyhow::{Result, anyhow, ensure};
use std::path::{Path, PathBuf};

pub fn prepare(p: &crate::wallpaper_ui::Picker) -> Result<(PathBuf, Option<Request>)> {
    ensure!(!p.busy, "다른 작업이 진행 중입니다");
    let path = p
        .files
        .get(p.selected)
        .ok_or_else(|| anyhow!("배경화면을 선택해주세요"))?
        .clone();
    if !p.enabled {
        return Ok((path, None));
    }
    let palette = p
        .palette
        .as_ref()
        .ok_or_else(|| anyhow!("색상 추출이 끝난 뒤 적용해주세요"))?;
    let mut targets: Vec<String> = ["glazebar", "terminal", "codex", "vscode"]
        .into_iter()
        .zip(p.targets)
        .filter(|(_, enabled)| *enabled)
        .map(|(name, _)| name.into())
        .collect();
    for (name, enabled) in [
        "rio",
        "zed",
        "antigravity",
        "claude",
        "opencode",
        "firefox",
        "chrome",
        "edge",
        "wezterm",
        "alacritty",
        "hyper",
        "putty",
    ]
    .into_iter()
    .zip(p.extra_targets)
    {
        if enabled {
            targets.push(name.into());
        }
    }
    if p.windows {
        targets.push("windows".into());
    }
    ensure!(!targets.is_empty(), "색상을 적용할 앱을 선택해주세요");
    Ok((
        path.clone(),
        Some(Request {
            action: "apply".into(),
            image: path.to_string_lossy().into_owned(),
            mode: crate::wallpaper_ui::ALL_MODES
                [p.mode.min(crate::wallpaper_ui::ALL_MODES.len() - 1)]
            .into(),
            light: p.light,
            palette: palette.to_vec(),
            targets,
            windows: Some(p.options.clone()),
            ..Default::default()
        }),
    ))
}

fn apply_with(
    request: Option<&Request>,
    mut theme: impl FnMut(&Request) -> Result<()>,
    wallpaper: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if let Some(request) = request {
        theme(request)?;
    }
    if let Err(error) = wallpaper() {
        if request.is_some() {
            if let Err(restore) = theme(&Request {
                action: "restore".into(),
                ..Default::default()
            }) {
                return Err(anyhow!("{error}; 색상 복원 실패: {restore}"));
            }
        }
        return Err(error);
    }
    Ok(())
}
pub fn apply(home: &Path, path: &Path, request: Option<&Request>) -> Result<()> {
    ensure!(
        path.is_file(),
        "배경화면 파일이 없습니다: {}",
        path.display()
    );
    apply_with(
        request,
        |r| theme_transaction::run(home, r).map(|_| ()),
        || crate::wallpaper::set(path),
    )
}
pub fn restore(home: &Path) -> Result<()> {
    theme_transaction::run(
        home,
        &Request {
            action: "restore".into(),
            ..Default::default()
        },
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    #[test]
    fn picker_request_respects_targets_and_pending_palette() {
        let mut p = crate::wallpaper_ui::Picker {
            operation: 0,
            revision: 0,
            loaded: Default::default(),
            files: vec!["image.png".into()],
            selected: 0,
            images: Default::default(),
            mode: 1,
            light: true,
            enabled: true,
            targets: [false, true, false, true],
            extra_targets: [false; 12],
            windows: true,
            options: crate::theme_windows::Options {
                transparency: Some(false),
                ..Default::default()
            },
            busy: false,
            palette: None,
            status: String::new(),
        };
        assert!(prepare(&p).is_err());
        p.enabled = false;
        assert!(prepare(&p).unwrap().1.is_none());
        p.enabled = true;
        p.palette = Some(std::array::from_fn(|_| "#123456".into()));
        let (_, request) = prepare(&p).unwrap();
        let request = request.unwrap();
        assert_eq!(request.targets, vec!["terminal", "vscode", "windows"]);
        assert_eq!(request.mode, "muted");
        assert!(request.light);
        assert_eq!(request.windows.unwrap().transparency, Some(false));
        p.extra_targets = [true; 12];
        let (_, request_all) = prepare(&p).unwrap();
        let request_all = request_all.unwrap();
        assert_eq!(
            request_all.targets,
            vec![
                "terminal",
                "vscode",
                "rio",
                "zed",
                "antigravity",
                "claude",
                "opencode",
                "firefox",
                "chrome",
                "edge",
                "wezterm",
                "alacritty",
                "hyper",
                "putty",
                "windows"
            ]
        );
        p.extra_targets = [false; 12];
        p.windows = false;
        p.targets = [false; 4];
        assert!(prepare(&p).is_err());
        p.enabled = false;
        p.busy = true;
        assert!(prepare(&p).is_err());
        p.busy = false;
        p.files.clear();
        assert!(prepare(&p).is_err());
    }
    #[test]
    fn application_order_and_failed_wallpaper_rollback() {
        let events = RefCell::new(vec![]);
        let request = Request {
            action: "apply".into(),
            ..Default::default()
        };
        let err = apply_with(
            Some(&request),
            |r| {
                events.borrow_mut().push(r.action.clone());
                Ok(())
            },
            || {
                events.borrow_mut().push("wallpaper".into());
                Err(anyhow!("wallpaper failed"))
            },
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "wallpaper failed");
        assert_eq!(*events.borrow(), vec!["apply", "wallpaper", "restore"]);
        events.borrow_mut().clear();
        apply_with(
            None,
            |_| panic!("Wallpaper-only must not touch theme"),
            || {
                events.borrow_mut().push("wallpaper".into());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*events.borrow(), vec!["wallpaper"]);
    }
    #[test]
    fn theme_failure_prevents_wallpaper_and_rollback_error_is_reported() {
        let request = Request {
            action: "apply".into(),
            ..Default::default()
        };
        assert!(
            apply_with(
                Some(&request),
                |_| Err(anyhow!("invalid theme")),
                || panic!("must not set wallpaper")
            )
            .is_err()
        );
        let err = apply_with(
            Some(&request),
            |r| {
                if r.action == "restore" {
                    Err(anyhow!("restore failed"))
                } else {
                    Ok(())
                }
            },
            || Err(anyhow!("wallpaper failed")),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("wallpaper failed") && err.contains("restore failed"));
    }
}
