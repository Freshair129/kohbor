use super::parse::{
    valid_duration_table, valid_hero_stats_table, valid_lane_role_table, DurationTable,
    HeroStatsTable, LaneRoleTable,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(crate) const CACHE_FILE: &str = "hero-stats-cache.json";
pub(crate) const FRESH_MS: u64 = 24 * 60 * 60 * 1000;
pub(crate) const DISCARD_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const CACHE_SCHEMA: u8 = 1;
const MAX_OWN_HERO_ENTRIES: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CacheEntry<T> {
    pub(crate) fetched_at_ms: u64,
    pub(crate) value: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DurationEntry {
    pub(crate) hero_id: u32,
    pub(crate) fetched_at_ms: u64,
    pub(crate) last_used_at_ms: u64,
    pub(crate) value: DurationTable,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StatsCache {
    pub(crate) hero_stats: Option<CacheEntry<HeroStatsTable>>,
    pub(crate) lane_roles: [Option<CacheEntry<LaneRoleTable>>; 3],
    pub(crate) own_durations: Vec<DurationEntry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Freshness {
    Fresh,
    Stale,
    Expired,
}

pub(crate) fn freshness(fetched_at_ms: u64, now_ms: u64) -> Freshness {
    let Some(age) = now_ms.checked_sub(fetched_at_ms) else {
        return Freshness::Expired;
    };
    if age < FRESH_MS {
        Freshness::Fresh
    } else if age < DISCARD_MS {
        Freshness::Stale
    } else {
        Freshness::Expired
    }
}

impl StatsCache {
    fn valid(&self) -> bool {
        let mut duration_hero_ids = HashSet::new();
        self.hero_stats
            .as_ref()
            .is_none_or(|entry| valid_hero_stats_table(&entry.value))
            && self.lane_roles.iter().all(|entry| {
                entry
                    .as_ref()
                    .is_none_or(|entry| valid_lane_role_table(&entry.value))
            })
            && self.own_durations.len() <= MAX_OWN_HERO_ENTRIES
            && self.own_durations.iter().all(|entry| {
                entry.hero_id > 0
                    && valid_duration_table(&entry.value)
                    && duration_hero_ids.insert(entry.hero_id)
            })
    }

    pub(crate) fn prune_expired(&mut self, now_ms: u64) {
        if self
            .hero_stats
            .as_ref()
            .is_some_and(|entry| freshness(entry.fetched_at_ms, now_ms) == Freshness::Expired)
        {
            self.hero_stats = None;
        }
        for entry in &mut self.lane_roles {
            if entry
                .as_ref()
                .is_some_and(|entry| freshness(entry.fetched_at_ms, now_ms) == Freshness::Expired)
            {
                *entry = None;
            }
        }
        self.own_durations
            .retain(|entry| freshness(entry.fetched_at_ms, now_ms) != Freshness::Expired);
        self.own_durations
            .sort_by_key(|entry| entry.last_used_at_ms);
        if self.own_durations.len() > MAX_OWN_HERO_ENTRIES {
            let remove = self.own_durations.len() - MAX_OWN_HERO_ENTRIES;
            self.own_durations.drain(0..remove);
        }
    }

    pub(crate) fn insert_duration(&mut self, entry: DurationEntry) {
        self.own_durations
            .retain(|old| old.hero_id != entry.hero_id);
        self.own_durations.push(entry);
        self.own_durations
            .sort_by_key(|entry| entry.last_used_at_ms);
        if self.own_durations.len() > MAX_OWN_HERO_ENTRIES {
            let remove = self.own_durations.len() - MAX_OWN_HERO_ENTRIES;
            self.own_durations.drain(0..remove);
        }
    }

    pub(crate) fn duration_for(&self, hero_id: u32) -> Option<&DurationEntry> {
        self.own_durations
            .iter()
            .find(|entry| entry.hero_id == hero_id)
    }

    pub(crate) fn touch_duration(&mut self, hero_id: u32, now_ms: u64) {
        if let Some(entry) = self
            .own_durations
            .iter_mut()
            .find(|entry| entry.hero_id == hero_id)
        {
            entry.last_used_at_ms = now_ms;
        }
        self.own_durations
            .sort_by_key(|entry| entry.last_used_at_ms);
    }

    pub(crate) fn public_tables_fresh(&self, now_ms: u64) -> bool {
        self.hero_stats
            .as_ref()
            .is_some_and(|entry| freshness(entry.fetched_at_ms, now_ms) == Freshness::Fresh)
            && self.lane_roles.iter().all(|entry| {
                entry
                    .as_ref()
                    .is_some_and(|entry| freshness(entry.fetched_at_ms, now_ms) == Freshness::Fresh)
            })
    }

    pub(crate) fn public_tables_usable(&self, now_ms: u64) -> bool {
        self.hero_stats
            .as_ref()
            .is_some_and(|entry| freshness(entry.fetched_at_ms, now_ms) != Freshness::Expired)
            && self.lane_roles.iter().all(|entry| {
                entry.as_ref().is_some_and(|entry| {
                    freshness(entry.fetched_at_ms, now_ms) != Freshness::Expired
                })
            })
    }

    pub(crate) fn public_tables_fetched_at(&self) -> Option<u64> {
        let hero = self.hero_stats.as_ref()?.fetched_at_ms;
        self.lane_roles
            .iter()
            .map(|entry| entry.as_ref().map(|entry| entry.fetched_at_ms))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .chain(std::iter::once(hero))
            .min()
    }
}

pub(crate) struct CacheStore {
    path: PathBuf,
}

impl CacheStore {
    pub(crate) fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(CACHE_FILE),
        }
    }

    pub(crate) fn load(&self, now_ms: u64) -> StatsCache {
        let mut cache = std::fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<StoredCache>(&bytes).ok())
            .filter(|stored| stored.schema_version == CACHE_SCHEMA)
            .map(|stored| stored.cache)
            .filter(StatsCache::valid)
            .unwrap_or_default();
        cache.prune_expired(now_ms);
        cache
    }

    pub(crate) fn save(&self, cache: &StatsCache) -> Result<(), String> {
        let bytes = serde_json::to_vec(&StoredCache {
            schema_version: CACHE_SCHEMA,
            cache: cache.clone(),
        })
        .map_err(|_| "DRAFT_STATS_CACHE_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "DRAFT_STATS_CACHE_WRITE_FAILED".to_owned())
    }

    pub(crate) fn delete(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("DRAFT_STATS_CACHE_DELETE_FAILED".to_owned()),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredCache {
    schema_version: u8,
    cache: StatsCache,
}

#[cfg(test)]
mod tests {
    use super::{
        freshness, CacheEntry, CacheStore, Freshness, StatsCache, StoredCache, CACHE_SCHEMA,
        DISCARD_MS, FRESH_MS,
    };
    use crate::hero_stats::parse::{Counts, HeroStatsRow};
    use std::collections::BTreeMap;

    #[test]
    fn freshness_has_exact_24_hour_and_seven_day_boundaries() {
        let now = 9_000_000_000;
        assert_eq!(freshness(now, now), Freshness::Fresh);
        assert_eq!(freshness(now - FRESH_MS + 1, now), Freshness::Fresh);
        assert_eq!(freshness(now - FRESH_MS, now), Freshness::Stale);
        assert_eq!(freshness(now - DISCARD_MS + 1, now), Freshness::Stale);
        assert_eq!(freshness(now - DISCARD_MS, now), Freshness::Expired);
        assert_eq!(freshness(now + 1, now), Freshness::Expired);
    }

    #[test]
    fn cache_retains_only_twenty_duration_entries_and_keeps_recent_heroes() {
        let mut cache = StatsCache::default();
        for hero_id in 1..=21 {
            cache.insert_duration(super::DurationEntry {
                hero_id,
                fetched_at_ms: hero_id as u64,
                last_used_at_ms: hero_id as u64,
                value: Default::default(),
            });
        }
        assert_eq!(cache.own_durations.len(), 20);
        assert!(cache.duration_for(1).is_none());
        assert!(cache.duration_for(21).is_some());
    }

    #[test]
    fn cache_with_missing_bracket_key_is_discarded_as_a_whole() {
        let dir =
            std::env::temp_dir().join(format!("kohbor-draft-cache-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut brackets = (1..=8)
            .map(|bracket| (bracket.to_string(), Counts { games: 10, wins: 5 }))
            .chain(std::iter::once((
                "pub".to_owned(),
                Counts { games: 10, wins: 5 },
            )))
            .collect::<BTreeMap<_, _>>();
        brackets.remove("8");
        let cache = StatsCache {
            hero_stats: Some(CacheEntry {
                fetched_at_ms: 100,
                value: BTreeMap::from([(
                    1,
                    HeroStatsRow {
                        id: 1,
                        name: "npc_dota_hero_test".to_owned(),
                        localized_name: "Test Hero".to_owned(),
                        roles: vec!["Carry".to_owned()],
                        brackets,
                    },
                )]),
            }),
            ..Default::default()
        };
        let bytes = serde_json::to_vec(&StoredCache {
            schema_version: CACHE_SCHEMA,
            cache,
        })
        .unwrap();
        std::fs::write(dir.join(super::CACHE_FILE), bytes).unwrap();

        assert_eq!(
            CacheStore::in_data_dir(&dir).load(100),
            StatsCache::default()
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
