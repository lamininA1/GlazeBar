//! File theme plans are completed before the first write; snapshots use Go's format.
use crate::{theme, theme_settings, theme_text};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

mod bytes {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(data: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(data))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = Option::<String>::deserialize(d)?;
        STANDARD
            .decode(s.unwrap_or_default())
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Change {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub registry_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub registry_name: String,
    pub path: PathBuf,
    #[serde(with = "bytes")]
    pub before: Vec<u8>,
    pub existed: bool,
    #[serde(with = "bytes")]
    pub after: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Snapshot {
    created: String,
    changes: Vec<Change>,
}
#[derive(Default, Clone, Deserialize)]
#[serde(default)]
pub struct Request {
    pub query: String,
    pub page: i32,
    pub id: String,
    pub folder: String,
    pub modes: Vec<String>,
    pub both: bool,
    pub windows: Option<crate::theme_windows::Options>,
    pub action: String,
    pub image: String,
    pub mode: String,
    pub light: bool,
    pub palette: Vec<String>,
    pub targets: Vec<String>,
}
pub struct Plan {
    changes: Vec<Change>,
    applied: Vec<String>,
    skipped: Vec<String>,
    state: PathBuf,
}

pub fn canonical_target(t: &str) -> Option<&'static str> {
    match t.trim().to_ascii_lowercase().as_str() {
        "glazebar" | "glazewm" => Some("glazebar"),
        "terminal" | "windows-terminal" | "wt" => Some("terminal"),
        "rio" | "rio-terminal" => Some("rio"),
        "vscode" | "code" => Some("vscode"),
        "zed" | "zed-editor" => Some("zed"),
        "codex" | "codex-cli" => Some("codex"),
        "antigravity" | "antigravity-cli" | "agy" => Some("antigravity"),
        "claude" | "claude-code" => Some("claude"),
        "opencode" => Some("opencode"),
        "firefox" => Some("firefox"),
        "chrome" | "google-chrome" => Some("chrome"),
        "edge" | "msedge" | "microsoft-edge" => Some("edge"),
        "windows" => Some("windows"),
        _ => None,
    }
}

fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}
fn text(path: &Path) -> Result<String> {
    Ok(String::from_utf8(
        read(path)?.unwrap_or_else(|| b"{}".to_vec()),
    )?)
}
fn text_raw(path: &Path) -> Result<String> {
    Ok(String::from_utf8(read(path)?.unwrap_or_default())?)
}
fn json(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut b = serde_json::to_vec_pretty(value)?;
    b.push(b'\n');
    Ok(b)
}
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().context("Missing parent directory")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::Builder::new()
        .prefix(".glazebar-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    file.write_all(data)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn state(home: &Path) -> PathBuf {
    home.join(".glzr/glazewm/theme-state")
}
fn is_real_home(home: &Path) -> bool {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .as_deref()
        == Some(home)
}
impl Plan {
    fn add(&mut self, path: PathBuf, after: Vec<u8>) -> Result<()> {
        // Native separators also let the existing Go reloader recognize config paths.
        let path: PathBuf = path.components().collect();
        let before = read(&path)?;
        self.changes.push(Change {
            registry_key: String::new(),
            registry_name: String::new(),
            path,
            before: before.clone().unwrap_or_default(),
            existed: before.is_some(),
            after,
        });
        Ok(())
    }
    pub fn build(home: &Path, r: &Request) -> Result<Self> {
        let p = theme::readable(&r.palette)?;
        ensure!(!r.targets.is_empty(), "select at least one theme target");
        let mut normalized = Vec::with_capacity(r.targets.len());
        for t in &r.targets {
            let Some(canon) = canonical_target(t) else {
                bail!("unsupported theme target: {t}");
            };
            if !normalized.contains(&canon) {
                normalized.push(canon);
            }
        }
        let wanted = |s: &str| normalized.contains(&s);
        let real_home = is_real_home(home);
        let mut plan = Self {
            changes: vec![],
            applied: vec![],
            skipped: vec![],
            state: state(home),
        };
        if wanted("glazebar") {
            let config = home.join(".glzr/glazewm/config.yaml");
            if let Some(data) = read(&config)? {
                if let Ok(updated) = theme_text::focused_border(
                    std::str::from_utf8(&data)?,
                    &theme::accent(&p, r.light),
                ) {
                    plan.add(config, updated.into_bytes())?;
                }
            }
            plan.add(
                plan.state.join("glazebar.json"),
                json(&serde_json::json!({"name":theme::NAME,"palette":p,"light":r.light}))?,
            )?;
            plan.applied.push("glazebar".into());
        }
        if wanted("terminal") {
            let paths = terminal_paths(home);
            if paths.is_empty() {
                plan.skipped.push("terminal".into());
            } else {
                let mut count = 0;
                for path in paths {
                    if let Ok(source) = text(&path) {
                        if let Ok(updated) = theme_settings::terminal_settings(&source, &p) {
                            plan.add(path, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    plan.applied.push("terminal".into());
                } else {
                    plan.skipped.push("terminal".into());
                }
            }
        }
        if wanted("rio") {
            let dirs = rio_dirs(home);
            if dirs.is_empty() {
                plan.skipped.push("rio".into());
            } else {
                for dir in dirs {
                    plan.add(
                        dir.join("themes/glazebar-aether.toml"),
                        theme_text::rio_theme(&p).into_bytes(),
                    )?;
                    let config_path = dir.join("config.toml");
                    let source = text_raw(&config_path)?;
                    plan.add(
                        config_path,
                        theme_text::rio_settings(&source, "glazebar-aether").into_bytes(),
                    )?;
                }
                plan.applied.push("rio".into());
            }
        }
        if wanted("codex") {
            let dir = if real_home {
                std::env::var_os("CODEX_HOME")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".codex"))
            } else {
                home.join(".codex")
            };
            if real_home && !dir.exists() {
                plan.skipped.push("codex".into());
            } else {
                plan.add(
                    dir.join("themes/glazebar-aether.tmTheme"),
                    theme::textmate(&p).into_bytes(),
                )?;
                let path = dir.join("config.toml");
                let source = text_raw(&path)?;
                plan.add(path, theme_text::codex_settings(&source).into_bytes())?;
                plan.applied.push("codex".into());
            }
        }
        if wanted("vscode") {
            let code_dir = home.join("AppData/Roaming/Code");
            let path = code_dir.join("User/settings.json");
            let installed = !real_home
                || path.exists()
                || code_dir.exists()
                || home.join(".vscode").exists()
                || home.join("AppData/Local/Programs/Microsoft VS Code").exists();
            if !installed {
                plan.skipped.push("vscode".into());
            } else {
                let updated = theme_settings::vscode_settings(&text(&path)?, &p)?;
                plan.add(path, updated.into_bytes())?;
                plan.applied.push("vscode".into());
            }
        }
        if wanted("zed") {
            let dirs = zed_dirs(home);
            if dirs.is_empty() {
                plan.skipped.push("zed".into());
            } else {
                let mut count = 0;
                for dir in dirs {
                    let settings_path = dir.join("settings.json");
                    if let Ok(source) = text(&settings_path) {
                        if let Ok(updated) = theme_settings::zed_settings(&source, &p, r.light) {
                            plan.add(
                                dir.join("themes/glazebar-aether.json"),
                                json(&theme::zed_theme(&p, r.light))?,
                            )?;
                            plan.add(settings_path, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    plan.applied.push("zed".into());
                } else {
                    plan.skipped.push("zed".into());
                }
            }
        }
        if wanted("antigravity") {
            let dir = home.join(".gemini/antigravity-cli");
            if !dir.is_dir() {
                plan.skipped.push("antigravity".into());
            } else {
                let settings_path = dir.join("settings.json");
                if let Ok(source) = text(&settings_path) {
                    if let Ok(updated) = theme_settings::antigravity_settings(&source, r.light) {
                        plan.add(settings_path, updated.into_bytes())?;
                        plan.add(
                            dir.join("glazebar-theme.json"),
                            json(&theme::antigravity_theme(&p, r.light))?,
                        )?;
                        plan.applied.push("antigravity".into());
                    } else {
                        plan.skipped.push("antigravity".into());
                    }
                } else {
                    plan.skipped.push("antigravity".into());
                }
            }
        }
        if wanted("claude") {
            let dir = home.join(".claude");
            let dotfile = home.join(".claude.json");
            if !dir.is_dir() && !dotfile.is_file() {
                plan.skipped.push("claude".into());
            } else {
                let mut count = 0;
                if dir.is_dir() {
                    let settings_path = dir.join("settings.json");
                    if let Ok(source) = text(&settings_path) {
                        if let Ok(updated) = theme_settings::claude_settings(&source, r.light) {
                            plan.add(settings_path, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if dotfile.is_file() {
                    if let Ok(source) = text(&dotfile) {
                        if let Ok(updated) = theme_settings::claude_settings(&source, r.light) {
                            plan.add(dotfile, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    plan.applied.push("claude".into());
                } else {
                    plan.skipped.push("claude".into());
                }
            }
        }
        if wanted("opencode") {
            let dirs = opencode_dirs(home);
            if dirs.is_empty() {
                plan.skipped.push("opencode".into());
            } else {
                let mut count = 0;
                for dir in dirs {
                    let config_path = dir.join("opencode.json");
                    if let Ok(source) = text(&config_path) {
                        if let Ok(updated) =
                            theme_settings::opencode_settings(&source, "glazebar-aether")
                        {
                            plan.add(
                                dir.join("themes/glazebar-aether.json"),
                                json(&theme::opencode_theme(&p, r.light))?,
                            )?;
                            plan.add(config_path, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    plan.applied.push("opencode".into());
                } else {
                    plan.skipped.push("opencode".into());
                }
            }
        }
        if wanted("firefox") {
            let profiles = firefox_profiles(home);
            if profiles.is_empty() {
                plan.skipped.push("firefox".into());
            } else {
                for profile in profiles {
                    let css_path = profile.join("chrome/userChrome.css");
                    let css_source = text_raw(&css_path)?;
                    plan.add(
                        css_path,
                        theme_text::firefox_user_chrome(&css_source, &p, r.light).into_bytes(),
                    )?;
                    let js_path = profile.join("user.js");
                    let js_source = text_raw(&js_path)?;
                    plan.add(
                        js_path,
                        theme_text::firefox_user_js(&js_source, r.light).into_bytes(),
                    )?;
                }
                plan.applied.push("firefox".into());
            }
        }
        if wanted("chrome") {
            let prefs = chromium_prefs(home, "Google/Chrome/User Data");
            if prefs.is_empty() {
                plan.skipped.push("chrome".into());
            } else {
                let mut count = 0;
                for pref in prefs {
                    if let Ok(source) = text(&pref) {
                        if let Ok(updated) =
                            theme_settings::chromium_preferences(&source, &p, r.light)
                        {
                            plan.add(pref, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    plan.applied.push("chrome".into());
                } else {
                    plan.skipped.push("chrome".into());
                }
            }
        }
        if wanted("edge") {
            let prefs = chromium_prefs(home, "Microsoft/Edge/User Data");
            if prefs.is_empty() {
                plan.skipped.push("edge".into());
            } else {
                let mut count = 0;
                for pref in prefs {
                    if let Ok(source) = text(&pref) {
                        if let Ok(updated) =
                            theme_settings::chromium_preferences(&source, &p, r.light)
                        {
                            plan.add(pref, updated.into_bytes())?;
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    plan.applied.push("edge".into());
                } else {
                    plan.skipped.push("edge".into());
                }
            }
        }
        plan.add(
            plan.state.join("current-palette.json"),
            json(&serde_json::json!({"name":theme::NAME,"image":r.image,"mode":if r.mode.is_empty(){"normal"}else{&r.mode},"light":r.light,"palette":p}))?,
        )?;
        if wanted("windows") {
            ensure!(
                real_home,
                "Windows integration cannot use an overridden home"
            );
            let options = r
                .windows
                .as_ref()
                .context("choose Windows integration options")?;
            plan.changes
                .extend(crate::theme_windows::plan(&p, r.light, options)?);
            plan.applied.push("windows".into());
        }
        Ok(plan)
    }
    fn apply_with(&self, notify: impl Fn(&[Change]) -> Result<()>) -> Result<PathBuf> {
        let now = chrono::Local::now();
        let backup = self
            .state
            .join("backups")
            .join(format!("{}.json", now.format("%Y%m%d-%H%M%S.%9f")));
        let mut applied_changes = Vec::with_capacity(self.changes.len());
        let result: Result<()> = (|| {
            for c in &self.changes {
                let is_browser_pref = c
                    .path
                    .file_name()
                    .is_some_and(|n| n.eq_ignore_ascii_case("Preferences"));
                let current = match read_change(c) {
                    Ok(cur) => cur,
                    Err(e) if is_browser_pref => {
                        eprintln!("[theme] skipping locked browser preference: {e}");
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                if current.is_some() != c.existed
                    || (c.existed && current.as_deref() != Some(c.before.as_slice()))
                {
                    if is_browser_pref {
                        // Browser updated its Preferences concurrently; skip rather than aborting other apps.
                        continue;
                    }
                    bail!("settings changed while applying: {}", c.path.display());
                }
                if let Err(e) = write_change(c, &c.after) {
                    if is_browser_pref {
                        eprintln!("[theme] skipping locked browser preference write: {e}");
                        continue;
                    }
                    return Err(e);
                }
                applied_changes.push(c.clone());
            }
            atomic_write(
                &backup,
                &json(&Snapshot {
                    created: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
                    changes: applied_changes.clone(),
                })?,
            )?;
            notify(&applied_changes)?;
            atomic_write(
                &self.state.join("last-backup.txt"),
                backup.to_string_lossy().as_bytes(),
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = atomic_write(
                &backup,
                &json(&Snapshot {
                    created: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
                    changes: self.changes.clone(),
                })
                .unwrap_or_default(),
            );
            let mut failures = vec![];
            for c in applied_changes.iter().rev() {
                if let Err(e) = undo(c) {
                    failures.push(e.to_string());
                }
            }
            if let Err(e) = notify(&applied_changes) {
                failures.push(e.to_string());
            }
            bail!(
                "{e:#}; recovery snapshot: {}; rollback errors: {}",
                backup.display(),
                failures.join("; ")
            );
        }
        Ok(backup)
    }
}
fn read_change(c: &Change) -> Result<Option<Vec<u8>>> {
    if c.registry_key.is_empty() {
        read(&c.path)
    } else {
        crate::theme_windows::read(&c.registry_key, &c.registry_name)
    }
}
fn write_change(c: &Change, data: &[u8]) -> Result<()> {
    if c.registry_key.is_empty() {
        atomic_write(&c.path, data)
    } else {
        crate::theme_windows::write(&c.registry_key, &c.registry_name, Some(data))
    }
}
fn undo(c: &Change) -> Result<()> {
    if !c.registry_key.is_empty() {
        return crate::theme_windows::write(
            &c.registry_key,
            &c.registry_name,
            if c.existed { Some(&c.before) } else { None },
        );
    }
    if c.existed {
        atomic_write(&c.path, &c.before)
    } else {
        match fs::remove_file(&c.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
fn terminal_paths(home: &Path) -> Vec<PathBuf> {
    let packages = home.join("AppData/Local/Packages");
    let mut paths = vec![];
    if let Ok(entries) = fs::read_dir(packages) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("Microsoft.WindowsTerminal")
            {
                let path = entry.path().join("LocalState/settings.json");
                if path.is_file() {
                    paths.push(path);
                }
            }
        }
    }
    paths.sort();
    for folder in ["Terminal", "Terminal Preview"] {
        let path = home
            .join("AppData/Local/Microsoft/Windows")
            .join(folder)
            .join("settings.json");
        if path.is_file() {
            paths.push(path);
        }
    }
    paths
}
fn rio_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];
    for rel in ["AppData/Local/rio", ".config/rio", "AppData/Roaming/rio"] {
        let p = home.join(rel);
        if p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}
fn zed_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];
    for rel in ["AppData/Roaming/Zed", ".config/zed"] {
        let p = home.join(rel);
        if p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    if dirs.is_empty() && home.join("AppData/Local/Zed").is_dir() {
        dirs.push(home.join("AppData/Roaming/Zed"));
    }
    dirs
}
fn opencode_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![];
    for rel in [
        ".config/opencode",
        "AppData/Roaming/opencode",
        "AppData/Local/opencode",
    ] {
        let p = home.join(rel);
        if p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}
fn firefox_profiles(home: &Path) -> Vec<PathBuf> {
    let mut profiles = vec![];
    let root = home.join("AppData/Roaming/Mozilla/Firefox/Profiles");
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                profiles.push(p);
            }
        }
    }
    profiles.sort();
    profiles
}
fn chromium_prefs(home: &Path, vendor_rel: &str) -> Vec<PathBuf> {
    let mut prefs = vec![];
    let user_data = home.join("AppData/Local").join(vendor_rel);
    let default_pref = user_data.join("Default/Preferences");
    if default_pref.is_file() {
        prefs.push(default_pref);
    }
    if let Ok(entries) = fs::read_dir(&user_data) {
        let mut extra = vec![];
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("Profile ") {
                let p = entry.path().join("Preferences");
                if p.is_file() {
                    extra.push(p);
                }
            }
        }
        extra.sort();
        prefs.extend(extra);
    }
    prefs
}
fn reload(changes: &[Change]) -> Result<()> {
    crate::theme_windows::notify(changes);
    let Some(home) = std::env::var_os("USERPROFILE") else {
        return Ok(());
    };
    let config: PathBuf = PathBuf::from(home)
        .join(".glzr/glazewm/config.yaml")
        .components()
        .collect();
    if changes.iter().any(|c| {
        c.path
            .to_string_lossy()
            .eq_ignore_ascii_case(&config.to_string_lossy())
            && c.before != c.after
    }) {
        let _ = crate::ipc::Client::new().request("command wm-reload-config");
    }
    Ok(())
}
fn restore_with(home: &Path, notify: impl Fn(&[Change]) -> Result<()>) -> Result<PathBuf> {
    let pointer = state(home).join("last-backup.txt");
    let backup = PathBuf::from(String::from_utf8(fs::read(&pointer)?)?);
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(&backup)?)?;
    for c in &snapshot.changes {
        ensure!(
            c.registry_key.is_empty()
                || crate::theme_windows::valid(&c.registry_key, &c.registry_name),
            "unrecognized registry value in backup"
        );
        let is_browser_pref = c
            .path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("Preferences"));
        if !is_browser_pref {
            ensure!(
                read_change(c)?.as_deref() == Some(c.after.as_slice()),
                "file changed since theme was applied; backup retained: {}",
                c.path.display()
            );
        }
    }
    let mut done = 0;
    let result: Result<()> = (|| {
        for c in &snapshot.changes {
            let is_browser_pref = c
                .path
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("Preferences"));
            if is_browser_pref {
                if read_change(c).ok().flatten().as_deref() == Some(c.after.as_slice()) {
                    let _ = undo(c);
                }
            } else {
                undo(c)?;
            }
            done += 1;
        }
        notify(&snapshot.changes)?;
        fs::remove_file(pointer)?;
        Ok(())
    })();
    if let Err(e) = result {
        let mut failures = vec![];
        for c in snapshot.changes[..done].iter().rev() {
            if let Err(e) = write_change(c, &c.after) {
                failures.push(e.to_string());
            }
        }
        if let Err(e) = notify(&snapshot.changes[..done]) {
            failures.push(e.to_string());
        }
        bail!(
            "{e:#}; backup retained: {}; recovery errors: {}",
            backup.display(),
            failures.join("; ")
        );
    }
    Ok(backup)
}
pub fn run(home: &Path, request: &Request) -> Result<serde_json::Value> {
    if request.action == "search" {
        return Ok(serde_json::to_value(crate::wallhaven::search(
            &request.query,
            request.page,
            &std::sync::atomic::AtomicBool::new(false),
        )?)?);
    }
    if request.action == "download" {
        let folder = if request.folder.is_empty() {
            home.join("Pictures/Wallpapers")
        } else {
            PathBuf::from(&request.folder)
        };
        return Ok(
            serde_json::json!({"path":crate::wallhaven::download(&request.id,&folder,&std::sync::atomic::AtomicBool::new(false))?}),
        );
    }
    if request.action == "extract" && !request.palette.is_empty() {
        return Ok(serde_json::json!({"palette":theme::readable(&request.palette)?}));
    }
    if request.action == "compare" {
        return crate::theme_extract::compare(
            Path::new(&request.image),
            request.light,
            request.both,
            &request.modes,
        );
    }
    if request.action == "extract"
        || ((request.action == "apply" || request.action == "plan") && request.palette.is_empty())
    {
        let mut request = request.clone();
        request.palette =
            crate::theme_extract::extract(Path::new(&request.image), request.light, &request.mode)?
                .to_vec();
        if request.action == "extract" {
            return Ok(serde_json::json!({"palette":request.palette}));
        }
        return run(home, &request);
    }
    match request.action.as_str() {
        "apply" => {
            let plan = Plan::build(home, request)?;
            let backup = plan.apply_with(reload)?;
            Ok(
                serde_json::json!({"backup":backup,"applied":plan.applied,"skipped":plan.skipped,"palette":theme::readable(&request.palette)?}),
            )
        }
        "restore" => Ok(serde_json::json!({"backup":restore_with(home,reload)?})),
        "plan" => {
            let plan = Plan::build(home, request)?;
            Ok(
                serde_json::json!({"changes":plan.changes,"applied":plan.applied,"skipped":plan.skipped}),
            )
        }
        _ => bail!("Expected file theme action apply, restore or plan"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            action: "apply".into(),
            palette: vec!["#123456".into(); 16],
            targets: vec!["codex".into(), "vscode".into()],
            ..Default::default()
        }
    }
    #[test]
    fn missing_programs_are_skipped_and_next_programs_applied() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        // Only Rio and Zed exist; Terminal, Firefox, Chrome, Edge, OpenCode, Claude, Antigravity do NOT exist.
        fs::create_dir_all(home.join("AppData/Local/rio")).unwrap();
        fs::create_dir_all(home.join("AppData/Roaming/Zed")).unwrap();
        let r = Request {
            action: "apply".into(),
            palette: vec!["#123456".into(); 16],
            targets: vec![
                "terminal".into(),
                "rio-terminal".into(),
                "zed-editor".into(),
                "antigravity-cli".into(),
                "claude".into(),
                "opencode".into(),
                "firefox".into(),
                "chrome".into(),
                "edge".into(),
            ],
            ..Default::default()
        };
        let plan = Plan::build(home, &r).unwrap();
        assert_eq!(plan.applied, vec!["rio", "zed"]);
        assert!(plan.skipped.contains(&"terminal".to_owned()));
        assert!(plan.skipped.contains(&"firefox".to_owned()));
        assert!(plan.skipped.contains(&"chrome".to_owned()));
        plan.apply_with(|_| Ok(())).unwrap();
        assert!(home.join("AppData/Local/rio/themes/glazebar-aether.toml").exists());
        assert!(home.join("AppData/Roaming/Zed/themes/glazebar-aether.json").exists());
    }
    #[test]
    fn installed_new_targets_are_updated_and_restored() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        fs::create_dir_all(home.join(".gemini/antigravity-cli")).unwrap();
        fs::create_dir_all(home.join(".claude")).unwrap();
        fs::create_dir_all(home.join(".config/opencode")).unwrap();
        fs::create_dir_all(home.join("AppData/Roaming/Mozilla/Firefox/Profiles/abc.default-release")).unwrap();
        atomic_write(
            &home.join("AppData/Local/Microsoft/Edge/User Data/Default/Preferences"),
            b"{\"browser\":{}}",
        )
        .unwrap();
        let r = Request {
            action: "apply".into(),
            palette: vec!["#123456".into(); 16],
            targets: vec![
                "antigravity".into(),
                "claude".into(),
                "opencode".into(),
                "firefox".into(),
                "edge".into(),
            ],
            ..Default::default()
        };
        let plan = Plan::build(home, &r).unwrap();
        assert_eq!(
            plan.applied,
            vec!["antigravity", "claude", "opencode", "firefox", "edge"]
        );
        plan.apply_with(|_| Ok(())).unwrap();
        assert!(home.join(".gemini/antigravity-cli/settings.json").exists());
        assert!(home.join(".claude/settings.json").exists());
        assert!(home.join(".config/opencode/themes/glazebar-aether.json").exists());
        assert!(
            home.join("AppData/Roaming/Mozilla/Firefox/Profiles/abc.default-release/chrome/userChrome.css")
                .exists()
        );
        restore_with(home, |_| Ok(())).unwrap();
        assert!(!home.join(".config/opencode/themes/glazebar-aether.json").exists());
    }
    #[test]
    fn apply_restore_preserves_bytes_and_removes_created_files() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let code = home.join("AppData/Roaming/Code/User/settings.json");
        let before = b"\xef\xbb\xbf{ // retain\n\"editor.fontSize\":15,}";
        atomic_write(&code, before).unwrap();
        let plan = Plan::build(home, &request()).unwrap();
        let backup = plan.apply_with(|_| Ok(())).unwrap();
        let data: serde_json::Value = serde_json::from_slice(&fs::read(backup).unwrap()).unwrap();
        assert!(data["Changes"][0]["After"].is_string());
        assert!(home.join(".codex/config.toml").exists());
        restore_with(home, |_| Ok(())).unwrap();
        assert_eq!(fs::read(code).unwrap(), before);
        assert!(!home.join(".codex/config.toml").exists());
    }
    #[test]
    fn validation_precedes_all_writes() {
        let temp = tempfile::tempdir().unwrap();
        let code = temp.path().join("AppData/Roaming/Code/User/settings.json");
        atomic_write(&code, b"{broken").unwrap();
        assert!(Plan::build(temp.path(), &request()).is_err());
        assert!(!temp.path().join(".codex").exists());
    }
    #[test]
    fn failed_notification_rolls_back_and_keeps_snapshot() {
        let temp = tempfile::tempdir().unwrap();
        let plan = Plan::build(temp.path(), &request()).unwrap();
        assert!(plan.apply_with(|_| bail!("injected reload error")).is_err());
        assert!(!temp.path().join(".codex/config.toml").exists());
        assert!(
            fs::read_dir(state(temp.path()).join("backups"))
                .unwrap()
                .next()
                .is_some()
        );
    }
    #[test]
    fn newer_edits_are_never_restored_over() {
        let temp = tempfile::tempdir().unwrap();
        Plan::build(temp.path(), &request())
            .unwrap()
            .apply_with(|_| Ok(()))
            .unwrap();
        let path = temp.path().join(".codex/config.toml");
        fs::write(&path, b"user edit").unwrap();
        assert!(restore_with(temp.path(), |_| Ok(())).is_err());
        assert_eq!(fs::read(path).unwrap(), b"user edit");
    }
    #[test]
    fn concurrent_edit_rolls_back_earlier_writes_only() {
        let temp = tempfile::tempdir().unwrap();
        let plan = Plan::build(temp.path(), &request()).unwrap();
        let conflict = temp.path().join(".codex/config.toml");
        atomic_write(&conflict, b"new").unwrap();
        assert!(plan.apply_with(|_| Ok(())).is_err());
        assert_eq!(fs::read(conflict).unwrap(), b"new");
        assert!(
            !temp
                .path()
                .join(".codex/themes/glazebar-aether.tmTheme")
                .exists()
        );
    }
    #[test]
    fn failed_restore_notification_reinstates_applied_state() {
        let temp = tempfile::tempdir().unwrap();
        let plan = Plan::build(temp.path(), &request()).unwrap();
        assert!(
            plan.changes
                .iter()
                .all(|c| !c.path.to_string_lossy().contains('/'))
        );
        plan.apply_with(|_| Ok(())).unwrap();
        assert!(restore_with(temp.path(), |_| bail!("injected restore reload error")).is_err());
        for c in &plan.changes {
            assert_eq!(fs::read(&c.path).unwrap(), c.after);
        }
        assert!(state(temp.path()).join("last-backup.txt").exists());
        restore_with(temp.path(), |_| Ok(())).unwrap();
    }
}
