use crate::{
    graphics::{Canvas, Rect},
    theme_windows::Options,
    views::{self, Action, Hit},
};
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    Enabled,
    Mode,
    Accent,
    Titlebars,
    Taskbar,
    Transparency,
    Save,
}
#[derive(Clone)]
pub struct Settings {
    pub enabled: bool,
    pub options: Options,
    pub light: bool,
    pub status: String,
}
impl Settings {
    pub fn change(&mut self, edit: &Edit) {
        let cycle = |v: &mut Option<bool>| {
            *v = match *v {
                None => Some(true),
                Some(true) => Some(false),
                Some(false) => None,
            }
        };
        match edit {
            Edit::Enabled => self.enabled = !self.enabled,
            Edit::Mode => self.options.mode = !self.options.mode,
            Edit::Accent => self.options.accent = !self.options.accent,
            Edit::Titlebars => cycle(&mut self.options.titlebars),
            Edit::Taskbar => cycle(&mut self.options.taskbar),
            Edit::Transparency => cycle(&mut self.options.transparency),
            Edit::Save => {}
        }
        self.status.clear();
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !(self.enabled
                && self.options.mode
                && self.light
                && self.options.taskbar == Some(true)),
            "라이트 모드에서는 작업 표시줄 강조색을 사용할 수 없습니다"
        );
        Ok(())
    }
    pub fn save(&self, root: &std::path::Path) -> anyhow::Result<()> {
        self.validate()?;
        crate::theme_transaction::atomic_write(
            &root.join("theme-state/windows-preferences.json"),
            &serde_json::to_vec_pretty(
                &serde_json::json!({"enabled":self.enabled,"options":self.options}),
            )?,
        )
    }
}
fn toggle(c: &mut Canvas, hits: &mut Vec<Hit>, label: &str, r: Rect, checked: bool, edit: Edit) {
    let side = c.p(10.);
    let y = r.y + (r.h - side) / 2;
    let col = if checked { c.ui.accent } else { c.ui.border };
    c.frame(Rect::new(r.x, y, side, side), col);
    if checked {
        let inset = c.p(3.);
        c.fill(
            Rect::new(r.x + inset, y + inset, side - 2 * inset, side - 2 * inset),
            col,
        );
    }
    c.text(
        label,
        Rect::new(r.x + side + c.p(8.), r.y, r.w - side - c.p(8.), r.h),
        c.ui.text,
        12.,
        false,
        0,
    );
    hits.push(Hit {
        rect: r,
        action: Action::WindowsOption(edit),
    });
}
pub fn paint(c: &mut Canvas, v: &Settings, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let mut hits = vec![];
    let scale = c.scale;
    let s = |n: f64| (n * scale).round_ties_even() as i32;
    c.text(
        "Windows 시스템 연동",
        Rect::new(s(24.), s(22.), s(450.), s(30.)),
        c.ui.text,
        18.,
        true,
        0,
    );
    toggle(
        c,
        &mut hits,
        "배경화면 적용 시 Windows도 함께 변경",
        Rect::new(s(24.), s(68.), s(460.), s(30.)),
        v.enabled,
        Edit::Enabled,
    );
    for (label, y, value, edit) in [
        (
            "다크 / 라이트를 선택한 테마에 맞추기",
            112.,
            v.options.mode,
            Edit::Mode,
        ),
        (
            "사진에서 추출한 강조색 적용",
            150.,
            v.options.accent,
            Edit::Accent,
        ),
    ] {
        toggle(
            c,
            &mut hits,
            label,
            Rect::new(s(24.), s(y), s(460.), s(30.)),
            value,
            edit,
        );
    }
    for (i, (label, value, edit)) in [
        (
            "제목 표시줄 · 창 테두리",
            v.options.titlebars,
            Edit::Titlebars,
        ),
        ("시작 메뉴 · 작업 표시줄", v.options.taskbar, Edit::Taskbar),
        ("투명 효과", v.options.transparency, Edit::Transparency),
    ]
    .into_iter()
    .enumerate()
    {
        let y = s((202 + i * 42) as f64);
        c.text(
            label,
            Rect::new(s(24.), y, s(260.), s(30.)),
            c.ui.text,
            13.,
            false,
            0,
        );
        let r = Rect::new(s(295.), y, s(190.), s(30.));
        views::button(
            c,
            &mut hits,
            match value {
                None => "현재 설정 유지",
                Some(true) => "사용",
                Some(false) => "사용 안 함",
            },
            r,
            value.is_some(),
            hover.is_some_and(|(x, y)| r.contains(x, y)),
            true,
            Action::WindowsOption(edit),
        );
    }
    c.text(
        if v.status.is_empty() {
            "작업 표시줄 강조색은 Windows 다크 모드에서 사용할 수 있습니다"
        } else {
            &v.status
        },
        Rect::new(s(24.), s(330.), s(460.), s(42.)),
        c.ui.muted,
        11.,
        false,
        0,
    );
    for (label, x, width, selected, action) in [
        ("취소", 270., 100., false, Action::Close),
        (
            "선택 완료",
            382.,
            104.,
            true,
            Action::WindowsOption(Edit::Save),
        ),
    ] {
        let r = Rect::new(s(x), s(383.), s(width), s(30.));
        views::button(
            c,
            &mut hits,
            label,
            r,
            selected,
            hover.is_some_and(|(x, y)| r.contains(x, y)),
            true,
            action,
        );
    }
    hits
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tri_state_and_light_taskbar_validation() {
        let mut s = Settings {
            enabled: true,
            options: Options {
                mode: true,
                ..Default::default()
            },
            light: true,
            status: String::new(),
        };
        s.change(&Edit::Taskbar);
        assert_eq!(s.options.taskbar, Some(true));
        assert!(s.validate().is_err());
        s.change(&Edit::Taskbar);
        assert_eq!(s.options.taskbar, Some(false));
        assert!(s.validate().is_ok());
        s.change(&Edit::Taskbar);
        assert_eq!(s.options.taskbar, None);
        let root = tempfile::tempdir().unwrap();
        s.save(root.path()).unwrap();
        let data: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.path().join("theme-state/windows-preferences.json")).unwrap(),
        )
        .unwrap();
        assert!(data["options"]["taskbar"].is_null());
        assert_eq!(data["enabled"], true);
    }
}
