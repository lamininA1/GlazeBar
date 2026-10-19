use crate::graphics::Rect;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{net::TcpStream, time::Duration};
use tungstenite::{Message, WebSocket};

pub struct Client {
    socket: Option<WebSocket<TcpStream>>,
    auto_signature: String,
    group_settle_until: Option<std::time::Instant>,
}
impl Client {
    pub fn new() -> Self {
        Self {
            socket: None,
            auto_signature: String::new(),
            group_settle_until: None,
        }
    }
    fn connect() -> Result<WebSocket<TcpStream>> {
        let stream =
            TcpStream::connect_timeout(&"127.0.0.1:6123".parse()?, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        stream.set_nodelay(true)?;
        let (socket, _) = tungstenite::client("ws://127.0.0.1:6123", stream)?;
        Ok(socket)
    }
    pub fn request(&mut self, command: &str) -> Result<Value> {
        let result = (|| {
            if self.socket.is_none() {
                self.socket = Some(Self::connect()?)
            }
            let socket = self.socket.as_mut().unwrap();
            socket.send(Message::Text(command.into()))?;
            loop {
                let message = socket.read()?;
                if let Message::Text(text) = message {
                    let v: Value = serde_json::from_str(&text)?;
                    if v["success"] != true {
                        bail!("{}", v["error"])
                    }
                    return Ok(v["data"].clone());
                }
            }
        })();
        if result.is_err() {
            self.socket = None
        }
        result
    }
    pub fn model(&mut self) -> Result<Model> {
        let v = self.request("query monitors")?;
        let monitors = v["monitors"]
            .as_array()
            .context("missing monitors")?
            .iter()
            .map(|m| Monitor {
                name: string(m, "deviceName"),
                bounds: Rect::new(
                    number(m, "x"),
                    number(m, "y"),
                    number(m, "width"),
                    number(m, "height"),
                ),
                scale: m["scaleFactor"].as_f64().unwrap_or(1.).max(0.5),
                workspaces: m["children"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| v["type"] == "workspace")
                    .map(|w| Workspace {
                        name: string(w, "name"),
                        display: w["displayName"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(w["name"].as_str().unwrap_or(""))
                            .to_string(),
                        focused: w["hasFocus"] == true,
                        displayed: w["isDisplayed"] == true,
                    })
                    .collect(),
            })
            .collect();
        let paused = self.request("query paused")?.as_bool().unwrap_or(false);
        let bindings = self.request("query binding-modes")?;
        let resize = bindings["bindingModes"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v["name"] == "resize");
        let vertical = self.request("query tiling-direction")?["tilingDirection"] == "vertical";
        Ok(Model {
            monitors,
            paused,
            resize,
            vertical,
        })
    }
    pub fn auto_split(&mut self, disabled: bool) -> Result<()> {
        if disabled || self.request("query paused")? == true {
            self.auto_signature.clear();
            return Ok(());
        }
        let data = self.request("query focused")?;
        let f = &data["focused"];
        if f["type"] != "window" || f["state"]["type"] != "tiling" {
            self.auto_signature.clear();
            return Ok(());
        }
        let sig = format!("{}:{}:{}", string(f, "id"), f["width"], f["height"]);
        // Native minimize/restore animations can deliver intermediate focus
        // events. Do not turn those transient targets into new split nodes.
        if self.group_settle_until.is_some_and(|until| std::time::Instant::now() < until) {
            self.auto_signature = sig;
            return Ok(());
        }
        if sig != self.auto_signature {
            let direction =
                if f["height"].as_f64().unwrap_or(0.) > f["width"].as_f64().unwrap_or(0.) {
                    "vertical"
                } else {
                    "horizontal"
                };
            self.request(&format!(
                "command --id {} set-tiling-direction {direction}",
                string(f, "id")
            ))?;
            self.auto_signature = sig;
        }
        Ok(())
    }
    fn wait_minimized(&mut self, id: &str, minimized: bool) -> Result<()> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let data = self.request("query windows")?;
            let rows = data["windows"].as_array().context("Missing windows")?;
            let window = rows.iter().find(|v| string(v, "id") == id);
            if window.is_none_or(|v| (v["state"]["type"] == "minimized") == minimized) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(40));
        }
        bail!("Window state timed out: {id}")
    }
    pub fn restore_group(&mut self, root: &std::path::Path) -> Result<()> {
        if self.request("query app-metadata")?["version"].as_str().is_some_and(|v| v.contains("-glazebar.")) {
            self.request("command wm-restore-minimized-group")?;
            self.group_settle_until = Some(std::time::Instant::now() + Duration::from_secs(2));
            let data = self.request("query focused")?;
            let f = &data["focused"];
            self.auto_signature = format!("{}:{}:{}", string(f, "id"), f["width"], f["height"]);
        }
        let path = root.join("resident-minimized.json");
        if !path.exists() {
            return Ok(());
        }
        let saved: SavedGroup = serde_json::from_slice(&std::fs::read(&path)?)?;
        let data = self.request("query windows")?;
        let rows = data["windows"].as_array().context("Missing windows")?;
        let restore = restorable(&saved, rows);
        if !restore.is_empty() {
            self.request(&format!("command focus --workspace {}", saved.workspace))?;
            for w in restore {
                self.request(&format!("command --id {} toggle-minimized", w.id))?;
                self.wait_minimized(&w.id, false)?;
            }
            self.request("command wm-redraw")?;
            if rows.iter().any(|v| string(v, "id") == saved.focused_id) {
                self.request(&format!(
                    "command focus --container-id {}",
                    saved.focused_id
                ))?;
            }
        }
        std::fs::remove_file(path)?;
        Ok(())
    }
    pub fn toggle_group(&mut self, root: &std::path::Path) -> Result<()> {
        if self.request("query paused")? == true {
            return Ok(());
        }
        if self.request("query app-metadata")?["version"].as_str().is_some_and(|v| v.contains("-glazebar.")) {
            self.request("command wm-toggle-minimized-group")?;
            self.group_settle_until = Some(std::time::Instant::now() + Duration::from_secs(2));
            let data = self.request("query focused")?;
            let f = &data["focused"];
            self.auto_signature = format!("{}:{}:{}", string(f, "id"), f["width"], f["height"]);
            return Ok(());
        }
        let path = root.join("resident-minimized.json");
        if path.exists() {
            return self.restore_group(root);
        }
        let data = self.request("query workspaces")?;
        let rows = data["workspaces"]
            .as_array()
            .context("Missing workspaces")?;
        let Some(saved) = minimize_plan(rows) else {
            return Ok(());
        };
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, serde_json::to_vec(&saved)?)?;
        std::fs::rename(temp, path)?;
        for w in saved.windows {
            self.request(&format!("command --id {} set-minimized", w.id))?;
            self.wait_minimized(&w.id, true)?;
        }
        Ok(())
    }
}
pub fn subscribe(mut changed: impl FnMut(), stop: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    while !stop.load(Ordering::Relaxed) {
        let mut run = || -> Result<()> {
            let mut socket = Client::connect()?;
            socket.send(Message::Text("sub --events focus_changed focused_container_moved window_managed window_unmanaged workspace_activated workspace_deactivated workspace_updated binding_modes_changed pause_changed tiling_direction_changed monitor_added monitor_removed monitor_updated user_config_changed".into()))?;
            loop {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                match socket.read() {
                    Ok(Message::Text(_)) => changed(),
                    Ok(_) => (),
                    Err(tungstenite::Error::Io(e))
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        ()
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        };
        let _ = run();
        if !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(2));
        }
    }
}
pub fn string(v: &Value, key: &str) -> String {
    match &v[key] {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
fn number(v: &Value, key: &str) -> i32 {
    v[key].as_f64().unwrap_or(0.) as i32
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub display: String,
    pub focused: bool,
    pub displayed: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub bounds: Rect,
    pub scale: f64,
    pub workspaces: Vec<Workspace>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub monitors: Vec<Monitor>,
    pub paused: bool,
    pub resize: bool,
    pub vertical: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedGroup {
    pub workspace: String,
    pub focused_id: String,
    pub windows: Vec<SavedWindow>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedWindow {
    pub id: String,
    pub handle: String,
}
fn window_nodes<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    if let Some(rows) = node["children"].as_array() {
        for row in rows {
            if row["type"] == "window" {
                out.push(row)
            } else {
                window_nodes(row, out)
            }
        }
    }
}
pub fn minimize_plan(rows: &[Value]) -> Option<SavedGroup> {
    let ws = rows.iter().find(|w| w["hasFocus"] == true)?;
    let mut nodes = vec![];
    window_nodes(ws, &mut nodes);
    nodes.sort_by_key(|w| w["hasFocus"] == true);
    let mut saved = SavedGroup {
        workspace: string(ws, "name"),
        focused_id: String::new(),
        windows: vec![],
    };
    for w in nodes {
        if w["state"]["type"] != "minimized" && w["displayState"] == "shown" {
            saved.windows.push(SavedWindow {
                id: string(w, "id"),
                handle: string(w, "handle"),
            });
            if w["hasFocus"] == true {
                saved.focused_id = string(w, "id");
            }
        }
    }
    (!saved.windows.is_empty()).then_some(saved)
}
fn restorable<'a>(saved: &'a SavedGroup, current: &[Value]) -> Vec<&'a SavedWindow> {
    saved
        .windows
        .iter()
        .filter(|w| {
            current.iter().any(|v| {
                string(v, "id") == w.id
                    && string(v, "handle") == w.handle
                    && v["state"]["type"] == "minimized"
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handles_stay_decimal() {
        assert_eq!(
            string(&serde_json::json!({"handle":2425086}), "handle"),
            "2425086"
        );
    }
    #[test]
    fn group_plan_restores_only_identical_windows() {
        let data = serde_json::json!([{"name":"1","hasFocus":true,"children":[{"type":"split","children":[{"type":"window","id":"focus","handle":123,"hasFocus":true,"state":{"type":"tiling"},"displayState":"shown"},{"type":"window","id":"other","handle":456,"hasFocus":false,"state":{"type":"tiling"},"displayState":"shown"},{"type":"window","id":"already","handle":789,"state":{"type":"minimized"},"displayState":"shown"}]}]}]);
        let plan = minimize_plan(data.as_array().unwrap()).unwrap();
        assert_eq!(plan.windows.len(), 2);
        assert_eq!(plan.windows[1].id, "focus");
        assert_eq!(plan.focused_id, "focus");
        let current = serde_json::json!([{"id":"focus","handle":123,"state":{"type":"minimized"}},{"id":"other","handle":999,"state":{"type":"minimized"}}]);
        let restore = restorable(&plan, current.as_array().unwrap());
        assert_eq!(restore.len(), 1);
        assert_eq!(restore[0].id, "focus");
    }
}
