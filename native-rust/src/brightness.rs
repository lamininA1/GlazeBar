use anyhow::{Result, ensure};
use std::{
    collections::HashMap,
    marker::PhantomData,
    mem::size_of,
    rc::Rc,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    Win32::System::{Com::*, Variant::VARIANT, Wmi::*},
    core::{BSTR, PCWSTR, w},
};
use windows_sys::Win32::Graphics::Gdi::{DISPLAY_DEVICEW, EnumDisplayDevicesW};

struct Apartment(PhantomData<Rc<()>>);
impl Apartment {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        Ok(Self(PhantomData))
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
pub struct Wmi {
    service: IWbemServices,
    _apartment: Apartment,
}
// windows::core::Error can own an IErrorInfo COM object. Convert failures to
// owned text while the apartment is still alive, so callers may retain errors
// after dropping Wmi (including the worker's reconnect path).
fn detached_error(error: anyhow::Error) -> anyhow::Error {
    anyhow::anyhow!("{error:#}")
}
fn brightness_unavailable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<windows::core::Error>().is_some_and(|e| {
        matches!(e.code().0, code if code == WBEM_E_NOT_SUPPORTED.0 || code == WBEM_E_INVALID_CLASS.0)
    })
}
impl Wmi {
    pub fn new() -> Result<Self> {
        let apartment = Apartment::new()?;
        let service = Self::connect().map_err(detached_error)?;
        Ok(Self {
            service,
            _apartment: apartment,
        })
    }
    fn connect() -> Result<IWbemServices> {
        unsafe {
            let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
            let service = locator.ConnectServer(
                &BSTR::from("ROOT\\WMI"),
                &BSTR::new(),
                &BSTR::new(),
                &BSTR::new(),
                0,
                &BSTR::new(),
                None,
            )?;
            CoSetProxyBlanket(
                &service,
                10,
                0,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )?;
            Ok(service)
        }
    }
    fn query(&self, class: &str) -> Result<Vec<IWbemClassObject>> {
        self.query_inner(class).map_err(detached_error)
    }
    fn query_inner(&self, class: &str) -> Result<Vec<IWbemClassObject>> {
        unsafe {
            let rows = self.service.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(format!("SELECT * FROM {class} WHERE Active = TRUE")),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )?;
            let mut output = vec![];
            loop {
                let mut row = [None];
                let mut count = 0;
                let hr = rows.Next(2000, &mut row, &mut count);
                hr.ok()?;
                ensure!(hr.0 != WBEM_S_TIMEDOUT.0, "WMI query timed out");
                if count == 0 {
                    break;
                }
                if let Some(row) = row[0].take() {
                    output.push(row)
                }
                ensure!(output.len() <= 64, "Too many brightness devices");
            }
            Ok(output)
        }
    }
    pub fn values(&self) -> Result<Vec<(String, u8)>> {
        self.values_inner().map_err(detached_error)
    }
    fn values_inner(&self) -> Result<Vec<(String, u8)>> {
        let mut out = vec![];
        let rows = match self.query_inner("WmiMonitorBrightness") {
            Ok(rows) => rows,
            // Desktop/external displays may have no WMI brightness provider.
            // This is a capability result, not a failed resident service.
            Err(error) if brightness_unavailable(&error) => return Ok(out),
            Err(error) => return Err(error),
        };
        for row in rows {
            let name = text(&row, "InstanceName")?;
            let value = u32::try_from(&get(&row, "CurrentBrightness")?)?;
            ensure!(value <= 100, "Invalid brightness value");
            out.push((name, value as u8));
        }
        Ok(out)
    }
    pub fn write(&self, instance: &str, value: u8) -> Result<()> {
        self.write_inner(instance, value).map_err(detached_error)
    }
    fn write_inner(&self, instance: &str, value: u8) -> Result<()> {
        ensure!(value <= 100, "Invalid brightness");
        let row = self
            .query("WmiMonitorBrightnessMethods")?
            .into_iter()
            .find(|r| text(r, "InstanceName").is_ok_and(|s| s.eq_ignore_ascii_case(instance)))
            .ok_or_else(|| anyhow::anyhow!("Brightness method not found"))?;
        let path = text(&row, "__PATH")?;
        unsafe {
            let mut class = None;
            self.service.GetObject(
                &BSTR::from("WmiMonitorBrightnessMethods"),
                WBEM_FLAG_RETURN_WBEM_COMPLETE,
                None,
                Some(&mut class),
                None,
            )?;
            let class = class.ok_or_else(|| anyhow::anyhow!("Brightness WMI class missing"))?;
            let mut signature = None;
            class.GetMethod(
                w!("WmiSetBrightness"),
                0,
                &mut signature,
                std::ptr::null_mut(),
            )?;
            let input = signature
                .ok_or_else(|| anyhow::anyhow!("Brightness WMI signature missing"))?
                .SpawnInstance(0)?;
            input.Put(w!("Timeout"), 0, &VARIANT::from(0i32), 0)?;
            input.Put(w!("Brightness"), 0, &VARIANT::from(i32::from(value)), 0)?;
            let mut output = None;
            self.service.ExecMethod(
                &BSTR::from(path),
                &BSTR::from("WmiSetBrightness"),
                WBEM_FLAG_RETURN_WBEM_COMPLETE,
                None,
                &input,
                Some(&mut output),
                None,
            )?;
            if let Some(output) = output {
                let code = u32::try_from(&get(&output, "ReturnValue")?)?;
                ensure!(code == 0, "WmiSetBrightness returned {code}");
            }
        }
        Ok(())
    }
}
fn get(row: &IWbemClassObject, name: &str) -> Result<VARIANT> {
    let mut value = VARIANT::default();
    let name = crate::graphics::wide(name);
    unsafe {
        row.Get(PCWSTR(name.as_ptr()), 0, &mut value, None, None)?;
    }
    Ok(value)
}
fn text(row: &IWbemClassObject, name: &str) -> Result<String> {
    Ok(BSTR::try_from(&get(row, name)?)?.to_string())
}
fn normalize_id(id: &str) -> String {
    let id = id.replace('#', "\\").to_uppercase();
    let id = id.strip_prefix("\\\\?\\").unwrap_or(&id);
    id.split("\\{").next().unwrap_or(id).to_string()
}
pub fn panel_id(device: &str) -> Option<String> {
    unsafe {
        let mut info = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if EnumDisplayDevicesW(crate::graphics::wide(device).as_ptr(), 0, &mut info, 1) == 0 {
            return None;
        }
        let end = info
            .DeviceID
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.DeviceID.len());
        let id = normalize_id(&String::from_utf16_lossy(&info.DeviceID[..end]));
        (!id.is_empty()).then_some(id)
    }
}
fn matching<'a>(rows: &'a [(String, u8)], panel: &str) -> Option<&'a (String, u8)> {
    if panel.is_empty() {
        return None;
    }
    rows.iter().find(|(name, _)| {
        normalize_id(name)
            .strip_prefix(panel)
            .is_some_and(|suffix| {
                suffix.is_empty() || suffix.starts_with('_') || suffix.starts_with('\\')
            })
    })
}
enum Command {
    Devices(Vec<String>),
    Set(String, u8, u64),
    Stop,
}
struct Device {
    panel: String,
    revision: u64,
    hold: Instant,
    last: Option<u8>,
}
pub struct Worker {
    tx: Sender<Command>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn spawn(
        mut updated: impl FnMut(String, Option<u8>, u64, Option<String>) + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = std::thread::spawn(move || {
            let mut service: Option<Wmi> = None;
            let mut devices: HashMap<String, Device> = HashMap::new();
            let mut pending: HashMap<String, (u8, u64)> = HashMap::new();
            let mut next_read = Instant::now();
            let mut flush: Option<Instant> = None;
            let mut retry = Instant::now();
            loop {
                let now = Instant::now();
                let deadline = flush.unwrap_or(next_read).min(next_read);
                match rx.recv_timeout(deadline.saturating_duration_since(now)) {
                    Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                    Ok(Command::Devices(names)) => {
                        let changed = names.len() != devices.len()
                            || names.iter().any(|name| !devices.contains_key(name));
                        devices.retain(|name, _| names.contains(name));
                        for name in names {
                            devices.entry(name.clone()).or_insert_with(|| Device {
                                panel: panel_id(&name).unwrap_or_default(),
                                revision: 0,
                                hold: Instant::now(),
                                last: None,
                            });
                        }
                        if changed {
                            next_read = Instant::now();
                        }
                    }
                    Ok(Command::Set(name, value, revision)) => {
                        pending.insert(name, (value, revision));
                        if flush.is_none() {
                            flush = Some(Instant::now() + Duration::from_millis(80));
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => (),
                }
                let now = Instant::now();
                if service.is_none() && now >= retry {
                    match Wmi::new() {
                        Ok(wmi) => service = Some(wmi),
                        Err(e) => {
                            for (name, d) in &devices {
                                updated(name.clone(), None, d.revision, Some(e.to_string()));
                            }
                            retry = now + Duration::from_secs(5);
                        }
                    }
                }
                if flush.is_some_and(|deadline| now >= deadline) {
                    flush = None;
                    for (name, (value, revision)) in pending.drain() {
                        if let Some(d) = devices.get_mut(&name) {
                            d.revision = revision;
                            d.hold = now + Duration::from_secs(2);
                            let result = (|| -> Result<()> {
                                let wmi = service
                                    .as_ref()
                                    .ok_or_else(|| anyhow::anyhow!("WMI unavailable"))?;
                                let rows = wmi.values()?;
                                let (instance, _) = matching(&rows, &d.panel)
                                    .filter(|_| !d.panel.is_empty())
                                    .ok_or_else(|| {
                                        anyhow::anyhow!("Brightness device unavailable")
                                    })?;
                                wmi.write(instance, value)
                            })();
                            match result {
                                Ok(()) => {
                                    d.last = Some(value);
                                    updated(name, Some(value), revision, None);
                                }
                                Err(e) => updated(name, d.last, revision, Some(e.to_string())),
                            }
                        }
                    }
                }
                if now >= next_read {
                    next_read = now + Duration::from_secs(1);
                    if let Some(wmi) = &service {
                        match wmi.values() {
                            Ok(rows) => {
                                if rows.is_empty() {
                                    // Recheck occasionally, or immediately after a monitor
                                    // change, rather than querying unsupported hardware each second.
                                    next_read = now + Duration::from_secs(60);
                                }
                                for (name, d) in &mut devices {
                                    if now < d.hold || pending.contains_key(name) {
                                        continue;
                                    }
                                    let value = if d.panel.is_empty() {
                                        None
                                    } else {
                                        matching(&rows, &d.panel).map(|(_, v)| *v)
                                    };
                                    if value != d.last {
                                        d.last = value;
                                        updated(name.clone(), value, d.revision, None);
                                    }
                                }
                            }
                            Err(e) => {
                                for (name, d) in &mut devices {
                                    updated(name.clone(), d.last, d.revision, Some(e.to_string()));
                                }
                                service = None;
                                retry = now + Duration::from_secs(5);
                            }
                        }
                    }
                }
            }
        });
        Self {
            tx,
            join: Some(join),
        }
    }
    pub fn devices(&self, names: Vec<String>) {
        let _ = self.tx.send(Command::Devices(names));
    }
    pub fn set(&self, name: String, value: u8, revision: u64) {
        let _ = self.tx.send(Command::Set(name, value.min(100), revision));
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
pub fn probe(devices: &[String], write_same: bool) -> Result<serde_json::Value> {
    let wmi = Wmi::new()?;
    let values = wmi.values()?;
    let mut rows = vec![];
    for name in devices {
        let panel = panel_id(name).unwrap_or_default();
        let value = matching(&values, &panel).filter(|_| !panel.is_empty());
        if write_same {
            if let Some((instance, value)) = value {
                wmi.write(instance, *value)?;
            }
        }
        rows.push(serde_json::json!({"device":name,"value":value.map(|(_,v)|v),"sameValueWrite":write_same&&value.is_some()}));
    }
    Ok(serde_json::Value::Array(rows))
}

pub fn validate_worker(names: Vec<String>) -> Result<serde_json::Value> {
    let (tx, rx) = mpsc::channel();
    let worker = Worker::spawn(move |name, value, revision, error| {
        let _ = tx.send((name, value, revision, error));
    });
    worker.devices(names);
    let deadline = Instant::now() + Duration::from_secs(8);
    let (name, value) = loop {
        let (name, value, _, error) =
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
        if let Some(error) = error {
            anyhow::bail!(error)
        }
        if let Some(value) = value {
            break (name, value);
        }
    };
    for revision in 1..=3 {
        worker.set(name.clone(), value, revision);
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut acknowledgements = vec![];
    loop {
        let (device, actual, revision, error) =
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
        if device != name {
            continue;
        }
        if let Some(error) = error {
            anyhow::bail!(error)
        }
        if revision > 0 {
            acknowledgements.push(revision);
            ensure!(
                actual == Some(value),
                "Brightness worker changed the requested value"
            );
            if revision == 3 {
                break;
            }
        }
    }
    ensure!(
        acknowledgements == vec![3],
        "Brightness writes were not coalesced: {acknowledgements:?}"
    );
    drop(worker);
    Ok(
        serde_json::json!({"device":name,"sameValue":value,"acknowledgements":acknowledgements,"coalesced":true,"shutdownJoined":true}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsupported_brightness_is_not_a_connection_failure() {
        for code in [WBEM_E_NOT_SUPPORTED.0, WBEM_E_INVALID_CLASS.0] {
            let error = windows::core::Error::from_hresult(windows::core::HRESULT(code));
            assert!(brightness_unavailable(&error.into()));
        }
        let denied =
            windows::core::Error::from_hresult(windows::core::HRESULT(0x80070005u32 as i32));
        assert!(!brightness_unavailable(&denied.into()));
        assert!(!brightness_unavailable(&anyhow::anyhow!(
            "unrelated failure"
        )));
    }
    #[test]
    fn query_error_outlives_com_apartment() {
        // An invalid class exercises WMI's real IErrorInfo failure path,
        // independent of whether this machine supports brightness control.
        for _ in 0..3 {
            let wmi = Wmi::new().expect("WMI connection");
            let error = wmi
                .query("GlazeBar_Nonexistent_Regression_Class")
                .unwrap_err();
            assert!(error.downcast_ref::<windows::core::Error>().is_none());
            drop(wmi);
            assert!(!error.to_string().is_empty());
            drop(error);
        }
    }
    #[test]
    fn device_identity() {
        assert_eq!(
            normalize_id("\\\\?\\DISPLAY#ABC#123#{GUID}"),
            "DISPLAY\\ABC\\123"
        );
        assert!(matching(&[("DISPLAY\\ABC\\123_0".into(), 42)], "DISPLAY\\ABC\\123").is_some());
        assert!(matching(&[("DISPLAY\\OTHER\\123_0".into(), 42)], "DISPLAY\\ABC\\123").is_none());
    }
}
