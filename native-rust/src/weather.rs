use anyhow::{Result, ensure};
use serde::Deserialize;
use std::{
    io::Read,
    sync::mpsc::{self, Sender},
    thread::JoinHandle,
    time::Duration,
};
#[derive(Clone, Debug, PartialEq)]
pub struct Weather {
    pub icon: String,
    pub text: String,
}
#[derive(Deserialize)]
struct Response {
    current_weather: Current,
}
#[derive(Deserialize)]
struct Current {
    temperature: f64,
    weathercode: i32,
    is_day: i32,
}
pub fn parse(bytes: &[u8]) -> Result<Weather> {
    let v: Response = serde_json::from_slice(bytes)?;
    let c = v.current_weather;
    ensure!(c.temperature.is_finite(), "Invalid weather temperature");
    let icon = match c.weathercode {
        0 => {
            if c.is_day == 0 {
                "moon"
            } else {
                "sun"
            }
        }
        95.. => "cloud-lightning",
        71..=86 => "cloud-snow",
        51.. => "cloud-rain",
        45.. => "cloud-fog",
        _ => "cloud",
    };
    Ok(Weather {
        icon: icon.into(),
        text: format!("{:.0}°C", c.temperature),
    })
}
pub fn fetch() -> Result<Weather> {
    let agent = ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(native_tls::TlsConnector::new()?))
        .timeout(Duration::from_secs(7))
        .build();
    let response=agent.get("https://api.open-meteo.com/v1/forecast?current_weather=true&latitude=37.5665&longitude=126.9780&timezone=Asia%2FSeoul").call()?;
    let mut bytes = vec![];
    response.into_reader().take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "Weather response too large");
    parse(&bytes)
}
pub struct Worker {
    stop: Sender<()>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn spawn(mut updated: impl FnMut(Result<Weather>) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = std::thread::spawn(move || {
            loop {
                updated(fetch());
                if rx.recv_timeout(Duration::from_secs(15 * 60))
                    != Err(mpsc::RecvTimeoutError::Timeout)
                {
                    break;
                }
            }
        });
        Self {
            stop: tx,
            join: Some(join),
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn daytime_and_night() {
        for (day, icon) in [(1, "sun"), (0, "moon")] {
            let bytes = format!(
                r#"{{"current_weather":{{"temperature":17,"weathercode":0,"is_day":{day}}}}}"#
            );
            let w = parse(bytes.as_bytes()).unwrap();
            assert_eq!(w.icon, icon);
            assert_eq!(w.text, "17°C");
        }
    }
    #[test]
    fn incomplete_response_rejected() {
        assert!(parse(b"{}").is_err());
    }
}
