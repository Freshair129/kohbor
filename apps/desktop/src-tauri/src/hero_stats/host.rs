use super::budget::{LedgerStore, RequestLedger};
use super::cache::{freshness, CacheEntry, CacheStore, DurationEntry, Freshness, StatsCache};
use super::groups::{build_position_groups, PositionGroups};
use super::parse::{parse_durations, parse_hero_stats, parse_lane_roles, ParseError};
use super::prefs::{patch_prefs, DraftStatsPrefs, PrefsStore};
use super::request::{HeroId, LaneRole, OutgoingRequest};
use super::transport::{Cancellation, HttpResponse, Transport, TransportError, WinHttpTransport};
use super::view::{
    build_view, DataState, DraftBarInput, DraftColumnView, DraftPhase, Failure, ViewInput,
};
use super::{draft_column_rect, supported_output, Monitor, Rect, PICK_CARD_STRATEGY_TIME_RECT};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

const STATE_EVENT: &str = "draft://view";
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const KEEPALIVE: Duration = Duration::from_secs(2);
const CONSENT_CHIP_DURATION: Duration = Duration::from_secs(8);
const DEFAULT_RETRY_AFTER_MS: u64 = 60 * 60 * 1000;

trait StatsStore: Send + Sync {
    fn load_prefs(&self) -> DraftStatsPrefs;
    fn save_prefs(&self, prefs: &DraftStatsPrefs) -> Result<(), String>;
    fn load_cache(&self, now_ms: u64) -> StatsCache;
    fn save_cache(&self, cache: &StatsCache) -> Result<(), String>;
    fn delete_cache(&self) -> Result<(), String>;
    fn load_ledger(&self, now_ms: u64) -> RequestLedger;
    fn save_ledger(&self, ledger: &RequestLedger) -> Result<(), String>;
}

struct FileStatsStore {
    prefs: PrefsStore,
    cache: CacheStore,
    ledger: LedgerStore,
}

impl FileStatsStore {
    fn in_data_dir(dir: &Path) -> Self {
        Self {
            prefs: PrefsStore::in_data_dir(dir),
            cache: CacheStore::in_data_dir(dir),
            ledger: LedgerStore::in_data_dir(dir),
        }
    }
}

impl StatsStore for FileStatsStore {
    fn load_prefs(&self) -> DraftStatsPrefs {
        self.prefs.load()
    }

    fn save_prefs(&self, prefs: &DraftStatsPrefs) -> Result<(), String> {
        self.prefs.save(prefs)
    }

    fn load_cache(&self, now_ms: u64) -> StatsCache {
        self.cache.load(now_ms)
    }

    fn save_cache(&self, cache: &StatsCache) -> Result<(), String> {
        self.cache.save(cache)
    }

    fn delete_cache(&self) -> Result<(), String> {
        self.cache.delete()
    }

    fn load_ledger(&self, now_ms: u64) -> RequestLedger {
        self.ledger.load(now_ms)
    }

    fn save_ledger(&self, ledger: &RequestLedger) -> Result<(), String> {
        self.ledger.save(ledger)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DraftObservation {
    pub(crate) phase: Option<DraftPhase>,
    pub(crate) admitted_session: bool,
    pub(crate) ownership_verified: bool,
    pub(crate) session_epoch: Option<u64>,
    pub(crate) age_ms: Option<u64>,
    pub(crate) own_hero_key: Option<String>,
}

/// Session-local 5b data. Hero keys are consumed to build the in-memory view and never persisted,
/// logged or passed to the transport.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DraftBarObservation {
    pub(crate) available: bool,
    pub(crate) hero_keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct MonitorState {
    monitor: Option<Monitor>,
    scale_factor: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestPurpose {
    PublicTable,
    OwnDuration { hero_id: u32, pick_generation: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RequestTicket {
    id: u64,
    consent_generation: u64,
    request: OutgoingRequest,
    purpose: RequestPurpose,
}

struct Core {
    prefs: DraftStatsPrefs,
    store: Arc<dyn StatsStore>,
    cache: StatsCache,
    ledger: RequestLedger,
    failure: Option<Failure>,
    failed_at_ms: Option<u64>,
    http_status: Option<u16>,
    retry_after_ms: Option<u64>,
    retry_not_before_ms: Option<u64>,
    consent_generation: u64,
    cache_blocked: bool,
    refresh_pending: bool,
    table_failure_latched: bool,
    duration_failure_latched_hero: Option<String>,
    ticket_sequence: u64,
    active: Option<RequestTicket>,
    active_cancel: Option<Cancellation>,
    pick_generation: u64,
    current_pick: Option<(Option<u64>, String)>,
    last_touched_duration: Option<u32>,
    observation: DraftObservation,
    draft_bar: DraftBarObservation,
    monitor: MonitorState,
    consent_chip_until: Option<Instant>,
    view: DraftColumnView,
}

impl Core {
    fn open(dir: &Path, now_ms: u64) -> Self {
        Self::open_with_store(Arc::new(FileStatsStore::in_data_dir(dir)), now_ms)
    }

    fn open_with_store(store: Arc<dyn StatsStore>, now_ms: u64) -> Self {
        let persisted_prefs = store.load_prefs();
        let persisted_consent = persisted_prefs.hero_stats_consent;
        // With consent off, deletion precedes the ledger read and cache is never read. The ledger
        // is loaded only to preserve request budgets and report a deletion failure.
        let mut startup_delete = if persisted_consent {
            None
        } else {
            Some(store.delete_cache())
        };
        let mut prefs = persisted_prefs;
        let mut ledger = store.load_ledger(now_ms);
        let mut ledger_changed = false;
        if persisted_consent && (ledger.revocation_pending || ledger.cache_delete_failed) {
            // A durable revocation marker covers a failed prefs write. Keep the effective in-memory
            // consent off and remove any cache before considering a load.
            prefs.hero_stats_consent = false;
            if !ledger.revocation_pending {
                ledger.set_revocation_pending(true);
                ledger_changed = true;
            }
            startup_delete = Some(store.delete_cache());
        }
        if let Some(result) = &startup_delete {
            let failed = result.is_err();
            if ledger.cache_delete_failed != failed {
                ledger.set_cache_delete_failed(failed);
                ledger_changed = true;
            }
        }
        let mut ledger_persist_failed = false;
        if ledger_changed && store.save_ledger(&ledger).is_err() {
            ledger_persist_failed = true;
        }
        let failed_delete = startup_delete.as_ref().is_some_and(Result::is_err)
            || ledger.cache_delete_failed
            || ledger_persist_failed;
        // Statistics are loaded only if persisted consent and effective in-memory consent both
        // remain true, and no revocation/deletion barrier is active.
        let cache = if persisted_consent
            && prefs.hero_stats_consent
            && !ledger.revocation_pending
            && !failed_delete
        {
            store.load_cache(now_ms)
        } else {
            StatsCache::default()
        };
        let refresh_pending = prefs.hero_stats_consent && !cache.public_tables_fresh(now_ms);
        let cache_blocked = !persisted_consent
            || ledger.revocation_pending
            || ledger.cache_delete_failed
            || failed_delete;
        let mut core = Self {
            prefs,
            store,
            cache,
            ledger,
            failure: failed_delete.then_some(Failure::CacheDeleteFailed),
            failed_at_ms: None,
            http_status: None,
            retry_after_ms: None,
            retry_not_before_ms: None,
            consent_generation: 1,
            cache_blocked,
            refresh_pending,
            table_failure_latched: false,
            duration_failure_latched_hero: None,
            ticket_sequence: 0,
            active: None,
            active_cancel: None,
            pick_generation: 0,
            current_pick: None,
            last_touched_duration: None,
            observation: DraftObservation::default(),
            draft_bar: DraftBarObservation::default(),
            monitor: MonitorState::default(),
            consent_chip_until: None,
            view: empty_view(),
        };
        core.rebuild_view(now_ms, Instant::now());
        core
    }

    #[cfg(test)]
    fn set_prefs(&mut self, patch: &Value, now_ms: u64) -> Result<(DraftStatsPrefs, bool), String> {
        self.set_prefs_with_cancel(patch, now_ms, || {})
    }

    fn set_prefs_with_cancel(
        &mut self,
        patch: &Value,
        now_ms: u64,
        cancel_native: impl FnOnce(),
    ) -> Result<(DraftStatsPrefs, bool), String> {
        let next = patch_prefs(&self.prefs, patch)?;
        let revoked = self.prefs.hero_stats_consent && !next.hero_stats_consent;
        let granted = !self.prefs.hero_stats_consent && next.hero_stats_consent;
        if revoked {
            self.prefs = next.clone();
            self.begin_revoke();
            cancel_native();
            self.finish_revoke(&next, now_ms)?;
        } else if granted {
            self.grant_consent(&next, now_ms)?;
        } else {
            self.store.save_prefs(&next)?;
            self.prefs = next.clone();
        }
        Ok((next, revoked))
    }

    fn begin_revoke(&mut self) {
        self.consent_generation = self.consent_generation.saturating_add(1);
        if let Some(cancel) = &self.active_cancel {
            cancel.cancel();
        }
        self.cache = StatsCache::default();
        self.cache_blocked = true;
        self.refresh_pending = false;
        self.table_failure_latched = false;
        self.duration_failure_latched_hero = None;
        self.last_touched_duration = None;
        self.clear_error();
    }

    fn finish_revoke(&mut self, prefs: &DraftStatsPrefs, now_ms: u64) -> Result<(), String> {
        let mut errors = Vec::new();
        // Persist the consent change before touching cache or ledger state.
        let prefs_error = self.store.save_prefs(prefs).err();
        if let Some(error) = &prefs_error {
            errors.push(error.clone());
        }
        self.ledger.set_revocation_pending(true);
        if let Err(error) = self.store.delete_cache() {
            self.ledger.set_cache_delete_failed(true);
            self.failure = Some(Failure::CacheDeleteFailed);
            self.failed_at_ms = Some(now_ms);
            errors.push(error);
        } else {
            self.ledger.set_cache_delete_failed(false);
        }
        if let Err(error) = self.store.save_ledger(&self.ledger) {
            errors.push(error);
        }
        self.rebuild_view(now_ms, Instant::now());
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn grant_consent(&mut self, next: &DraftStatsPrefs, now_ms: u64) -> Result<(), String> {
        let delete_before_grant =
            self.cache_blocked || self.ledger.revocation_pending || self.ledger.cache_delete_failed;
        if delete_before_grant && self.store.delete_cache().is_err() {
            self.ledger.set_cache_delete_failed(true);
            self.ledger.set_revocation_pending(true);
            self.failure = Some(Failure::CacheDeleteFailed);
            self.failed_at_ms = Some(now_ms);
            let _ = self.store.save_ledger(&self.ledger);
            self.rebuild_view(now_ms, Instant::now());
            return Err("DRAFT_STATS_CACHE_DELETE_FAILED".to_owned());
        }
        self.store.save_prefs(next)?;
        if self.ledger.revocation_pending || self.ledger.cache_delete_failed {
            self.ledger.set_revocation_pending(false);
            self.ledger.set_cache_delete_failed(false);
            self.store.save_ledger(&self.ledger)?;
        }
        self.prefs = next.clone();
        self.table_failure_latched = false;
        self.duration_failure_latched_hero = None;
        self.cache = self.store.load_cache(now_ms);
        self.cache_blocked = false;
        self.clear_error();
        self.refresh_pending = !self.cache.public_tables_fresh(now_ms);
        Ok(())
    }

    fn retry(&mut self, now_ms: u64) -> Result<(), String> {
        if !self.prefs.hero_stats_consent {
            return Err("DRAFT_STATS_CONSENT_REQUIRED".to_owned());
        }
        if self.retry_not_before_ms.is_some_and(|until| now_ms < until) {
            return Err("DRAFT_STATS_RETRY_AFTER".to_owned());
        }
        self.clear_error();
        self.table_failure_latched = false;
        self.duration_failure_latched_hero = None;
        self.refresh_pending = !self.cache.public_tables_fresh(now_ms);
        Ok(())
    }

    fn clear_error(&mut self) {
        self.failure = None;
        self.failed_at_ms = None;
        self.http_status = None;
        self.retry_after_ms = None;
        self.retry_not_before_ms = None;
    }

    fn observe(
        &mut self,
        observation: DraftObservation,
        draft_bar: DraftBarObservation,
        monitor: MonitorState,
        now_ms: u64,
        now: Instant,
    ) {
        let valid_draft_screen = observation.phase.is_some()
            && observation.admitted_session
            && observation.ownership_verified
            && observation
                .age_ms
                .is_some_and(|age| age <= super::view::GSI_STALE_HIDE_MS);
        if valid_draft_screen && self.consent_chip_until.is_none() {
            self.consent_chip_until = Some(now + CONSENT_CHIP_DURATION);
        }
        let pick = observation
            .phase
            .filter(|_| {
                observation.admitted_session
                    && observation.ownership_verified
                    && observation.session_epoch.is_some()
                    && observation
                        .age_ms
                        .is_some_and(|age| age <= super::view::GSI_STALE_HIDE_MS)
            })
            .and_then(|_| {
                observation
                    .own_hero_key
                    .as_ref()
                    .filter(|key| !key.is_empty())
            })
            .map(|key| (observation.session_epoch, key.clone()));
        if pick != self.current_pick {
            self.pick_generation = self.pick_generation.saturating_add(1);
            if self
                .duration_failure_latched_hero
                .as_ref()
                .is_some_and(|failed| pick.as_ref().is_some_and(|(_, current)| current != failed))
            {
                self.duration_failure_latched_hero = None;
            }
            self.current_pick = pick;
            self.last_touched_duration = None;
        }
        self.observation = observation;
        self.draft_bar = draft_bar;
        self.monitor = monitor;
        if let Some((_, key)) = &self.current_pick {
            if let Some(hero_id) = self.hero_id_for_key(key) {
                if self.last_touched_duration != Some(hero_id) {
                    self.cache.touch_duration(hero_id, now_ms);
                    self.last_touched_duration = Some(hero_id);
                    if self.prefs.hero_stats_consent && !self.cache_blocked {
                        let _ = self.store.save_cache(&self.cache);
                    }
                }
            }
        }
        self.rebuild_view(now_ms, now);
    }

    fn next_request(&mut self, now_ms: u64) -> Option<(RequestTicket, Cancellation)> {
        if !self.prefs.hero_stats_consent || self.active.is_some() {
            return None;
        }
        if self.retry_not_before_ms.is_some_and(|until| now_ms < until) {
            return None;
        }
        let pending_table = if self.refresh_pending && !self.table_failure_latched {
            self.cache.prune_expired(now_ms);
            let mut requests = Vec::new();
            if self
                .cache
                .hero_stats
                .as_ref()
                .is_none_or(|entry| freshness(entry.fetched_at_ms, now_ms) != Freshness::Fresh)
            {
                requests.push(OutgoingRequest::HeroStats);
            }
            for lane in LaneRole::ALL {
                if self.cache.lane_roles[lane.index()]
                    .as_ref()
                    .is_none_or(|entry| freshness(entry.fetched_at_ms, now_ms) != Freshness::Fresh)
                {
                    requests.push(OutgoingRequest::LaneRoles(lane));
                }
            }
            requests.into_iter().next()
        } else {
            None
        };
        let (request, purpose) = if let Some(request) = pending_table {
            if let Err(wait) = self.ledger.can_send_table(now_ms) {
                self.set_rate_limited(now_ms, wait, None);
                self.refresh_pending = false;
                return None;
            }
            self.ledger.record_table(now_ms);
            if self.store.save_ledger(&self.ledger).is_err() {
                self.table_failure_latched = true;
                self.failure = Some(Failure::Network);
                self.failed_at_ms = Some(now_ms);
                self.refresh_pending = false;
                return None;
            }
            (request, RequestPurpose::PublicTable)
        } else {
            if self.refresh_pending && self.cache.public_tables_fresh(now_ms) {
                self.refresh_pending = false;
            }
            let (_, key) = self.current_pick.clone()?;
            if self.duration_failure_latched_hero.as_deref() == Some(key.as_str()) {
                return None;
            }
            let hero_id = self.hero_id_for_key(&key)?;
            if self
                .cache
                .duration_for(hero_id)
                .is_some_and(|entry| freshness(entry.fetched_at_ms, now_ms) == Freshness::Fresh)
            {
                return None;
            }
            if let Err(wait) = self.ledger.can_send_own_duration(now_ms) {
                self.set_rate_limited(now_ms, wait, None);
                return None;
            }
            self.ledger.record_own_duration(now_ms);
            if self.store.save_ledger(&self.ledger).is_err() {
                self.duration_failure_latched_hero = Some(key);
                self.failure = Some(Failure::Network);
                self.failed_at_ms = Some(now_ms);
                return None;
            }
            (
                OutgoingRequest::OwnHeroDurations(HeroId::new(hero_id)?),
                RequestPurpose::OwnDuration {
                    hero_id,
                    pick_generation: self.pick_generation,
                },
            )
        };
        self.ticket_sequence = self.ticket_sequence.saturating_add(1);
        let ticket = RequestTicket {
            id: self.ticket_sequence,
            consent_generation: self.consent_generation,
            request,
            purpose,
        };
        let cancellation = Cancellation::default();
        self.active = Some(ticket);
        self.active_cancel = Some(cancellation.clone());
        self.rebuild_view(now_ms, Instant::now());
        Some((ticket, cancellation))
    }

    fn finish(
        &mut self,
        ticket: RequestTicket,
        result: Result<HttpResponse, TransportError>,
        now_ms: u64,
    ) {
        if self.active.is_some_and(|active| active.id == ticket.id) {
            self.active = None;
            self.active_cancel = None;
        }
        // Consent and pick generations fence late network results. They do not reach memory,
        // disk or the view after revocation or a new own-hero selection.
        if ticket.consent_generation != self.consent_generation
            || !self.prefs.hero_stats_consent
            || self.cache_blocked
        {
            self.rebuild_view(now_ms, Instant::now());
            return;
        }
        if let RequestPurpose::OwnDuration {
            pick_generation, ..
        } = ticket.purpose
        {
            if pick_generation != self.pick_generation {
                self.rebuild_view(now_ms, Instant::now());
                return;
            }
        }
        self.clear_error();
        match result {
            Err(TransportError::Cancelled) => {
                self.latch_failed_ticket(ticket);
                self.set_failure(Failure::Network, now_ms, None, None);
            }
            Err(TransportError::TooLarge) => {
                self.latch_failed_ticket(ticket);
                self.set_failure(Failure::TooLarge, now_ms, None, None)
            }
            Err(TransportError::Timeout) => {
                self.latch_failed_ticket(ticket);
                self.set_failure(Failure::Timeout, now_ms, None, None)
            }
            Err(TransportError::Network) => {
                self.latch_failed_ticket(ticket);
                self.set_failure(Failure::Network, now_ms, None, None)
            }
            Ok(response) => self.consume_response(ticket, response, now_ms),
        }
        self.rebuild_view(now_ms, Instant::now());
    }

    fn consume_response(&mut self, ticket: RequestTicket, response: HttpResponse, now_ms: u64) {
        if response.status == 429 {
            self.latch_failed_ticket(ticket);
            let wait = response
                .retry_after
                .as_deref()
                .map_or(DEFAULT_RETRY_AFTER_MS, |value| {
                    parse_retry_after(value, now_ms)
                });
            self.set_rate_limited(now_ms, wait, Some(response.status));
            self.refresh_pending = false;
            return;
        }
        if !(200..300).contains(&response.status) {
            self.latch_failed_ticket(ticket);
            self.set_failure(Failure::HttpStatus, now_ms, Some(response.status), None);
            self.refresh_pending = false;
            return;
        }
        let cache_result = match ticket.request {
            OutgoingRequest::HeroStats => parse_hero_stats(&response.body).map(|value| {
                self.cache.hero_stats = Some(CacheEntry {
                    fetched_at_ms: now_ms,
                    value,
                });
            }),
            OutgoingRequest::LaneRoles(role) => {
                parse_lane_roles(&response.body, role).map(|value| {
                    self.cache.lane_roles[role.index()] = Some(CacheEntry {
                        fetched_at_ms: now_ms,
                        value,
                    });
                })
            }
            OutgoingRequest::OwnHeroDurations(hero_id) => {
                parse_durations(&response.body).map(|value| {
                    self.cache.insert_duration(DurationEntry {
                        hero_id: hero_id.get(),
                        fetched_at_ms: now_ms,
                        last_used_at_ms: now_ms,
                        value,
                    });
                })
            }
        };
        if let Err(error) = cache_result {
            self.latch_failed_ticket(ticket);
            let failure = match error {
                ParseError::InvalidBody => Failure::InvalidBody,
                ParseError::TooLarge => Failure::TooLarge,
            };
            self.set_failure(failure, now_ms, Some(response.status), None);
            self.refresh_pending = false;
            return;
        }
        if self.store.save_cache(&self.cache).is_err() {
            self.latch_failed_ticket(ticket);
            self.set_failure(Failure::Network, now_ms, Some(response.status), None);
            self.refresh_pending = false;
            return;
        }
        self.http_status = Some(response.status);
        if ticket.purpose == RequestPurpose::PublicTable {
            self.refresh_pending = true;
        }
    }

    fn latch_failed_ticket(&mut self, ticket: RequestTicket) {
        match ticket.purpose {
            RequestPurpose::PublicTable => {
                self.table_failure_latched = true;
                self.refresh_pending = false;
            }
            RequestPurpose::OwnDuration {
                pick_generation, ..
            } if pick_generation == self.pick_generation => {
                self.duration_failure_latched_hero = self
                    .current_pick
                    .as_ref()
                    .map(|(_, hero_key)| hero_key.clone());
            }
            RequestPurpose::OwnDuration { .. } => {}
        }
    }

    fn set_failure(
        &mut self,
        failure: Failure,
        now_ms: u64,
        status: Option<u16>,
        retry: Option<u64>,
    ) {
        self.failure = Some(failure);
        self.failed_at_ms = Some(now_ms);
        self.http_status = status;
        self.retry_after_ms = retry;
        self.retry_not_before_ms = retry.map(|wait| now_ms.saturating_add(wait));
    }

    fn set_rate_limited(&mut self, now_ms: u64, wait: u64, status: Option<u16>) {
        self.set_failure(Failure::RateLimited, now_ms, status, Some(wait));
    }

    fn hero_id_for_key(&self, key: &str) -> Option<u32> {
        self.cache
            .hero_stats
            .as_ref()?
            .value
            .values()
            .find(|hero| hero.name == key)
            .map(|hero| hero.id)
    }

    fn build_groups(&self, now_ms: u64) -> Option<PositionGroups> {
        if !self.cache.public_tables_usable(now_ms) {
            return None;
        }
        let heroes = &self.cache.hero_stats.as_ref()?.value;
        let lanes = [
            &self.cache.lane_roles[0].as_ref()?.value,
            &self.cache.lane_roles[1].as_ref()?.value,
            &self.cache.lane_roles[2].as_ref()?.value,
        ];
        build_position_groups(heroes, lanes).ok()
    }

    fn taken_hero_ids(&self) -> HashSet<u32> {
        let Some(hero_table) = self.cache.hero_stats.as_ref().map(|entry| &entry.value) else {
            return HashSet::new();
        };
        hero_table
            .values()
            .filter(|hero| self.draft_bar.hero_keys.iter().any(|key| key == &hero.name))
            .map(|hero| hero.id)
            .collect()
    }

    fn rebuild_view(&mut self, now_ms: u64, now: Instant) {
        self.cache.prune_expired(now_ms);
        let output_supported = self
            .monitor
            .monitor
            .is_some_and(|monitor| supported_output(monitor, self.monitor.scale_factor));
        let cache_is_fresh = self.cache.public_tables_fresh(now_ms);
        let data_state = if !self.prefs.hero_stats_consent {
            DataState::ConsentOff
        } else if self.active.is_some() || (self.refresh_pending && !cache_is_fresh) {
            DataState::Loading
        } else if self.failure == Some(Failure::RateLimited) {
            DataState::RateLimited
        } else if self.failure == Some(Failure::InvalidBody)
            || self.failure == Some(Failure::TooLarge)
        {
            DataState::ProviderError
        } else if self.failure.is_some() {
            DataState::FetchFailed
        } else if self.cache.public_tables_usable(now_ms) {
            if cache_is_fresh {
                DataState::Live
            } else {
                DataState::Stale
            }
        } else {
            DataState::Loading
        };
        let groups = self.build_groups(now_ms);
        let mut observation = self.observation.clone();
        if let Some((_, key)) = &self.current_pick {
            observation.own_hero_key = Some(key.clone());
        } else {
            observation.own_hero_key = None;
        }
        self.view = build_view(ViewInput {
            now_ms,
            prefs: &self.prefs,
            phase: observation.phase,
            admitted_session: observation.admitted_session,
            ownership_verified: observation.ownership_verified,
            gsi_age_ms: observation.age_ms,
            own_hero_key: observation.own_hero_key.as_deref(),
            output_supported,
            consent_chip_visible: self.consent_chip_until.is_some_and(|until| now < until),
            data_state,
            failed_at_ms: self.failed_at_ms,
            failure: self.failure,
            http_status: self.http_status,
            retry_after_ms: self.retry_after_ms,
            table_requests_24h: self.ledger.table_count_24h(now_ms).min(u32::MAX as usize) as u32,
            cache: &self.cache,
            groups: groups.as_ref(),
            draft_bar: DraftBarInput {
                available: self.draft_bar.available,
                identified: self.draft_bar.hero_keys.len().min(u8::MAX as usize) as u8,
            },
            taken_hero_ids: &self.taken_hero_ids(),
        });
    }
}

fn empty_view() -> DraftColumnView {
    DraftColumnView {
        schema: 1,
        mode: super::view::Mode::Hidden,
        overlay: super::view::OverlayView {
            visible: false,
            hidden_reason: Some(super::view::HiddenReason::NoDraft),
        },
        phase: None,
        placement_verified: false,
        data_state: DataState::ConsentOff,
        context: super::view::ViewContext {
            position: None,
            bracket: None,
        },
        source: super::view::ViewSource {
            provider: "OpenDota",
            window: None,
            fetched_at_ms: None,
            failed_at_ms: None,
            failure: None,
            http_status: None,
            retry_after_ms: None,
            table_requests_24h: 0,
            table_request_budget: super::budget::TABLE_REQUEST_LIMIT as u32,
        },
        suggestions: None,
        pick_card: None,
    }
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn parse_retry_after(value: &str, now_ms: u64) -> u64 {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return seconds.saturating_mul(1000);
    }
    parse_http_date_ms(value.trim()).map_or(DEFAULT_RETRY_AFTER_MS, |deadline| {
        deadline.saturating_sub(now_ms)
    })
}

/// Parse the IMF-fixdate form required for HTTP-date Retry-After values without introducing a
/// second date/time dependency. Invalid dates fall back to the specified one-hour delay.
fn parse_http_date_ms(value: &str) -> Option<u64> {
    let (_, rest) = value.split_once(", ")?;
    let fields = rest.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 || fields[4] != "GMT" {
        return None;
    }
    let day = fields[0].parse::<u32>().ok()?;
    let month = match fields[1] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year = fields[2].parse::<i64>().ok()?;
    let mut time = fields[3].split(':');
    let hour = time.next()?.parse::<u32>().ok()?;
    let minute = time.next()?.parse::<u32>().ok()?;
    let second = time.next()?.parse::<u32>().ok()?;
    if time.next().is_some() || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60
    {
        return None;
    }
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    if days < 0 {
        return None;
    }
    let seconds = u64::try_from(days)
        .ok()?
        .checked_mul(86_400)?
        .checked_add(u64::from(hour) * 3600 + u64::from(minute) * 60 + u64::from(second))?;
    seconds.checked_mul(1000)
}

fn control_window_allowed(label: &str) -> bool {
    label == "control"
}

pub(crate) struct HeroStatsHost {
    core: Arc<Mutex<Core>>,
    stop: Arc<AtomicBool>,
    join: Mutex<Option<std::thread::JoinHandle<()>>>,
    transport: Arc<dyn Transport>,
}

impl HeroStatsHost {
    pub(crate) fn start(
        app: tauri::AppHandle<tauri::Wry>,
        window: Option<tauri::WebviewWindow<tauri::Wry>>,
        data_dir: &Path,
        read_draft: Arc<dyn Fn() -> DraftObservation + Send + Sync>,
        read_draft_bar: Arc<dyn Fn() -> DraftBarObservation + Send + Sync>,
    ) -> Self {
        Self::start_with_transport(
            app,
            window,
            data_dir,
            read_draft,
            read_draft_bar,
            Arc::new(WinHttpTransport::default()),
        )
    }

    fn start_with_transport(
        app: tauri::AppHandle<tauri::Wry>,
        window: Option<tauri::WebviewWindow<tauri::Wry>>,
        data_dir: &Path,
        read_draft: Arc<dyn Fn() -> DraftObservation + Send + Sync>,
        read_draft_bar: Arc<dyn Fn() -> DraftBarObservation + Send + Sync>,
        transport: Arc<dyn Transport>,
    ) -> Self {
        let core = Arc::new(Mutex::new(Core::open(data_dir, epoch_ms())));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_core = core.clone();
        let thread_stop = stop.clone();
        let thread_transport = transport.clone();
        let thread = std::thread::Builder::new()
            .name("draft-stats-host".to_owned())
            .spawn(move || {
                run_loop(
                    app,
                    window,
                    thread_core,
                    thread_stop,
                    thread_transport,
                    read_draft,
                    read_draft_bar,
                )
            });
        let join = match thread {
            Ok(join) => Some(join),
            Err(_) => {
                crate::log_warn!(
                    "draft",
                    "host_thread_unavailable",
                    "draft statistics host did not start"
                );
                None
            }
        };
        Self {
            core,
            stop,
            join: Mutex::new(join),
            transport,
        }
    }

    pub(crate) fn prefs_get(&self, window_label: &str) -> Result<DraftStatsPrefs, String> {
        if !control_window_allowed(window_label) {
            return Err("DRAFT_STATS_CONTROL_WINDOW_ONLY".to_owned());
        }
        Ok(lock(&self.core).prefs.clone())
    }

    pub(crate) fn prefs_set(
        &self,
        window_label: &str,
        patch: Value,
    ) -> Result<DraftStatsPrefs, String> {
        if !control_window_allowed(window_label) {
            return Err("DRAFT_STATS_CONTROL_WINDOW_ONLY".to_owned());
        }
        let result = {
            let mut core = lock(&self.core);
            let result = core.set_prefs_with_cancel(&patch, epoch_ms(), || {
                self.transport.cancel_current();
            });
            core.rebuild_view(epoch_ms(), Instant::now());
            result
        };
        result.map(|(prefs, _)| prefs)
    }

    pub(crate) fn retry(&self, window_label: &str) -> Result<(), String> {
        if !control_window_allowed(window_label) {
            return Err("DRAFT_STATS_CONTROL_WINDOW_ONLY".to_owned());
        }
        lock(&self.core).retry(epoch_ms())
    }

    pub(crate) fn view(&self, window_label: &str) -> Result<DraftColumnView, String> {
        if !control_window_allowed(window_label) {
            return Err("DRAFT_STATS_CONTROL_WINDOW_ONLY".to_owned());
        }
        Ok(lock(&self.core).view.clone())
    }

    pub(crate) fn stop_for_exit(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(cancel) = lock(&self.core).active_cancel.as_ref() {
            cancel.cancel();
        }
        self.transport.cancel_current();
        // Detach rather than join while the Tauri event loop is exiting; the worker owns no data
        // outside its Arc state and observes `stop` before its next poll.
        drop(lock(&self.join).take());
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn run_loop(
    app: tauri::AppHandle<tauri::Wry>,
    window: Option<tauri::WebviewWindow<tauri::Wry>>,
    core: Arc<Mutex<Core>>,
    stop: Arc<AtomicBool>,
    transport: Arc<dyn Transport>,
    read_draft: Arc<dyn Fn() -> DraftObservation + Send + Sync>,
    read_draft_bar: Arc<dyn Fn() -> DraftBarObservation + Send + Sync>,
) {
    let (completion_tx, completion_rx) =
        mpsc::channel::<(RequestTicket, Result<HttpResponse, TransportError>)>();
    let mut last_payload: Option<Value> = None;
    let mut last_emit = Instant::now() - KEEPALIVE;
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let monitor = window
            .as_ref()
            .map_or(MonitorState::default(), monitor_state);
        let observation = read_draft();
        let draft_bar = read_draft_bar();
        let now_ms = epoch_ms();
        let view = {
            let mut state = lock(&core);
            match completion_rx.try_recv() {
                Ok((ticket, result)) => state.finish(ticket, result, now_ms),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
            }
            state.observe(observation, draft_bar, monitor, now_ms, now);
            if let Some((ticket, cancellation)) = state.next_request(now_ms) {
                let transport = transport.clone();
                let sender = completion_tx.clone();
                let spawn = std::thread::Builder::new()
                    .name("draft-stats-http".to_owned())
                    .spawn(move || {
                        let result = transport.get(ticket.request, &cancellation);
                        let _ = sender.send((ticket, result));
                    });
                if spawn.is_err() {
                    state.finish(ticket, Err(TransportError::Network), now_ms);
                }
            }
            state.rebuild_view(now_ms, now);
            state.view.clone()
        };
        if let Some(window) = &window {
            let _ = apply_window_state(window, &view);
        }
        if let Ok(payload) = serde_json::to_value(&view) {
            if last_payload.as_ref() != Some(&payload)
                || now.saturating_duration_since(last_emit) >= KEEPALIVE
            {
                let _ = app.emit_to("draft", STATE_EVENT, payload.clone());
                last_payload = Some(payload);
                last_emit = now;
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn monitor_state(window: &tauri::WebviewWindow<tauri::Wry>) -> MonitorState {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return MonitorState::default();
    };
    let position = monitor.position();
    let size = monitor.size();
    MonitorState {
        monitor: Some(Monitor {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }),
        scale_factor: monitor.scale_factor(),
    }
}

fn apply_window_state(window: &tauri::WebviewWindow<tauri::Wry>, view: &DraftColumnView) -> bool {
    let visible = view.overlay.visible;
    if !visible {
        let _ = window.hide();
        return false;
    }
    let Some(monitor) = window.primary_monitor().ok().flatten() else {
        let _ = window.hide();
        return false;
    };
    let position = monitor.position();
    let size = monitor.size();
    let monitor_rect = Monitor {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };
    let rect = if view.mode == super::view::Mode::PickCard
        && view.phase == Some(DraftPhase::StrategyTime)
    {
        Rect {
            x: position.x + PICK_CARD_STRATEGY_TIME_RECT.x,
            y: position.y + PICK_CARD_STRATEGY_TIME_RECT.y,
            width: PICK_CARD_STRATEGY_TIME_RECT.width,
            height: PICK_CARD_STRATEGY_TIME_RECT.height,
        }
    } else {
        let Some(rect) = draft_column_rect(monitor_rect) else {
            let _ = window.hide();
            return false;
        };
        rect
    };
    if !supported_output(monitor_rect, monitor.scale_factor()) {
        let _ = window.hide();
        return false;
    }
    if window
        .set_position(tauri::PhysicalPosition::new(rect.x, rect.y))
        .is_err()
        || window
            .set_size(tauri::PhysicalSize::new(rect.width, rect.height))
            .is_err()
        || window.set_ignore_cursor_events(true).is_err()
    {
        let _ = window.hide();
        return false;
    }
    if !crate::show_without_activation(window) {
        let _ = window.hide();
        return false;
    }
    true
}

pub(crate) fn create_window(
    app: &mut tauri::App<tauri::Wry>,
) -> Option<tauri::WebviewWindow<tauri::Wry>> {
    #[cfg(windows)]
    {
        let window = tauri::WebviewWindowBuilder::new(
            app,
            "draft",
            tauri::WebviewUrl::App("index.html?view=draft".into()),
        )
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .shadow(false)
        .resizable(false)
        .visible(false)
        .additional_browser_args(crate::WEBVIEW2_BROWSER_ARGS)
        .inner_size(180.0, 488.0)
        .build()
        .ok()?;
        let configured = window.set_ignore_cursor_events(true).is_ok()
            && crate::overlay_hwnd(&window).is_some_and(apply_no_activate_styles);
        if !configured {
            let _ = window.destroy();
            return None;
        }
        Some(window)
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        None
    }
}

#[cfg(windows)]
fn apply_no_activate_styles(hwnd: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let required = WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0;
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (current | required) as isize);
        GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & required == required
    }
}

#[cfg(test)]
mod tests {
    use super::{
        control_window_allowed, parse_http_date_ms, parse_retry_after, Core, DraftObservation,
        FileStatsStore, MonitorState, RequestPurpose, StatsStore,
    };
    use crate::hero_stats::budget::RequestLedger;
    use crate::hero_stats::cache::{CacheEntry, StatsCache};
    use crate::hero_stats::prefs::DraftStatsPrefs;
    use crate::hero_stats::request::{LaneRole, OutgoingRequest};
    use crate::hero_stats::transport::{Cancellation, HttpResponse, Transport, TransportError};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    struct InjectedStore {
        inner: FileStatsStore,
        fail_prefs_save: AtomicBool,
        fail_cache_delete: AtomicBool,
        events: Mutex<Vec<&'static str>>,
    }

    impl InjectedStore {
        fn in_data_dir(dir: &Path) -> Self {
            Self {
                inner: FileStatsStore::in_data_dir(dir),
                fail_prefs_save: AtomicBool::new(false),
                fail_cache_delete: AtomicBool::new(false),
                events: Mutex::new(Vec::new()),
            }
        }

        fn event(&self, name: &'static str) {
            self.events.lock().unwrap().push(name);
        }
    }

    impl StatsStore for InjectedStore {
        fn load_prefs(&self) -> DraftStatsPrefs {
            self.event("load_prefs");
            self.inner.load_prefs()
        }

        fn save_prefs(&self, prefs: &DraftStatsPrefs) -> Result<(), String> {
            self.event("save_prefs");
            if self.fail_prefs_save.load(Ordering::SeqCst) {
                return Err("DRAFT_STATS_PREFS_WRITE_FAILED".to_owned());
            }
            self.inner.save_prefs(prefs)
        }

        fn load_cache(&self, now_ms: u64) -> StatsCache {
            self.event("load_cache");
            self.inner.load_cache(now_ms)
        }

        fn save_cache(&self, cache: &StatsCache) -> Result<(), String> {
            self.event("save_cache");
            self.inner.save_cache(cache)
        }

        fn delete_cache(&self) -> Result<(), String> {
            self.event("delete_cache");
            if self.fail_cache_delete.load(Ordering::SeqCst) {
                return Err("DRAFT_STATS_CACHE_DELETE_FAILED".to_owned());
            }
            self.inner.delete_cache()
        }

        fn load_ledger(&self, now_ms: u64) -> RequestLedger {
            self.event("load_ledger");
            self.inner.load_ledger(now_ms)
        }

        fn save_ledger(&self, ledger: &RequestLedger) -> Result<(), String> {
            self.event("save_ledger");
            self.inner.save_ledger(ledger)
        }
    }

    #[derive(Default)]
    struct MockTransport {
        requests: Mutex<Vec<OutgoingRequest>>,
    }
    impl Transport for MockTransport {
        fn get(
            &self,
            request: OutgoingRequest,
            cancellation: &Cancellation,
        ) -> Result<HttpResponse, TransportError> {
            self.requests.lock().unwrap().push(request);
            if cancellation.is_cancelled() {
                return Err(TransportError::Cancelled);
            }
            Ok(HttpResponse {
                status: 200,
                retry_after: None,
                body: b"[]".to_vec(),
            })
        }
    }

    fn core(dir: &Path, now_ms: u64) -> Core {
        let mut core = Core::open(dir, now_ms);
        // Most host unit tests set in-memory consent directly to keep fixtures concise; the
        // restart-specific tests exercise the persisted-consent barrier through open_with_store.
        core.cache_blocked = false;
        core
    }

    fn valid_hero_stats() -> Vec<u8> {
        let mut row = serde_json::json!({
            "id": 1,
            "name": "npc_dota_hero_test",
            "localized_name": "Test Hero",
            "roles": ["Carry"],
            "pub_pick": 1000,
            "pub_win": 500
        });
        for bracket in 1..=8 {
            row[format!("{bracket}_pick")] = serde_json::json!(1000);
            row[format!("{bracket}_win")] = serde_json::json!(500);
        }
        serde_json::to_vec(&serde_json::json!([row])).unwrap()
    }

    fn valid_two_hero_stats() -> Vec<u8> {
        let first: serde_json::Value = serde_json::from_slice(&valid_hero_stats()).unwrap();
        let mut second = first[0].clone();
        second["id"] = serde_json::json!(2);
        second["name"] = serde_json::json!("npc_dota_hero_test_two");
        let mut rows = first.as_array().unwrap().clone();
        rows.push(second);
        serde_json::to_vec(&rows).unwrap()
    }

    fn seed_consent_and_cache(store: &InjectedStore, now_ms: u64) {
        store
            .inner
            .save_prefs(&DraftStatsPrefs {
                hero_stats_consent: true,
                ..Default::default()
            })
            .unwrap();
        store
            .inner
            .save_cache(&StatsCache {
                hero_stats: Some(CacheEntry {
                    fetched_at_ms: now_ms,
                    value: crate::hero_stats::parse::parse_hero_stats(&valid_hero_stats()).unwrap(),
                }),
                ..Default::default()
            })
            .unwrap();
        store.events.lock().unwrap().clear();
    }

    fn public_cache(now_ms: u64) -> StatsCache {
        StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: now_ms,
                value: crate::hero_stats::parse::parse_hero_stats(&valid_hero_stats()).unwrap(),
            }),
            lane_roles: std::array::from_fn(|_| {
                Some(CacheEntry {
                    fetched_at_ms: now_ms,
                    value: BTreeMap::new(),
                })
            }),
            ..Default::default()
        }
    }

    #[test]
    fn preferences_and_view_commands_reject_every_non_control_label() {
        assert!(control_window_allowed("control"));
        for label in ["draft", "main", "overlay", "hud", ""] {
            assert!(!control_window_allowed(label));
        }
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_date_with_one_hour_fallback() {
        assert_eq!(parse_retry_after("12", 10_000), 12_000);
        let date = parse_http_date_ms("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        assert_eq!(date, 784_111_777_000);
        assert_eq!(
            parse_retry_after("Sun, 06 Nov 1994 08:49:37 GMT", date - 10_000),
            10_000
        );
        assert_eq!(parse_retry_after("not a date", 10_000), 3_600_000);
    }

    #[test]
    fn consent_chip_clock_starts_at_first_verified_draft_and_does_not_restart() {
        let dir = std::env::temp_dir().join(format!("kohbor-draft-chip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut core = core(&dir, 1_000_000);
        let first_draft = std::time::Instant::now();
        core.observe(
            DraftObservation::default(),
            Default::default(),
            MonitorState::default(),
            1_000_000,
            first_draft,
        );
        assert_eq!(core.consent_chip_until, None);

        let valid = DraftObservation {
            phase: Some(crate::hero_stats::DraftPhase::HeroSelection),
            admitted_session: true,
            ownership_verified: true,
            session_epoch: Some(1),
            age_ms: Some(0),
            own_hero_key: None,
        };
        core.observe(
            valid.clone(),
            Default::default(),
            MonitorState::default(),
            1_000_001,
            first_draft + std::time::Duration::from_secs(2),
        );
        let deadline =
            first_draft + std::time::Duration::from_secs(2) + super::CONSENT_CHIP_DURATION;
        assert_eq!(core.consent_chip_until, Some(deadline));

        core.observe(
            valid,
            Default::default(),
            MonitorState::default(),
            1_000_002,
            first_draft + std::time::Duration::from_secs(12),
        );
        assert_eq!(core.consent_chip_until, Some(deadline));
        assert!(!core
            .consent_chip_until
            .is_some_and(|until| { first_draft + std::time::Duration::from_secs(12) < until }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn consent_revocation_drops_late_response_and_deletes_public_cache() {
        let dir = std::env::temp_dir().join(format!("kohbor-draft-revoke-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut core = core(&dir, 1_000_000);
        core.prefs = crate::hero_stats::DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        core.cache.hero_stats = Some(crate::hero_stats::CacheEntry {
            fetched_at_ms: 1_000_000,
            value: BTreeMap::new(),
        });
        core.store.save_cache(&core.cache).unwrap();
        let ticket = super::RequestTicket {
            id: 1,
            consent_generation: core.consent_generation,
            request: OutgoingRequest::HeroStats,
            purpose: RequestPurpose::PublicTable,
        };
        core.active = Some(ticket);
        core.active_cancel = Some(Cancellation::default());
        let (_, revoked) = core
            .set_prefs(&serde_json::json!({"hero_stats_consent": false}), 1_000_001)
            .unwrap();
        assert!(revoked);
        core.finish(
            ticket,
            Ok(HttpResponse {
                status: 200,
                retry_after: None,
                body: b"[]".to_vec(),
            }),
            1_000_002,
        );
        assert!(core.cache.hero_stats.is_none());
        assert!(!dir.join(crate::hero_stats::CACHE_FILE).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prefs_write_failure_is_reported_and_restart_stays_revoked_without_reading_cache() {
        let dir =
            std::env::temp_dir().join(format!("kohbor-draft-prefs-failure-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = 30_000_000;
        let store = std::sync::Arc::new(InjectedStore::in_data_dir(&dir));
        seed_consent_and_cache(&store, now);
        let mut core = Core::open_with_store(store.clone(), now);
        store.events.lock().unwrap().clear();
        store.fail_prefs_save.store(true, Ordering::SeqCst);
        let in_flight = Cancellation::default();
        core.active_cancel = Some(in_flight.clone());

        let error = core
            .set_prefs_with_cancel(
                &serde_json::json!({"hero_stats_consent": false}),
                now + 1,
                || store.event("cancel_in_flight"),
            )
            .unwrap_err();
        assert!(error.contains("DRAFT_STATS_PREFS_WRITE_FAILED"));
        assert!(in_flight.is_cancelled());
        assert!(!core.prefs.hero_stats_consent);
        assert!(core.cache.hero_stats.is_none());
        assert_eq!(
            *store.events.lock().unwrap(),
            [
                "cancel_in_flight",
                "save_prefs",
                "delete_cache",
                "save_ledger"
            ]
        );
        assert!(!dir.join(crate::hero_stats::CACHE_FILE).exists());
        assert!(core.ledger.revocation_pending);

        store.fail_prefs_save.store(false, Ordering::SeqCst);
        store.events.lock().unwrap().clear();
        let restarted = Core::open_with_store(store.clone(), now + 2);
        assert!(!restarted.prefs.hero_stats_consent);
        assert!(restarted.cache.hero_stats.is_none());
        assert_eq!(
            *store.events.lock().unwrap(),
            ["load_prefs", "load_ledger", "delete_cache"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cache_delete_failure_is_reported_and_restart_deletes_before_any_cache_read() {
        let dir = std::env::temp_dir().join(format!(
            "kohbor-draft-delete-failure-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = 40_000_000;
        let store = std::sync::Arc::new(InjectedStore::in_data_dir(&dir));
        seed_consent_and_cache(&store, now);
        let mut core = Core::open_with_store(store.clone(), now);
        store.events.lock().unwrap().clear();
        store.fail_cache_delete.store(true, Ordering::SeqCst);

        let result = core.set_prefs(&serde_json::json!({"hero_stats_consent": false}), now + 1);
        assert!(result.is_err());
        assert!(!core.prefs.hero_stats_consent);
        assert!(core.cache.hero_stats.is_none());
        assert_eq!(
            core.failure,
            Some(crate::hero_stats::Failure::CacheDeleteFailed)
        );
        assert_eq!(
            *store.events.lock().unwrap(),
            ["save_prefs", "delete_cache", "save_ledger"]
        );
        assert!(dir.join(crate::hero_stats::CACHE_FILE).exists());

        store.fail_cache_delete.store(false, Ordering::SeqCst);
        store.events.lock().unwrap().clear();
        let restarted = Core::open_with_store(store.clone(), now + 2);
        assert!(!restarted.prefs.hero_stats_consent);
        assert!(restarted.cache.hero_stats.is_none());
        assert!(!dir.join(crate::hero_stats::CACHE_FILE).exists());
        assert_eq!(
            *store.events.lock().unwrap(),
            ["load_prefs", "delete_cache", "load_ledger", "save_ledger"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cache_deletion_failure_is_reported_and_old_cache_stays_inaccessible() {
        let dir =
            std::env::temp_dir().join(format!("kohbor-draft-revoke-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(crate::hero_stats::CACHE_FILE)).unwrap();
        let mut core = core(&dir, 2_000_000);
        core.prefs = crate::hero_stats::DraftStatsPrefs {
            hero_stats_consent: true,
            ..Default::default()
        };
        core.cache.hero_stats = Some(crate::hero_stats::CacheEntry {
            fetched_at_ms: 2_000_000,
            value: BTreeMap::new(),
        });
        let error = core
            .set_prefs(&serde_json::json!({"hero_stats_consent": false}), 2_000_001)
            .unwrap_err();
        assert!(error.contains("DRAFT_STATS_CACHE_DELETE_FAILED"));
        assert!(core.cache.hero_stats.is_none());
        assert!(core.ledger.cache_delete_failed);
        assert_eq!(
            core.failure,
            Some(crate::hero_stats::Failure::CacheDeleteFailed)
        );
        assert_eq!(
            core.view.source.failure,
            Some(crate::hero_stats::Failure::CacheDeleteFailed)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn expired_public_and_duration_entries_are_removed_before_the_pick_card_is_built() {
        let dir = std::env::temp_dir().join(format!("kohbor-draft-expired-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = 1_000_000_000;
        let expired_at = now - (7 * 24 * 60 * 60 * 1000) - 1;
        let mut core = core(&dir, now);
        core.prefs.hero_stats_consent = true;
        core.cache = public_cache(expired_at);
        core.cache
            .own_durations
            .push(crate::hero_stats::cache::DurationEntry {
                hero_id: 1,
                fetched_at_ms: expired_at,
                last_used_at_ms: expired_at,
                value: BTreeMap::from([(
                    2_400,
                    crate::hero_stats::parse::Counts {
                        games: 300,
                        wins: 150,
                    },
                )]),
            });
        core.observation = DraftObservation {
            phase: Some(crate::hero_stats::DraftPhase::HeroSelection),
            admitted_session: true,
            ownership_verified: true,
            session_epoch: Some(1),
            age_ms: Some(0),
            own_hero_key: Some("npc_dota_hero_test".to_owned()),
        };
        core.current_pick = Some((Some(1), "npc_dota_hero_test".to_owned()));
        core.monitor = MonitorState {
            monitor: Some(super::super::Monitor {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }),
            scale_factor: 1.0,
        };

        core.rebuild_view(now, std::time::Instant::now());

        assert!(core.cache.hero_stats.is_none());
        assert!(core.cache.lane_roles.iter().all(Option::is_none));
        assert!(core.cache.own_durations.is_empty());
        let card = core.view.pick_card.as_ref().unwrap();
        assert_eq!(card.hero_id, None);
        assert_eq!(card.name, None);
        assert_eq!(card.games, None);
        assert_eq!(card.win_rate_tenths, None);
        assert!(card.durations.cells.iter().all(|cell| cell.games.is_none()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ownership_mismatch_hides_the_pick_card_and_drops_old_values_in_one_update() {
        let dir =
            std::env::temp_dir().join(format!("kohbor-draft-mismatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = 60_000_000;
        let mut core = core(&dir, now);
        core.prefs.hero_stats_consent = true;
        core.cache_blocked = false;
        core.cache = public_cache(now);
        let monitor = MonitorState {
            monitor: Some(super::super::Monitor {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }),
            scale_factor: 1.0,
        };
        let first = DraftObservation {
            phase: Some(crate::hero_stats::DraftPhase::HeroSelection),
            admitted_session: true,
            ownership_verified: true,
            session_epoch: Some(2),
            age_ms: Some(0),
            own_hero_key: Some("npc_dota_hero_test".to_owned()),
        };
        let now_instant = std::time::Instant::now();
        core.observe(first, Default::default(), monitor, now, now_instant);
        let shown = core.view.pick_card.as_ref().unwrap();
        assert_eq!(core.view.mode, crate::hero_stats::view::Mode::PickCard);
        assert_eq!(shown.hero_id, Some(1));
        assert_eq!(shown.win_rate_tenths, Some(500));

        let mismatch = DraftObservation {
            phase: Some(crate::hero_stats::DraftPhase::HeroSelection),
            admitted_session: true,
            ownership_verified: false,
            session_epoch: Some(2),
            age_ms: Some(1),
            own_hero_key: None,
        };
        core.observe(mismatch, Default::default(), monitor, now + 1, now_instant);
        assert_eq!(core.current_pick, None);
        assert_eq!(core.view.mode, crate::hero_stats::view::Mode::Hidden);
        assert!(!core.view.overlay.visible);
        assert!(core.view.pick_card.is_none());
        assert!(core.view.suggestions.is_none());
        let serialized = serde_json::to_string(&core.view).unwrap();
        assert!(!serialized.contains("Test Hero"));
        assert!(!serialized.contains(r#""win_rate_tenths":500"#));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mock_transport_never_contacts_the_network_and_429_sets_retry_state() {
        let dir = std::env::temp_dir().join(format!("kohbor-draft-429-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mock = MockTransport::default();
        let request = OutgoingRequest::LaneRoles(LaneRole::Safe);
        let result = mock.get(request, &Cancellation::default()).unwrap();
        assert_eq!(result.status, 200);
        assert_eq!(*mock.requests.lock().unwrap(), [request]);
        let mut core = core(&dir, 5_000_000);
        core.prefs.hero_stats_consent = true;
        let ticket = super::RequestTicket {
            id: 1,
            consent_generation: core.consent_generation,
            request,
            purpose: RequestPurpose::PublicTable,
        };
        core.active = Some(ticket);
        core.finish(
            ticket,
            Ok(HttpResponse {
                status: 429,
                retry_after: Some("60".to_owned()),
                body: Vec::new(),
            }),
            5_000_000,
        );
        assert_eq!(core.failure, Some(crate::hero_stats::Failure::RateLimited));
        assert_eq!(core.retry_after_ms, Some(60_000));
        assert_eq!(core.retry_not_before_ms, Some(5_060_000));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn transport_failure_sends_one_table_request_across_repeated_ticks() {
        let dir =
            std::env::temp_dir().join(format!("kohbor-draft-no-retry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut core = core(&dir, 10_000_000);
        core.prefs.hero_stats_consent = true;
        core.refresh_pending = true;

        let mut sends = Vec::new();
        let (ticket, _) = core.next_request(10_000_000).unwrap();
        sends.push(ticket.request);
        core.finish(ticket, Err(TransportError::Network), 10_000_001);
        for tick in 1..=20 {
            if let Some((ticket, _)) = core.next_request(10_000_001 + tick * 1_000) {
                sends.push(ticket.request);
            }
        }
        assert_eq!(sends, [OutgoingRequest::HeroStats]);
        core.retry(10_030_000).unwrap();
        let (retried, _) = core.next_request(10_030_001).unwrap();
        assert_eq!(retried.request, OutgoingRequest::HeroStats);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn duration_429_does_not_send_again_after_retry_after_expires() {
        let dir =
            std::env::temp_dir().join(format!("kohbor-draft-duration-429-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = 20_000_000;
        let mut core = core(&dir, now);
        core.prefs.hero_stats_consent = true;
        core.refresh_pending = false;
        core.cache.hero_stats = Some(crate::hero_stats::CacheEntry {
            fetched_at_ms: now,
            value: crate::hero_stats::parse::parse_hero_stats(&valid_hero_stats()).unwrap(),
        });
        core.current_pick = Some((Some(1), "npc_dota_hero_test".to_owned()));
        core.pick_generation = 1;

        let (ticket, _) = core.next_request(now).unwrap();
        let mut sends = vec![ticket.request];
        core.finish(
            ticket,
            Ok(HttpResponse {
                status: 429,
                retry_after: Some("1".to_owned()),
                body: Vec::new(),
            }),
            now,
        );
        if let Some((ticket, _)) = core.next_request(now + 1_001) {
            sends.push(ticket.request);
        }
        assert_eq!(
            sends,
            [OutgoingRequest::OwnHeroDurations(
                crate::hero_stats::request::HeroId::new(1).unwrap()
            )]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_duration_stays_latched_across_ticks_and_a_different_hero_can_send() {
        let dir = std::env::temp_dir().join(format!(
            "kohbor-draft-duration-latch-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let now = 70_000_000;
        let mut core = core(&dir, now);
        core.prefs.hero_stats_consent = true;
        core.refresh_pending = false;
        core.cache.hero_stats = Some(crate::hero_stats::CacheEntry {
            fetched_at_ms: now,
            value: crate::hero_stats::parse::parse_hero_stats(&valid_two_hero_stats()).unwrap(),
        });
        core.current_pick = Some((Some(1), "npc_dota_hero_test".to_owned()));
        core.pick_generation = 1;

        let (ticket, _) = core.next_request(now).unwrap();
        let mut sends = vec![ticket.request];
        core.finish(ticket, Err(TransportError::Network), now + 1);
        for tick in 1..=10 {
            if let Some((ticket, _)) = core.next_request(now + 1 + tick * 1_000) {
                sends.push(ticket.request);
            }
        }
        assert_eq!(sends.len(), 1);

        core.observe(
            DraftObservation {
                phase: Some(crate::hero_stats::DraftPhase::HeroSelection),
                admitted_session: true,
                ownership_verified: true,
                session_epoch: Some(1),
                age_ms: Some(0),
                own_hero_key: Some("npc_dota_hero_test_two".to_owned()),
            },
            Default::default(),
            MonitorState::default(),
            now + 20_000,
            std::time::Instant::now(),
        );
        let (next_ticket, _) = core.next_request(now + 20_001).unwrap();
        assert_eq!(
            next_ticket.request,
            OutgoingRequest::OwnHeroDurations(crate::hero_stats::request::HeroId::new(2).unwrap())
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn table_fetch_plan_is_closed_to_hero_stats_and_three_fixed_lanes() {
        let dir = std::env::temp_dir().join(format!("kohbor-draft-routes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut core = core(&dir, 8_000_000);
        core.prefs.hero_stats_consent = true;
        core.refresh_pending = true;
        let mut requests = Vec::new();
        for i in 0..4 {
            let Some((ticket, _)) = core.next_request(8_000_000 + i) else {
                break;
            };
            requests.push(ticket.request);
            let body = match ticket.request {
                OutgoingRequest::HeroStats => valid_hero_stats(),
                OutgoingRequest::LaneRoles(_) => b"[]".to_vec(),
                OutgoingRequest::OwnHeroDurations(_) => unreachable!(),
            };
            core.finish(
                ticket,
                Ok(HttpResponse {
                    status: 200,
                    retry_after: None,
                    body,
                }),
                8_000_000 + i,
            );
        }
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0], OutgoingRequest::HeroStats);
        assert_eq!(requests[1], OutgoingRequest::LaneRoles(LaneRole::Safe));
        assert_eq!(requests[2], OutgoingRequest::LaneRoles(LaneRole::Mid));
        assert_eq!(requests[3], OutgoingRequest::LaneRoles(LaneRole::Off));
        let _ = DraftObservation::default();
        let _ = std::fs::remove_dir_all(dir);
    }
}
