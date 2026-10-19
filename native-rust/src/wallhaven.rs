use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{IpAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use url::Url;
pub const MAX_IMAGE_BYTES: u64 = 50 << 20;
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Thumbs {
    pub small: String,
}
#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Wallpaper {
    pub id: String,
    pub url: String,
    pub path: String,
    pub resolution: String,
    pub purity: String,
    pub thumbs: Thumbs,
}
#[derive(Deserialize)]
struct Meta {
    current_page: u32,
    last_page: u32,
}
#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<Wallpaper>,
    meta: Meta,
}
#[derive(Serialize)]
pub struct Search {
    pub wallpapers: Vec<Wallpaper>,
    pub page: u32,
    #[serde(rename = "lastPage")]
    pub last_page: u32,
}
pub fn valid_id(id: &str) -> bool {
    id.len() == 6
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}
fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && b == 168)
                || (a == 100 && (64..=127).contains(&b))
                || (a == 192 && b == 0 && (c == 0 || c == 2))
                || (a == 198 && (b == 18 || b == 19))
                || (a == 198 && b == 51 && c == 100)
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public(IpAddr::V4(v4));
            }
            let s = ip.segments();
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || (s[0] == 0x2001 && s[1] == 0xdb8))
        }
    }
}
fn validate_url(url: &Url, expected: Option<&str>) -> Result<()> {
    ensure!(url.scheme() == "https", "URL must use HTTPS");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "URL credentials are not allowed"
    );
    let host = url
        .host_str()
        .context("URL must include a host")?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if let Some(expected) = expected {
        ensure!(
            host == expected && url.port_or_known_default() == Some(443),
            "Unexpected wallpaper host"
        );
    }
    ensure!(
        !(host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
            || host == "home.arpa"
            || host.ends_with(".home.arpa")),
        "URL host must be public"
    );
    if let Some(url::Host::Ipv4(ip)) = url.host() {
        ensure!(public(IpAddr::V4(ip)), "URL address must be public");
    }
    if let Some(url::Host::Ipv6(ip)) = url.host() {
        ensure!(public(IpAddr::V6(ip)), "URL address must be public");
    }
    Ok(())
}
fn stopped(cancel: &AtomicBool) -> Result<()> {
    ensure!(!cancel.load(Ordering::Acquire), "Cancelled");
    Ok(())
}
fn get(mut url: Url, timeout: Duration, cancel: &AtomicBool) -> Result<ureq::Response> {
    let deadline = std::time::Instant::now() + timeout;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(native_tls::TlsConnector::new()?))
        .try_proxy_from_env(false)
        .redirects(0)
        .timeout(timeout)
        .timeout_connect(Duration::from_secs(15))
        .resolver(|netloc: &str| {
            let addresses: Vec<_> = netloc
                .to_socket_addrs()?
                .filter(|s| public(s.ip()))
                .collect();
            if addresses.is_empty() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "No public addresses",
                ));
            }
            Ok(addresses)
        })
        .build();
    for redirect in 0..5 {
        stopped(cancel)?;
        validate_url(&url, None)?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        ensure!(!remaining.is_zero(), "Wallhaven request timed out");
        let response = match agent
            .get(url.as_str())
            .timeout(remaining)
            .set("User-Agent", "GlazeBar/1.0 wallpaper selector")
            .call()
        {
            Ok(r) => r,
            Err(ureq::Error::Status(status, _)) => bail!("Wallhaven HTTP {status}"),
            Err(e) => return Err(e.into()),
        };
        if [301, 302, 303, 307, 308].contains(&response.status()) {
            ensure!(redirect < 4, "Too many redirects");
            url = url.join(
                response
                    .header("Location")
                    .context("Redirect has no location")?,
            )?;
            continue;
        }
        ensure!(
            response.status() == 200,
            "Wallhaven HTTP {}",
            response.status()
        );
        return Ok(response);
    }
    unreachable!()
}
fn bounded(
    mut reader: impl Read,
    mut output: impl Write,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<u64> {
    let mut buffer = [0u8; 65536];
    let mut total = 0;
    loop {
        stopped(cancel)?;
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        ensure!(total <= limit, "Download exceeds {limit}-byte limit");
        output.write_all(&buffer[..n])?;
    }
    Ok(total)
}
fn json(url: Url, cancel: &AtomicBool) -> Result<Vec<u8>> {
    validate_url(&url, Some("wallhaven.cc"))?;
    let response = get(url, Duration::from_secs(20), cancel)?;
    let mut bytes = Vec::new();
    bounded(response.into_reader(), &mut bytes, 2 << 20, cancel)?;
    Ok(bytes)
}
pub fn search_url(query: &str, page: i32) -> Url {
    let mut url = Url::parse("https://wallhaven.cc/api/v1/search").unwrap();
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("purity", "100")
        .append_pair("categories", "111")
        .append_pair("page", &page.max(1).to_string())
        .append_pair(
            "sorting",
            if query.is_empty() {
                "toplist"
            } else {
                "relevance"
            },
        );
    if query.is_empty() {
        url.query_pairs_mut().append_pair("topRange", "1M");
    }
    url
}
fn parse_search(bytes: &[u8]) -> Result<Search> {
    let r: SearchResponse = serde_json::from_slice(bytes)?;
    Ok(Search {
        wallpapers: r.data.into_iter().filter(|w| w.purity == "sfw").collect(),
        page: r.meta.current_page,
        last_page: r.meta.last_page,
    })
}
pub fn search(query: &str, page: i32, cancel: &AtomicBool) -> Result<Search> {
    parse_search(&json(search_url(query, page), cancel)?)
}
fn image_url(item: &Wallpaper, id: &str) -> Result<(Url, String)> {
    ensure!(
        valid_id(id) && item.id == id && item.purity == "sfw",
        "Only SFW wallpaper downloads are supported"
    );
    let url = Url::parse(&item.path)?;
    validate_url(&url, Some("w.wallhaven.cc"))?;
    let extension = Path::new(url.path())
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_owned();
    ensure!(
        ["jpg", "jpeg", "png"].contains(&extension.as_str()),
        "Unsupported wallpaper format"
    );
    Ok((url, extension))
}
fn validate_image(path: &Path) -> Result<()> {
    use image::ImageDecoder;
    let decoder = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0 && h > 0 && w <= 16384 && h <= 16384 && u64::from(w) * u64::from(h) <= 40_000_000,
        "Unsafe image dimensions {w}x{h}"
    );
    Ok(())
}
pub fn download(id: &str, folder: &Path, cancel: &AtomicBool) -> Result<PathBuf> {
    ensure!(valid_id(id), "Invalid Wallhaven ID");
    #[derive(Deserialize)]
    struct Details {
        data: Wallpaper,
    }
    let item: Details = serde_json::from_slice(&json(
        Url::parse(&format!("https://wallhaven.cc/api/v1/w/{id}"))?,
        cancel,
    )?)?;
    let (url, extension) = image_url(&item.data, id)?;
    let folder = std::path::absolute(folder)?;
    let path = folder.join(format!("wallhaven-{id}.{extension}"));
    match std::fs::symlink_metadata(&path) {
        Ok(meta) => {
            ensure!(
                meta.file_type().is_file() && meta.len() <= MAX_IMAGE_BYTES,
                "Cached download is not a regular file within size limit"
            );
            validate_image(&path)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let response = get(url, Duration::from_secs(60), cancel)?;
            if let Some(length) = response
                .header("Content-Length")
                .and_then(|s| s.parse::<u64>().ok())
            {
                ensure!(length <= MAX_IMAGE_BYTES, "Download exceeds size limit");
            }
            std::fs::create_dir_all(&folder)?;
            let mut temp = tempfile::Builder::new()
                .prefix(".part-")
                .tempfile_in(&folder)?;
            bounded(
                response.into_reader(),
                temp.as_file_mut(),
                MAX_IMAGE_BYTES,
                cancel,
            )?;
            temp.as_file_mut().sync_all()?;
            validate_image(temp.path())?;
            stopped(cancel)?;
            temp.persist_noclobber(&path).map_err(|e| e.error)?;
        }
        Err(e) => return Err(e.into()),
    }
    stopped(cancel)?;
    crate::theme_transaction::atomic_write(
        &PathBuf::from(format!("{}.source.json", path.display())),
        &serde_json::to_vec_pretty(
            &serde_json::json!({"provider":"Wallhaven","source":item.data.url,"downloaded":chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs,false),"wallpaper":item.data}),
        )?,
    )?;
    Ok(path)
}
pub fn thumbnail(url: &str, cancel: &AtomicBool) -> Result<image::RgbaImage> {
    let url = Url::parse(url)?;
    validate_url(&url, Some("th.wallhaven.cc"))?;
    let response = get(url, Duration::from_secs(12), cancel)?;
    let mut bytes = vec![];
    bounded(response.into_reader(), &mut bytes, 4 << 20, cancel)?;
    use image::ImageDecoder;
    let decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .into_decoder()?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 2_000_000,
        "Thumbnail exceeds size limit"
    );
    Ok(image::DynamicImage::from_decoder(decoder)?.into_rgba8())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_addresses_and_redirect_policy() {
        for ip in [
            "127.0.0.1",
            "10.0.0.2",
            "169.254.169.254",
            "100.64.1.2",
            "192.0.2.1",
            "198.19.1.2",
            "203.0.113.8",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
        ] {
            assert!(!public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public(ip.parse().unwrap()));
        }
        for url in [
            "http://wallhaven.cc/a",
            "https://user:pass@wallhaven.cc/a",
            "https://localhost/a",
            "https://x.home.arpa/a",
            "https://[::1]/a",
        ] {
            assert!(validate_url(&Url::parse(url).unwrap(), None).is_err());
        }
    }
    #[test]
    fn search_and_details_match_expected_contract() {
        let query = search_url("sky & 한글", 0);
        let pairs: std::collections::HashMap<_, _> = query.query_pairs().collect();
        assert_eq!(pairs["q"], "sky & 한글");
        assert_eq!(pairs["page"], "1");
        assert_eq!(pairs["purity"], "100");
        assert!(search_url("", 1).as_str().contains("topRange=1M"));
        let r=parse_search(br#"{"data":[{"id":"abc123","purity":"sfw"},{"id":"def456","purity":"sketchy"}],"meta":{"current_page":2,"last_page":7}}"#).unwrap();
        assert_eq!(r.wallpapers.len(), 1);
        assert_eq!(r.last_page, 7);
        let mut item = Wallpaper {
            id: "abc123".into(),
            purity: "sfw".into(),
            path: "https://w.wallhaven.cc/full/ab/wallhaven-abc123.jpg".into(),
            ..Default::default()
        };
        assert_eq!(image_url(&item, "abc123").unwrap().1, "jpg");
        for path in [
            "https://w.wallhaven.cc.evil.test/a.jpg",
            "https://w.wallhaven.cc/a.exe",
            "file:///a.jpg",
        ] {
            item.path = path.into();
            assert!(image_url(&item, "abc123").is_err());
        }
        assert!(!valid_id("../abc"));
        assert!(!valid_id("ABC123"));
    }
    #[test]
    fn size_limit_and_cancellation() {
        let cancel = AtomicBool::new(false);
        let mut out = vec![];
        assert!(bounded(&b"12345"[..], &mut out, 4, &cancel).is_err());
        assert!(out.is_empty());
        cancel.store(true, Ordering::Release);
        assert!(bounded(&b"1"[..], &mut out, 4, &cancel).is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        std::fs::write(&path, b"not an image").unwrap();
        assert!(validate_image(&path).is_err());
    }
}
