use super::context::{HISTORY_ESTIMATED_BYTES_PER_TOKEN, HISTORY_TOKEN_BUDGET};
use crate::{atomic, secrets};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

const PREFS_SCHEMA: u32 = 2;
pub(crate) const CONSENT_TEXT_VERSION: u32 = 2;
const MAX_CHAT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LEDGER_ROWS: usize = 200;
const SYSTEM_NOTE: &str = "สรุปแมตช์.md";
const MAX_REPORT_BYTES: u64 = 128 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalyticsPreferences {
    pub(crate) ai_on: bool,
    pub(crate) provider: String,
    pub(crate) consent: bool,
    pub(crate) model: String,
    pub(crate) provider_endpoints: std::collections::BTreeMap<String, String>,
    pub(crate) provider_consents: std::collections::BTreeMap<String, ProviderConsent>,
    pub(crate) verified_providers: std::collections::BTreeMap<String, String>,
    pub(crate) tools_modes: std::collections::BTreeMap<String, String>,
    pub(crate) show_untested: bool,
    pub(crate) keep_newest: usize,
    pub(crate) max_bytes: u64,
    pub(crate) auto_analyse: bool,
}

impl Default for AnalyticsPreferences {
    fn default() -> Self {
        Self {
            ai_on: false,
            provider: "anthropic".into(),
            consent: false,
            model: "claude-opus-5-5".into(),
            provider_endpoints: std::collections::BTreeMap::new(),
            provider_consents: std::collections::BTreeMap::new(),
            verified_providers: std::collections::BTreeMap::new(),
            tools_modes: std::collections::BTreeMap::new(),
            show_untested: false,
            keep_newest: 20,
            max_bytes: 2 * 1024 * 1024 * 1024,
            auto_analyse: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderConsent {
    pub(crate) origin: String,
    pub(crate) text_version: u32,
    pub(crate) granted_at_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreferencesFile {
    schema_version: u32,
    pub(crate) settings: AnalyticsPreferences,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAnalyticsPreferences {
    ai_on: bool,
    consent: bool,
    model: String,
    keep_newest: usize,
    max_bytes: u64,
    auto_analyse: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPreferencesFile {
    schema_version: u32,
    settings: LegacyAnalyticsPreferences,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ProviderKeyRecord {
    origin: String,
    key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LedgerRow {
    pub(crate) time_unix_ms: u64,
    #[serde(default = "default_anthropic_provider")]
    pub(crate) provider: String,
    #[serde(default = "default_cloud_endpoint_class")]
    pub(crate) endpoint_class: String,
    pub(crate) model: String,
    pub(crate) bytes_sent: u64,
    pub(crate) bytes_received: u64,
    pub(crate) input_tokens: Option<u64>,
    pub(crate) cache_read_tokens: Option<u64>,
    pub(crate) cache_creation_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) status: String,
}

fn default_anthropic_provider() -> String {
    "anthropic".into()
}
fn default_cloud_endpoint_class() -> String {
    "cloud".into()
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NoteRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) ai: bool,
    pub(crate) bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeMeta {
    pub(crate) bundle_hash: String,
    pub(crate) knowledge_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) own_hero_id: Option<String>,
    pub(crate) created_unix_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AiReportMeta {
    pub(crate) cache_key: String,
    pub(crate) created_unix_ms: u64,
    pub(crate) model: String,
    pub(crate) prompt_version: u32,
    pub(crate) knowledge_version: u32,
    pub(crate) bundle_hash: String,
    pub(crate) own_hero_id: Option<String>,
    pub(crate) usage: Value,
    pub(crate) tool_calls: u32,
    pub(crate) cost_estimate_usd: Option<f64>,
    pub(crate) cost_estimate_reason: Option<String>,
    pub(crate) stale: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct AiReportSummary {
    pub(crate) id: String,
    pub(crate) meta: AiReportMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct StoredAiReport {
    pub(crate) id: String,
    pub(crate) markdown: String,
    pub(crate) meta: AiReportMeta,
}

#[derive(Clone)]
pub(crate) struct AnalyticsStore {
    root: PathBuf,
    install_key: Option<[u8; 32]>,
    write_gate: Arc<Mutex<()>>,
}

impl AnalyticsStore {
    pub(crate) fn open(data_dir: &Path, install_key: Option<[u8; 32]>) -> Self {
        Self {
            root: data_dir.join("analytics"),
            install_key,
            write_gate: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    fn ensure_root(&self) -> Result<(), &'static str> {
        fs::create_dir_all(&self.root).map_err(|_| "storage_unavailable")
    }

    pub(crate) fn key_status(&self) -> Result<Value, &'static str> {
        let settings = self.preferences()?;
        self.key_status_for(&settings.provider)
    }

    pub(crate) fn read_key(&self) -> Result<Option<Vec<u8>>, &'static str> {
        let settings = self.preferences()?;
        self.read_provider_key(&settings.provider)
    }

    pub(crate) fn set_key(&self, key: &str) -> Result<(), &'static str> {
        let settings = self.preferences()?;
        self.set_provider_key(&settings.provider, key)
    }

    pub(crate) fn key_status_for(&self, provider: &str) -> Result<Value, &'static str> {
        let key = self.read_provider_key(provider)?;
        let last4 = key
            .as_ref()
            .filter(|bytes| bytes.len() >= 16)
            .and_then(|bytes| bytes.get(bytes.len().saturating_sub(4)..))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_owned);
        Ok(json!({"provider":provider,"set":key.is_some(),"last4":last4}))
    }

    pub(crate) fn read_provider_key(
        &self,
        provider: &str,
    ) -> Result<Option<Vec<u8>>, &'static str> {
        self.ensure_root()?;
        let settings = self.preferences()?;
        let origin = self.provider_endpoint(provider, &settings)?.origin();
        let path = self.provider_key_path(provider)?;
        let Some(bytes) = secrets::load_provider_key(&path) else {
            return Ok(None);
        };
        let record = serde_json::from_slice::<ProviderKeyRecord>(&bytes)
            .map_err(|_| "key_record_invalid")?;
        if record.origin != origin {
            return Err("key_origin_mismatch");
        }
        Ok(Some(record.key.into_bytes()))
    }

    pub(crate) fn set_provider_key(&self, provider: &str, key: &str) -> Result<(), &'static str> {
        if !(16..=512).contains(&key.len())
            || !key.is_ascii()
            || key
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        {
            return Err("key_invalid");
        }
        self.ensure_root()?;
        let settings = self.preferences()?;
        if let Some(spec) = super::providers::registry::spec(provider) {
            if spec.auth_header.is_none() {
                return Err("provider_key_not_supported");
            }
        } else if !provider.starts_with("custom-") {
            return Err("provider_not_allowed");
        }
        let endpoint = self.provider_endpoint(provider, &settings)?;
        let record = ProviderKeyRecord {
            origin: endpoint.origin(),
            key: key.to_owned(),
        };
        let bytes = serde_json::to_vec(&record).map_err(|_| "key_storage_unavailable")?;
        let path = self.provider_key_path(provider)?;
        fs::create_dir_all(path.parent().ok_or("key_storage_unavailable")?)
            .map_err(|_| "key_storage_unavailable")?;
        secrets::store_provider_key(&path, &bytes).ok_or("key_storage_unavailable")
    }

    pub(crate) fn delete_key(&self) -> Result<(), &'static str> {
        let settings = self.preferences()?;
        self.delete_provider_key(&settings.provider)
    }

    pub(crate) fn delete_provider_key(&self, provider: &str) -> Result<(), &'static str> {
        self.ensure_root()?;
        match fs::remove_file(self.provider_key_path(provider)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("key_storage_unavailable"),
        }
    }

    fn provider_key_path(&self, provider: &str) -> Result<PathBuf, &'static str> {
        if !valid_provider_id(provider) {
            return Err("provider_not_allowed");
        }
        Ok(self.root.join("keys").join(format!("{provider}.dpapi")))
    }

    pub(crate) fn provider_endpoint(
        &self,
        provider: &str,
        settings: &AnalyticsPreferences,
    ) -> Result<super::providers::endpoint::Endpoint, &'static str> {
        if !valid_provider_id(provider) {
            return Err("provider_not_allowed");
        }
        if let Some(spec) = super::providers::registry::spec(provider) {
            match spec.kind {
                super::providers::registry::EndpointKind::Fixed => {
                    super::providers::registry::fixed_endpoint(provider, None)
                }
                super::providers::registry::EndpointKind::Choice => {
                    super::providers::registry::fixed_endpoint(
                        provider,
                        settings
                            .provider_endpoints
                            .get(provider)
                            .map(String::as_str),
                    )
                }
                super::providers::registry::EndpointKind::User => {
                    let raw = settings
                        .provider_endpoints
                        .get(provider)
                        .map(String::as_str)
                        .unwrap_or(spec.default_endpoint);
                    super::providers::endpoint::Endpoint::parse(raw)
                }
            }
        } else {
            let raw = settings
                .provider_endpoints
                .get(provider)
                .ok_or("endpoint_required")?;
            super::providers::endpoint::Endpoint::parse(raw)
        }
    }

    pub(crate) fn set_provider_endpoint(
        &self,
        provider: &str,
        raw: &str,
    ) -> Result<AnalyticsPreferences, &'static str> {
        if !valid_provider_id(provider) {
            return Err("provider_not_allowed");
        }
        let next_endpoint = if let Some(spec) = super::providers::registry::spec(provider) {
            match spec.kind {
                super::providers::registry::EndpointKind::Fixed => return Err("endpoint_fixed"),
                super::providers::registry::EndpointKind::Choice => {
                    super::providers::registry::fixed_endpoint(provider, Some(raw))?
                }
                super::providers::registry::EndpointKind::User => {
                    super::providers::endpoint::Endpoint::parse(raw)?
                }
            }
        } else if provider.starts_with("custom-") {
            super::providers::endpoint::Endpoint::parse(raw)?
        } else {
            return Err("provider_not_allowed");
        };
        let mut settings = self.preferences()?;
        let prior_endpoint = self.provider_endpoint(provider, &settings).ok();
        settings
            .provider_endpoints
            .insert(provider.to_owned(), raw.to_owned());
        if prior_endpoint.as_ref() != Some(&next_endpoint) {
            self.delete_provider_key(provider)?;
            settings.provider_consents.remove(provider);
            settings.verified_providers.remove(provider);
            if settings.provider == provider {
                settings.ai_on = false;
                settings.consent = false;
            }
        }
        self.write_preferences(&settings)?;
        Ok(settings)
    }

    pub(crate) fn register_custom_endpoint(
        &self,
        raw: &str,
    ) -> Result<(String, AnalyticsPreferences), &'static str> {
        super::providers::endpoint::Endpoint::parse(raw)?;
        let token = secrets::random::<4>().ok_or("provider_id_unavailable")?;
        let id = format!(
            "custom-{}",
            token
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let mut settings = self.preferences()?;
        settings
            .provider_endpoints
            .insert(id.clone(), raw.to_owned());
        settings.provider = id.clone();
        settings.model = String::new();
        settings.ai_on = false;
        settings.consent = false;
        self.write_preferences(&settings)?;
        Ok((id, settings))
    }

    pub(crate) fn set_provider_consent(
        &self,
        provider: &str,
        granted: bool,
    ) -> Result<AnalyticsPreferences, &'static str> {
        let mut settings = self.preferences()?;
        let origin = self.provider_endpoint(provider, &settings)?.origin();
        if granted {
            settings.provider_consents.insert(
                provider.to_owned(),
                ProviderConsent {
                    origin,
                    text_version: CONSENT_TEXT_VERSION,
                    granted_at_unix_ms: now_ms(),
                },
            );
        } else {
            settings.provider_consents.remove(provider);
            if settings.provider == provider {
                settings.ai_on = false;
            }
        }
        settings.consent = current_consent_valid(
            &settings,
            &settings.provider,
            &self
                .provider_endpoint(&settings.provider, &settings)
                .ok()
                .map(|endpoint| endpoint.origin())
                .unwrap_or_default(),
        );
        self.write_preferences(&settings)?;
        Ok(settings)
    }

    pub(crate) fn set_provider_verified(
        &self,
        provider: &str,
        evidence_sha256: &str,
    ) -> Result<AnalyticsPreferences, &'static str> {
        if !valid_provider_id(provider)
            || evidence_sha256.len() != 64
            || !evidence_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("verification_receipt_invalid");
        }
        let mut settings = self.preferences()?;
        settings
            .verified_providers
            .insert(provider.to_owned(), evidence_sha256.to_ascii_lowercase());
        self.write_preferences(&settings)?;
        Ok(settings)
    }

    pub(crate) fn preferences(&self) -> Result<AnalyticsPreferences, &'static str> {
        self.ensure_root()?;
        let path = self.root.join("settings.json");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let defaults = AnalyticsPreferences::default();
                self.write_preferences(&defaults)?;
                return Ok(defaults);
            }
            Err(_) => return Err("storage_unavailable"),
        };
        let header: Value = serde_json::from_slice(&bytes).map_err(|_| "settings_invalid")?;
        let version = header
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or("settings_invalid")?;
        match version {
            1 => {
                let legacy: LegacyPreferencesFile =
                    serde_json::from_slice(&bytes).map_err(|_| "settings_invalid")?;
                if legacy.schema_version != 1 {
                    return Err("settings_version_unsupported");
                }
                self.migrate_legacy_preferences(legacy.settings)
            }
            value if value == u64::from(PREFS_SCHEMA) => {
                let file: PreferencesFile =
                    serde_json::from_slice(&bytes).map_err(|_| "settings_invalid")?;
                let mut settings = file.settings;
                settings.consent = self
                    .provider_endpoint(&settings.provider, &settings)
                    .ok()
                    .is_some_and(|endpoint| {
                        current_consent_valid(&settings, &settings.provider, &endpoint.origin())
                    });
                validate_preferences(&settings)?;
                Ok(settings)
            }
            _ => Err("settings_version_unsupported"),
        }
    }

    fn migrate_legacy_preferences(
        &self,
        legacy: LegacyAnalyticsPreferences,
    ) -> Result<AnalyticsPreferences, &'static str> {
        let mut settings = AnalyticsPreferences::default();
        settings.model = legacy.model;
        settings.keep_newest = legacy.keep_newest;
        settings.max_bytes = legacy.max_bytes;
        settings.auto_analyse = legacy.auto_analyse;
        let old_key_path = self.root.join("anthropic-key.dpapi");
        let key_present = old_key_path.exists();
        // Legacy consent predates the current disclosure and must be granted again.
        let _legacy_ai_enabled = legacy.ai_on;
        let _legacy_consent = legacy.consent;
        settings.ai_on = false;
        settings.consent = false;
        validate_preferences(&settings)?;
        let path = self.root.join("settings.json");
        migrate_legacy_files(&self.root, settings, &path, key_present, |path, bytes| {
            atomic::write(path, bytes).map_err(|_| "settings_migration_failed")
        })
    }

    pub(crate) fn write_preferences(
        &self,
        settings: &AnalyticsPreferences,
    ) -> Result<(), &'static str> {
        self.ensure_root()?;
        let mut settings = settings.clone();
        settings.consent = self
            .provider_endpoint(&settings.provider, &settings)
            .ok()
            .is_some_and(|endpoint| {
                current_consent_valid(&settings, &settings.provider, &endpoint.origin())
            });
        validate_preferences(&settings)?;
        let file = PreferencesFile {
            schema_version: PREFS_SCHEMA,
            settings,
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|_| "settings_write_failed")?;
        atomic::write(&self.root.join("settings.json"), &bytes).map_err(|_| "settings_write_failed")
    }

    pub(crate) fn ledger(&self) -> Result<Vec<LedgerRow>, &'static str> {
        self.ensure_root()?;
        match fs::read(self.root.join("ledger.json")) {
            Ok(bytes) if bytes.len() <= 128 * 1024 => {
                serde_json::from_slice(&bytes).map_err(|_| "ledger_invalid")
            }
            Ok(_) => Err("ledger_invalid"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err("storage_unavailable"),
        }
    }

    pub(crate) fn add_ledger(&self, row: LedgerRow) -> Result<(), &'static str> {
        let mut rows = self.ledger()?;
        rows.insert(0, row);
        rows.truncate(MAX_LEDGER_ROWS);
        let bytes = serde_json::to_vec_pretty(&rows).map_err(|_| "ledger_write_failed")?;
        atomic::write(&self.root.join("ledger.json"), &bytes).map_err(|_| "ledger_write_failed")
    }

    pub(crate) fn match_key(&self, match_id: &str) -> Result<String, &'static str> {
        let Some(key) = self.install_key.as_ref() else {
            return Err("install_key_unavailable");
        };
        let id = match_id
            .parse::<u64>()
            .ok()
            .filter(|id| *id > 0)
            .ok_or("invalid_match_id")?
            .to_string();
        Ok(hmac_match_key(key, id.as_bytes()))
    }

    fn match_dir(&self, match_id: &str) -> Result<PathBuf, &'static str> {
        Ok(self.root.join(self.match_key(match_id)?))
    }

    pub(crate) fn set_own_hero(&self, match_id: &str, hero: &str) -> Result<(), &'static str> {
        if !valid_hero(hero) {
            return Err("invalid_hero");
        }
        let directory = self.match_dir(match_id)?;
        fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        let path = directory.join("state.json");
        let mut state: Value = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|_| "state_invalid")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
            Err(_) => return Err("storage_unavailable"),
        };
        state["own_hero"] = json!(hero);
        let bytes = serde_json::to_vec_pretty(&state).map_err(|_| "storage_unavailable")?;
        atomic::write(&path, &bytes).map_err(|_| "storage_unavailable")
    }

    pub(crate) fn own_hero(&self, match_id: &str) -> Option<String> {
        let path = self.match_dir(match_id).ok()?.join("state.json");
        let bytes = fs::read(path).ok()?;
        let state: Value = serde_json::from_slice(&bytes).ok()?;
        let hero = state.get("own_hero")?.as_str()?;
        valid_hero(hero).then(|| hero.to_owned())
    }

    pub(crate) fn write_knowledge(
        &self,
        match_id: &str,
        bundle_hash: &str,
        knowledge_version: u32,
        bytes: &[u8],
    ) -> Result<(), &'static str> {
        if !valid_sha256(bundle_hash) || bytes.len() > 2 * 1024 * 1024 {
            return Err("knowledge_invalid");
        }
        let _guard = lock(&self.write_gate);
        let directory = self.match_dir(match_id)?.join("knowledge");
        fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        let stem = format!("k{knowledge_version}-{}", &bundle_hash[..12]);
        let digest_path = directory.join(format!("{stem}.json"));
        if fs::read(&digest_path).ok().as_deref() != Some(bytes) {
            atomic::write(&digest_path, bytes).map_err(|_| "knowledge_write_failed")?;
        }
        let meta = KnowledgeMeta {
            bundle_hash: bundle_hash.to_owned(),
            knowledge_version,
            own_hero_id: None,
            created_unix_ms: now_unix_ms(),
        };
        let meta_bytes = serde_json::to_vec(&meta).map_err(|_| "knowledge_write_failed")?;
        let meta_path = directory.join(format!("{stem}.meta.json"));
        if fs::read(&meta_path).ok().as_deref() != Some(meta_bytes.as_slice()) {
            atomic::write(&meta_path, &meta_bytes).map_err(|_| "knowledge_write_failed")?;
        }
        Ok(())
    }

    pub(crate) fn read_knowledge(
        &self,
        match_id: &str,
        bundle_hash: &str,
    ) -> Result<Option<(KnowledgeMeta, Value)>, &'static str> {
        if !valid_sha256(bundle_hash) {
            return Err("knowledge_invalid");
        }
        let bundle_hash12 = &bundle_hash[..12];
        let directory = self.match_dir(match_id)?.join("knowledge");
        let mut matches = fs::read_dir(&directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with('k') || !name.ends_with(&format!("-{}.json", bundle_hash12)) {
                    return None;
                }
                Some(entry.path())
            })
            .collect::<Vec<_>>();
        matches.sort();
        let Some(digest_path) = matches.into_iter().next() else {
            return Ok(None);
        };
        let stem = digest_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("knowledge_invalid")?;
        let meta_path = digest_path.with_file_name(format!("{stem}.meta.json"));
        let meta: KnowledgeMeta =
            serde_json::from_slice(&fs::read(meta_path).map_err(|_| "knowledge_invalid")?)
                .map_err(|_| "knowledge_invalid")?;
        if meta.bundle_hash != bundle_hash {
            return Ok(None);
        }
        let bytes = fs::read(digest_path).map_err(|_| "knowledge_invalid")?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("knowledge_invalid");
        }
        let digest: Value = serde_json::from_slice(&bytes).map_err(|_| "knowledge_invalid")?;
        Ok(Some((meta, digest)))
    }

    pub(crate) fn read_latest_knowledge(
        &self,
        match_id: &str,
    ) -> Result<Option<(KnowledgeMeta, Value)>, &'static str> {
        let directory = self.match_dir(match_id)?.join("knowledge");
        let mut candidates = fs::read_dir(&directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                (name.starts_with('k') && name.ends_with(".meta.json")).then_some(entry.path())
            })
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.reverse();
        for meta_path in candidates {
            let Ok(meta_bytes) = fs::read(&meta_path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_slice::<KnowledgeMeta>(&meta_bytes) else {
                continue;
            };
            if !valid_sha256(&meta.bundle_hash) {
                continue;
            }
            let digest_path = meta_path.with_file_name(format!(
                "k{}-{}.json",
                meta.knowledge_version,
                &meta.bundle_hash[..12]
            ));
            let Ok(bytes) = fs::read(digest_path) else {
                continue;
            };
            if bytes.len() > 2 * 1024 * 1024 {
                continue;
            }
            if let Ok(digest) = serde_json::from_slice::<Value>(&bytes) {
                return Ok(Some((meta, digest)));
            }
        }
        Ok(None)
    }

    pub(crate) fn write_ai_report(
        &self,
        match_id: &str,
        bundle_hash: &str,
        model: &str,
        prompt_version: u32,
        knowledge_version: u32,
        own_hero_id: Option<&str>,
        markdown: &str,
        usage: Value,
        tool_calls: u32,
    ) -> Result<StoredAiReport, &'static str> {
        if !valid_sha256(bundle_hash)
            || markdown.is_empty()
            || markdown.len() as u64 > MAX_REPORT_BYTES
        {
            return Err("report_invalid");
        }
        if super::providers::registry::validate_model_id(model).is_err() {
            return Err("report_invalid");
        }
        let _guard = lock(&self.write_gate);
        let directory = self.match_dir(match_id)?.join("reports");
        fs::create_dir_all(&directory).map_err(|_| "report_write_failed")?;
        let cache_key = format!("{bundle_hash}:{model}:p{prompt_version}:k{knowledge_version}");
        let model_slug = safe_model_slug(model);
        let base = format!(
            "ai-{}-{model_slug}-p{prompt_version}-k{knowledge_version}-{}",
            &bundle_hash[..12],
            now_unix_ms()
        );
        let id = (1_u32..=10_000)
            .map(|suffix| {
                if suffix == 1 {
                    base.clone()
                } else {
                    format!("{base}-{suffix}")
                }
            })
            .find(|id| {
                !directory.join(format!("{id}.md")).exists()
                    && !directory.join(format!("{id}.meta.json")).exists()
            })
            .ok_or("report_name_exhausted")?;
        let meta = AiReportMeta {
            cache_key,
            created_unix_ms: now_unix_ms(),
            model: model.to_owned(),
            prompt_version,
            knowledge_version,
            bundle_hash: bundle_hash.to_owned(),
            own_hero_id: own_hero_id.map(str::to_owned),
            usage,
            tool_calls,
            cost_estimate_usd: None,
            cost_estimate_reason: Some(
                "provider token pricing is not verified in this build".into(),
            ),
            stale: false,
        };
        let markdown_path = directory.join(format!("{id}.md"));
        let meta_path = directory.join(format!("{id}.meta.json"));
        atomic::write(&markdown_path, markdown.as_bytes()).map_err(|_| "report_write_failed")?;
        let meta_bytes = serde_json::to_vec_pretty(&meta).map_err(|_| "report_write_failed")?;
        atomic::write(&meta_path, &meta_bytes).map_err(|_| "report_write_failed")?;
        Ok(StoredAiReport {
            id,
            markdown: markdown.to_owned(),
            meta,
        })
    }

    pub(crate) fn list_ai_reports(
        &self,
        match_id: &str,
    ) -> Result<Vec<AiReportSummary>, &'static str> {
        let directory = self.match_dir(match_id)?.join("reports");
        let mut rows = fs::read_dir(&directory)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let id = name.strip_suffix(".meta.json")?;
                if !valid_report_id(id) || !directory.join(format!("{id}.md")).is_file() {
                    return None;
                }
                let meta =
                    serde_json::from_slice::<AiReportMeta>(&fs::read(entry.path()).ok()?).ok()?;
                Some(AiReportSummary {
                    id: id.to_owned(),
                    meta,
                })
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .meta
                .created_unix_ms
                .cmp(&left.meta.created_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(rows)
    }

    pub(crate) fn read_ai_report(
        &self,
        match_id: &str,
        id: &str,
    ) -> Result<StoredAiReport, &'static str> {
        if !valid_report_id(id) {
            return Err("report_not_found");
        }
        let directory = self.match_dir(match_id)?.join("reports");
        let markdown_path = directory.join(format!("{id}.md"));
        let meta_path = directory.join(format!("{id}.meta.json"));
        let metadata = fs::metadata(&markdown_path).map_err(|_| "report_not_found")?;
        if !metadata.is_file() || metadata.len() > MAX_REPORT_BYTES {
            return Err("report_invalid");
        }
        let markdown = String::from_utf8(fs::read(markdown_path).map_err(|_| "report_not_found")?)
            .map_err(|_| "report_invalid")?;
        let meta_bytes = fs::read(meta_path).map_err(|_| "report_not_found")?;
        if meta_bytes.len() > 16 * 1024 {
            return Err("report_invalid");
        }
        let meta: AiReportMeta =
            serde_json::from_slice(&meta_bytes).map_err(|_| "report_invalid")?;
        if !valid_sha256(&meta.bundle_hash)
            || meta
                .cost_estimate_usd
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err("report_invalid");
        }
        Ok(StoredAiReport {
            id: id.to_owned(),
            markdown,
            meta,
        })
    }

    pub(crate) fn append_chat(
        &self,
        match_id: &str,
        provider: &str,
        model: &str,
        message: Value,
    ) -> Result<Vec<Value>, &'static str> {
        if !valid_provider_id(provider)
            || model.is_empty()
            || super::providers::registry::validate_model_id(model).is_err()
        {
            return Err("chat_invalid");
        }
        let _guard = lock(&self.write_gate);
        let directory = self.match_dir(match_id)?;
        fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        let path = directory.join("chat.json");
        let mut messages: Vec<Value> = match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_CHAT_BYTES => return Err("chat_too_large"),
            Ok(_) => serde_json::from_slice(&fs::read(&path).map_err(|_| "storage_unavailable")?)
                .map_err(|_| "chat_invalid")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err("storage_unavailable"),
        };
        messages.push(json!({"provider":provider,"model":model,"message":message}));
        let bytes = serde_json::to_vec(&messages).map_err(|_| "storage_unavailable")?;
        if bytes.len() as u64 > MAX_CHAT_BYTES {
            return Err("chat_too_large");
        }
        atomic::write(&path, &bytes).map_err(|_| "storage_unavailable")?;
        Ok(messages)
    }

    pub(crate) fn chat(&self, match_id: &str) -> Result<Vec<Value>, &'static str> {
        let path = self.match_dir(match_id)?.join("chat.json");
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() > MAX_CHAT_BYTES => Err("chat_too_large"),
            Ok(_) => serde_json::from_slice(&fs::read(path).map_err(|_| "storage_unavailable")?)
                .map_err(|_| "chat_invalid"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err("storage_unavailable"),
        }
    }

    pub(crate) fn chat_transcript(&self, match_id: &str) -> Result<Vec<Value>, &'static str> {
        Ok(self
            .chat(match_id)?
            .iter()
            .filter_map(transcript_turn)
            .collect())
    }

    pub(crate) fn start_new_chat_from_summary(
        &self,
        match_id: &str,
        summary: &str,
    ) -> Result<String, &'static str> {
        if summary.trim().is_empty() || summary.len() > 64 * 1024 {
            return Err("note_invalid");
        }
        let _guard = lock(&self.write_gate);
        let history = self.chat(match_id)?;
        let history_bytes = serde_json::to_vec(&history).map_err(|_| "chat_invalid")?;
        if history_bytes.len() / HISTORY_ESTIMATED_BYTES_PER_TOKEN <= HISTORY_TOKEN_BUDGET {
            return Err("history_budget_not_exceeded");
        }
        let note_id = self.write_note_unlocked(match_id, "Chat recap", summary)?;
        let chat_path = self.match_dir(match_id)?.join("chat.json");
        atomic::write(&chat_path, b"[]").map_err(|_| "chat_reset_failed")?;
        Ok(note_id)
    }

    pub(crate) fn list_notes(
        &self,
        match_id: &str,
        system_markdown: &str,
    ) -> Result<Vec<NoteRow>, &'static str> {
        let _guard = lock(&self.write_gate);
        let directory = self.match_dir(match_id)?.join("notes");
        fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        atomic::write(&directory.join(SYSTEM_NOTE), system_markdown.as_bytes())
            .map_err(|_| "note_write_failed")?;
        let mut rows = vec![NoteRow {
            id: SYSTEM_NOTE.into(),
            title: SYSTEM_NOTE.into(),
            ai: false,
            bytes: fs::metadata(directory.join(SYSTEM_NOTE))
                .map_err(|_| "storage_unavailable")?
                .len(),
        }];
        for item in fs::read_dir(&directory).map_err(|_| "storage_unavailable")? {
            let item = item.map_err(|_| "storage_unavailable")?;
            let path = item.path();
            if !path.is_file()
                || path.file_name().and_then(|name| name.to_str()) == Some(SYSTEM_NOTE)
                || path.extension().and_then(|ext| ext.to_str()) != Some("md")
            {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            rows.push(NoteRow {
                id: name.to_owned(),
                title: name.to_owned(),
                ai: true,
                bytes: fs::metadata(&path)
                    .map_err(|_| "storage_unavailable")?
                    .len(),
            });
        }
        rows[1..].sort_by(|left, right| left.title.cmp(&right.title));
        Ok(rows)
    }

    pub(crate) fn read_note(&self, match_id: &str, note_id: &str) -> Result<String, &'static str> {
        if note_id != SYSTEM_NOTE && !valid_note_id(note_id) {
            return Err("note_not_found");
        }
        let path = self.match_dir(match_id)?.join("notes").join(note_id);
        let metadata = fs::metadata(&path).map_err(|_| "note_not_found")?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 {
            return Err("note_invalid");
        }
        String::from_utf8(fs::read(path).map_err(|_| "note_not_found")?).map_err(|_| "note_invalid")
    }

    pub(crate) fn write_note(
        &self,
        match_id: &str,
        title: &str,
        markdown: &str,
    ) -> Result<String, &'static str> {
        let _guard = lock(&self.write_gate);
        self.write_note_unlocked(match_id, title, markdown)
    }

    fn write_note_unlocked(
        &self,
        match_id: &str,
        title: &str,
        markdown: &str,
    ) -> Result<String, &'static str> {
        if markdown.len() > 64 * 1024 {
            return Err("note_too_large");
        }
        let base = safe_note_stem(title)?;
        let directory = self.match_dir(match_id)?.join("notes");
        fs::create_dir_all(&directory).map_err(|_| "note_write_failed")?;
        for suffix in 1_u32..=10_000 {
            let file = if suffix == 1 {
                format!("{base}.md")
            } else {
                format!("{base} ({suffix}).md")
            };
            if file == SYSTEM_NOTE {
                continue;
            }
            let path = directory.join(&file);
            if path.exists() {
                continue;
            }
            atomic::write(&path, markdown.as_bytes()).map_err(|_| "note_write_failed")?;
            return Ok(file);
        }
        Err("note_name_exhausted")
    }

    pub(crate) fn delete_note(&self, match_id: &str, note_id: &str) -> Result<(), &'static str> {
        let _guard = lock(&self.write_gate);
        if note_id == SYSTEM_NOTE || !valid_note_id(note_id) {
            return Err("system_note_read_only");
        }
        let path = self.match_dir(match_id)?.join("notes").join(note_id);
        fs::remove_file(path).map_err(|_| "note_not_found")
    }

    pub(crate) fn open_note_folder(&self, match_id: &str) -> Result<PathBuf, &'static str> {
        let directory = self.match_dir(match_id)?.join("notes");
        fs::create_dir_all(&directory).map_err(|_| "storage_unavailable")?;
        Ok(directory)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_model_slug(model: &str) -> String {
    let slug = model
        .chars()
        .take(48)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned();
    if slug.is_empty() {
        "model".into()
    } else {
        slug
    }
}

fn valid_report_id(id: &str) -> bool {
    (8..=180).contains(&id.len())
        && id.starts_with("ai-")
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn now_unix_ms() -> u64 {
    now_ms()
}

fn validate_preferences(settings: &AnalyticsPreferences) -> Result<(), &'static str> {
    if !valid_provider_id(&settings.provider) {
        return Err("provider_not_allowed");
    }
    if settings.provider == "anthropic" {
        if !matches!(
            settings.model.as_str(),
            "claude-opus-5-5" | "claude-sonnet-5-5" | "claude-haiku-5-5"
        ) {
            return Err("model_not_allowed");
        }
    } else if !settings.model.is_empty() {
        super::providers::registry::validate_model_id(&settings.model)?;
    }
    for (provider, raw) in &settings.provider_endpoints {
        if !valid_provider_id(provider) {
            return Err("provider_not_allowed");
        }
        let spec = super::providers::registry::spec(provider);
        let parsed = if spec
            .is_some_and(|spec| spec.kind == super::providers::registry::EndpointKind::Choice)
        {
            super::providers::registry::fixed_endpoint(provider, Some(raw))?
        } else {
            if spec.is_some_and(|spec| spec.kind == super::providers::registry::EndpointKind::Fixed)
            {
                return Err("settings_invalid");
            }
            super::providers::endpoint::Endpoint::parse(raw)?
        };
        if provider.starts_with("custom-") && parsed.origin().is_empty() {
            return Err("settings_invalid");
        }
    }
    for (provider, record) in &settings.provider_consents {
        if !valid_provider_id(provider)
            || record.origin.is_empty()
            || record.text_version == 0
            || record.text_version > CONSENT_TEXT_VERSION
        {
            return Err("settings_invalid");
        }
    }
    for (provider, receipt) in &settings.verified_providers {
        if !valid_provider_id(provider)
            || receipt.len() != 64
            || !receipt.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("settings_invalid");
        }
    }
    for (provider, mode) in &settings.tools_modes {
        if !valid_provider_id(provider)
            || !matches!(mode.as_str(), "unknown" | "supported" | "unsupported")
        {
            return Err("settings_invalid");
        }
    }
    if settings.show_untested && settings.provider == "" {
        return Err("settings_invalid");
    }
    if !(5..=100).contains(&settings.keep_newest) || settings.max_bytes == 0 {
        return Err("invalid_library_limits");
    }
    Ok(())
}

fn valid_provider_id(provider: &str) -> bool {
    if super::providers::registry::spec(provider).is_some() {
        return true;
    }
    let Some(suffix) = provider.strip_prefix("custom-") else {
        return false;
    };
    suffix.len() == 8
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn current_consent_valid(settings: &AnalyticsPreferences, provider: &str, origin: &str) -> bool {
    settings
        .provider_consents
        .get(provider)
        .is_some_and(|record| {
            record.origin == origin
                && record.text_version == CONSENT_TEXT_VERSION
                && record.granted_at_unix_ms > 0
        })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn migrate_legacy_files(
    root: &Path,
    settings: AnalyticsPreferences,
    settings_path: &Path,
    key_present: bool,
    write_settings: impl FnOnce(&Path, &[u8]) -> Result<(), &'static str>,
) -> Result<AnalyticsPreferences, &'static str> {
    if key_present {
        let old_path = root.join("anthropic-key.dpapi");
        let old_key = secrets::load_provider_key(&old_path).ok_or("key_migration_failed")?;
        let endpoint = super::providers::registry::fixed_endpoint("anthropic", None)?;
        let record = ProviderKeyRecord {
            origin: endpoint.origin(),
            key: String::from_utf8(old_key).map_err(|_| "key_migration_failed")?,
        };
        let target = root.join("keys").join("anthropic.dpapi");
        if target.exists() {
            let bytes = secrets::load_provider_key(&target).ok_or("key_migration_conflict")?;
            let existing = serde_json::from_slice::<ProviderKeyRecord>(&bytes)
                .map_err(|_| "key_migration_conflict")?;
            if existing != record {
                return Err("key_migration_conflict");
            }
        } else {
            let bytes = serde_json::to_vec(&record).map_err(|_| "key_migration_failed")?;
            fs::create_dir_all(target.parent().ok_or("key_migration_failed")?)
                .map_err(|_| "key_migration_failed")?;
            secrets::store_provider_key(&target, &bytes).ok_or("key_migration_failed")?;
        }
    }
    let file = PreferencesFile {
        schema_version: PREFS_SCHEMA,
        settings: settings.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(|_| "settings_migration_failed")?;
    write_settings(settings_path, &bytes)?;
    if key_present {
        fs::remove_file(root.join("anthropic-key.dpapi"))
            .map_err(|_| "key_migration_cleanup_failed")?;
    }
    Ok(settings)
}

fn valid_hero(hero: &str) -> bool {
    hero.len() <= 64
        && hero.starts_with("npc_dota_hero_")
        && hero
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn safe_note_stem(title: &str) -> Result<String, &'static str> {
    let trimmed = title.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 120
        || trimmed.contains("..")
        || trimmed
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '\0'))
    {
        return Err("note_name_invalid");
    }
    let stem = trimmed
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ' | '(' | ')') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_owned();
    if stem.is_empty() || stem.chars().count() > 100 {
        return Err("note_name_invalid");
    }
    let windows_reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if windows_reserved
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return Err("note_name_invalid");
    }
    Ok(stem)
}

fn valid_note_id(id: &str) -> bool {
    if id.len() < 4
        || id.len() > 128
        || id == SYSTEM_NOTE
        || !id.ends_with(".md")
        || id.contains("..")
    {
        return false;
    }
    id.chars().all(|character| {
        character.is_alphanumeric() || matches!(character, '-' | '_' | ' ' | '(' | ')' | '.')
    })
}

fn transcript_turn(entry: &Value) -> Option<Value> {
    let message = entry.get("message").unwrap_or(entry);
    let role = message.get("role").and_then(Value::as_str)?;
    if !matches!(role, "user" | "assistant") {
        return None;
    }
    let content = message.get("content")?;
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|block| block.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    };
    (!text.is_empty()).then(|| json!({"role":role,"content":text}))
}

fn hmac_match_key(key: &[u8; 32], id: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= *byte;
        outer_pad[index] ^= *byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(b"match\0");
    inner.update(id);
    let digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(digest);
    outer
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "analytics-store-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn store(dir: &Path) -> AnalyticsStore {
        AnalyticsStore::open(dir, Some([7; 32]))
    }

    #[test]
    fn knowledge_cache_is_keyed_by_full_bundle_hash_and_report_regeneration_preserves_old_file() {
        let dir = root();
        let store = store(&dir);
        let bundle_hash = "a".repeat(64);
        let digest = br#"{"knowledge_version":1,"meta":{"duration_s":1}}"#;
        store
            .write_knowledge("9001", &bundle_hash, 1, digest)
            .unwrap();
        let (meta, value) = store.read_knowledge("9001", &bundle_hash).unwrap().unwrap();
        assert_eq!(meta.bundle_hash, bundle_hash);
        assert!(meta.own_hero_id.is_none());
        assert_eq!(value["knowledge_version"], 1);
        assert!(store
            .read_knowledge("9001", &"b".repeat(64))
            .unwrap()
            .is_none());

        let first = store
            .write_ai_report(
                "9001",
                &bundle_hash,
                "claude-haiku-5-5",
                2,
                1,
                Some("npc_dota_hero_axe"),
                "# First",
                json!({"input":1,"output":2}),
                3,
            )
            .unwrap();
        let second = store
            .write_ai_report(
                "9001",
                &bundle_hash,
                "claude-haiku-5-5",
                2,
                1,
                Some("npc_dota_hero_axe"),
                "# Second",
                json!({"input":4,"output":5}),
                1,
            )
            .unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(
            store.read_ai_report("9001", &first.id).unwrap().markdown,
            "# First"
        );
        assert_eq!(
            store.read_ai_report("9001", &second.id).unwrap().markdown,
            "# Second"
        );
        assert_eq!(store.list_ai_reports("9001").unwrap().len(), 2);
        assert_eq!(
            first.meta.cache_key,
            format!("{bundle_hash}:claude-haiku-5-5:p2:k1")
        );
        assert!(first
            .id
            .starts_with("ai-aaaaaaaaaaaa-claude-haiku-5-5-p2-k1-"));
        assert_eq!(
            store.read_ai_report("9001", "..\\outside").err(),
            Some("report_not_found")
        );
        assert!(first.meta.cost_estimate_usd.is_none());
        assert!(first.meta.cost_estimate_reason.as_deref().is_some());
        let raw = fs::read_to_string(
            store
                .match_dir("9001")
                .unwrap()
                .join("reports")
                .join(format!("{}.meta.json", first.id)),
        )
        .unwrap();
        assert!(!raw.contains("question"));
        assert!(!raw.contains("api_key"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn chat_transcript_returns_only_user_and_assistant_text() {
        let dir = root();
        let store = store(&dir);
        store
            .append_chat(
                "9001",
                "anthropic",
                "claude-opus-5-5",
                json!({"role":"user","content":"Why was the lane lost?"}),
            )
            .unwrap();
        store
            .append_chat(
                "9001",
                "anthropic",
                "claude-opus-5-5",
                json!({"role":"assistant","content":[
                    {"type":"thinking","thinking":"private reasoning"},
                    {"type":"text","text":"The lane was behind at 10:00."},
                    {"type":"tool_use","id":"private-tool-id","name":"get_lane"}
                ]}),
            )
            .unwrap();
        store
            .append_chat(
                "9001",
                "anthropic",
                "claude-opus-5-5",
                json!({"role":"user","content":[{"type":"tool_result","content":"private result"}]}),
            )
            .unwrap();

        assert_eq!(
            store.chat_transcript("9001").unwrap(),
            vec![
                json!({"role":"user","content":"Why was the lane lost?"}),
                json!({"role":"assistant","content":"The lane was behind at 10:00."})
            ]
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn recap_restart_requires_over_budget_history_and_preserves_a_local_note() {
        let dir = root();
        let store = store(&dir);
        assert_eq!(
            store
                .start_new_chat_from_summary("9001", "# Chat recap")
                .err(),
            Some("history_budget_not_exceeded")
        );
        store
            .append_chat(
                "9001",
                "anthropic",
                "claude-opus-5-5",
                json!({"role":"user","content":"x".repeat(36_100)}),
            )
            .unwrap();
        let note_id = store
            .start_new_chat_from_summary(
                "9001",
                "# Chat recap\n\nCreated locally without an AI request.\n\n**User:** Why?",
            )
            .unwrap();
        assert_eq!(
            store.read_note("9001", &note_id).unwrap(),
            "# Chat recap\n\nCreated locally without an AI request.\n\n**User:** Why?"
        );
        assert!(store.chat("9001").unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    fn write_v1_settings(dir: &Path, raw: &[u8]) -> PathBuf {
        let analytics = dir.join("analytics");
        fs::create_dir_all(&analytics).unwrap();
        let path = analytics.join("settings.json");
        fs::write(&path, raw).unwrap();
        path
    }

    fn v1_bytes(ai_on: bool, consent: bool) -> Vec<u8> {
        serde_json::to_vec(&json!({"schema_version":1,"settings":{"ai_on":ai_on,"consent":consent,"model":"claude-opus-5-5","keep_newest":24,"max_bytes":4096,"auto_analyse":false}})).unwrap()
    }

    #[test]
    fn key_status_never_returns_key_and_settings_default_off() {
        let dir = root();
        let store = store(&dir);
        assert_eq!(
            store.preferences().unwrap(),
            AnalyticsPreferences::default()
        );
        assert_eq!(
            store.key_status().unwrap(),
            json!({"provider":"anthropic","set":false,"last4":null})
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn old_consent_text_version_requires_reconsent_and_current_version_stays_valid() {
        let dir = root();
        let store = store(&dir);
        store.preferences().unwrap();
        let settings_path = store.root().join("settings.json");
        let settings_for = |text_version| {
            let mut settings = AnalyticsPreferences::default();
            settings.ai_on = true;
            settings.provider_consents.insert(
                "anthropic".into(),
                ProviderConsent {
                    origin: "https://api.anthropic.com".into(),
                    text_version,
                    granted_at_unix_ms: 1,
                },
            );
            PreferencesFile {
                schema_version: PREFS_SCHEMA,
                settings,
            }
        };

        let old = serde_json::to_vec(&settings_for(1)).unwrap();
        fs::write(&settings_path, old).unwrap();
        let old_settings = store.preferences().unwrap();
        assert!(
            !old_settings.consent,
            "old consent must not authorize the updated disclosure"
        );

        let current = serde_json::to_vec(&settings_for(2)).unwrap();
        fs::write(&settings_path, current).unwrap();
        let current_settings = store.preferences();
        assert!(
            current_settings
                .as_ref()
                .is_ok_and(|settings| settings.consent),
            "current consent must remain valid without another prompt"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_v1_migrates_to_v2_and_keeps_its_user_limits() {
        let dir = root();
        let settings_path = write_v1_settings(&dir, &v1_bytes(false, false));
        let next = store(&dir).preferences().unwrap();
        assert_eq!(next.provider, "anthropic");
        assert_eq!(next.keep_newest, 24);
        assert_eq!(next.max_bytes, 4096);
        assert!(!next.ai_on && !next.consent);
        let written: Value = serde_json::from_slice(&fs::read(settings_path).unwrap()).unwrap();
        assert_eq!(
            written.get("schema_version").and_then(Value::as_u64),
            Some(2)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_v1_settings_are_preserved_and_fail_closed() {
        let dir = root();
        let raw = br#"{"schema_version":1,"settings":{"ai_on":true,"consent":true,"model":"claude-opus-5-5","keep_newest":20,"max_bytes":2048,"auto_analyse":true,"unexpected":"x"}}"#;
        let path = write_v1_settings(&dir, raw);
        assert_eq!(store(&dir).preferences(), Err("settings_invalid"));
        assert_eq!(fs::read(path).unwrap(), raw);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn migration_copies_and_wraps_a_present_key_and_handles_an_absent_key() {
        let with_key = root();
        let analytics = with_key.join("analytics");
        fs::create_dir_all(&analytics).unwrap();
        assert!(secrets::store(
            &analytics.join("anthropic-key.dpapi"),
            b"sk-ant-test-key-123456"
        )
        .is_some());
        write_v1_settings(&with_key, &v1_bytes(true, true));
        let migrated = store(&with_key);
        let migrated_settings = migrated.preferences().unwrap();
        assert!(!migrated_settings.ai_on && !migrated_settings.consent);
        assert_eq!(
            migrated.read_provider_key("anthropic").unwrap().unwrap(),
            b"sk-ant-test-key-123456"
        );
        assert!(!analytics.join("anthropic-key.dpapi").exists());
        assert!(analytics.join("keys").join("anthropic.dpapi").exists());
        let _ = fs::remove_dir_all(with_key);

        let without_key = root();
        write_v1_settings(&without_key, &v1_bytes(true, true));
        let migrated = store(&without_key);
        let prefs = migrated.preferences().unwrap();
        assert!(!prefs.ai_on && !prefs.consent);
        assert_eq!(migrated.read_provider_key("anthropic").unwrap(), None);
        let _ = fs::remove_dir_all(without_key);
    }

    #[test]
    fn existing_target_key_is_never_overwritten_and_legacy_settings_survive_conflict() {
        let dir = root();
        let analytics = dir.join("analytics");
        let keys = analytics.join("keys");
        fs::create_dir_all(&keys).unwrap();
        assert!(secrets::store(
            &analytics.join("anthropic-key.dpapi"),
            b"sk-ant-old-key-123456"
        )
        .is_some());
        let other = serde_json::to_vec(&ProviderKeyRecord {
            origin: "https://api.anthropic.com".into(),
            key: "sk-ant-target-key-123456".into(),
        })
        .unwrap();
        assert!(secrets::store_provider_key(&keys.join("anthropic.dpapi"), &other).is_some());
        let raw = v1_bytes(false, false);
        let settings_path = write_v1_settings(&dir, &raw);
        assert_eq!(store(&dir).preferences(), Err("key_migration_conflict"));
        assert_eq!(fs::read(settings_path).unwrap(), raw);
        let after = secrets::load_provider_key(&keys.join("anthropic.dpapi")).unwrap();
        assert_eq!(
            serde_json::from_slice::<ProviderKeyRecord>(&after)
                .unwrap()
                .key,
            "sk-ant-target-key-123456"
        );
        assert!(analytics.join("anthropic-key.dpapi").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failure_after_key_copy_leaves_v1_settings_and_key_intact() {
        let dir = root();
        let analytics = dir.join("analytics");
        fs::create_dir_all(&analytics).unwrap();
        assert!(secrets::store(
            &analytics.join("anthropic-key.dpapi"),
            b"sk-ant-migrate-key-123456"
        )
        .is_some());
        let raw = v1_bytes(true, true);
        let settings_path = write_v1_settings(&dir, &raw);
        let mut next = AnalyticsPreferences::default();
        next.ai_on = true;
        next.consent = true;
        next.provider_consents.insert(
            "anthropic".into(),
            ProviderConsent {
                origin: "https://api.anthropic.com".into(),
                text_version: CONSENT_TEXT_VERSION,
                granted_at_unix_ms: 1,
            },
        );
        let result =
            migrate_legacy_files(&analytics, next, &settings_path, true, |_path, _bytes| {
                Err("settings_migration_failed")
            });
        assert_eq!(result, Err("settings_migration_failed"));
        assert_eq!(fs::read(settings_path).unwrap(), raw);
        assert!(analytics.join("anthropic-key.dpapi").exists());
        assert!(analytics.join("keys").join("anthropic.dpapi").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn changing_user_endpoint_deletes_provider_key_and_consent() {
        let dir = root();
        let store = store(&dir);
        let mut prefs = AnalyticsPreferences::default();
        prefs.provider = "litellm".into();
        prefs
            .provider_endpoints
            .insert("litellm".into(), "http://127.0.0.1:4000/v1".into());
        prefs.provider_consents.insert(
            "litellm".into(),
            ProviderConsent {
                origin: "http://127.0.0.1:4000".into(),
                text_version: CONSENT_TEXT_VERSION,
                granted_at_unix_ms: 1,
            },
        );
        store.write_preferences(&prefs).unwrap();
        let path = store.root().join("keys/litellm.dpapi");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"protected-test-blob").unwrap();
        let next = store
            .set_provider_endpoint("litellm", "http://127.0.0.1:5000/v1")
            .unwrap();
        assert!(!path.exists());
        assert!(!next.provider_consents.contains_key("litellm"));
        assert!(!next.ai_on);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn note_name_safety_clash_system_read_only_and_limits() {
        for invalid in [
            "..",
            "a/b",
            "a\\b",
            "C:secret",
            "CON",
            "NUL",
            "",
            "a".repeat(121).as_str(),
        ] {
            assert!(safe_note_stem(invalid).is_err(), "{invalid}");
        }
        let dir = root();
        let store = store(&dir);
        let one = store.write_note("123", "Plan", "# one").unwrap();
        let two = store.write_note("123", "Plan", "# two").unwrap();
        assert_eq!(one, "Plan.md");
        assert_eq!(two, "Plan (2).md");
        assert_ne!(
            store.write_note("123", "สรุปแมตช์", "AI").unwrap(),
            SYSTEM_NOTE
        );
        assert_eq!(
            store.delete_note("123", SYSTEM_NOTE),
            Err("system_note_read_only")
        );
        assert_eq!(
            store.write_note("123", "large", &"x".repeat(65 * 1024)),
            Err("note_too_large")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn install_key_paths_and_own_hero_are_hashed_and_survive_replay_removal() {
        let dir = root();
        let store = store(&dir);
        store.set_own_hero("12345", "npc_dota_hero_axe").unwrap();
        assert_eq!(
            store.own_hero("12345").as_deref(),
            Some("npc_dota_hero_axe")
        );
        let hash = store.match_key("12345").unwrap();
        assert_eq!(hash.len(), 64);
        assert!(!hash.contains("12345"));
        let notes = store.list_notes("12345", "# system").unwrap();
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].ai);
        assert_eq!(store.read_note("12345", SYSTEM_NOTE).unwrap(), "# system");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ledger_contains_metadata_only() {
        let dir = root();
        let store = store(&dir);
        store
            .add_ledger(LedgerRow {
                time_unix_ms: 1,
                provider: "anthropic".into(),
                endpoint_class: "cloud".into(),
                model: "claude-opus-5-5".into(),
                bytes_sent: 20,
                bytes_received: 30,
                input_tokens: Some(4),
                cache_read_tokens: Some(2),
                cache_creation_tokens: Some(1),
                output_tokens: Some(3),
                status: "success".into(),
            })
            .unwrap();
        let bytes = fs::read(store.root().join("ledger.json")).unwrap();
        let raw = String::from_utf8(bytes).unwrap();
        for secret in ["sk-ant", "question", "markdown", "prompt", "match_id"] {
            assert!(!raw.contains(secret));
        }
        assert_eq!(store.ledger().unwrap().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
}
