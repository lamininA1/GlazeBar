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
pub const SEARCH_CACHE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
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
    #[serde(default)]
    total: u64,
    #[serde(default)]
    seed: Option<String>,
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
    pub total: u64,
    pub seed: Option<String>,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sorting {
    #[default]
    Auto,
    Relevance,
    Toplist,
    DateAdded,
    Views,
    Favorites,
    Random,
}
impl Sorting {
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Relevance,
            Self::Relevance => Self::Toplist,
            Self::Toplist => Self::DateAdded,
            Self::DateAdded => Self::Views,
            Self::Views => Self::Favorites,
            Self::Favorites => Self::Random,
            Self::Random => Self::Auto,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "자동",
            Self::Relevance => "관련도",
            Self::Toplist => "인기",
            Self::DateAdded => "최신",
            Self::Views => "조회수",
            Self::Favorites => "저장 수",
            Self::Random => "무작위",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Filters {
    pub sorting: Sorting,
    pub categories: [bool; 3],
    pub minimum: usize,
    pub top_range: usize,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            sorting: Sorting::Auto,
            categories: [true; 3],
            minimum: 0,
            top_range: 3,
        }
    }
}
pub const MINIMUMS: [&str; 4] = ["", "1920x1080", "2560x1440", "3840x2160"];
pub const TOP_RANGES: [&str; 7] = ["1d", "3d", "1w", "1M", "3M", "6M", "1y"];
impl Filters {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.categories.iter().any(|v| *v),
            "카테고리를 하나 이상 선택해주세요"
        );
        ensure!(
            self.minimum < MINIMUMS.len() && self.top_range < TOP_RANGES.len(),
            "잘못된 검색 필터입니다"
        );
        Ok(())
    }
}
#[derive(Clone, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct SearchOptions {
    pub query: String,
    pub filters: Filters,
    pub seed: Option<String>,
}
impl SearchOptions {
    pub fn validate(&self) -> Result<()> {
        self.filters.validate()?;
        ensure!(
            self.query.chars().count() <= 2048,
            "검색어는 2048자까지 입력할 수 있습니다"
        );
        if let Some(seed) = &self.seed {
            ensure!(
                seed.len() == 6 && seed.bytes().all(|c| c.is_ascii_alphanumeric()),
                "잘못된 무작위 검색 시드입니다"
            );
        }
        Ok(())
    }
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
#[cfg(test)]
pub fn search_url(query: &str, page: i32) -> Url {
    search_options_url(
        &SearchOptions {
            query: query.trim().into(),
            ..Default::default()
        },
        page,
    )
    .unwrap()
}
pub fn search_options_url(options: &SearchOptions, page: i32) -> Result<Url> {
    options.validate()?;
    let query = options.query.trim();
    let sorting = match options.filters.sorting {
        Sorting::Auto if query.is_empty() => "toplist",
        Sorting::Auto | Sorting::Relevance => "relevance",
        Sorting::Toplist => "toplist",
        Sorting::DateAdded => "date_added",
        Sorting::Views => "views",
        Sorting::Favorites => "favorites",
        Sorting::Random => "random",
    };
    let categories: String = options
        .filters
        .categories
        .iter()
        .map(|v| if *v { '1' } else { '0' })
        .collect();
    let mut url = Url::parse("https://wallhaven.cc/api/v1/search").unwrap();
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("purity", "100")
        .append_pair("categories", &categories)
        .append_pair("page", &page.max(1).to_string())
        .append_pair("sorting", sorting)
        .append_pair("order", "desc");
    if sorting == "toplist" {
        url.query_pairs_mut()
            .append_pair("topRange", TOP_RANGES[options.filters.top_range]);
    }
    if options.filters.minimum != 0 {
        url.query_pairs_mut()
            .append_pair("atleast", MINIMUMS[options.filters.minimum]);
    }
    if sorting == "random" {
        if let Some(seed) = &options.seed {
            url.query_pairs_mut().append_pair("seed", seed);
        }
    }
    Ok(url)
}
fn parse_search(bytes: &[u8]) -> Result<Search> {
    let r: SearchResponse = serde_json::from_slice(bytes)?;
    Ok(Search {
        wallpapers: r.data.into_iter().filter(|w| w.purity == "sfw").collect(),
        page: r.meta.current_page.max(1),
        last_page: r.meta.last_page.max(1),
        total: r.meta.total,
        seed: r.meta.seed,
    })
}
pub fn search(query: &str, page: i32, cancel: &AtomicBool) -> Result<Search> {
    search_options(
        &SearchOptions {
            query: query.trim().into(),
            ..Default::default()
        },
        page,
        cancel,
    )
}
pub fn search_options(options: &SearchOptions, page: i32, cancel: &AtomicBool) -> Result<Search> {
    search_options_fresh(options, page, cancel, false)
}
pub fn search_options_fresh(
    options: &SearchOptions,
    page: i32,
    cancel: &AtomicBool,
    refresh: bool,
) -> Result<Search> {
    search_cached_in(
        options,
        page,
        cancel,
        crate::ui_cache::directory("wallhaven-search").as_deref(),
        refresh,
        |url, cancel| json(url.clone(), cancel),
    )
}
fn cache_path(directory: Option<&Path>, url: &Url, extension: &str) -> Option<PathBuf> {
    let hash =
        crate::theme_cache::hash("SHA256", 32, format!("wallhaven-v1|{url}").as_bytes()).ok()?;
    Some(directory?.join(format!("{hash}.{extension}")))
}
fn cached_bytes(path: &Path, limit: u64, max_age: Option<Duration>) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > limit {
        return None;
    }
    if let Some(age) = max_age {
        if meta.modified().ok()?.elapsed().ok()? > age {
            return None;
        }
    }
    std::fs::read(path).ok()
}
fn search_cached_in(
    options: &SearchOptions,
    page: i32,
    cancel: &AtomicBool,
    directory: Option<&Path>,
    refresh: bool,
    fetch: impl FnOnce(&Url, &AtomicBool) -> Result<Vec<u8>>,
) -> Result<Search> {
    stopped(cancel)?;
    let url = search_options_url(options, page)?;
    let cache = if options.filters.sorting == Sorting::Random && options.seed.is_none() {
        None
    } else {
        cache_path(directory, &url, "json")
    };
    if !refresh {
        if let Some(cache) = &cache {
            if let Some(bytes) = cached_bytes(cache, 2 << 20, Some(SEARCH_CACHE_AGE)) {
                if let Ok(result) = parse_search(&bytes) {
                    return Ok(result);
                }
            }
        }
    }
    let bytes = fetch(&url, cancel)?;
    stopped(cancel)?;
    let result = parse_search(&bytes)?;
    if let Some(cache) = cache {
        let _ = crate::theme_transaction::atomic_write(&cache, &bytes);
    }
    Ok(result)
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
    thumbnail_cached_in(
        url,
        cancel,
        crate::ui_cache::directory("wallhaven-thumbnail").as_deref(),
        |url, cancel| {
            let response = get(url.clone(), Duration::from_secs(8), cancel)?;
            let mut bytes = vec![];
            bounded(response.into_reader(), &mut bytes, 4 << 20, cancel)?;
            Ok(bytes)
        },
    )
}
fn thumbnail_cached_in(
    url: &str,
    cancel: &AtomicBool,
    directory: Option<&Path>,
    fetch: impl FnOnce(&Url, &AtomicBool) -> Result<Vec<u8>>,
) -> Result<image::RgbaImage> {
    stopped(cancel)?;
    let url = Url::parse(url)?;
    validate_url(&url, Some("th.wallhaven.cc"))?;
    let cache = cache_path(directory, &url, "jpg");
    if let Some(cache) = &cache {
        if let Some(bytes) = cached_bytes(cache, 2 << 20, None) {
            if let Ok(image) = decode_thumbnail(bytes, true) {
                return Ok(image);
            }
        }
    }
    let image = decode_thumbnail(fetch(&url, cancel)?, false)?;
    stopped(cancel)?;
    if let Some(cache) = cache {
        let save = || -> Result<()> {
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).into_rgb8();
            let mut bytes = vec![];
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82).encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )?;
            crate::theme_transaction::atomic_write(&cache, &bytes)
        };
        let _ = save();
    }
    Ok(image)
}
fn decode_thumbnail(bytes: Vec<u8>, cached: bool) -> Result<image::RgbaImage> {
    use image::ImageDecoder;
    let decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()?
        .into_decoder()?;
    let (w, h) = decoder.dimensions();
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= 2_000_000,
        "Thumbnail exceeds size limit"
    );
    ensure!(
        !cached || (w <= 480 && h <= 480),
        "Invalid thumbnail cache dimensions"
    );
    let image = image::DynamicImage::from_decoder(decoder)?;
    Ok(image.thumbnail(480, 480).into_rgba8())
}
#[derive(Clone, Default, Debug, Serialize)]
pub struct CacheStats {
    pub bytes: u64,
    pub previews: usize,
    pub searches: usize,
}
#[derive(Serialize, Deserialize)]
struct PreviewEntry {
    wallpaper: Wallpaper,
    seen: u64,
}
pub struct Gallery {
    pub rows: Vec<Wallpaper>,
    pub stats: CacheStats,
}
fn thumbnail_location(
    row: &Wallpaper,
    directory: Option<&Path>,
    extension: &str,
) -> Option<PathBuf> {
    if !valid_id(&row.id) || row.purity != "sfw" {
        return None;
    }
    let url = Url::parse(&row.thumbs.small).ok()?;
    validate_url(&url, Some("th.wallhaven.cc")).ok()?;
    cache_path(directory, &url, extension)
}
pub fn remember_preview(row: &Wallpaper) {
    let directory = crate::ui_cache::directory("wallhaven-thumbnail");
    let Some(path) = thumbnail_location(row, directory.as_deref(), "json") else {
        return;
    };
    let seen = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Ok(bytes) = serde_json::to_vec(&PreviewEntry {
        wallpaper: row.clone(),
        seen,
    }) {
        let _ = crate::theme_transaction::atomic_write(&path, &bytes);
    }
}
pub fn cached_thumbnail(row: &Wallpaper) -> Result<image::RgbaImage> {
    let directory = crate::ui_cache::directory("wallhaven-thumbnail");
    let path = thumbnail_location(row, directory.as_deref(), "jpg")
        .context("저장한 미리보기가 없습니다")?;
    decode_thumbnail(
        cached_bytes(&path, 2 << 20, None).context("저장한 미리보기가 없습니다")?,
        true,
    )
}
fn directory_stats(directory: Option<&Path>) -> (u64, usize) {
    let Some(entries) = directory.and_then(|p| std::fs::read_dir(p).ok()) else {
        return (0, 0);
    };
    let mut bytes = 0;
    let mut count = 0;
    for entry in entries.flatten() {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                bytes += meta.len();
                if entry.path().extension().is_some_and(|v| v == "jpg") {
                    count += 1
                }
            }
        }
    }
    (bytes, count)
}
pub fn cache_stats() -> CacheStats {
    let thumbnails = crate::ui_cache::directory("wallhaven-thumbnail");
    let searches = crate::ui_cache::directory("wallhaven-search");
    let (thumb_bytes, previews) = directory_stats(thumbnails.as_deref());
    let (search_bytes, _) = directory_stats(searches.as_deref());
    let searches = searches
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|e| {
            e.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .count()
        })
        .unwrap_or(0);
    CacheStats {
        bytes: thumb_bytes + search_bytes,
        previews,
        searches,
    }
}
pub fn gallery(saved: &[Wallpaper], hearts_only: bool, cancel: &AtomicBool) -> Result<Gallery> {
    gallery_in(
        crate::ui_cache::directory("wallhaven-thumbnail").as_deref(),
        saved,
        hearts_only,
        cancel,
    )
}
fn gallery_in(
    directory: Option<&Path>,
    saved: &[Wallpaper],
    hearts_only: bool,
    cancel: &AtomicBool,
) -> Result<Gallery> {
    let mut entries = std::collections::HashMap::<String, (u64, Wallpaper)>::new();
    if !hearts_only {
        if let Some(files) = directory.and_then(|p| std::fs::read_dir(p).ok()) {
            for file in files.flatten() {
                stopped(cancel)?;
                if file.path().extension().is_none_or(|e| e != "json") {
                    continue;
                }
                if let Some(raw) = cached_bytes(&file.path(), 65536, None) {
                    if let Ok(entry) = serde_json::from_slice::<PreviewEntry>(&raw) {
                        if thumbnail_location(&entry.wallpaper, directory, "jpg")
                            .is_some_and(|p| p.is_file())
                        {
                            let id = entry.wallpaper.id.clone();
                            if entries.get(&id).is_none_or(|(seen, _)| *seen < entry.seen) {
                                entries.insert(id, (entry.seen, entry.wallpaper));
                            }
                        }
                    }
                }
            }
        }
    }
    for row in saved {
        stopped(cancel)?;
        if valid_id(&row.id) && row.purity == "sfw" {
            entries
                .entry(row.id.clone())
                .or_insert((u64::MAX, row.clone()));
        }
    }
    let mut entries: Vec<_> = entries.into_values().collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.id.cmp(&b.1.id)));
    Ok(Gallery {
        rows: entries.into_iter().map(|(_, row)| row).collect(),
        stats: cache_stats(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_gallery_merges_deduplicates_and_keeps_hearts() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let row = Wallpaper {
            id: "abc123".into(),
            purity: "sfw".into(),
            thumbs: Thumbs {
                small: "https://th.wallhaven.cc/small/ab/abc123.jpg".into(),
            },
            ..Default::default()
        };
        let cache = thumbnail_location(&row, Some(directory.path()), "jpg").unwrap();
        image::RgbImage::new(32, 16).save(&cache).unwrap();
        let metadata = thumbnail_location(&row, Some(directory.path()), "json").unwrap();
        std::fs::write(
            &metadata,
            serde_json::to_vec(&PreviewEntry {
                wallpaper: row.clone(),
                seen: 1,
            })
            .unwrap(),
        )
        .unwrap();
        let another = Wallpaper {
            id: "def456".into(),
            purity: "sfw".into(),
            ..Default::default()
        };
        let gallery = gallery_in(
            Some(directory.path()),
            &[row.clone(), another.clone()],
            false,
            &cancel,
        )
        .unwrap();
        assert_eq!(gallery.rows.len(), 2);
        assert_eq!(directory_stats(Some(directory.path())).1, 1);
        let hearts = gallery_in(Some(directory.path()), &[another], true, &cancel).unwrap();
        assert_eq!(hearts.rows.len(), 1);
        assert_eq!(hearts.rows[0].id, "def456");
        std::fs::write(&metadata, b"broken").unwrap();
        assert!(
            gallery_in(Some(directory.path()), &[], false, &cancel)
                .unwrap()
                .rows
                .is_empty()
        );
        cancel.store(true, Ordering::Release);
        assert!(gallery_in(Some(directory.path()), &[row], true, &cancel).is_err());
    }
    #[test]
    fn search_cache_reuses_recovers_expires_and_refreshes() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let options = SearchOptions {
            query: "landscape".into(),
            ..Default::default()
        };
        let bytes=br#"{"data":[{"id":"abc123","purity":"sfw"}],"meta":{"current_page":1,"last_page":2,"total":30}}"#;
        let fetch = |_: &Url, _: &AtomicBool| Ok(bytes.to_vec());
        assert_eq!(
            search_cached_in(&options, 1, &cancel, Some(dir.path()), false, fetch)
                .unwrap()
                .total,
            30
        );
        search_cached_in(&options, 1, &cancel, Some(dir.path()), false, |_, _| {
            panic!("warm search cache fetched network")
        })
        .unwrap();
        let cache = cache_path(
            Some(dir.path()),
            &search_options_url(&options, 1).unwrap(),
            "json",
        )
        .unwrap();
        std::fs::write(&cache, b"broken").unwrap();
        search_cached_in(&options, 1, &cancel, Some(dir.path()), false, fetch).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&cache)
            .unwrap()
            .set_modified(std::time::SystemTime::now() - SEARCH_CACHE_AGE - Duration::from_secs(1))
            .unwrap();
        let fetched = std::cell::Cell::new(false);
        search_cached_in(&options, 1, &cancel, Some(dir.path()), false, |u, c| {
            fetched.set(true);
            fetch(u, c)
        })
        .unwrap();
        assert!(fetched.get());
        fetched.set(false);
        search_cached_in(&options, 1, &cancel, Some(dir.path()), true, |u, c| {
            fetched.set(true);
            fetch(u, c)
        })
        .unwrap();
        assert!(fetched.get());
        cancel.store(true, Ordering::Release);
        assert!(
            search_cached_in(
                &options,
                1,
                &cancel,
                Some(dir.path()),
                false,
                |_, _| panic!("cancelled request fetched")
            )
            .is_err()
        );
    }
    #[test]
    fn web_thumbnail_cache_is_small_persistent_and_self_healing() {
        let dir = tempfile::tempdir().unwrap();
        let cancel = AtomicBool::new(false);
        let url = "https://th.wallhaven.cc/small/ab/abc123.jpg";
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            960,
            540,
            image::Rgb([60, 100, 140]),
        ));
        let mut bytes = std::io::Cursor::new(vec![]);
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        let bytes = bytes.into_inner();
        let first =
            thumbnail_cached_in(url, &cancel, Some(dir.path()), |_, _| Ok(bytes.clone())).unwrap();
        assert_eq!(first.width(), 480);
        assert!(first.height() <= 480);
        let cache = cache_path(Some(dir.path()), &Url::parse(url).unwrap(), "jpg").unwrap();
        assert!(std::fs::metadata(&cache).unwrap().len() < 100_000);
        let warm = thumbnail_cached_in(url, &cancel, Some(dir.path()), |_, _| {
            panic!("warm thumbnail fetched network")
        })
        .unwrap();
        assert_eq!(warm.dimensions(), first.dimensions());
        std::fs::write(&cache, b"broken").unwrap();
        thumbnail_cached_in(url, &cancel, Some(dir.path()), |_, _| Ok(bytes)).unwrap();
        assert!(
            thumbnail_cached_in(
                "https://th.wallhaven.cc.evil.test/a.jpg",
                &cancel,
                Some(dir.path()),
                |_, _| panic!("invalid URL fetched")
            )
            .is_err()
        );
        cancel.store(true, Ordering::Release);
        assert!(
            thumbnail_cached_in(url, &cancel, Some(dir.path()), |_, _| panic!(
                "cancelled thumbnail fetched"
            ))
            .is_err()
        );
    }
    #[test]
    fn filters_encoding_seed_and_empty_pagination() {
        let options = SearchOptions {
            query: "+sky -people 한글 & sea".into(),
            filters: Filters {
                sorting: Sorting::Random,
                categories: [true, false, false],
                minimum: 1,
                ..Default::default()
            },
            seed: Some("aB12cD".into()),
        };
        let url = search_options_url(&options, 3).unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(pairs["q"], options.query);
        assert_eq!(pairs["categories"], "100");
        assert_eq!(pairs["atleast"], "1920x1080");
        assert_eq!(pairs["sorting"], "random");
        assert_eq!(pairs["seed"], "aB12cD");
        assert_eq!(pairs["page"], "3");
        assert_eq!(pairs["purity"], "100");
        assert!(!pairs.contains_key("topRange"));
        let mut invalid = options.clone();
        invalid.filters.categories = [false; 3];
        assert!(search_options_url(&invalid, 1).is_err());
        invalid = options.clone();
        invalid.seed = Some("bad&seed".into());
        assert!(search_options_url(&invalid, 1).is_err());
        invalid = options;
        invalid.filters.minimum = 99;
        assert!(search_options_url(&invalid, 1).is_err());
        let empty = parse_search(
            br#"{"data":[],"meta":{"current_page":0,"last_page":0,"total":0,"seed":null}}"#,
        )
        .unwrap();
        assert_eq!(empty.page, 1);
        assert_eq!(empty.last_page, 1);
        assert_eq!(empty.total, 0);
    }
    #[test]
    #[ignore = "explicit live Wallhaven verification; requires internet"]
    fn live_search_filters_and_pagination() {
        let cancel = AtomicBool::new(false);
        let mut options = SearchOptions {
            query: "landscape".into(),
            filters: Filters {
                sorting: Sorting::Random,
                categories: [true, false, false],
                minimum: 1,
                ..Default::default()
            },
            seed: None,
        };
        let first = search_options(&options, 1, &cancel).unwrap();
        assert!(!first.wallpapers.is_empty());
        assert!(first.total > 0);
        assert!(first.wallpapers.iter().all(|w| w.purity == "sfw"));
        options.seed = first.seed.clone();
        assert!(options.seed.is_some());
        let second = search_options(&options, 2, &cancel).unwrap();
        assert_eq!(second.page, 2);
        assert!(second.wallpapers.iter().all(|w| w.purity == "sfw"));
        // Some Wallhaven deployments regenerate the response seed even when
        // given a valid one. Verify our request, without claiming server-side
        // random ordering is stable across pages.
        assert_eq!(
            search_options_url(&options, 2)
                .unwrap()
                .query_pairs()
                .find(|(k, _)| k == "seed")
                .unwrap()
                .1,
            first.seed.unwrap()
        );
    }
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
