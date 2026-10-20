use anyhow::{Result, ensure};
use serde::Deserialize;
use std::{
    io::Read,
    sync::mpsc::{self, Sender},
    thread::JoinHandle,
    time::Duration,
};
#[derive(Clone, Copy, Debug)]
pub struct City {
    pub name: &'static str,
    pub lat: &'static str,
    pub lon: &'static str,
    pub tz: &'static str,
}
pub const CITIES: [City; 10] = [
    City {
        name: "서울",
        lat: "37.5665",
        lon: "126.9780",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "부산",
        lat: "35.1796",
        lon: "129.0756",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "인천",
        lat: "37.4563",
        lon: "126.7052",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "대구",
        lat: "35.8714",
        lon: "128.6014",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "대전",
        lat: "36.3504",
        lon: "127.3845",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "광주",
        lat: "35.1595",
        lon: "126.8526",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "제주",
        lat: "33.4996",
        lon: "126.5312",
        tz: "Asia%2FSeoul",
    },
    City {
        name: "도쿄",
        lat: "35.6762",
        lon: "139.6503",
        tz: "Asia%2FTokyo",
    },
    City {
        name: "뉴욕",
        lat: "40.7128",
        lon: "-74.0060",
        tz: "America%2FNew_York",
    },
    City {
        name: "런던",
        lat: "51.5074",
        lon: "-0.1278",
        tz: "Europe%2FLondon",
    },
];
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
    fetch_city(0)
}
pub fn fetch_city(city_index: usize) -> Result<Weather> {
    let city = CITIES.get(city_index).copied().unwrap_or(CITIES[0]);
    let agent = ureq::AgentBuilder::new()
        .tls_connector(std::sync::Arc::new(native_tls::TlsConnector::new()?))
        .timeout(Duration::from_secs(7))
        .build();
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?current_weather=true&latitude={}&longitude={}&timezone={}",
        city.lat, city.lon, city.tz
    );
    let response = agent.get(&url).call()?;
    let mut bytes = vec![];
    response.into_reader().take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "Weather response too large");
    parse(&bytes)
}
pub struct Worker {
    stop: Sender<Option<usize>>,
    join: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn spawn_with_city(
        initial_city: usize,
        mut updated: impl FnMut(Result<Weather>) + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Option<usize>>();
        let join = std::thread::spawn(move || {
            let mut city = initial_city;
            loop {
                updated(fetch_city(city));
                match rx.recv_timeout(Duration::from_secs(15 * 60)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Ok(Some(next_city)) => {
                        city = next_city;
                    }
                    Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        Self {
            stop: tx,
            join: Some(join),
        }
    }
    pub fn set_city(&self, city: usize) {
        let _ = self.stop.send(Some(city));
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop.send(None);
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
