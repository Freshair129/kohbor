use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(crate) const TABLE_REQUEST_LIMIT: usize = 4;
pub(crate) const DURATION_REQUESTS_PER_HOUR: usize = 30;
pub(crate) const DURATION_REQUESTS_PER_DAY: usize = 200;
pub(crate) const HOUR_MS: u64 = 60 * 60 * 1000;
pub(crate) const DAY_MS: u64 = 24 * HOUR_MS;
const LEDGER_FILE: &str = "draft-stats-request-ledger.json";
const LEDGER_SCHEMA: u8 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RequestLedger {
    table_requests_ms: Vec<u64>,
    own_duration_requests_ms: Vec<u64>,
    pub(crate) cache_delete_failed: bool,
    pub(crate) revocation_pending: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredLedger {
    schema_version: u8,
    table_requests_ms: Vec<u64>,
    own_duration_requests_ms: Vec<u64>,
    cache_delete_failed: bool,
    #[serde(default)]
    revocation_pending: bool,
}

impl From<StoredLedger> for RequestLedger {
    fn from(stored: StoredLedger) -> Self {
        Self {
            table_requests_ms: stored.table_requests_ms,
            own_duration_requests_ms: stored.own_duration_requests_ms,
            cache_delete_failed: stored.cache_delete_failed,
            revocation_pending: stored.revocation_pending,
        }
    }
}

impl From<&RequestLedger> for StoredLedger {
    fn from(ledger: &RequestLedger) -> Self {
        Self {
            schema_version: LEDGER_SCHEMA,
            table_requests_ms: ledger.table_requests_ms.clone(),
            own_duration_requests_ms: ledger.own_duration_requests_ms.clone(),
            cache_delete_failed: ledger.cache_delete_failed,
            revocation_pending: ledger.revocation_pending,
        }
    }
}

impl RequestLedger {
    fn in_window(timestamp_ms: u64, now_ms: u64, window_ms: u64) -> bool {
        timestamp_ms > now_ms || now_ms - timestamp_ms < window_ms
    }

    pub(crate) fn prune(&mut self, now_ms: u64) {
        self.table_requests_ms
            .retain(|timestamp| Self::in_window(*timestamp, now_ms, DAY_MS));
        self.own_duration_requests_ms
            .retain(|timestamp| Self::in_window(*timestamp, now_ms, DAY_MS));
    }

    pub(crate) fn table_count_24h(&self, now_ms: u64) -> usize {
        self.table_requests_ms
            .iter()
            .filter(|timestamp| Self::in_window(**timestamp, now_ms, DAY_MS))
            .count()
    }

    pub(crate) fn can_send_table(&self, now_ms: u64) -> Result<(), u64> {
        if self.table_count_24h(now_ms) < TABLE_REQUEST_LIMIT {
            return Ok(());
        }
        let retry_at = self
            .table_requests_ms
            .iter()
            .filter(|timestamp| Self::in_window(**timestamp, now_ms, DAY_MS))
            .map(|timestamp| timestamp.saturating_add(DAY_MS))
            .min()
            .unwrap_or(now_ms.saturating_add(DAY_MS));
        Err(retry_at.saturating_sub(now_ms))
    }

    pub(crate) fn can_send_own_duration(&self, now_ms: u64) -> Result<(), u64> {
        let in_hour: Vec<u64> = self
            .own_duration_requests_ms
            .iter()
            .copied()
            .filter(|timestamp| Self::in_window(*timestamp, now_ms, HOUR_MS))
            .collect();
        let in_day: Vec<u64> = self
            .own_duration_requests_ms
            .iter()
            .copied()
            .filter(|timestamp| Self::in_window(*timestamp, now_ms, DAY_MS))
            .collect();
        let hour_wait = if in_hour.len() >= DURATION_REQUESTS_PER_HOUR {
            in_hour
                .iter()
                .map(|timestamp| timestamp.saturating_add(HOUR_MS))
                .min()
                .unwrap_or(now_ms.saturating_add(HOUR_MS))
                .saturating_sub(now_ms)
        } else {
            0
        };
        let day_wait = if in_day.len() >= DURATION_REQUESTS_PER_DAY {
            in_day
                .iter()
                .map(|timestamp| timestamp.saturating_add(DAY_MS))
                .min()
                .unwrap_or(now_ms.saturating_add(DAY_MS))
                .saturating_sub(now_ms)
        } else {
            0
        };
        let wait = hour_wait.max(day_wait);
        (wait == 0).then_some(()).ok_or(wait)
    }

    pub(crate) fn record_table(&mut self, now_ms: u64) {
        self.prune(now_ms);
        self.table_requests_ms.push(now_ms);
    }

    pub(crate) fn record_own_duration(&mut self, now_ms: u64) {
        self.prune(now_ms);
        self.own_duration_requests_ms.push(now_ms);
    }

    pub(crate) fn set_cache_delete_failed(&mut self, failed: bool) {
        self.cache_delete_failed = failed;
    }

    pub(crate) fn set_revocation_pending(&mut self, pending: bool) {
        self.revocation_pending = pending;
    }
}

pub(crate) struct LedgerStore {
    path: PathBuf,
}

impl LedgerStore {
    pub(crate) fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(LEDGER_FILE),
        }
    }

    pub(crate) fn load(&self, now_ms: u64) -> RequestLedger {
        let ledger = match std::fs::read(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => RequestLedger::default(),
            Err(_) => saturated_ledger(now_ms),
            Ok(bytes) => serde_json::from_slice::<StoredLedger>(&bytes)
                .ok()
                .filter(|stored| stored.schema_version == LEDGER_SCHEMA)
                .map(RequestLedger::from)
                .unwrap_or_else(|| saturated_ledger(now_ms)),
        };
        let mut ledger = ledger;
        ledger.prune(now_ms);
        ledger
    }

    pub(crate) fn save(&self, ledger: &RequestLedger) -> Result<(), String> {
        let bytes = serde_json::to_vec(&StoredLedger::from(ledger))
            .map_err(|_| "DRAFT_STATS_LEDGER_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "DRAFT_STATS_LEDGER_WRITE_FAILED".to_owned())
    }
}

fn saturated_ledger(now_ms: u64) -> RequestLedger {
    RequestLedger {
        table_requests_ms: vec![now_ms; TABLE_REQUEST_LIMIT],
        own_duration_requests_ms: vec![now_ms; DURATION_REQUESTS_PER_DAY],
        cache_delete_failed: false,
        revocation_pending: false,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LedgerStore, RequestLedger, DAY_MS, DURATION_REQUESTS_PER_DAY, DURATION_REQUESTS_PER_HOUR,
        HOUR_MS, TABLE_REQUEST_LIMIT,
    };

    #[test]
    fn table_request_budget_is_four_across_a_restart_and_expires_at_24_hours() {
        let dir = std::env::temp_dir().join(format!("kohbor-draft-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = LedgerStore::in_data_dir(&dir);
        let now = 100_000_000;
        let mut ledger = RequestLedger::default();
        for offset in 0..TABLE_REQUEST_LIMIT {
            ledger.record_table(now + offset as u64);
        }
        store.save(&ledger).unwrap();
        let restarted = store.load(now + 4);
        assert_eq!(restarted.table_count_24h(now + 4), 4);
        assert_eq!(restarted.can_send_table(now + 4), Err(DAY_MS - 4));
        assert!(restarted.can_send_table(now + DAY_MS + 3).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn own_duration_budget_enforces_thirty_per_hour_and_two_hundred_per_day() {
        let now = 500_000_000;
        let mut hourly = RequestLedger::default();
        for index in 0..DURATION_REQUESTS_PER_HOUR {
            hourly.record_own_duration(now + index as u64);
        }
        assert!(hourly.can_send_own_duration(now + 100).is_err());
        assert!(hourly.can_send_own_duration(now + HOUR_MS + 29).is_ok());

        let mut daily = RequestLedger::default();
        for index in 0..DURATION_REQUESTS_PER_DAY {
            daily.record_own_duration(now + index as u64);
        }
        assert!(daily.can_send_own_duration(now + DAY_MS - 1).is_err());
        assert!(daily.can_send_own_duration(now + DAY_MS + 199).is_ok());
    }

    #[test]
    fn corrupt_ledger_fails_closed_for_both_request_budgets() {
        let dir = std::env::temp_dir().join(format!(
            "kohbor-draft-ledger-invalid-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("draft-stats-request-ledger.json"), b"{ truncated").unwrap();
        let ledger = LedgerStore::in_data_dir(&dir).load(1000);
        assert!(ledger.can_send_table(1000).is_err());
        assert!(ledger.can_send_own_duration(1000).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
