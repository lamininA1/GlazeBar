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
    state: PathBuf,
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
        for t in &r.targets {
            ensure!(
                ["glazebar", "terminal", "codex", "vscode", "windows"].contains(&t.as_str()),
                "unsupported theme target: {t}"
            );
        }
        let wanted = |s: &str| r.targets.iter().any(|v| v == s);
        let mut plan = Self {
            changes: vec![],
            applied: vec![],
            state: state(home),
        };
        if wanted("glazebar") {
            let config = home.join(".glzr/glazewm/config.yaml");
            if let Some(data) = read(&config)? {
                plan.add(
                    config,
                    theme_text::focused_border(
                        std::str::from_utf8(&data)?,
                        &theme::accent(&p, r.light),
                    )?
                    .into_bytes(),
                )?;
            }
            plan.add(
                plan.state.join("glazebar.json"),
                json(&serde_json::json!({"name":theme::NAME,"palette":p,"light":r.light}))?,
            )?;
            plan.applied.push("glazebar".into());
        }
        if wanted("terminal") {
            let paths = terminal_paths(home);
            ensure!(
                !paths.is_empty(),
                "Windows Terminal settings.json not found"
            );
            for path in paths {
                let updated = theme_settings::terminal_settings(&text(&path)?, &p)?;
                plan.add(path, updated.into_bytes())?;
            }
            plan.applied.push("terminal".into());
        }
        if wanted("codex") {
            let actual = std::env::var_os("USERPROFILE").map(PathBuf::from);
            let dir = if actual.as_deref() == Some(home) {
                std::env::var_os("CODEX_HOME")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(".codex"))
            } else {
                home.join(".codex")
            };
            plan.add(
                dir.join("themes/glazebar-aether.tmTheme"),
                theme::textmate(&p).into_bytes(),
            )?;
            let path = dir.join("config.toml");
            let source = String::from_utf8(read(&path)?.unwrap_or_default())?;
            plan.add(path, theme_text::codex_settings(&source).into_bytes())?;
            plan.applied.push("codex".into());
        }
        if wanted("vscode") {
            let path = home.join("AppData/Roaming/Code/User/settings.json");
            let updated = theme_settings::vscode_settings(&text(&path)?, &p)?;
            plan.add(path, updated.into_bytes())?;
            plan.applied.push("vscode".into());
        }
        plan.add(plan.state.join("current-palette.json"),json(&serde_json::json!({"name":theme::NAME,"image":r.image,"mode":if r.mode.is_empty(){"normal"}else{&r.mode},"light":r.light,"palette":p}))?)?;
        if wanted("windows") {
            ensure!(
                std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .as_deref()
                    == Some(home),
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
        atomic_write(
            &backup,
            &json(&Snapshot {
                created: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
                changes: self.changes.clone(),
            })?,
        )?;
        let mut done = 0;
        let result: Result<()> = (|| {
            for c in &self.changes {
                let current = read_change(c)?;
                ensure!(
                    current.is_some() == c.existed
                        && (!c.existed || current.as_deref() == Some(c.before.as_slice())),
                    "settings changed while applying: {}",
                    c.path.display()
                );
                write_change(c, &c.after)?;
                done += 1;
            }
            notify(&self.changes)?;
            atomic_write(
                &self.state.join("last-backup.txt"),
                backup.to_string_lossy().as_bytes(),
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            let mut failures = vec![];
            for c in self.changes[..done].iter().rev() {
                if let Err(e) = undo(c) {
                    failures.push(e.to_string());
                }
            }
            if let Err(e) = notify(&self.changes[..done]) {
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
        crate::ipc::Client::new().request("command wm-reload-config")?;
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
        ensure!(
            read_change(c)?.as_deref() == Some(c.after.as_slice()),
            "file changed since theme was applied; backup retained: {}",
            c.path.display()
        );
    }
    let mut done = 0;
    let result: Result<()> = (|| {
        for c in &snapshot.changes {
            undo(c)?;
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
                serde_json::json!({"backup":backup,"applied":plan.applied,"palette":theme::readable(&request.palette)?}),
            )
        }
        "restore" => Ok(serde_json::json!({"backup":restore_with(home,reload)?})),
        "plan" => {
            let plan = Plan::build(home, request)?;
            Ok(serde_json::json!({"changes":plan.changes,"applied":plan.applied}))
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
