//! Optional all-rank OpenDota hero benchmarks for the Analytics compare view.
//!
//! Requests are represented by the closed `OutgoingRequest` enum, use the existing WinHTTP
//! transport, and contain no account, match, player, rank, or canonical hero-key data.

use super::budget::DAY_MS;
use super::request::{HeroId, OutgoingRequest};
use super::transport::{Cancellation, HttpResponse, Transport, TransportError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const CACHE_FILE: &str = "hero-benchmarks-cache.json";
pub(crate) const LEDGER_FILE: &str = "hero-benchmarks-request-ledger.json";
const SCHEMA_VERSION: u8 = 1;
const REQUEST_BUDGET: usize = 4;
const MAX_CACHE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CACHED_HEROES: usize = 512;

type HeroIdLookup = dyn Fn(&str) -> Option<u32> + Send + Sync;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PercentilePoint {
    pub(crate) percentile: f64,
    pub(crate) value: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct HeroBenchmarks {
    pub(crate) hero_id: u32,
    #[serde(default)]
    pub(crate) gold_per_min: Option<Vec<PercentilePoint>>,
    #[serde(default)]
    pub(crate) xp_per_min: Option<Vec<PercentilePoint>>,
    #[serde(default)]
    pub(crate) kills_per_min: Option<Vec<PercentilePoint>>,
    #[serde(default)]
    pub(crate) last_hits_per_min: Option<Vec<PercentilePoint>>,
    #[serde(default)]
    pub(crate) hero_damage_per_min: Option<Vec<PercentilePoint>>,
    #[serde(default)]
    pub(crate) hero_healing_per_min: Option<Vec<PercentilePoint>>,
    #[serde(default)]
    pub(crate) tower_damage: Option<Vec<PercentilePoint>>,
}

impl HeroBenchmarks {
    fn valid(&self, expected_id: u32) -> bool {
        self.hero_id == expected_id
            && [
                &self.gold_per_min,
                &self.xp_per_min,
                &self.kills_per_min,
                &self.last_hits_per_min,
                &self.hero_damage_per_min,
                &self.hero_healing_per_min,
                &self.tower_damage,
            ]
            .into_iter()
            .flatten()
            .all(|curve| valid_curve(curve))
    }
}

fn valid_curve(curve: &[PercentilePoint]) -> bool {
    if curve.is_empty()
        || curve.iter().any(|point| {
            !point.percentile.is_finite()
                || !(0.0..=100.0).contains(&point.percentile)
                || !point.value.is_finite()
                || point.value < 0.0
        })
    {
        return false;
    }
    let mut points = curve.to_vec();
    points.sort_by(|left, right| left.percentile.total_cmp(&right.percentile));
    points
        .windows(2)
        .all(|pair| pair[0].percentile < pair[1].percentile && pair[0].value <= pair[1].value)
}

fn parse_curve(value: Option<&Value>) -> Option<Vec<PercentilePoint>> {
    let points = value?.as_array()?;
    if points.is_empty() {
        return None;
    }
    let curve: Vec<PercentilePoint> = points
        .iter()
        .map(|point| {
            Some(PercentilePoint {
                percentile: point.get("percentile")?.as_f64()?,
                value: point.get("value")?.as_f64()?,
            })
        })
        .collect::<Option<_>>()?;
    valid_curve(&curve).then_some(curve)
}

/// Parse each known metric independently. A missing or malformed field stays unknown while valid
/// partial fields survive. The top-level hero ID must match the request.
pub(crate) fn parse_benchmarks(body: &[u8], expected_id: u32) -> Option<HeroBenchmarks> {
    if body.len() > MAX_CACHE_BYTES || expected_id == 0 {
        return None;
    }
    let root: Value = serde_json::from_slice(body).ok()?;
    let hero_id = u32::try_from(root.get("hero_id")?.as_u64()?).ok()?;
    if hero_id != expected_id {
        return None;
    }
    let result = root.get("result")?.as_object()?;
    let parsed = HeroBenchmarks {
        hero_id,
        gold_per_min: parse_curve(result.get("gold_per_min")),
        xp_per_min: parse_curve(result.get("xp_per_min")),
        kills_per_min: parse_curve(result.get("kills_per_min")),
        last_hits_per_min: parse_curve(result.get("last_hits_per_min")),
        hero_damage_per_min: parse_curve(result.get("hero_damage_per_min")),
        hero_healing_per_min: parse_curve(result.get("hero_healing_per_min")),
        tower_damage: parse_curve(result.get("tower_damage")),
    };
    parsed.valid(expected_id).then_some(parsed)
}

fn canonical_hero_key(value: &str) -> bool {
    value.strip_prefix("npc_dota_hero_").is_some_and(|tail| {
        !tail.is_empty()
            && tail
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    })
}

/// OpenDota's static hero list is used only when the consented hero-stats cache lacks an ID.
/// Invalid and ambiguous rows are omitted instead of receiving a fabricated ID.
fn parse_hero_ids(body: &[u8]) -> Option<BTreeMap<String, u32>> {
    if body.len() > MAX_CACHE_BYTES {
        return None;
    }
    let root: Value = serde_json::from_slice(body).ok()?;
    let rows = root.as_array()?;
    let mut result = BTreeMap::new();
    let mut duplicate_keys = HashSet::new();
    for row in rows {
        let Some(id) = row
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|id| *id > 0)
        else {
            continue;
        };
        let Some(key) = row.get("name").and_then(Value::as_str) else {
            continue;
        };
        if !canonical_hero_key(key) {
            continue;
        }
        if result.contains_key(key) {
            result.remove(key);
            duplicate_keys.insert(key.to_owned());
            continue;
        }
        if !duplicate_keys.contains(key) {
            result.insert(key.to_owned(), id);
        }
    }
    Some(result)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CachedHeroBenchmarks {
    fetched_at_ms: u64,
    value: HeroBenchmarks,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BenchmarksCache {
    schema_version: u8,
    heroes_fetched_at_ms: Option<u64>,
    hero_ids: BTreeMap<String, u32>,
    benchmarks: BTreeMap<u32, CachedHeroBenchmarks>,
}

impl BenchmarksCache {
    fn valid(&self) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.hero_ids.len() <= MAX_CACHED_HEROES
            && self
                .hero_ids
                .iter()
                .all(|(key, id)| canonical_hero_key(key) && *id > 0)
            && self.benchmarks.len() <= MAX_CACHED_HEROES
            && self
                .benchmarks
                .iter()
                .all(|(id, cached)| *id > 0 && cached.value.valid(*id))
    }
}

struct CacheStore {
    path: PathBuf,
}

impl CacheStore {
    fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(CACHE_FILE),
        }
    }

    fn load(&self) -> BenchmarksCache {
        std::fs::read(&self.path)
            .ok()
            .filter(|bytes| bytes.len() <= MAX_CACHE_BYTES)
            .and_then(|bytes| serde_json::from_slice::<BenchmarksCache>(&bytes).ok())
            .filter(BenchmarksCache::valid)
            .unwrap_or_default()
    }

    fn save(&self, cache: &BenchmarksCache) -> Result<(), String> {
        let mut stored = cache.clone();
        stored.schema_version = SCHEMA_VERSION;
        let bytes =
            serde_json::to_vec(&stored).map_err(|_| "BENCHMARKS_CACHE_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "BENCHMARKS_CACHE_WRITE_FAILED".to_owned())
    }

    fn delete(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("BENCHMARKS_CACHE_DELETE_FAILED".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RequestLedger {
    schema_version: u8,
    requests_ms: Vec<u64>,
}

impl RequestLedger {
    fn in_window(timestamp_ms: u64, now_ms: u64) -> bool {
        timestamp_ms > now_ms || now_ms.saturating_sub(timestamp_ms) < DAY_MS
    }

    fn load(path: &Path, now_ms: u64) -> Self {
        let parsed = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok());
        let mut ledger = match parsed {
            Some(value)
                if value.schema_version == SCHEMA_VERSION && value.requests_ms.len() <= 64 =>
            {
                value
            }
            Some(_) => Self {
                schema_version: SCHEMA_VERSION,
                requests_ms: vec![now_ms; REQUEST_BUDGET],
            },
            None if !path.exists() => Self {
                schema_version: SCHEMA_VERSION,
                requests_ms: Vec::new(),
            },
            None => Self {
                schema_version: SCHEMA_VERSION,
                requests_ms: vec![now_ms; REQUEST_BUDGET],
            },
        };
        ledger.prune(now_ms);
        ledger
    }

    fn prune(&mut self, now_ms: u64) {
        self.requests_ms
            .retain(|timestamp| Self::in_window(*timestamp, now_ms));
    }

    fn count_24h(&self, now_ms: u64) -> usize {
        self.requests_ms
            .iter()
            .filter(|timestamp| Self::in_window(**timestamp, now_ms))
            .count()
    }

    fn reserve(&mut self, now_ms: u64) -> Result<u64, ()> {
        self.prune(now_ms);
        if self.count_24h(now_ms) >= REQUEST_BUDGET {
            return Err(());
        }
        self.requests_ms.push(now_ms);
        Ok(now_ms)
    }

    fn unrecord(&mut self, timestamp_ms: u64) -> bool {
        match self
            .requests_ms
            .iter()
            .rposition(|timestamp| *timestamp == timestamp_ms)
        {
            Some(index) => {
                self.requests_ms.remove(index);
                true
            }
            None => false,
        }
    }
}

struct LedgerStore {
    path: PathBuf,
}

impl LedgerStore {
    fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(LEDGER_FILE),
        }
    }

    fn load(&self, now_ms: u64) -> RequestLedger {
        let ledger = RequestLedger::load(&self.path, now_ms);
        let _ = self.save(&ledger);
        ledger
    }

    fn save(&self, ledger: &RequestLedger) -> Result<(), String> {
        let mut stored = ledger.clone();
        stored.schema_version = SCHEMA_VERSION;
        let bytes =
            serde_json::to_vec(&stored).map_err(|_| "BENCHMARKS_LEDGER_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "BENCHMARKS_LEDGER_WRITE_FAILED".to_owned())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BenchmarksState {
    Disabled,
    Unknown,
    Loading,
    Ready,
    Offline,
    BudgetExhausted,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BenchmarksView {
    pub(crate) enabled: bool,
    pub(crate) state: BenchmarksState,
    pub(crate) budget_used: usize,
    pub(crate) hero_id: Option<u32>,
    pub(crate) fetched_at_ms: Option<u64>,
    pub(crate) benchmarks: Option<HeroBenchmarks>,
}

struct Inner {
    enabled: bool,
    generation: u64,
    cache: BenchmarksCache,
    ledger: RequestLedger,
    state: BenchmarksState,
    current_hero_id: Option<u32>,
    active_cancel: Option<Cancellation>,
}

enum RequestFailure {
    Disabled,
    BudgetExhausted,
    Offline,
    Unknown,
    Cancelled,
}

pub(crate) struct BenchmarksHost {
    cache_store: CacheStore,
    ledger_store: LedgerStore,
    transport: Arc<dyn Transport>,
    hero_id_lookup: Arc<HeroIdLookup>,
    inner: Mutex<Inner>,
    pipeline: Mutex<()>,
}

impl BenchmarksHost {
    pub(crate) fn open(
        dir: &Path,
        enabled: bool,
        transport: Arc<dyn Transport>,
        hero_id_lookup: Arc<HeroIdLookup>,
    ) -> Self {
        let cache_store = CacheStore::in_data_dir(dir);
        let cache = if enabled {
            cache_store.load()
        } else {
            let _ = cache_store.delete();
            BenchmarksCache::default()
        };
        let now_ms = unix_millis();
        let ledger_store = LedgerStore::in_data_dir(dir);
        let ledger = ledger_store.load(now_ms);
        Self {
            cache_store,
            ledger_store,
            transport,
            hero_id_lookup,
            inner: Mutex::new(Inner {
                enabled,
                generation: 0,
                cache,
                ledger,
                state: if enabled {
                    BenchmarksState::Unknown
                } else {
                    BenchmarksState::Disabled
                },
                current_hero_id: None,
                active_cancel: None,
            }),
            pipeline: Mutex::new(()),
        }
    }

    pub(crate) fn view(&self) -> BenchmarksView {
        view_locked(&lock(&self.inner), unix_millis())
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<BenchmarksView, String> {
        let cancellation = {
            let mut inner = lock(&self.inner);
            if inner.enabled != enabled {
                inner.generation = inner.generation.wrapping_add(1);
            }
            inner.enabled = enabled;
            inner.active_cancel.take().inspect(|token| token.cancel())
        };
        if let Some(cancellation) = cancellation {
            self.transport.cancel_request(&cancellation);
        }
        if !enabled {
            {
                let mut inner = lock(&self.inner);
                inner.cache = BenchmarksCache::default();
                inner.current_hero_id = None;
                inner.state = BenchmarksState::Disabled;
            }
            self.cache_store.delete()?;
        } else {
            let mut inner = lock(&self.inner);
            inner.state = BenchmarksState::Unknown;
            inner.current_hero_id = None;
        }
        Ok(self.view())
    }

    pub(crate) fn get_for_hero(&self, hero_key: &str) -> BenchmarksView {
        let _pipeline = lock(&self.pipeline);
        if !canonical_hero_key(hero_key) {
            let mut inner = lock(&self.inner);
            inner.current_hero_id = None;
            if inner.enabled {
                inner.state = BenchmarksState::Unknown;
            }
            return view_locked(&inner, unix_millis());
        }

        let (generation, cancellation) = {
            let mut inner = lock(&self.inner);
            if !inner.enabled {
                return view_locked(&inner, unix_millis());
            }
            if let Some(id) = inner.cache.hero_ids.get(hero_key).copied() {
                inner.current_hero_id = Some(id);
                if inner.cache.benchmarks.contains_key(&id) {
                    inner.state = BenchmarksState::Ready;
                    return view_locked(&inner, unix_millis());
                }
            }
            let cancellation = Cancellation::default();
            let generation = inner.generation;
            inner.active_cancel = Some(cancellation.clone());
            inner.current_hero_id = None;
            inner.state = BenchmarksState::Loading;
            (generation, cancellation)
        };

        let outcome = self.fetch_for_hero(hero_key, generation, &cancellation);
        let mut inner = lock(&self.inner);
        if inner.generation == generation {
            inner.active_cancel = None;
            if let Err(failure) = outcome {
                inner.state = match failure {
                    RequestFailure::Disabled | RequestFailure::Cancelled => {
                        BenchmarksState::Disabled
                    }
                    RequestFailure::BudgetExhausted => BenchmarksState::BudgetExhausted,
                    RequestFailure::Offline => BenchmarksState::Offline,
                    RequestFailure::Unknown => BenchmarksState::Unknown,
                };
            }
        }
        view_locked(&inner, unix_millis())
    }

    fn fetch_for_hero(
        &self,
        hero_key: &str,
        generation: u64,
        cancellation: &Cancellation,
    ) -> Result<(), RequestFailure> {
        let mapped = (self.hero_id_lookup)(hero_key).filter(|id| *id > 0);
        let from_public_cache = {
            let inner = lock(&self.inner);
            inner.cache.hero_ids.get(hero_key).copied()
        };
        let hero_id = if let Some(id) = mapped.or(from_public_cache) {
            Some(id)
        } else {
            let have_hero_list = lock(&self.inner).cache.heroes_fetched_at_ms.is_some();
            if !have_hero_list {
                let response =
                    self.send(generation, cancellation, OutgoingRequest::BenchmarksHeroes)?;
                if !(200..300).contains(&response.status) {
                    return Err(RequestFailure::Offline);
                }
                let Some(hero_ids) = parse_hero_ids(&response.body) else {
                    return Err(RequestFailure::Unknown);
                };
                let mut inner = lock(&self.inner);
                ensure_current(&inner, generation, cancellation)?;
                inner.cache.hero_ids = hero_ids;
                inner.cache.heroes_fetched_at_ms = Some(unix_millis());
                self.cache_store
                    .save(&inner.cache)
                    .map_err(|_| RequestFailure::Unknown)?;
            }
            lock(&self.inner).cache.hero_ids.get(hero_key).copied()
        };
        let Some(hero_id) = hero_id.filter(|id| *id > 0) else {
            let mut inner = lock(&self.inner);
            if inner.generation == generation {
                inner.current_hero_id = None;
                inner.state = BenchmarksState::Unknown;
            }
            return Err(RequestFailure::Unknown);
        };
        {
            let mut inner = lock(&self.inner);
            ensure_current(&inner, generation, cancellation)?;
            inner.current_hero_id = Some(hero_id);
            if inner.cache.benchmarks.contains_key(&hero_id) {
                inner.state = BenchmarksState::Ready;
                return Ok(());
            }
        }

        let response = self.send(
            generation,
            cancellation,
            OutgoingRequest::Benchmarks(HeroId::new(hero_id).ok_or(RequestFailure::Unknown)?),
        )?;
        if !(200..300).contains(&response.status) {
            return Err(RequestFailure::Offline);
        }
        let Some(benchmarks) = parse_benchmarks(&response.body, hero_id) else {
            return Err(RequestFailure::Unknown);
        };
        let mut inner = lock(&self.inner);
        ensure_current(&inner, generation, cancellation)?;
        inner.cache.benchmarks.insert(
            hero_id,
            CachedHeroBenchmarks {
                fetched_at_ms: unix_millis(),
                value: benchmarks,
            },
        );
        self.cache_store
            .save(&inner.cache)
            .map_err(|_| RequestFailure::Unknown)?;
        inner.state = BenchmarksState::Ready;
        Ok(())
    }

    fn send(
        &self,
        generation: u64,
        cancellation: &Cancellation,
        request: OutgoingRequest,
    ) -> Result<HttpResponse, RequestFailure> {
        let at_ms = {
            let mut inner = lock(&self.inner);
            ensure_current(&inner, generation, cancellation)?;
            let now_ms = unix_millis();
            let at_ms = inner
                .ledger
                .reserve(now_ms)
                .map_err(|_| RequestFailure::BudgetExhausted)?;
            if self.ledger_store.save(&inner.ledger).is_err() {
                inner.ledger.unrecord(at_ms);
                return Err(RequestFailure::Unknown);
            }
            at_ms
        };
        let result = self.transport.get(request, cancellation);
        let reached_network = match &result {
            Ok(_) => true,
            Err(error) => error.reached_network(),
        };
        let mut inner = lock(&self.inner);
        if !reached_network {
            inner.ledger.unrecord(at_ms);
            let _ = self.ledger_store.save(&inner.ledger);
        }
        match result {
            Ok(response) => Ok(response),
            Err(TransportError::NotSent | TransportError::CancelledBeforeSend) => {
                Err(RequestFailure::Unknown)
            }
            Err(TransportError::Cancelled) if cancellation.is_cancelled() => {
                Err(RequestFailure::Cancelled)
            }
            Err(TransportError::Network | TransportError::Timeout | TransportError::TooLarge) => {
                Err(RequestFailure::Offline)
            }
            Err(TransportError::Cancelled) => Err(RequestFailure::Offline),
        }
    }
}

fn ensure_current(
    inner: &Inner,
    generation: u64,
    cancellation: &Cancellation,
) -> Result<(), RequestFailure> {
    if !inner.enabled {
        Err(RequestFailure::Disabled)
    } else if inner.generation != generation || cancellation.is_cancelled() {
        Err(RequestFailure::Cancelled)
    } else {
        Ok(())
    }
}

fn view_locked(inner: &Inner, now_ms: u64) -> BenchmarksView {
    let cached = inner
        .current_hero_id
        .and_then(|id| inner.cache.benchmarks.get(&id));
    BenchmarksView {
        enabled: inner.enabled,
        state: if inner.enabled {
            inner.state
        } else {
            BenchmarksState::Disabled
        },
        budget_used: inner.ledger.count_24h(now_ms),
        hero_id: inner.current_hero_id,
        fetched_at_ms: cached.map(|entry| entry.fetched_at_ms),
        benchmarks: cached.map(|entry| entry.value.clone()),
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_benchmarks, parse_hero_ids, BenchmarksHost, BenchmarksState, CacheStore,
        RequestLedger, CACHE_FILE, LEDGER_FILE,
    };
    use crate::hero_stats::request::OutgoingRequest;
    use crate::hero_stats::transport::{Cancellation, HttpResponse, Transport, TransportError};
    use std::collections::VecDeque;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;

    // These fixture bytes are synthetic and follow only the documented OpenDota response shape.
    const PARTIAL_BENCHMARK_FIXTURE: &[u8] = br#"{"hero_id":2,"result":{"gold_per_min":[{"percentile":10,"value":300},{"percentile":50,"value":450}],"xp_per_min":"malformed","tower_damage":[{"percentile":25,"value":0}]}}"#;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kohbor-hero-benchmarks-{tag}-{}-{}",
            std::process::id(),
            super::unix_millis()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct FixtureTransport {
        responses: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
        requests: Mutex<Vec<OutgoingRequest>>,
        cancelled: AtomicUsize,
    }

    impl FixtureTransport {
        fn new(responses: impl IntoIterator<Item = Result<HttpResponse, TransportError>>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
                requests: Mutex::new(Vec::new()),
                cancelled: AtomicUsize::new(0),
            }
        }
    }

    impl Transport for FixtureTransport {
        fn get(
            &self,
            request: OutgoingRequest,
            _cancellation: &Cancellation,
        ) -> Result<HttpResponse, TransportError> {
            self.requests.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(TransportError::NotSent))
        }

        fn cancel_request(&self, cancellation: &Cancellation) {
            cancellation.cancel();
            self.cancelled.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct BlockingTransport {
        started: Mutex<Option<mpsc::Sender<()>>>,
        release: Mutex<mpsc::Receiver<()>>,
        request_seen_cancelled: Arc<AtomicBool>,
    }

    impl Transport for BlockingTransport {
        fn get(
            &self,
            _request: OutgoingRequest,
            cancellation: &Cancellation,
        ) -> Result<HttpResponse, TransportError> {
            if let Some(started) = self.started.lock().unwrap().take() {
                started.send(()).unwrap();
            }
            self.release.lock().unwrap().recv().unwrap();
            self.request_seen_cancelled
                .store(cancellation.is_cancelled(), Ordering::SeqCst);
            Ok(HttpResponse {
                status: 200,
                retry_after: None,
                body: PARTIAL_BENCHMARK_FIXTURE.to_vec(),
            })
        }
    }

    fn host(
        dir: &Path,
        transport: Arc<dyn Transport>,
        enabled: bool,
        hero_id: Option<u32>,
    ) -> BenchmarksHost {
        BenchmarksHost::open(
            dir,
            enabled,
            transport,
            Arc::new(move |key| (key == "npc_dota_hero_axe").then_some(hero_id).flatten()),
        )
    }

    fn response(body: &[u8]) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status: 200,
            retry_after: None,
            body: body.to_vec(),
        })
    }

    #[test]
    fn synthetic_partial_and_malformed_benchmark_fields_remain_unknown() {
        let parsed = parse_benchmarks(PARTIAL_BENCHMARK_FIXTURE, 2).unwrap();
        assert!(parsed.gold_per_min.is_some());
        assert!(parsed.tower_damage.is_some());
        assert!(parsed.xp_per_min.is_none());
        assert!(parsed.kills_per_min.is_none());
        assert!(parse_benchmarks(PARTIAL_BENCHMARK_FIXTURE, 3).is_none());
        assert!(parse_benchmarks(b"not json", 2).is_none());
        assert!(
            parse_hero_ids(br#"[{"id":0,"name":"npc_dota_hero_axe"},{"id":3,"name":false}]"#)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn benchmark_ledger_caps_all_request_types_and_uses_revision_four_counting() {
        let mut ledger = RequestLedger::default();
        let now = 3_000_000;
        for offset in 0..4 {
            ledger.reserve(now + offset).unwrap();
        }
        assert!(ledger.reserve(now + 5).is_err());
        assert_eq!(ledger.count_24h(now + 5), 4);

        // A response or an error after connect/send counts. Local setup and pre-send cancel do not.
        let mut requests = ledger.requests_ms.clone();
        let reserved = requests.pop().unwrap();
        assert!(matches!(response(PARTIAL_BENCHMARK_FIXTURE), Ok(_)));
        assert!(TransportError::Network.reached_network());
        assert!(!TransportError::NotSent.reached_network());
        assert!(!TransportError::CancelledBeforeSend.reached_network());
        assert!(ledger.unrecord(reserved));
        assert_eq!(ledger.count_24h(now + 5), 3);
    }

    #[test]
    fn live_attempt_ledger_rolls_back_local_setup_but_counts_reached_network_errors() {
        let dir = temp_dir("counting-rule");
        let transport = Arc::new(FixtureTransport::new([
            Err(TransportError::NotSent),
            Err(TransportError::Network),
            response(PARTIAL_BENCHMARK_FIXTURE),
        ]));
        let service = host(&dir, transport.clone(), true, Some(2));

        assert_eq!(service.get_for_hero("npc_dota_hero_axe").budget_used, 0);
        assert_eq!(service.get_for_hero("npc_dota_hero_axe").budget_used, 1);
        let ready = service.get_for_hero("npc_dota_hero_axe");
        assert_eq!(ready.state, BenchmarksState::Ready);
        assert_eq!(ready.budget_used, 2);
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn service_stops_after_four_network_reaching_attempts_in_rolling_day() {
        let dir = temp_dir("service-budget-cap");
        let transport = Arc::new(FixtureTransport::new(
            (0..4).map(|_| Err(TransportError::Network)),
        ));
        let service = host(&dir, transport.clone(), true, Some(2));
        for used in 1..=4 {
            let view = service.get_for_hero("npc_dota_hero_axe");
            assert_eq!(view.budget_used, used);
        }
        let exhausted = service.get_for_hero("npc_dota_hero_axe");
        assert_eq!(exhausted.state, BenchmarksState::BudgetExhausted);
        assert_eq!(exhausted.budget_used, 4);
        assert_eq!(transport.requests.lock().unwrap().len(), 4);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn missing_local_mapping_requests_parameterless_hero_list_then_only_the_numeric_id() {
        let dir = temp_dir("hero-map");
        let transport = Arc::new(FixtureTransport::new([
            response(br#"[{"id":2,"name":"npc_dota_hero_axe","localized_name":"Axe"}]"#),
            response(PARTIAL_BENCHMARK_FIXTURE),
        ]));
        let service = host(&dir, transport.clone(), true, None);
        let view = service.get_for_hero("npc_dota_hero_axe");
        assert_eq!(view.state, BenchmarksState::Ready);
        assert_eq!(view.hero_id, Some(2));
        assert_eq!(view.budget_used, 2);
        assert!(view.fetched_at_ms.is_some());
        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests[0].path(), "/api/heroes");
        assert_eq!(requests[1].path(), "/api/benchmarks?hero_id=2");
        drop(requests);
        let cached = service.get_for_hero("npc_dota_hero_axe");
        assert_eq!(cached.fetched_at_ms, view.fetched_at_ms);
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
        assert!(dir.join(CACHE_FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn consented_hero_stats_mapping_skips_the_public_hero_list_request() {
        let dir = temp_dir("consented-map");
        let transport = Arc::new(FixtureTransport::new([response(PARTIAL_BENCHMARK_FIXTURE)]));
        let service = host(&dir, transport.clone(), true, Some(2));
        let view = service.get_for_hero("npc_dota_hero_axe");
        assert_eq!(view.state, BenchmarksState::Ready);
        assert_eq!(view.budget_used, 1);
        assert_eq!(
            transport.requests.lock().unwrap().as_slice(),
            &[OutgoingRequest::Benchmarks(
                crate::hero_stats::request::HeroId::new(2).unwrap()
            )]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_hero_after_public_list_does_not_send_a_benchmark_request() {
        let dir = temp_dir("unknown-map");
        let transport = Arc::new(FixtureTransport::new([response(
            br#"[{"id":2,"name":"npc_dota_hero_axe"}]"#,
        )]));
        let service = host(&dir, transport.clone(), true, None);
        let view = service.get_for_hero("npc_dota_hero_lina");
        assert_eq!(view.state, BenchmarksState::Unknown);
        assert_eq!(view.budget_used, 1);
        assert_eq!(transport.requests.lock().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn withdrawal_cancels_discards_a_late_response_deletes_cache_and_keeps_budget() {
        let dir = temp_dir("late-withdrawal");
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let observed_cancel = Arc::new(AtomicBool::new(false));
        let transport = Arc::new(BlockingTransport {
            started: Mutex::new(Some(started_tx)),
            release: Mutex::new(release_rx),
            request_seen_cancelled: observed_cancel.clone(),
        });
        let service = Arc::new(host(&dir, transport, true, Some(2)));
        let worker_service = service.clone();
        let worker = thread::spawn(move || worker_service.get_for_hero("npc_dota_hero_axe"));
        started_rx.recv().unwrap();
        service.set_enabled(false).unwrap();
        release_tx.send(()).unwrap();
        let result = worker.join().unwrap();
        assert_eq!(result.state, BenchmarksState::Disabled);
        assert!(observed_cancel.load(Ordering::SeqCst));
        assert!(!dir.join(CACHE_FILE).exists());
        assert!(dir.join(LEDGER_FILE).exists());
        assert!(service.view().benchmarks.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn withdrawn_switch_is_off_and_cache_deletion_is_idempotent() {
        let dir = temp_dir("off-default");
        let cache = CacheStore::in_data_dir(&dir);
        cache.save(&super::BenchmarksCache::default()).unwrap();
        let transport = Arc::new(FixtureTransport::new([]));
        let service = host(&dir, transport, false, None);
        assert_eq!(service.view().state, BenchmarksState::Disabled);
        assert!(!dir.join(CACHE_FILE).exists());
        service.set_enabled(false).unwrap();
        assert!(!dir.join(CACHE_FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
