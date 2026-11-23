//! Community news for the Overview page (OVERVIEW-003, owner decision 2026-11-23 item 4).
//!
//! The host fetches ONE fixed file, `news.json` in the public repository
//! Freshair129/kohbor-updates, at app start (one retry) and when the user presses retry. The
//! request carries no account, Steam or match data; GitHub sees the IP address and the time.
//! It is on by default, disclosed on first view and in Settings, and has an off switch.
//!
//! - Fixed host and paths only: no URL, host or query comes from the renderer or the feed.
//! - WinHTTP with no proxy, no redirects and no cookies (the draft-statistics transport).
//! - 256 KiB feed cap; the feed is validated against contracts/news-feed.schema.json rules and is
//!   either accepted whole or rejected whole (unknown post types are skipped, as the schema says).
//! - The last valid feed is cached with its fetch time; a failure keeps the cache.
//! - Images: only `images/...` paths of the same repository, at most 512 KiB, SHA-256 checked,
//!   served to the renderer as `data:` URLs (no new CSP host).
//! - Logs carry outcome codes only, never a URL, a query or feed content.

use crate::hero_stats::transport::{
    Cancellation, Endpoint, HttpResponse, TransportError, WinHttpTransport,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

pub const HOST: &str = "raw.githubusercontent.com";
const REPO_PATH: &str = "/Freshair129/kohbor-updates/main/";
pub const FEED_PATH: &str = "/Freshair129/kohbor-updates/main/news.json";
/// Shown in Settings and the disclosure; the only feed URL the app ever requests.
pub const FEED_URL: &str =
    "https://raw.githubusercontent.com/Freshair129/kohbor-updates/main/news.json";
pub const MAX_FEED_BYTES: usize = 256 * 1024;
pub const MAX_IMAGE_BYTES: usize = 512 * 1024;
pub const MAX_POSTS: usize = 50;
pub const SHOWN_POSTS: usize = 20;
const MAX_LINKS: usize = 3;
const MAX_URL_CHARS: usize = 2048;
const RETRY_DELAY: Duration = Duration::from_secs(3);

const FEED_ENDPOINT: Endpoint = Endpoint {
    host: HOST,
    body_limit: MAX_FEED_BYTES,
    log_module: "news",
};
const IMAGE_ENDPOINT: Endpoint = Endpoint {
    host: HOST,
    body_limit: MAX_IMAGE_BYTES,
    log_module: "news",
};

const CACHE_DIR: &str = "news-cache";
const CACHE_FEED: &str = "news.json";
const CACHE_META: &str = "meta.json";
const CACHE_IMAGES: &str = "images";
pub const PREFS_FILE: &str = "news-prefs.json";

// ---------------------------------------------------------------------------------------------
// Feed model and validation

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Localized {
    pub th: String,
    pub en: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostType {
    Announcement,
    AppUpdate,
    PatchNotes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostImage {
    /// `images/<file>` inside the same repository.
    pub path: String,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub alt: Localized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostLink {
    pub label: Localized,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Post {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: PostType,
    pub published_at: u64,
    pub expires_at: Option<u64>,
    pub pinned: bool,
    pub title: Localized,
    pub summary: Localized,
    pub body: Option<Localized>,
    pub image: Option<PostImage>,
    pub links: Vec<PostLink>,
    pub app_version: Option<String>,
    pub game_patch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub updated_at: u64,
    pub posts: Vec<Post>,
}

/// Why a feed body was rejected. The code is the only thing logged.
pub type Invalid = &'static str;

/// Parse `YYYY-MM-DDTHH:MM:SSZ` (UTC) to Unix seconds.
pub fn parse_utc(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
    {
        return None;
    }
    let num = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = text.get(range)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1970..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || d < 1
        || d > days_in_month[(m - 1) as usize]
        || hh > 23
        || mm > 59
        || ss > 59
    {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

fn text(value: Option<&Value>, max_chars: usize, allow_newlines: bool) -> Result<String, Invalid> {
    let s = value.and_then(Value::as_str).ok_or("text_missing")?;
    let count = s.chars().count();
    if count == 0 || count > max_chars {
        return Err("text_length");
    }
    if s.chars()
        .any(|c| c.is_control() && !(allow_newlines && c == '\n'))
    {
        return Err("text_control");
    }
    Ok(s.to_owned())
}

fn localized(
    value: Option<&Value>,
    max_chars: usize,
    newlines: bool,
) -> Result<Localized, Invalid> {
    let object = value
        .and_then(Value::as_object)
        .ok_or("localized_missing")?;
    Ok(Localized {
        th: text(object.get("th"), max_chars, newlines)?,
        en: text(object.get("en"), max_chars, newlines)?,
    })
}

fn nullable<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object.get(key).filter(|v| !v.is_null())
}

fn valid_id(id: &str) -> bool {
    (3..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `images/<name>.<webp|png|jpg|jpeg>`, one path component, no traversal.
pub fn valid_image_path(path: &str) -> bool {
    let Some(name) = path.strip_prefix("images/") else {
        return false;
    };
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    (1..=100).contains(&stem.len())
        && stem.as_bytes()[0].is_ascii_alphanumeric()
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !stem.contains("..")
        && matches!(ext, "webp" | "png" | "jpg" | "jpeg")
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `https://` followed by printable ASCII with no whitespace, quotes, angle brackets or backslash.
pub fn valid_link_url(url: &str) -> bool {
    url.len() <= MAX_URL_CHARS
        && url.len() > "https://".len()
        && url.starts_with("https://")
        && url.bytes().all(|b| {
            b.is_ascii_graphic() && !matches!(b, b'"' | b'\'' | b'<' | b'>' | b'\\' | b'`')
        })
}

fn short_string(value: Option<&Value>, max: usize) -> Result<Option<String>, Invalid> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s))
            if (1..=max).contains(&s.len())
                && s.bytes().all(|b| {
                    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+' | b'_')
                }) =>
        {
            Ok(Some(s.clone()))
        }
        Some(_) => Err("short_string"),
    }
}

fn dimension(value: Option<&Value>, max: u64) -> Result<u32, Invalid> {
    value
        .and_then(Value::as_u64)
        .filter(|v| (1..=max).contains(v))
        .map(|v| v as u32)
        .ok_or("image_dimension")
}

fn parse_post(object: &Map<String, Value>) -> Result<Option<Post>, Invalid> {
    let kind = match object.get("type").and_then(Value::as_str) {
        Some("announcement") => PostType::Announcement,
        Some("app_update") => PostType::AppUpdate,
        Some("patch_notes") => PostType::PatchNotes,
        // Unknown post types are skipped, so later feeds can add types.
        Some(_) => return Ok(None),
        None => return Err("type_missing"),
    };
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| valid_id(id))
        .ok_or("id")?
        .to_owned();
    let published_at = object
        .get("published_at")
        .and_then(Value::as_str)
        .and_then(parse_utc)
        .ok_or("published_at")?;
    let expires_at = match nullable(object, "expires_at") {
        None => None,
        Some(value) => Some(value.as_str().and_then(parse_utc).ok_or("expires_at")?),
    };
    let pinned = match object.get("pinned") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("pinned"),
    };
    let body = match nullable(object, "body") {
        None => None,
        Some(v) => Some(localized(Some(v), 4000, true)?),
    };
    let image = match nullable(object, "image") {
        None => None,
        Some(v) => {
            let image = v.as_object().ok_or("image")?;
            let path = image
                .get("path")
                .and_then(Value::as_str)
                .filter(|p| valid_image_path(p))
                .ok_or("image_path")?;
            let sha256 = image
                .get("sha256")
                .and_then(Value::as_str)
                .filter(|s| valid_sha256(s))
                .ok_or("image_sha256")?;
            Some(PostImage {
                path: path.to_owned(),
                sha256: sha256.to_owned(),
                width: dimension(image.get("width"), 1600)?,
                height: dimension(image.get("height"), 900)?,
                alt: localized(image.get("alt"), 280, false)?,
            })
        }
    };
    let links = match nullable(object, "links") {
        None => Vec::new(),
        Some(v) => {
            let items = v.as_array().ok_or("links")?;
            if items.len() > MAX_LINKS {
                return Err("links_count");
            }
            items
                .iter()
                .map(|item| {
                    let link = item.as_object().ok_or("link")?;
                    let url = link
                        .get("url")
                        .and_then(Value::as_str)
                        .filter(|u| valid_link_url(u))
                        .ok_or("link_url")?;
                    Ok(PostLink {
                        label: localized(link.get("label"), 80, false)?,
                        url: url.to_owned(),
                    })
                })
                .collect::<Result<Vec<_>, Invalid>>()?
        }
    };
    Ok(Some(Post {
        id,
        kind,
        published_at,
        expires_at,
        pinned,
        title: localized(object.get("title"), 80, false)?,
        summary: localized(object.get("summary"), 280, true)?,
        body,
        image,
        links,
        app_version: short_string(object.get("app_version"), 32)?,
        game_patch: short_string(object.get("game_patch"), 16)?,
    }))
}

/// Validate a complete feed body. Any structural error rejects the whole file.
pub fn parse_feed(bytes: &[u8]) -> Result<Feed, Invalid> {
    if bytes.len() > MAX_FEED_BYTES {
        return Err("too_large");
    }
    let value = kohbor_gsi::strict_json::parse(bytes).map_err(|_| "json")?;
    let root = value.as_object().ok_or("root")?;
    let version = root
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or("schema_version")?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("schema_version");
    }
    if parts[0] != "1" {
        return Err("schema_major");
    }
    let updated_at = root
        .get("updated_at")
        .and_then(Value::as_str)
        .and_then(parse_utc)
        .ok_or("updated_at")?;
    let posts = root.get("posts").and_then(Value::as_array).ok_or("posts")?;
    if posts.len() > MAX_POSTS {
        return Err("posts_count");
    }
    let mut ids = BTreeSet::new();
    let mut out = Vec::new();
    for post in posts {
        let object = post.as_object().ok_or("post")?;
        if let Some(post) = parse_post(object)? {
            if !ids.insert(post.id.clone()) {
                return Err("id_duplicate");
            }
            out.push(post);
        }
    }
    Ok(Feed {
        updated_at,
        posts: out,
    })
}

/// Posts to show at `now`: published, not expired, newest first, at most 20; only the newest
/// pinned post stays pinned.
pub fn visible_posts(feed: &Feed, now: u64) -> Vec<Post> {
    let mut posts: Vec<Post> = feed
        .posts
        .iter()
        .filter(|p| p.published_at <= now && p.expires_at.is_none_or(|e| e > now))
        .cloned()
        .collect();
    posts.sort_by(|a, b| b.published_at.cmp(&a.published_at).then(a.id.cmp(&b.id)));
    posts.truncate(SHOWN_POSTS);
    let mut pinned_seen = false;
    for post in &mut posts {
        if post.pinned {
            post.pinned = !pinned_seen;
            pinned_seen = true;
        }
    }
    posts
}

fn image_mime(path: &str, bytes: &[u8]) -> Option<&'static str> {
    let ext = path.rsplit_once('.').map(|(_, e)| e)?;
    match ext {
        "png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => Some("image/png"),
        "jpg" | "jpeg" if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) => Some("image/jpeg"),
        "webp" if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
            Some("image/webp")
        }
        _ => None,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Requests and transport

/// The only requests the news module can construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewsRequest {
    Feed,
    /// A path already checked by [`valid_image_path`].
    Image(String),
}

impl NewsRequest {
    pub fn path(&self) -> String {
        match self {
            Self::Feed => FEED_PATH.to_owned(),
            Self::Image(path) => format!("{REPO_PATH}{path}"),
        }
    }
}

pub(crate) trait NewsFetch: Send + Sync {
    fn get(&self, request: &NewsRequest) -> Result<HttpResponse, TransportError>;
}

#[derive(Default)]
pub(crate) struct WinHttpNews(WinHttpTransport);

impl NewsFetch for WinHttpNews {
    fn get(&self, request: &NewsRequest) -> Result<HttpResponse, TransportError> {
        let endpoint = match request {
            NewsRequest::Feed => FEED_ENDPOINT,
            NewsRequest::Image(path) if valid_image_path(path) => IMAGE_ENDPOINT,
            NewsRequest::Image(_) => return Err(TransportError::NotSent),
        };
        self.0
            .get_from(endpoint, &request.path(), &Cancellation::default())
    }
}

fn transport_code(error: TransportError) -> &'static str {
    match error {
        TransportError::Network => "network",
        TransportError::Timeout => "timeout",
        TransportError::TooLarge => "too_large",
        TransportError::Cancelled | TransportError::CancelledBeforeSend => "cancelled",
        TransportError::NotSent => "not_sent",
    }
}

// ---------------------------------------------------------------------------------------------
// Preferences

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewsPrefs {
    pub schema_version: u8,
    /// On by default (owner decision item 4); the user can switch it off.
    pub enabled: bool,
    /// The user has seen the "this contacts GitHub" disclosure on the Overview.
    pub disclosure_seen: bool,
}

impl Default for NewsPrefs {
    fn default() -> Self {
        Self {
            schema_version: 1,
            enabled: true,
            disclosure_seen: false,
        }
    }
}

fn load_prefs(path: &Path) -> NewsPrefs {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<NewsPrefs>(&bytes).ok())
        .filter(|p| p.schema_version == 1)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Service

#[derive(Debug, Clone)]
struct Cached {
    fetched_at: u64,
    feed: Feed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// No fetch attempted in this run.
    Idle,
    Loading,
    Fetched,
    Failed,
}

struct State {
    prefs: NewsPrefs,
    cache: Option<Cached>,
    phase: Phase,
    last_error: Option<&'static str>,
}

struct Inner {
    dir: PathBuf,
    fetch: Arc<dyn NewsFetch>,
    state: Mutex<State>,
    retry_delay: Duration,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone)]
pub(crate) struct NewsService {
    inner: Arc<Inner>,
}

impl NewsService {
    pub(crate) fn open(data_dir: &Path) -> Self {
        Self::with_fetch(data_dir, Arc::new(WinHttpNews::default()), RETRY_DELAY)
    }

    pub(crate) fn with_fetch(
        data_dir: &Path,
        fetch: Arc<dyn NewsFetch>,
        retry_delay: Duration,
    ) -> Self {
        let prefs = load_prefs(&data_dir.join(PREFS_FILE));
        let cache = load_cache(&data_dir.join(CACHE_DIR));
        Self {
            inner: Arc::new(Inner {
                dir: data_dir.to_owned(),
                fetch,
                state: Mutex::new(State {
                    prefs,
                    cache,
                    phase: Phase::Idle,
                    last_error: None,
                }),
                retry_delay,
            }),
        }
    }

    /// App start: one background fetch when news is on.
    pub(crate) fn start(&self) {
        if lock(&self.inner.state).prefs.enabled {
            self.spawn_fetch();
        }
    }

    /// The user's retry. Ignored while off or while a fetch is running.
    pub(crate) fn refresh(&self) -> Value {
        let started = {
            let state = lock(&self.inner.state);
            state.prefs.enabled && state.phase != Phase::Loading
        };
        if started {
            self.spawn_fetch();
        }
        json!({"started": started})
    }

    fn spawn_fetch(&self) {
        {
            let mut state = lock(&self.inner.state);
            if state.phase == Phase::Loading {
                return;
            }
            state.phase = Phase::Loading;
        }
        let service = self.clone();
        if std::thread::Builder::new()
            .name("news-fetch".into())
            .spawn(move || service.fetch_now())
            .is_err()
        {
            let mut state = lock(&self.inner.state);
            state.phase = Phase::Failed;
            state.last_error = Some("not_sent");
        }
    }

    /// Fetch, validate and cache (blocking). Two attempts at most.
    pub(crate) fn fetch_now(&self) {
        lock(&self.inner.state).phase = Phase::Loading;
        let mut outcome: Result<(Feed, Vec<u8>), &'static str> = Err("not_sent");
        for attempt in 1..=2u8 {
            if attempt == 2 {
                std::thread::sleep(self.inner.retry_delay);
            }
            let (result, http_status) = match self.inner.fetch.get(&NewsRequest::Feed) {
                Ok(response) if response.status == 200 => (
                    parse_feed(&response.body).map(|feed| (feed, response.body)),
                    Some(response.status),
                ),
                Ok(response) => (Err("http_status"), Some(response.status)),
                Err(error) => (Err(transport_code(error)), None),
            };
            crate::log_info!(
                "news",
                "feed_fetch",
                "news feed fetch attempt finished",
                attempt = attempt,
                outcome = result.as_ref().map_or_else(|code| *code, |_| "ok"),
                http_status = http_status
            );
            outcome = result;
            if outcome.is_ok() {
                break;
            }
        }
        match outcome {
            Ok((feed, bytes)) => {
                let fetched_at = now_unix();
                let cache_dir = self.inner.dir.join(CACHE_DIR);
                if save_cache(&cache_dir, &bytes, fetched_at).is_err() {
                    crate::log_warn!(
                        "news",
                        "cache_write_failed",
                        "could not cache the news feed",
                        code = "NEWS_CACHE_WRITE_FAILED"
                    );
                }
                self.fetch_images(&feed, &cache_dir);
                let mut state = lock(&self.inner.state);
                state.cache = Some(Cached { fetched_at, feed });
                state.phase = Phase::Fetched;
                state.last_error = None;
            }
            Err(code) => {
                let mut state = lock(&self.inner.state);
                state.phase = Phase::Failed;
                state.last_error = Some(code);
            }
        }
    }

    fn fetch_images(&self, feed: &Feed, cache_dir: &Path) {
        let images_dir = cache_dir.join(CACHE_IMAGES);
        let wanted: Vec<PostImage> = visible_posts(feed, now_unix())
            .into_iter()
            .filter_map(|p| p.image)
            .collect();
        let mut keep = BTreeSet::new();
        for image in &wanted {
            let file = image_file_name(image);
            keep.insert(file.clone());
            let target = images_dir.join(&file);
            if read_image(&target, image).is_some() {
                continue;
            }
            let outcome = match self
                .inner
                .fetch
                .get(&NewsRequest::Image(image.path.clone()))
            {
                Ok(r) if r.status == 200 => {
                    if r.body.len() > MAX_IMAGE_BYTES {
                        "too_large"
                    } else if sha256_hex(&r.body) != image.sha256 {
                        "sha256_mismatch"
                    } else if image_mime(&image.path, &r.body).is_none() {
                        "format"
                    } else if fs::create_dir_all(&images_dir).is_err()
                        || crate::atomic::write(&target, &r.body).is_err()
                    {
                        "write_failed"
                    } else {
                        "ok"
                    }
                }
                Ok(_) => "http_status",
                Err(error) => transport_code(error),
            };
            crate::log_info!(
                "news",
                "image_fetch",
                "news image fetch finished",
                outcome = outcome
            );
        }
        // Drop cached images no longer referenced by the shown posts.
        if let Ok(read) = fs::read_dir(&images_dir) {
            for entry in read.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !keep.contains(&name) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    pub(crate) fn prefs(&self) -> NewsPrefs {
        lock(&self.inner.state).prefs
    }

    /// Patch `{enabled?: bool, disclosure_seen?: bool}`. Turning news on starts a fetch when
    /// none has run in this session.
    pub(crate) fn set_prefs(&self, patch: &Value) -> Result<NewsPrefs, String> {
        let object = patch.as_object().ok_or("SOURCE_INVALID")?;
        if object.is_empty()
            || object
                .keys()
                .any(|k| k != "enabled" && k != "disclosure_seen")
        {
            return Err("SOURCE_INVALID".to_owned());
        }
        let mut next = self.prefs();
        for (key, value) in object {
            let flag = value.as_bool().ok_or("SOURCE_INVALID")?;
            match key.as_str() {
                "enabled" => next.enabled = flag,
                _ => next.disclosure_seen = flag,
            }
        }
        let bytes = serde_json::to_vec(&next).map_err(|_| "NEWS_PREFS_WRITE_FAILED")?;
        crate::atomic::write(&self.inner.dir.join(PREFS_FILE), &bytes)
            .map_err(|_| "NEWS_PREFS_WRITE_FAILED".to_owned())?;
        let start = {
            let mut state = lock(&self.inner.state);
            let start = next.enabled && !state.prefs.enabled && state.phase == Phase::Idle;
            state.prefs = next;
            start
        };
        crate::log_info!(
            "news",
            "prefs_changed",
            "news preferences changed",
            enabled = next.enabled
        );
        if start {
            self.spawn_fetch();
        }
        Ok(next)
    }

    /// The renderer view. No URL of the feed host is exposed; images are `data:` URLs.
    pub(crate) fn view(&self) -> Value {
        self.view_at(now_unix())
    }

    fn view_at(&self, now: u64) -> Value {
        let (prefs, cache, phase, last_error) = {
            let state = lock(&self.inner.state);
            (
                state.prefs,
                state.cache.clone(),
                state.phase,
                state.last_error,
            )
        };
        let state = if !prefs.enabled {
            "disabled"
        } else {
            match (phase, cache.is_some()) {
                (Phase::Idle | Phase::Loading, _) => "loading",
                (Phase::Fetched, _) => "ok",
                (Phase::Failed, true) => "offline",
                (Phase::Failed, false) => "unavailable",
            }
        };
        let images_dir = self.inner.dir.join(CACHE_DIR).join(CACHE_IMAGES);
        let posts: Vec<Value> = match (&cache, prefs.enabled) {
            (Some(cached), true) => visible_posts(&cached.feed, now)
                .into_iter()
                .map(|post| {
                    let image = post.image.as_ref().map(|image| {
                        json!({
                            "data_url": read_image(&images_dir.join(image_file_name(image)), image),
                            "width": image.width,
                            "height": image.height,
                            "alt": image.alt,
                        })
                    });
                    json!({
                        "id": post.id,
                        "type": post.kind,
                        "published_at": post.published_at,
                        "pinned": post.pinned,
                        "title": post.title,
                        "summary": post.summary,
                        "body": post.body,
                        "image": image,
                        "links": post.links.iter().map(|l| json!({"label": l.label, "url": l.url})).collect::<Vec<_>>(),
                        "app_version": post.app_version,
                        "game_patch": post.game_patch,
                    })
                })
                .collect(),
            _ => Vec::new(),
        };
        json!({
            "state": state,
            "enabled": prefs.enabled,
            "disclosure_seen": prefs.disclosure_seen,
            "feed_url": FEED_URL,
            "fetched_at": cache.as_ref().map(|c| c.fetched_at),
            "last_error": last_error,
            "posts": posts,
        })
    }

    /// The validated https URL of a shown post's link, looked up by post id and index. The
    /// renderer never supplies a URL.
    pub(crate) fn link_url(&self, post_id: &str, index: usize) -> Option<String> {
        let state = lock(&self.inner.state);
        if !state.prefs.enabled {
            return None;
        }
        let cached = state.cache.as_ref()?;
        visible_posts(&cached.feed, now_unix())
            .into_iter()
            .find(|p| p.id == post_id)?
            .links
            .get(index)
            .map(|l| l.url.clone())
            .filter(|url| valid_link_url(url))
    }
}

fn image_file_name(image: &PostImage) -> String {
    let ext = image.path.rsplit_once('.').map_or("bin", |(_, e)| e);
    format!("{}.{ext}", image.sha256)
}

/// A cached image as a `data:` URL, only when its bytes still match the declared hash and format.
fn read_image(path: &Path, image: &PostImage) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > MAX_IMAGE_BYTES || sha256_hex(&bytes) != image.sha256 {
        return None;
    }
    let mime = image_mime(&image.path, &bytes)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheMeta {
    schema_version: String,
    fetched_at: u64,
    sha256: String,
}

fn save_cache(dir: &Path, bytes: &[u8], fetched_at: u64) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    crate::atomic::write(&dir.join(CACHE_FEED), bytes)?;
    let meta = CacheMeta {
        schema_version: "1.0.0".into(),
        fetched_at,
        sha256: sha256_hex(bytes),
    };
    let meta = serde_json::to_vec(&meta).map_err(std::io::Error::other)?;
    crate::atomic::write(&dir.join(CACHE_META), &meta)
}

fn load_cache(dir: &Path) -> Option<Cached> {
    let meta: CacheMeta = serde_json::from_slice(&fs::read(dir.join(CACHE_META)).ok()?).ok()?;
    let bytes = fs::read(dir.join(CACHE_FEED)).ok()?;
    if meta.schema_version != "1.0.0" || sha256_hex(&bytes) != meta.sha256 {
        return None;
    }
    let feed = parse_feed(&bytes).ok()?;
    Some(Cached {
        fetched_at: meta.fetched_at,
        feed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const SAMPLE: &str = include_str!("../../../../docs/examples/news.sample.json");
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-synthetic-image-bytes-";

    fn temp_dir(tag: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kohbor-news-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Scripted transport: never touches the network.
    struct Fake {
        responses: Mutex<Vec<Result<HttpResponse, TransportError>>>,
        requests: Mutex<Vec<NewsRequest>>,
        image: Vec<u8>,
    }

    impl Fake {
        fn new(responses: Vec<Result<HttpResponse, TransportError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
                image: PNG.to_vec(),
            })
        }
    }

    impl NewsFetch for Fake {
        fn get(&self, request: &NewsRequest) -> Result<HttpResponse, TransportError> {
            lock(&self.requests).push(request.clone());
            if let NewsRequest::Image(_) = request {
                return Ok(ok(self.image.clone()));
            }
            let mut queue = lock(&self.responses);
            if queue.is_empty() {
                Err(TransportError::Network)
            } else {
                queue.remove(0)
            }
        }
    }

    fn ok(body: Vec<u8>) -> HttpResponse {
        HttpResponse {
            status: 200,
            retry_after: None,
            body,
        }
    }

    fn feed_json(posts: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": "1.0.0",
            "updated_at": "2026-11-23T02:40:00Z",
            "posts": posts,
        }))
        .unwrap()
    }

    fn post(id: &str, kind: &str, published: &str) -> Value {
        json!({
            "id": id, "type": kind, "published_at": published, "pinned": false, "expires_at": null,
            "title": {"th": "หัวข้อ", "en": "Title"},
            "summary": {"th": "สรุป", "en": "Summary"},
        })
    }

    #[test]
    fn utc_parser_matches_known_instants_and_rejects_other_shapes() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2026-11-23T02:40:00Z"), Some(1_795_401_600));
        assert_eq!(parse_utc("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(parse_utc("2023-02-29T12:00:00Z"), None);
        assert_eq!(parse_utc("2026-11-23T02:40:00+07:00"), None);
        assert_eq!(parse_utc("2026-11-23 02:40:00Z"), None);
    }

    #[test]
    fn the_sample_feed_is_valid_and_has_one_post_of_each_type() {
        let feed = parse_feed(SAMPLE.as_bytes()).expect("sample feed validates");
        let kinds: BTreeSet<String> = feed
            .posts
            .iter()
            .map(|p| {
                serde_json::to_value(p.kind)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(kinds.len(), 3);
        assert!(feed.posts.iter().any(|p| p.pinned));
    }

    #[test]
    fn invalid_feeds_are_rejected_whole_and_unknown_types_are_skipped() {
        let good = post("good-post", "announcement", "2026-11-20T00:00:00Z");
        let skipped = post("future-type", "poll", "2026-11-20T00:00:00Z");
        let feed = parse_feed(&feed_json(json!([good.clone(), skipped]))).unwrap();
        assert_eq!(feed.posts.len(), 1);

        let mut bad_image = good.clone();
        bad_image["image"] = json!({"path": "https://example.invalid/x.png", "sha256": "0".repeat(64),
            "width": 10, "height": 10, "alt": {"th": "ก", "en": "a"}});
        let mut traversal = good.clone();
        traversal["image"] = json!({"path": "images/../news.json", "sha256": "0".repeat(64),
            "width": 10, "height": 10, "alt": {"th": "ก", "en": "a"}});
        let mut http_link = good.clone();
        http_link["links"] =
            json!([{"label": {"th": "ก", "en": "a"}, "url": "http://example.invalid"}]);
        let mut long_title = good.clone();
        long_title["title"]["en"] = json!("x".repeat(81));
        let mut missing_thai = good.clone();
        missing_thai["summary"] = json!({"en": "only english"});
        for (name, posts) in [
            ("image url", json!([bad_image])),
            ("image traversal", json!([traversal])),
            ("http link", json!([http_link])),
            ("long title", json!([long_title])),
            ("missing th", json!([missing_thai])),
            ("duplicate id", json!([good.clone(), good.clone()])),
        ] {
            assert!(parse_feed(&feed_json(posts)).is_err(), "{name}");
        }
        let major_two =
            br#"{"schema_version":"2.0.0","updated_at":"2026-11-23T02:40:00Z","posts":[]}"#;
        assert_eq!(parse_feed(major_two), Err("schema_major"));
        let dup_key = br#"{"schema_version":"1.0.0","schema_version":"1.0.0","updated_at":"2026-11-23T02:40:00Z","posts":[]}"#;
        assert!(parse_feed(dup_key).is_err());
        assert_eq!(
            parse_feed(&vec![b' '; MAX_FEED_BYTES + 1]),
            Err("too_large")
        );
    }

    #[test]
    fn visible_posts_hide_future_and_expired_and_keep_one_pin() {
        let mut a = post("pinned-old", "announcement", "2026-11-10T00:00:00Z");
        a["pinned"] = json!(true);
        let mut b = post("pinned-new", "announcement", "2026-11-20T00:00:00Z");
        b["pinned"] = json!(true);
        let future = post("future-post", "app_update", "2027-01-01T00:00:00Z");
        let mut expired = post("expired-post", "patch_notes", "2026-11-01T00:00:00Z");
        expired["expires_at"] = json!("2026-11-02T00:00:00Z");
        let feed = parse_feed(&feed_json(json!([a, b, future, expired]))).unwrap();
        let now = parse_utc("2026-11-23T00:00:00Z").unwrap();
        let shown = visible_posts(&feed, now);
        let ids: Vec<&str> = shown.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["pinned-new", "pinned-old"]);
        assert!(shown[0].pinned && !shown[1].pinned);
    }

    #[test]
    fn requests_are_only_the_fixed_feed_and_same_repo_images() {
        assert_eq!(
            NewsRequest::Feed.path(),
            "/Freshair129/kohbor-updates/main/news.json"
        );
        assert_eq!(
            NewsRequest::Image("images/a.png".into()).path(),
            "/Freshair129/kohbor-updates/main/images/a.png"
        );
        assert_eq!(FEED_URL, format!("https://{HOST}{FEED_PATH}"));
        assert!(!valid_image_path("images/sub/a.png"));
        assert!(!valid_image_path("images/.hidden.png"));
        assert!(!valid_image_path("images/a.gif"));
        assert!(valid_image_path("images/2026-11-overview.webp"));
        // A crafted path never reaches the transport.
        let fetch = WinHttpNews::default();
        assert_eq!(
            fetch.get(&NewsRequest::Image("../x".into())),
            Err(TransportError::NotSent)
        );
    }

    #[test]
    fn a_fetch_failure_retries_once_then_keeps_the_cache() {
        let dir = temp_dir("retry");
        let mut image_post = post("with-image", "announcement", "2026-11-20T00:00:00Z");
        image_post["image"] = json!({"path": "images/a.png", "sha256": sha256_hex(PNG),
            "width": 1200, "height": 675, "alt": {"th": "ภาพ", "en": "Image"}});
        let body = feed_json(json!([image_post]));
        // First run: one network failure, then the feed.
        let fake = Fake::new(vec![Err(TransportError::Network), Ok(ok(body))]);
        let service = NewsService::with_fetch(&dir, fake.clone(), Duration::ZERO);
        assert_eq!(service.view()["state"], "loading");
        service.fetch_now();
        let view = service.view();
        assert_eq!(view["state"], "ok");
        assert_eq!(view["posts"].as_array().unwrap().len(), 1);
        let data_url = view["posts"][0]["image"]["data_url"].as_str().unwrap();
        assert!(data_url.starts_with("data:image/png;base64,"));
        assert_eq!(
            lock(&fake.requests).as_slice(),
            &[
                NewsRequest::Feed,
                NewsRequest::Feed,
                NewsRequest::Image("images/a.png".into())
            ]
        );

        // Next run: both attempts fail; the cached feed is shown as offline with its fetch time.
        let fake = Fake::new(vec![
            Err(TransportError::Timeout),
            Ok(HttpResponse {
                status: 404,
                retry_after: None,
                body: vec![],
            }),
        ]);
        let service = NewsService::with_fetch(&dir, fake, Duration::ZERO);
        service.fetch_now();
        let view = service.view();
        assert_eq!(view["state"], "offline");
        assert_eq!(view["last_error"], "http_status");
        assert!(view["fetched_at"].as_u64().is_some());
        assert_eq!(view["posts"].as_array().unwrap().len(), 1);

        // An invalid body never replaces the cache.
        let fake = Fake::new(vec![Ok(ok(b"{}".to_vec())), Ok(ok(b"[]".to_vec()))]);
        let service = NewsService::with_fetch(&dir, fake, Duration::ZERO);
        service.fetch_now();
        assert_eq!(service.view()["state"], "offline");
        assert_eq!(service.view()["posts"].as_array().unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tampered_image_is_not_served() {
        let dir = temp_dir("image");
        let mut image_post = post("with-image", "announcement", "2026-11-20T00:00:00Z");
        image_post["image"] = json!({"path": "images/a.png", "sha256": "1".repeat(64),
            "width": 1200, "height": 675, "alt": {"th": "ภาพ", "en": "Image"}});
        let fake = Fake::new(vec![Ok(ok(feed_json(json!([image_post]))))]);
        let service = NewsService::with_fetch(&dir, fake, Duration::ZERO);
        service.fetch_now();
        let view = service.view();
        assert_eq!(view["posts"][0]["image"]["data_url"], Value::Null);
        assert!(!dir
            .join(CACHE_DIR)
            .join(CACHE_IMAGES)
            .join(format!("{}.png", "1".repeat(64)))
            .exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn switched_off_news_shows_nothing_and_never_fetches() {
        let dir = temp_dir("off");
        let fake = Fake::new(vec![Ok(ok(SAMPLE.as_bytes().to_vec()))]);
        let service = NewsService::with_fetch(&dir, fake.clone(), Duration::ZERO);
        assert!(service.prefs().enabled, "on by default");
        assert!(!service.prefs().disclosure_seen);
        service.fetch_now();
        assert!(!service.view()["posts"].as_array().unwrap().is_empty());
        service.set_prefs(&json!({"enabled": false})).unwrap();
        let view = service.view();
        assert_eq!(view["state"], "disabled");
        assert!(view["posts"].as_array().unwrap().is_empty());
        assert_eq!(service.refresh(), json!({"started": false}));
        assert!(service.link_url("2026-11-23-overview-launch", 0).is_none());
        // Persisted across a restart, and start() does not fetch.
        let fake2 = Fake::new(vec![]);
        let restarted = NewsService::with_fetch(&dir, fake2.clone(), Duration::ZERO);
        restarted.start();
        assert!(!restarted.prefs().enabled);
        assert!(lock(&fake2.requests).is_empty());
        assert!(service.set_prefs(&json!({"enabled": "yes"})).is_err());
        assert!(service.set_prefs(&json!({"url": "https://x"})).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn links_open_only_by_post_id_and_index_from_the_validated_feed() {
        let dir = temp_dir("links");
        let fake = Fake::new(vec![Ok(ok(SAMPLE.as_bytes().to_vec()))]);
        let service = NewsService::with_fetch(&dir, fake, Duration::ZERO);
        service.fetch_now();
        let feed = parse_feed(SAMPLE.as_bytes()).unwrap();
        let with_link = feed.posts.iter().find(|p| !p.links.is_empty()).unwrap();
        assert_eq!(
            service.link_url(&with_link.id, 0).as_deref(),
            Some(with_link.links[0].url.as_str())
        );
        assert!(service.link_url(&with_link.id, 9).is_none());
        assert!(service.link_url("no-such-post", 0).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
