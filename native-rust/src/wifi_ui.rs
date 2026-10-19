use crate::{
    graphics::{Canvas, Rect},
    views::{Action, Hit, button},
    wifi::Network,
};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone)]
pub struct Wifi {
    pub token: u64,
    pub rows: Vec<Network>,
    pub selected: usize,
    pub offset: usize,
    pub loading: bool,
    pub connecting: bool,
    pub status: String,
}
impl Wifi {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            token: NEXT.fetch_add(1, Ordering::Relaxed),
            rows: vec![],
            selected: 0,
            offset: 0,
            loading: false,
            connecting: false,
            status: String::new(),
        }
    }
    pub fn needs_password(&self) -> bool {
        self.rows
            .get(self.selected)
            .is_some_and(|n| n.secure && n.profile.is_empty() && matches!(n.auth, 4 | 7 | 9))
    }
    pub fn can_connect(&self) -> bool {
        !self.connecting
            && self.rows.get(self.selected).is_some_and(|n| {
                !n.connected
                    && n.connectable
                    && (!n.secure || matches!(n.auth, 4 | 7 | 9) || !n.profile.is_empty())
            })
    }
    pub fn bottom(&self) -> f64 {
        62. + self.rows.len().clamp(1, 6) as f64 * 48.
    }
    pub fn input_y(&self) -> f64 {
        self.bottom() + if self.needs_password() { 32. } else { 14. }
    }
    pub fn height(&self) -> f64 {
        let mut bottom = self.bottom() + 12.;
        if self.can_connect() || self.connecting {
            bottom = self.input_y() + 30.;
        }
        if !self.status.is_empty() {
            bottom += 44.;
        }
        bottom + 50.
    }
    pub fn select(&mut self, index: usize) {
        if self.connecting {
            return;
        }
        self.selected = index.min(self.rows.len().saturating_sub(1));
        self.offset = self
            .offset
            .max(self.selected.saturating_sub(5))
            .min(self.selected);
        self.status.clear();
    }
    pub fn update(&mut self, rows: Vec<Network>) {
        let key = self
            .rows
            .get(self.selected)
            .map(|n| (n.ssid.clone(), n.auth));
        self.rows = rows;
        self.selected = key
            .and_then(|(ssid, auth)| {
                self.rows
                    .iter()
                    .position(|n| n.ssid == ssid && n.auth == auth)
            })
            .unwrap_or(0);
        self.offset = self.offset.min(self.rows.len().saturating_sub(6));
        self.loading = false;
    }
}

pub fn fixture() -> Wifi {
    let mut v = Wifi::new();
    v.rows = [
        "Sample Wi-Fi",
        "Studio_5G",
        "Public WiFi Free",
        "Public WiFi Secure",
        "Guest",
    ]
    .iter()
    .enumerate()
    .map(|(i, name)| Network {
        adapter: windows_sys::core::GUID::from_u128(0),
        ssid: name.as_bytes().to_vec(),
        name: (*name).into(),
        profile: if i == 0 {
            "Sample".into()
        } else {
            String::new()
        },
        signal: 94 - i as u32 * 20,
        auth: if i == 3 { 6 } else { 7 },
        cipher: 4,
        connected: i == 0,
        secure: i != 2,
        connectable: true,
    })
    .collect();
    v.selected = 1;
    v
}
fn dotted(c: &Canvas, x: i32, y: i32, width: i32) {
    for i in (0..width).step_by(2) {
        c.fill(Rect::new(x + i, y, 1, 1), c.ui.border);
    }
}
pub fn paint(c: &mut Canvas, v: &Wifi, hover: Option<(i32, i32)>) -> Vec<Hit> {
    let ui = c.ui;
    let mut hits = vec![];
    let hovered = |r: Rect| hover.is_some_and(|(x, y)| r.contains(x, y));
    c.frame(Rect::new(0, 0, c.width, c.height), ui.border);
    c.text(
        "[ WI-FI ]",
        Rect::new(c.p(20.), c.p(15.), c.p(220.), c.p(30.)),
        ui.text,
        17.,
        true,
        0,
    );
    let r = Rect::new(c.width - c.p(100.), c.p(17.), c.p(80.), c.p(28.));
    button(
        c,
        &mut hits,
        "↻ 검색",
        r,
        false,
        hovered(r),
        !v.loading && !v.connecting,
        Action::WifiScan(true),
    );
    dotted(c, c.p(20.), c.p(54.), c.width - c.p(40.));
    dotted(c, c.p(20.), c.p(v.bottom() + 8.), c.width - c.p(40.));
    for (row, n) in v.rows.iter().skip(v.offset).take(6).enumerate() {
        let index = v.offset + row;
        let r = Rect::new(
            c.p(16.),
            c.p(62.) + row as i32 * c.p(48.),
            c.width - c.p(32.),
            c.p(48.),
        );
        if index == v.selected {
            c.fill(r, ui.selected)
        } else if hovered(r) {
            c.fill(r, ui.surface)
        }
        let col = if n.connected { ui.accent } else { ui.muted };
        c.text(
            if n.connected {
                "●"
            } else if index == v.selected {
                ">"
            } else {
                "·"
            },
            Rect::new(r.x + c.p(5.), r.y, c.p(16.), r.h),
            col,
            13.,
            false,
            0,
        );
        c.icon(
            if n.signal >= 67 {
                "wifi"
            } else if n.signal >= 34 {
                "wifi-level-2"
            } else {
                "wifi-level-1"
            },
            Rect::new(r.x + c.p(27.), r.y + c.p(15.), c.p(18.), c.p(18.)),
            col,
        );
        c.text(
            &n.name,
            Rect::new(r.x + c.p(57.), r.y + c.p(4.), r.w - c.p(126.), c.p(22.)),
            ui.text,
            13.,
            false,
            0,
        );
        let mut security = if !n.secure {
            "OPEN"
        } else if matches!(n.auth, 4 | 7) {
            "WPA/WPA2"
        } else if n.auth == 9 {
            "WPA3"
        } else {
            "802.1X / 기타"
        }
        .to_string();
        if !n.profile.is_empty() {
            security.push_str(" · 저장됨")
        }
        if n.connected {
            security = "연결됨".into()
        }
        c.text(
            &security,
            Rect::new(r.x + c.p(57.), r.y + c.p(26.), r.w - c.p(126.), c.p(17.)),
            ui.muted,
            11.,
            false,
            0,
        );
        c.text(
            &format!("{}%", n.signal),
            Rect::new(r.x + r.w - c.p(55.), r.y, c.p(49.), r.h),
            ui.muted,
            11.,
            false,
            0,
        );
        dotted(c, r.x + c.p(56.), r.y + r.h - 1, r.w - c.p(61.));
        if !v.connecting {
            hits.push(Hit {
                rect: r,
                action: Action::WifiSelect(index),
            });
        }
    }
    if v.rows.is_empty() {
        c.text(
            if v.loading {
                "검색 중…"
            } else {
                "목록이 비어 있습니다."
            },
            Rect::new(c.p(28.), c.p(85.), c.width - c.p(56.), c.p(35.)),
            ui.muted,
            13.,
            false,
            0,
        );
    }
    if v.needs_password() {
        c.text(
            "비밀번호",
            Rect::new(c.p(20.), c.p(v.bottom() + 9.), c.width - c.p(40.), c.p(18.)),
            ui.muted,
            11.,
            false,
            0,
        );
    }
    if v.can_connect() || v.connecting {
        let r = Rect::new(
            c.width - c.p(100.),
            c.p(v.input_y()) - c.p(1.),
            c.p(80.),
            c.p(29.),
        );
        button(
            c,
            &mut hits,
            if v.connecting { "연결 중" } else { "연결" },
            r,
            false,
            hovered(r),
            !v.connecting,
            Action::WifiConnect,
        );
    }
    let r = Rect::new(c.p(20.), c.height - c.p(38.), c.p(112.), c.p(25.));
    button(
        c,
        &mut hits,
        "Windows 설정",
        r,
        false,
        hovered(r),
        true,
        Action::WifiSettings,
    );
    if !v.status.is_empty() {
        c.text(
            &v.status,
            Rect::new(c.p(20.), c.height - c.p(86.), c.width - c.p(40.), c.p(40.)),
            ui.accent,
            11.,
            false,
            0,
        );
    }
    if v.rows.len() > 6 {
        c.text(
            &format!(
                "{}–{} / {}",
                v.offset + 1,
                (v.offset + 6).min(v.rows.len()),
                v.rows.len()
            ),
            Rect::new(c.width - c.p(110.), c.height - c.p(38.), c.p(90.), c.p(25.)),
            ui.muted,
            11.,
            false,
            0,
        );
    }
    hits
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_layout() {
        let w = Wifi::new();
        assert_eq!(w.height(), 172.);
        assert!(!w.needs_password());
        assert!(!w.can_connect());
    }
}
