//! Owned game configuration, loopback intake server and own read model
//! (CORE-004, DATA-001..004; GSI-INTAKE-SPEC §2–§7).

use crate::secrets;
use base64::Engine;
use kohbor_gsi::admission::{token_matches, Ownership, Phase};
use kohbor_gsi::config::{
    choose_port, claimed_ports, GsiConfig, BODY_LIMIT_BYTES, M1_DATA_BLOCKS,
    MAX_REQUESTS_PER_SECOND,
};
use kohbor_gsi::session::{PersistedSessions, SessionChange, SessionTracker};
use kohbor_gsi::{admit, build_snapshot, strict_json, Outcome, SourceState};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SLUG: &str = "kohbor";

#[derive(Debug, Clone, PartialEq)]
pub enum IntakeState {
    NotConfigured(&'static str),
    Listening,
    Unavailable(&'static str),
}

pub struct Shared {
    pub state: IntakeState,
    pub port: Option<u16>,
    pub generation: u64,
    sequence: u64,
    last_admitted: Option<(Instant, Outcome)>,
    sessions: SessionTracker,
    pub epoch: Option<u64>,
    pub phase: Option<Phase>,
    pub ownership: Option<Ownership>,
    pub ownership_reason: Option<&'static str>,
    token: Option<String>,
    data_dir: PathBuf,
    window: (Instant, u32),
}

pub type Handle = Arc<Mutex<Shared>>;

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_json_atomic(path: &Path, v: &Value) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(v).expect("serializable"))?;
    fs::rename(tmp, path)
}

fn settings_path(dir: &Path) -> PathBuf {
    dir.join("user-settings.json")
}
fn runtime_path(dir: &Path) -> PathBuf {
    dir.join("runtime-state.json")
}

fn load_settings(dir: &Path) -> Value {
    read_json(&settings_path(dir)).unwrap_or_else(|| {
        json!({"schema_version": "1.0.0", "revision": 1, "locale": "th",
               "gsi": {"game_dir": null, "port": null, "config_sha256": null, "token_ref": null}})
    })
}

fn platform_steamid64() -> Option<u64> {
    #[cfg(windows)]
    {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey(r"Software\Valve\Steam\ActiveProcess")
            .ok()?;
        let id: u32 = key.get_value("ActiveUser").ok()?;
        (id != 0).then(|| kohbor_gsi::steamid64_from_account_id(id))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn game_running() -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq dota2.exe", "/NH"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_lowercase()
                .contains("dota2.exe")
        })
        .unwrap_or(false)
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Locate exactly one game installation (GSI-INTAKE-SPEC §2.1).
pub fn locate_game() -> Result<PathBuf, &'static str> {
    #[cfg(windows)]
    let steam: PathBuf = {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let key = hkcu
            .open_subkey(r"Software\Valve\Steam")
            .map_err(|_| "GSI_GAME_NOT_FOUND")?;
        let p: String = key
            .get_value("SteamPath")
            .map_err(|_| "GSI_GAME_NOT_FOUND")?;
        PathBuf::from(p)
    };
    #[cfg(not(windows))]
    let steam: PathBuf = return Err("GSI_GAME_NOT_FOUND");
    let mut libs = vec![steam.clone()];
    if let Ok(text) = fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("\"path\"") {
                let v = rest.trim().trim_matches('"').replace("\\\\", "\\");
                if !v.is_empty() {
                    libs.push(PathBuf::from(v));
                }
            }
        }
    }
    libs.sort();
    libs.dedup();
    let found: Vec<PathBuf> = libs
        .into_iter()
        .filter_map(|lib| {
            let text = fs::read_to_string(lib.join("steamapps/appmanifest_570.acf")).ok()?;
            let dir = text.lines().find_map(|l| {
                l.trim()
                    .strip_prefix("\"installdir\"")
                    .map(|r| r.trim().trim_matches('"').to_owned())
            })?;
            let game = lib.join("steamapps/common").join(dir);
            game.join("game/dota/cfg").is_dir().then_some(game)
        })
        .collect();
    match found.as_slice() {
        [g] => Ok(g.clone()),
        _ => Err("GSI_GAME_NOT_FOUND"),
    }
}

fn gsi_dir(game: &Path) -> PathBuf {
    game.join("game/dota/cfg/gamestate_integration")
}

fn token_from(dir: &Path) -> Option<String> {
    let raw = secrets::load_or_create(&dir.join("gsi-token.bin"), || {
        secrets::random::<32>().map(|b| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(b)
                .into_bytes()
        })
    })?;
    String::from_utf8(raw).ok()
}

impl Shared {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let rt = read_json(&runtime_path(&data_dir));
        let generation = rt
            .as_ref()
            .and_then(|v| v["source_generation"].as_u64())
            .unwrap_or(0);
        let key = secrets::load_or_create(&data_dir.join("install-key.bin"), || {
            secrets::random::<32>().map(|b| b.to_vec())
        });
        let key: Option<[u8; 32]> = key.and_then(|k| k.try_into().ok());
        let persisted: Option<PersistedSessions> = rt
            .as_ref()
            .and_then(|v| serde_json::from_value(v["sessions"].clone()).ok());
        let (sessions, secret_ok) = match (key, persisted) {
            (Some(k), Some(p)) => (SessionTracker::restore(k, p), true),
            (Some(k), None) => (SessionTracker::new(k), true),
            (None, _) => (SessionTracker::new([0; 32]), false),
        };
        let token = if secret_ok {
            token_from(&data_dir)
        } else {
            None
        };
        let state = if token.is_none() {
            IntakeState::Unavailable("GSI_SECRET_UNAVAILABLE")
        } else {
            IntakeState::NotConfigured("SOURCE_UNAVAILABLE")
        };
        Self {
            state,
            port: None,
            generation,
            sequence: 0,
            last_admitted: None,
            sessions,
            epoch: None,
            phase: None,
            ownership: None,
            ownership_reason: None,
            token,
            data_dir,
            window: (Instant::now(), 0),
        }
    }

    fn persist_runtime(&self) {
        let v = json!({"schema_version": "1.0.0", "source_generation": self.generation,
                       "sessions": self.sessions.persisted()});
        let _ = write_json_atomic(&runtime_path(&self.data_dir), &v);
    }

    fn next_generation(&mut self) {
        self.generation += 1;
        self.sequence = 0;
        self.last_admitted = None;
        self.persist_runtime();
    }

    /// Own snapshot (own-snapshot 2.0.0) for the latest admitted payload.
    pub fn snapshot(&self) -> Value {
        let generation = self.generation.max(1);
        match &self.last_admitted {
            None => build_snapshot(
                &Outcome::NotPlaying,
                SourceState {
                    generation,
                    sequence: None,
                    age_ms: None,
                    session_epoch: None,
                    identity_revision: 1,
                },
            ),
            Some((at, outcome)) => build_snapshot(
                outcome,
                SourceState {
                    generation,
                    sequence: Some(self.sequence.max(1)),
                    age_ms: Some(at.elapsed().as_millis() as u64),
                    session_epoch: self.epoch,
                    identity_revision: 1,
                },
            ),
        }
    }

    pub fn last_age_ms(&self) -> Option<u64> {
        self.last_admitted
            .as_ref()
            .map(|(at, _)| at.elapsed().as_millis() as u64)
    }

    fn rate_ok(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.window.0) >= Duration::from_secs(1) {
            self.window = (now, 0);
        }
        self.window.1 += 1;
        self.window.1 <= MAX_REQUESTS_PER_SECOND
    }

    /// Handle one parsed, size-checked body. Returns the HTTP status to send.
    fn accept(&mut self, body: &Value, arrival: Instant) -> u16 {
        let Some(token) = self.token.clone() else {
            return 503;
        };
        let presented = body.pointer("/auth/token").and_then(Value::as_str);
        if !presented.is_some_and(|t| token_matches(&token, t)) {
            return 401;
        }
        if !self.rate_ok() {
            return 429;
        }
        // A different verified owner within a match is handled by the session tracker
        // as an owner change (new epoch, values withdrawn), so no session owner is passed.
        let outcome = admit(body, &token, platform_steamid64(), None);
        if let Outcome::Admitted(a) = &outcome {
            self.phase = Some(a.phase);
            self.ownership = Some(a.ownership);
            self.ownership_reason = match a.ownership {
                Ownership::Verified => None,
                _ => Some("OWNER_UNVERIFIED"),
            };
            if a.ownership == Ownership::Verified {
                if let Some(owner) = a.steamid64 {
                    let change = self.sessions.observe(a.match_id.as_deref(), owner, || {
                        secrets::random::<16>()
                            .map(|b| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b))
                            .unwrap_or_default()
                    });
                    match change {
                        SessionChange::Started { epoch }
                        | SessionChange::OwnerChanged { epoch } => {
                            self.epoch = Some(epoch);
                            self.persist_runtime();
                        }
                        SessionChange::Resumed { epoch } | SessionChange::Unchanged { epoch } => {
                            self.epoch = Some(epoch)
                        }
                        SessionChange::NoEvidence => {}
                    }
                }
            }
        } else {
            self.phase = None;
        }
        self.sequence += 1;
        self.last_admitted = Some((arrival, outcome));
        200
    }
}

fn serve(handle: Handle, server: Arc<tiny_http::Server>) {
    loop {
        let Ok(mut req) = server.recv() else { break };
        let arrival = Instant::now();
        let status = if *req.method() != tiny_http::Method::Post || req.url() != "/" {
            404
        } else if req.body_length().is_some_and(|n| n > BODY_LIMIT_BYTES) {
            413
        } else {
            let mut body = Vec::new();
            let _ = req
                .as_reader()
                .take(BODY_LIMIT_BYTES as u64 + 1)
                .read_to_end(&mut body);
            if body.len() > BODY_LIMIT_BYTES {
                413
            } else {
                match strict_json::parse(&body) {
                    Ok(v) => handle
                        .lock()
                        .map(|mut s| s.accept(&v, arrival))
                        .unwrap_or(500),
                    Err(_) => 400,
                }
            }
        };
        let _ = req.respond(tiny_http::Response::empty(status));
    }
}

pub struct Runtime {
    pub shared: Handle,
    server: Mutex<Option<Arc<tiny_http::Server>>>,
}

impl Runtime {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            shared: Arc::new(Mutex::new(Shared::new(data_dir))),
            server: Mutex::new(None),
        })
    }

    fn bind(&self, port: u16) -> Result<(), &'static str> {
        let server = Arc::new(
            tiny_http::Server::http(("127.0.0.1", port)).map_err(|_| "GSI_PORT_UNAVAILABLE")?,
        );
        if let Some(old) = self.server.lock().unwrap().replace(server.clone()) {
            old.unblock();
        }
        let handle = self.shared.clone();
        std::thread::Builder::new()
            .name("gsi-intake".into())
            .spawn(move || serve(handle, server))
            .map_err(|_| "GSI_PORT_UNAVAILABLE")?;
        let mut s = self.shared.lock().unwrap();
        s.port = Some(port);
        s.state = IntakeState::Listening;
        s.next_generation();
        Ok(())
    }

    /// Startup: bind the persisted port only (GSI-INTAKE-SPEC §2.4).
    pub fn start(&self) {
        let (dir, has_token) = {
            let s = self.shared.lock().unwrap();
            (s.data_dir.clone(), s.token.is_some())
        };
        if !has_token {
            return;
        }
        let settings = load_settings(&dir);
        let Some(port) = settings["gsi"]["port"].as_u64().map(|p| p as u16) else {
            return;
        };
        if self.bind(port).is_ok() {
            return;
        }
        if game_running() {
            self.shared.lock().unwrap().state = IntakeState::Unavailable("GSI_PORT_UNAVAILABLE");
        } else if let Err(code) = self.setup() {
            self.shared.lock().unwrap().state = IntakeState::Unavailable(code);
        }
    }

    /// User-initiated setup: write only the owned configuration, then listen.
    pub fn setup(&self) -> Result<(), &'static str> {
        let (dir, token) = {
            let s = self.shared.lock().unwrap();
            (s.data_dir.clone(), s.token.clone())
        };
        let token = token.ok_or("GSI_SECRET_UNAVAILABLE")?;
        let game = locate_game()?;
        let gdir = gsi_dir(&game);
        fs::create_dir_all(&gdir).map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        let others: Vec<String> = fs::read_dir(&gdir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        e.file_name().to_string_lossy()
                            != format!("gamestate_integration_{SLUG}.cfg")
                    })
                    .filter_map(|e| fs::read_to_string(e.path()).ok())
                    .collect()
            })
            .unwrap_or_default();
        let claimed = claimed_ports(others.iter().map(String::as_str));
        // Release our own listener first so its port can be reconsidered.
        if let Some(old) = self.server.lock().unwrap().take() {
            old.unblock();
        }
        let port = choose_port(&claimed, port_free).ok_or("GSI_PORT_EXHAUSTED")?;
        let cfg = GsiConfig {
            display_name: "Kohbor",
            slug: SLUG,
            port,
            token: &token,
            data_blocks: &M1_DATA_BLOCKS,
        };
        let text = cfg.render().map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        let target = gdir.join(cfg.file_name().map_err(|_| "GSI_CONFIG_WRITE_FAILED")?);
        let mut settings = load_settings(&dir);
        if let Ok(existing) = fs::read(&target) {
            let known = settings["gsi"]["config_sha256"].as_str().unwrap_or("");
            if sha256_hex(&existing) != known {
                return Err("GSI_CONFIG_CONFLICT");
            }
        }
        let tmp = target.with_extension("cfg.tmp");
        fs::write(&tmp, text.as_bytes()).map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        fs::rename(&tmp, &target).map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        let revision = settings["revision"].as_u64().unwrap_or(1) + 1;
        settings["revision"] = json!(revision);
        settings["gsi"] = json!({
            "game_dir": game.to_string_lossy(),
            "port": port,
            "config_sha256": sha256_hex(text.as_bytes()),
            "token_ref": "dpapi:gsi-token",
        });
        write_json_atomic(&settings_path(&dir), &settings)
            .map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        self.bind(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_with_token() -> Shared {
        let dir = std::env::temp_dir().join(format!(
            "kohbor-intake-test-{}-{}",
            std::process::id(),
            line!()
        ));
        let mut s = Shared::new(dir);
        s.token = Some("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_".into());
        s.generation = 1;
        s
    }

    #[test]
    fn wrong_token_never_renews_freshness() {
        let mut s = shared_with_token();
        let body = json!({"provider": {"appid": 570}, "player": {}, "auth": {"token": "x"}});
        assert_eq!(s.accept(&body, Instant::now()), 401);
        assert!(s.last_admitted.is_none());
        assert_eq!(s.snapshot()["availability"], "unknown");
    }

    #[test]
    fn menu_payload_is_admitted_as_not_playing() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let body = json!({"provider": {"appid": 570}, "player": {}, "auth": {"token": tok}});
        assert_eq!(s.accept(&body, Instant::now()), 200);
        assert_eq!(s.snapshot()["availability"], "unknown");
        assert!(s.last_age_ms().is_some());
    }

    #[test]
    fn rate_limit_drops_beyond_50_per_second() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let body = json!({"provider": {"appid": 570}, "player": {}, "auth": {"token": tok}});
        let codes: Vec<u16> = (0..55).map(|_| s.accept(&body, Instant::now())).collect();
        assert_eq!(codes.iter().filter(|c| **c == 429).count(), 5);
    }
}
