//! Owned game configuration, loopback intake server and own read model
//! (CORE-004, DATA-001..004; GSI-INTAKE-SPEC §2–§7).

use crate::{atomic, secrets};
use base64::Engine;
use kohbor_gsi::admission::{token_matches, Ownership, Phase};
use kohbor_gsi::config::{
    choose_port, claimed_ports, GsiConfig, BODY_LIMIT_BYTES, M1_DATA_BLOCKS,
    MAX_REQUESTS_PER_SECOND, PORT_FIRST, PORT_LAST,
};
use kohbor_gsi::session::{PersistedSessions, SessionChange, SessionTracker};
use kohbor_gsi::{admit, build_snapshot, strict_json, Outcome, SourceState};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use socket2::SockRef;
use std::fs;
use std::io::{self, Read};
use std::net::{TcpListener, ToSocketAddrs};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub const SLUG: &str = "kohbor";
/// Other applications' configuration files larger than this are not read for port claims.
const OTHER_CONFIG_MAX_BYTES: u64 = 65_536;
const INTAKE_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Lock that survives a panicked holder, so status stays readable (CORE-005)
/// instead of every later command panicking on a poisoned mutex.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

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
    /// Current verified hero identity is transient and scoped to the process.
    session_hero_id: Option<String>,
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
    atomic::write(path, &serde_json::to_vec_pretty(v).expect("serializable"))
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

/// Whether the game process exists, from the process list only (GSI-INTAKE-SPEC
/// §2.4). `None` means the list could not be read; callers must not treat that
/// as "not running". No helper process is spawned.
#[cfg(windows)]
fn game_running() -> Option<bool> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_err() {
            let _ = CloseHandle(snapshot);
            return None;
        }
        let mut found = false;
        loop {
            let len = entry
                .szExeFile
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case("dota2.exe") {
                found = true;
                break;
            }
            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snapshot);
        Some(found)
    }
}

#[cfg(not(windows))]
fn game_running() -> Option<bool> {
    None
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Bind the loopback intake with a read timeout before tiny_http accepts clients.
/// tiny_http can read a small body before returning its Request to `recv()`.
fn bind_server(addr: impl ToSocketAddrs) -> io::Result<tiny_http::Server> {
    let listener = TcpListener::bind(addr)?;
    SockRef::from(&listener).set_read_timeout(Some(INTAKE_READ_TIMEOUT))?;
    tiny_http::Server::from_listener(listener, None).map_err(io::Error::other)
}

/// A manifest `installdir` must be one plain folder name: no separators, no
/// `..`, no drive or root. Anything else could point the owned file elsewhere.
fn single_component(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    )
}

/// Find exactly one game installation among the given Steam libraries.
///
/// The Steam path in the registry and the same library listed in
/// `libraryfolders.vdf` usually differ in case and separators, so candidates are
/// compared by their resolved location, not by spelling.
fn find_game(libraries: impl IntoIterator<Item = PathBuf>) -> Result<PathBuf, &'static str> {
    let mut found: Vec<(PathBuf, PathBuf)> = Vec::new();
    for lib in libraries {
        if !lib.is_absolute() {
            continue;
        }
        let Ok(text) = fs::read_to_string(lib.join("steamapps/appmanifest_570.acf")) else {
            continue;
        };
        let Some(dir) = text.lines().find_map(|l| {
            l.trim()
                .strip_prefix("\"installdir\"")
                .map(|r| r.trim().trim_matches('"').to_owned())
        }) else {
            continue;
        };
        if !single_component(&dir) {
            continue;
        }
        let game = lib.join("steamapps/common").join(dir);
        if !game.join("game/dota/cfg").is_dir() {
            continue;
        }
        let Ok(resolved) = fs::canonicalize(&game) else {
            continue;
        };
        if !found.iter().any(|(known, _)| *known == resolved) {
            found.push((resolved, game));
        }
    }
    match found.as_slice() {
        [(_, game)] => Ok(game.clone()),
        _ => Err("GSI_GAME_NOT_FOUND"),
    }
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
    find_game(libs)
}

fn gsi_dir(game: &Path) -> PathBuf {
    game.join("game/dota/cfg/gamestate_integration")
}

fn owned_config_name() -> String {
    format!("gamestate_integration_{SLUG}.cfg")
}

fn new_token() -> Option<String> {
    secrets::random::<32>().map(|b| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b))
}

fn token_from(dir: &Path) -> Option<String> {
    let raw = secrets::load_or_create(&dir.join("gsi-token.bin"), || {
        new_token().map(String::into_bytes)
    })?;
    String::from_utf8(raw).ok()
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Shared {
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&data_dir);
        let rt_path = runtime_path(&data_dir);
        let rt = read_json(&rt_path);
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
        // A runtime-state file that exists but cannot be read back is corrupt. The
        // epoch counter must still never be reused (GSI-INTAKE-SPEC §6): with no
        // recoverable value, jump once to wall-clock seconds.
        let corrupt = persisted.is_none() && rt_path.exists();
        let (sessions, secret_ok) = match (key, persisted) {
            (Some(k), Some(p)) => (SessionTracker::restore(k, p), true),
            (Some(k), None) if corrupt => {
                eprintln!("runtime-state unreadable: session epoch recovered from wall clock");
                let recovered = PersistedSessions {
                    schema_version: "1.0.0".into(),
                    last_session_epoch: unix_seconds(),
                    current_session: None,
                };
                (SessionTracker::restore(k, recovered), true)
            }
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
            session_hero_id: None,
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
    ///
    /// `platform` is the signed-in platform identity read by the caller for this
    /// request (`None` when unreadable, which can never yield verified ownership).
    fn accept(&mut self, body: &Value, arrival: Instant, platform: Option<u64>) -> u16 {
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
        let mut outcome = admit(body, &token, platform, None);
        if let Outcome::Rejected(_) = &outcome {
            // Authenticated but failing admission (for example a foreign provider):
            // it never renews freshness, changes session identity or consumes a
            // sequence number (GSI-INTAKE-SPEC §3, §5). The reply stays 200 so the
            // response reveals no admission detail.
            return 200;
        }
        if let Outcome::Admitted(a) = &mut outcome {
            self.phase = Some(a.phase);
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
                            self.session_hero_id = a.hero.hero_id.value().cloned();
                            self.persist_runtime();
                        }
                        SessionChange::Resumed { epoch } => {
                            self.epoch = Some(epoch);
                            self.verify_session_hero(a);
                        }
                        SessionChange::Unchanged { epoch } => {
                            self.epoch = Some(epoch);
                            self.verify_session_hero(a);
                        }
                        SessionChange::NoEvidence => self.verify_session_hero(a),
                    }
                }
            }
            self.ownership = Some(a.ownership);
            self.ownership_reason = match a.ownership {
                Ownership::Unverified | Ownership::Mismatch => Some("OWNER_UNVERIFIED"),
                Ownership::Verified => None,
            };
        } else {
            self.phase = None;
            if matches!(outcome, Outcome::NotPlaying) {
                // Menu/idle data is admitted as a fresh source heartbeat, but it
                // carries no active ownership assertion or reason.
                self.ownership = None;
                self.ownership_reason = None;
                self.session_hero_id = None;
            }
        }
        self.sequence += 1;
        self.last_admitted = Some((arrival, outcome));
        200
    }

    /// Preserve the session's first observed non-empty hero. Missing hero data,
    /// including the empty name used during selection, is not a change.
    fn verify_session_hero(&mut self, admitted: &mut kohbor_gsi::admission::Admitted) {
        let Some(hero_id) = admitted.hero.hero_id.value() else {
            return;
        };
        match self.session_hero_id.as_deref() {
            Some(previous) if previous != hero_id => admitted.ownership = Ownership::Mismatch,
            Some(_) => {}
            None => self.session_hero_id = Some(hero_id.clone()),
        }
    }
}

/// Checks that run before any body byte is read. Returns the status to send, or
/// `None` to go on and read the body.
fn precheck(req: &tiny_http::Request) -> Option<u16> {
    if *req.method() != tiny_http::Method::Post || req.url() != "/" {
        return Some(404);
    }
    // The game client is not a browser. A request carrying `Origin` was issued by
    // web content (a page, or a DNS-rebinding host) and is refused unread, so web
    // pages cannot make the intake parse bodies.
    if req.headers().iter().any(|h| h.field.equiv("Origin")) {
        return Some(403);
    }
    if req.body_length().is_some_and(|n| n > BODY_LIMIT_BYTES) {
        return Some(413);
    }
    None
}

fn serve(handle: Handle, server: Arc<tiny_http::Server>) {
    loop {
        let Ok(mut req) = server.recv() else { break };
        let status = precheck(&req).unwrap_or_else(|| {
            let mut body = Vec::new();
            let read = req
                .as_reader()
                .take(BODY_LIMIT_BYTES as u64 + 1)
                .read_to_end(&mut body);
            // Arrival: the complete body is in hand, before parsing (§5).
            let arrival = Instant::now();
            if read.is_err() {
                400
            } else if body.len() > BODY_LIMIT_BYTES {
                413
            } else {
                match strict_json::parse(&body) {
                    Ok(v) => {
                        let platform = platform_steamid64();
                        lock(&handle).accept(&v, arrival, platform)
                    }
                    Err(_) => 400,
                }
            }
        });
        let _ = req.respond(tiny_http::Response::empty(status));
    }
}

pub struct Runtime {
    pub shared: Handle,
    server: Mutex<Option<Arc<tiny_http::Server>>>,
    /// Serializes setup: overlapping runs must not interleave file and port work.
    setup_gate: Mutex<()>,
}

impl Runtime {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            shared: Arc::new(Mutex::new(Shared::new(data_dir))),
            server: Mutex::new(None),
            setup_gate: Mutex::new(()),
        })
    }

    /// Stop the current listener, if any, and return the port it held.
    fn release(&self) -> Option<u16> {
        let old = lock(&self.server).take()?;
        old.unblock();
        lock(&self.shared).port
    }

    /// Listen on `port`. `retries` covers the short window in which a listener we
    /// released ourselves is still closing its socket.
    fn bind(&self, port: u16, retries: u32) -> Result<(), &'static str> {
        let mut attempts = 0;
        let server = loop {
            match bind_server(("127.0.0.1", port)) {
                Ok(s) => break Arc::new(s),
                Err(_) if attempts < retries => {
                    attempts += 1;
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return Err("GSI_PORT_UNAVAILABLE"),
            }
        };
        if let Some(old) = lock(&self.server).replace(server.clone()) {
            old.unblock();
        }
        let handle = self.shared.clone();
        if std::thread::Builder::new()
            .name("gsi-intake".into())
            .spawn(move || serve(handle, server))
            .is_err()
        {
            lock(&self.server).take();
            return Err("GSI_PORT_UNAVAILABLE");
        }
        let mut s = lock(&self.shared);
        s.port = Some(port);
        s.state = IntakeState::Listening;
        s.next_generation();
        Ok(())
    }

    /// Startup: bind the persisted port only (GSI-INTAKE-SPEC §2.4).
    pub fn start(&self) {
        let (dir, has_token) = {
            let s = lock(&self.shared);
            (s.data_dir.clone(), s.token.is_some())
        };
        if !has_token {
            return;
        }
        let settings = load_settings(&dir);
        let persisted = settings["gsi"]["port"]
            .as_u64()
            .and_then(|p| u16::try_from(p).ok())
            .filter(|p| (PORT_FIRST..=PORT_LAST).contains(p));
        let Some(port) = persisted else {
            return;
        };
        if self.bind(port, 0).is_ok() {
            return;
        }
        match game_running() {
            // Game closed: move to another candidate and rewrite the owned file.
            Some(false) => {
                let _ = self.setup(false);
            }
            // Game running, or the process list is unreadable: never rewrite the
            // configuration under a game that may be using it.
            _ => lock(&self.shared).state = IntakeState::Unavailable("GSI_PORT_UNAVAILABLE"),
        }
    }

    /// User-initiated setup: write only the owned configuration, then listen.
    ///
    /// `replace` is the explicit "replace owned configuration" action for a file
    /// that was edited outside the app (GSI-INTAKE-SPEC §2.2); it rotates the token.
    pub fn setup(&self, replace: bool) -> Result<(), &'static str> {
        self.setup_with(locate_game, replace)
    }

    fn setup_with(
        &self,
        locate: impl FnOnce() -> Result<PathBuf, &'static str>,
        replace: bool,
    ) -> Result<(), &'static str> {
        let _gate = lock(&self.setup_gate);
        let result = self.setup_inner(locate, replace);
        if let Err(code) = result {
            // Nothing is listening: report why, never a leftover "listening".
            if lock(&self.server).is_none() {
                let mut s = lock(&self.shared);
                s.port = None;
                s.state = IntakeState::Unavailable(code);
            }
        }
        result
    }

    fn setup_inner(
        &self,
        locate: impl FnOnce() -> Result<PathBuf, &'static str>,
        replace: bool,
    ) -> Result<(), &'static str> {
        if lock(&self.shared).token.is_none() {
            return Err("GSI_SECRET_UNAVAILABLE");
        }
        let game = locate()?;
        let mut own_port = None;
        let port = self.install_config(&game, replace, |claimed| {
            // Release our own listener only now, after the conflict rule passed. Its
            // port stays a candidate even while the socket is still closing.
            own_port = self.release();
            choose_port(claimed, |p| Some(p) == own_port || port_free(p))
        })?;
        self.bind(port, if own_port == Some(port) { 20 } else { 0 })
    }

    /// File half of setup (GSI-INTAKE-SPEC §2.2–§2.4): the conflict rule, token
    /// rotation on replacement, the owned file and the persisted settings.
    /// `pick_port` receives the loopback ports other configurations claim.
    fn install_config(
        &self,
        game: &Path,
        replace: bool,
        pick_port: impl FnOnce(&[u16]) -> Option<u16>,
    ) -> Result<u16, &'static str> {
        let (dir, token) = {
            let s = lock(&self.shared);
            (s.data_dir.clone(), s.token.clone())
        };
        let mut token = token.ok_or("GSI_SECRET_UNAVAILABLE")?;
        let gdir = gsi_dir(game);
        fs::create_dir_all(&gdir).map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        let own_name = owned_config_name();
        let target = gdir.join(&own_name);
        let mut settings = load_settings(&dir);
        // An owned file that no longer matches the recorded hash was edited outside
        // the app and is never overwritten without the explicit replace action.
        let conflict = fs::read(&target).is_ok_and(|existing| {
            settings["gsi"]["config_sha256"].as_str() != Some(sha256_hex(&existing).as_str())
        });
        if conflict && !replace {
            return Err("GSI_CONFIG_CONFLICT");
        }
        let others: Vec<String> = fs::read_dir(&gdir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        let name = e.file_name().to_string_lossy().to_lowercase();
                        name.starts_with("gamestate_integration_")
                            && name.ends_with(".cfg")
                            && name != own_name
                    })
                    .filter(|e| {
                        e.metadata()
                            .is_ok_and(|m| m.is_file() && m.len() <= OTHER_CONFIG_MAX_BYTES)
                    })
                    .filter_map(|e| fs::read_to_string(e.path()).ok())
                    .collect()
            })
            .unwrap_or_default();
        let claimed = claimed_ports(others.iter().map(String::as_str));
        let port = pick_port(&claimed).ok_or("GSI_PORT_EXHAUSTED")?;
        if conflict {
            token = new_token().ok_or("GSI_SECRET_UNAVAILABLE")?;
            secrets::store(&dir.join("gsi-token.bin"), token.as_bytes())
                .ok_or("GSI_SECRET_UNAVAILABLE")?;
            lock(&self.shared).token = Some(token.clone());
        }
        let cfg = GsiConfig {
            display_name: "Kohbor",
            slug: SLUG,
            port,
            token: &token,
            data_blocks: &M1_DATA_BLOCKS,
        };
        let text = cfg.render().map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
        atomic::write(&target, text.as_bytes()).map_err(|_| "GSI_CONFIG_WRITE_FAILED")?;
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
        Ok(port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicU32, Ordering};

    const ACCOUNT: u32 = 12_345;

    fn owner() -> u64 {
        kohbor_gsi::steamid64_from_account_id(ACCOUNT)
    }

    fn temp_dir(tag: &str) -> PathBuf {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "kohbor-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn shared_with_token() -> Shared {
        let mut s = Shared::new(temp_dir("intake"));
        s.token = Some("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_".into());
        s.generation = 1;
        s
    }

    fn menu(token: &str) -> Value {
        json!({"provider": {"appid": 570}, "player": {}, "auth": {"token": token}})
    }

    /// Fabricated in-game payload (synthetic evidence only, never shipped).
    fn in_game(token: &str) -> Value {
        json!({
            "provider": {"name": "Dota 2", "appid": 570},
            "map": {"matchid": "8000000001", "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "customgamename": ""},
            "player": {"steamid": owner().to_string(), "activity": "playing", "team_name": "radiant"},
            "hero": {"name": "npc_dota_hero_axe", "level": 12, "health": 850, "max_health": 1200,
                     "mana_percent": 43, "alive": true, "respawn_seconds": 0},
            "auth": {"token": token}
        })
    }

    #[test]
    fn wrong_token_never_renews_freshness() {
        let mut s = shared_with_token();
        assert_eq!(s.accept(&menu("x"), Instant::now(), None), 401);
        assert!(s.last_admitted.is_none());
        assert_eq!(s.snapshot()["availability"], "unknown");
    }

    #[test]
    fn menu_payload_is_admitted_as_not_playing() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        assert_eq!(s.accept(&menu(&tok), Instant::now(), None), 200);
        assert_eq!(s.snapshot()["availability"], "unknown");
        assert!(s.last_age_ms().is_some());
    }

    #[test]
    fn rate_limit_drops_beyond_50_per_second() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let body = menu(&tok);
        let codes: Vec<u16> = (0..55)
            .map(|_| s.accept(&body, Instant::now(), None))
            .collect();
        assert_eq!(codes.iter().filter(|c| **c == 429).count(), 5);
    }

    #[test]
    fn authenticated_foreign_provider_never_renews_freshness_or_sequence() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let mut body = in_game(&tok);
        body["provider"]["appid"] = json!(730);
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert!(s.last_admitted.is_none());
        assert_eq!(s.sequence, 0);
        assert!(s.epoch.is_none() && s.ownership.is_none());
        // A valid payload afterwards takes the first sequence number.
        assert_eq!(s.accept(&in_game(&tok), Instant::now(), Some(owner())), 200);
        assert_eq!(s.snapshot()["source"]["sequence"], 1);
    }

    #[test]
    fn own_values_need_verified_ownership() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let body = in_game(&tok);
        // Platform identity unreadable: never verified, no values, no session.
        assert_eq!(s.accept(&body, Instant::now(), None), 200);
        assert_eq!(s.ownership, Some(Ownership::Unverified));
        assert_eq!(s.ownership_reason, Some("OWNER_UNVERIFIED"));
        assert!(s.snapshot()["own"].is_null() && s.epoch.is_none());
        // A different signed-in account: mismatch, still no values.
        assert_eq!(s.accept(&body, Instant::now(), Some(owner() + 1)), 200);
        assert_eq!(s.ownership, Some(Ownership::Mismatch));
        assert!(s.snapshot()["own"].is_null() && s.epoch.is_none());
        // Matching identity: values and a session appear.
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(s.ownership, Some(Ownership::Verified));
        let snap = s.snapshot();
        assert_eq!(snap["availability"], "fresh");
        assert_eq!(snap["own"]["fields"]["hp"]["value"], 850);
        assert_eq!(snap["session_epoch"], 1);
    }

    #[test]
    fn changed_hero_within_session_is_mismatched_and_withdrawn() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let mut body = in_game(&tok);
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(s.ownership, Some(Ownership::Verified));
        let epoch = s.epoch;
        assert_eq!(s.snapshot()["own"]["fields"]["hp"]["value"], 850);

        body["hero"]["name"] = json!("npc_dota_hero_lina");
        body["hero"]["health"] = json!(111);
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(
            s.ownership,
            Some(Ownership::Mismatch),
            "a hero change within the same session must mismatch ownership"
        );
        assert_eq!(
            s.epoch, epoch,
            "a hero mismatch must not create a session boundary"
        );
        assert!(
            s.snapshot()["own"].is_null(),
            "mismatched hero values must be withdrawn"
        );
    }

    #[test]
    fn returning_to_menu_releases_verified_ownership() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        assert_eq!(s.accept(&in_game(&tok), Instant::now(), Some(owner())), 200);
        assert_eq!(s.ownership, Some(Ownership::Verified));
        assert_eq!(s.accept(&menu(&tok), Instant::now(), Some(owner())), 200);
        assert!(
            s.ownership.is_none(),
            "menu must release ownership to unknown"
        );
        assert!(
            s.ownership_reason.is_none(),
            "menu must clear the prior ownership reason"
        );
        assert!(
            s.snapshot()["own"].is_null(),
            "menu must withdraw own values"
        );
    }

    #[test]
    fn corrupt_runtime_state_never_reuses_an_epoch() {
        let dir = temp_dir("corrupt");
        fs::write(runtime_path(&dir), b"{ not json").unwrap();
        let before = unix_seconds();
        let mut s = Shared::new(dir);
        let tok = s
            .token
            .clone()
            .expect("DPAPI available on the test machine");
        assert_eq!(s.accept(&in_game(&tok), Instant::now(), Some(owner())), 200);
        assert!(
            s.epoch.unwrap() > before,
            "epoch jumps past wall-clock seconds"
        );
        // A missing file is a fresh installation and starts at 1.
        let mut fresh = Shared::new(temp_dir("fresh"));
        let tok = fresh.token.clone().unwrap();
        fresh.accept(&in_game(&tok), Instant::now(), Some(owner()));
        assert_eq!(fresh.epoch, Some(1));
    }

    fn fake_library(root: &Path, installdir: &str) -> PathBuf {
        let lib = root.join("SteamLib");
        fs::create_dir_all(lib.join("steamapps/common/dota 2 beta/game/dota/cfg")).unwrap();
        fs::write(
            lib.join("steamapps/appmanifest_570.acf"),
            format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"570\"\n\t\"installdir\"\t\t\"{installdir}\"\n}}\n"),
        )
        .unwrap();
        lib
    }

    #[test]
    fn one_library_spelled_two_ways_is_one_game() {
        let root = temp_dir("locate");
        let lib = fake_library(&root, "dota 2 beta");
        // The registry spells the library in lower case with forward slashes; the
        // library list spells it with its real case.
        let other_spelling = PathBuf::from(lib.to_string_lossy().to_lowercase().replace('\\', "/"));
        let game = find_game([other_spelling, lib.clone()]).expect("exactly one game");
        assert!(game.join("game/dota/cfg").is_dir());
        assert_eq!(find_game([root.join("missing")]), Err("GSI_GAME_NOT_FOUND"));
        assert_eq!(
            find_game([PathBuf::from("relative/lib")]),
            Err("GSI_GAME_NOT_FOUND")
        );
    }

    #[test]
    fn manifest_installdir_cannot_leave_the_library() {
        let root = temp_dir("traverse");
        // A real game folder exists outside steamapps/common; the manifest points at it.
        fs::create_dir_all(root.join("SteamLib/steamapps/elsewhere/game/dota/cfg")).unwrap();
        let lib = fake_library(&root, "..\\\\elsewhere");
        assert_eq!(find_game([lib]), Err("GSI_GAME_NOT_FOUND"));
        assert!(single_component("dota 2 beta"));
        for bad in ["", "..", "a/b", "a\\b", "C:\\x", "\\x"] {
            assert!(!single_component(bad), "{bad:?}");
        }
    }

    fn token_in(cfg_text: &str) -> String {
        cfg_text
            .lines()
            .find_map(|l| l.trim().strip_prefix("\"token\""))
            .map(|r| r.trim().trim_matches('"').to_owned())
            .expect("token line")
    }

    #[test]
    fn edited_owned_config_is_kept_until_explicit_replace_which_rotates_the_token() {
        let rt = Runtime::new(temp_dir("setup-data"));
        let game = temp_dir("setup-game");
        let gdir = gsi_dir(&game);
        fs::create_dir_all(&gdir).unwrap();
        fs::write(
            gdir.join("gamestate_integration_other.cfg"),
            "\"Other\"\n{\n  \"uri\" \"http://127.0.0.1:3000/\"\n}\n",
        )
        .unwrap();
        // A leftover temporary file of our own must not claim our port.
        fs::write(
            gdir.join("gamestate_integration_kohbor.cfg.tmp"),
            "\"x\"\n{\n  \"uri\" \"http://127.0.0.1:3005/\"\n}\n",
        )
        .unwrap();
        let target = gdir.join(owned_config_name());
        let pick = |claimed: &[u16]| {
            assert_eq!(claimed, [3000]);
            Some(3005)
        };

        assert_eq!(rt.install_config(&game, false, pick), Ok(3005));
        let first = fs::read_to_string(&target).unwrap();
        assert!(first.contains("http://127.0.0.1:3005/"));
        let first_token = token_in(&first);
        assert_eq!(
            lock(&rt.shared).token.as_deref(),
            Some(first_token.as_str())
        );
        // Unchanged file: a rewrite is allowed and keeps the token.
        assert_eq!(rt.install_config(&game, false, pick), Ok(3005));
        assert_eq!(token_in(&fs::read_to_string(&target).unwrap()), first_token);

        // Edited outside the app: refused, byte-for-byte untouched, port never picked.
        let edited = first.replace("3005", "3009");
        fs::write(&target, &edited).unwrap();
        let never = |_: &[u16]| -> Option<u16> { panic!("port must not be picked on conflict") };
        assert_eq!(
            rt.install_config(&game, false, never),
            Err("GSI_CONFIG_CONFLICT")
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), edited);

        // Explicit replace: owned file rewritten with a new token.
        assert_eq!(rt.install_config(&game, true, pick), Ok(3005));
        let replaced = fs::read_to_string(&target).unwrap();
        let rotated = token_in(&replaced);
        assert_ne!(rotated, first_token);
        assert_eq!(lock(&rt.shared).token.as_deref(), Some(rotated.as_str()));
        let data_dir = lock(&rt.shared).data_dir.clone();
        assert_eq!(token_from(&data_dir).as_deref(), Some(rotated.as_str()));
        let settings = load_settings(&data_dir);
        assert_eq!(
            settings["gsi"]["config_sha256"],
            sha256_hex(replaced.as_bytes())
        );
        assert_eq!(settings["gsi"]["port"], 3005);
        assert!(!serde_json::to_string(&settings).unwrap().contains(&rotated));
        // The other application's file is untouched.
        assert!(
            fs::read_to_string(gdir.join("gamestate_integration_other.cfg"))
                .unwrap()
                .contains("3000")
        );
    }

    #[test]
    fn failed_setup_reports_its_reason_as_unavailable() {
        let rt = Runtime::new(temp_dir("setup-fail"));
        assert_eq!(
            rt.setup_with(|| Err("GSI_GAME_NOT_FOUND"), false),
            Err("GSI_GAME_NOT_FOUND")
        );
        let s = lock(&rt.shared);
        assert_eq!(s.state, IntakeState::Unavailable("GSI_GAME_NOT_FOUND"));
        assert_eq!(s.port, None);
    }

    fn http(port: u16, request: &str) -> u16 {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        line.split_whitespace().nth(1).unwrap().parse().unwrap()
    }

    fn post(body: &str, extra_headers: &str) -> String {
        format!(
            "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn stalled_body_is_cut_off_and_later_request_is_admitted() {
        let shared = shared_with_token();
        let tok = shared.token.clone().unwrap();
        let handle: Handle = Arc::new(Mutex::new(shared));
        let server = Arc::new(bind_server("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let worker = {
            let (handle, server) = (handle.clone(), server.clone());
            std::thread::spawn(move || serve(handle, server))
        };
        let mut stalled = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stalled
            .set_read_timeout(Some(Duration::from_secs(6)))
            .unwrap();
        stalled.write_all(b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{").unwrap();
        // Give the server time to enter the partial body read before the next request.
        std::thread::sleep(Duration::from_millis(200));
        let good = post(&menu(&tok).to_string(), "");
        let (tx, rx) = std::sync::mpsc::channel();
        let client = std::thread::spawn(move || {
            let _ = tx.send(http(port, &good));
        });
        let mut response = Vec::new();
        let cutoff = stalled.read_to_end(&mut response);
        let later = rx.recv_timeout(Duration::from_secs(2));
        // Always release the stalled connection and join both threads before asserting.
        drop(stalled);
        let _ = client.join();
        server.unblock();
        worker.join().unwrap();
        assert!(
            cutoff.is_ok(),
            "stalled body must be cut off within six seconds: {cutoff:?}"
        );
        assert_eq!(
            later.ok(),
            Some(200),
            "later authenticated request must be admitted while the stalled client remains open"
        );
        assert_eq!(
            lock(&handle).sequence,
            1,
            "only the complete request advances the sequence"
        );
    }

    #[test]
    fn loopback_server_refuses_before_parsing_and_admits_only_the_token() {
        let shared = shared_with_token();
        let tok = shared.token.clone().unwrap();
        let handle: Handle = Arc::new(Mutex::new(shared));
        let server = Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let worker = {
            let (handle, server) = (handle.clone(), server.clone());
            std::thread::spawn(move || serve(handle, server))
        };
        let good = menu(&tok).to_string();

        assert_eq!(
            http(
                port,
                "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
            ),
            404
        );
        assert_eq!(
            http(port, &post(&good, "").replace("POST / ", "POST /other ")),
            404
        );
        // Web content always sends Origin on a cross-origin POST: refused unread,
        // even with the right token.
        assert_eq!(
            http(port, &post(&good, "Origin: https://example.invalid\r\n")),
            403
        );
        assert_eq!(
            http(
                port,
                "POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 131073\r\nConnection: close\r\n\r\n"
            ),
            413
        );
        assert_eq!(http(port, &post("{\"a\":1,\"a\":2}", "")), 400);
        assert_eq!(http(port, &post(&menu("wrong").to_string(), "")), 401);
        assert!(
            lock(&handle).last_admitted.is_none(),
            "nothing admitted so far"
        );
        assert_eq!(http(port, &post(&good, "")), 200);
        assert!(lock(&handle).last_age_ms().is_some());

        server.unblock();
        worker.join().unwrap();
    }

    #[test]
    fn idle_listener_survives_longer_than_read_timeout() {
        let shared = shared_with_token();
        let tok = shared.token.clone().unwrap();
        let handle: Handle = Arc::new(Mutex::new(shared));
        let server = Arc::new(bind_server(("127.0.0.1", 0)).unwrap());
        let addr = server.server_addr().to_ip().unwrap();
        let worker = {
            let (handle, server) = (handle.clone(), server.clone());
            std::thread::spawn(move || serve(handle, server))
        };
        std::thread::sleep(Duration::from_secs(6));
        let running_after_idle = !worker.is_finished();
        let started = Instant::now();
        let response = (|| -> io::Result<String> {
            let mut stream = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            stream.write_all(post(&menu(&tok).to_string(), "").as_bytes())?;
            let mut line = String::new();
            BufReader::new(stream).read_line(&mut line)?;
            Ok(line)
        })();
        let elapsed = started.elapsed();
        let running_after_request = !worker.is_finished();
        server.unblock();
        worker.join().unwrap();
        assert!(running_after_idle, "serve thread exited while idle");
        assert!(running_after_request, "serve thread exited after request");
        let line = response.expect("menu POST must receive a response within two seconds");
        assert!(
            elapsed <= Duration::from_secs(2),
            "menu POST took {elapsed:?}"
        );
        assert_eq!(line.split_whitespace().nth(1), Some("200"));
    }

    #[test]
    fn resumed_session_does_not_replace_established_hero() {
        let key = [7; 32];
        let mut previous = SessionTracker::new(key);
        assert_eq!(
            previous.observe(Some("8000000001"), owner(), || "session".into()),
            SessionChange::Started { epoch: 1 }
        );
        let persisted = previous.persisted();
        let epoch = persisted.current_session.as_ref().unwrap().epoch;
        let mut s = shared_with_token();
        s.sessions = SessionTracker::restore(key, persisted);
        let tok = s.token.clone().unwrap();
        let mut body = in_game(&tok);
        body["map"].as_object_mut().unwrap().remove("matchid");
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(s.ownership, Some(Ownership::Verified));
        assert_eq!(s.session_hero_id.as_deref(), Some("npc_dota_hero_axe"));

        body["map"]["matchid"] = json!("8000000001");
        body["hero"]["name"] = json!("npc_dota_hero_lina");
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(
            s.epoch,
            Some(epoch),
            "resuming must retain the persisted epoch"
        );
        assert_eq!(s.sessions.current().unwrap().epoch, epoch);
        assert_eq!(
            s.ownership,
            Some(Ownership::Mismatch),
            "a resumed session must compare an already established hero"
        );
        assert!(
            s.snapshot()["own"].is_null(),
            "mismatched values must be withdrawn"
        );
    }

    #[test]
    fn menu_clears_hero_baseline_so_new_game_without_match_id_is_not_mismatch() {
        let mut s = shared_with_token();
        let tok = s.token.clone().unwrap();
        let mut body = in_game(&tok);
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(s.ownership, Some(Ownership::Verified));
        assert_eq!(s.accept(&menu(&tok), Instant::now(), Some(owner())), 200);
        body["map"].as_object_mut().unwrap().remove("matchid");
        body["hero"]["name"] = json!("npc_dota_hero_lina");
        assert_eq!(s.accept(&body, Instant::now(), Some(owner())), 200);
        assert_eq!(
            s.ownership,
            Some(Ownership::Verified),
            "menu ends the previous game's hero baseline"
        );
        assert_eq!(s.snapshot()["own"]["fields"]["hp"]["value"], 850);
    }
}

#[cfg(test)]
mod s0_6_tests {
    use super::*;
    use crate::{containment::Sample, status};

    fn fresh_runtime() -> Arc<Runtime> {
        let dir = std::env::temp_dir().join(format!(
            "kohbor-s0-6-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).expect("create a new empty data directory");
        Runtime::new(dir)
    }

    #[test]
    fn cold_start_status_preserves_unknowns_and_matches_schema() {
        let runtime = fresh_runtime();
        runtime.start();
        let shared = lock(&runtime.shared);
        let sample = Sample {
            cpu_percent: None,
            working_set_bytes: None,
            process_count: None,
        };
        let value = status::build(&shared, &sample, true, 1);
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../contracts/runtime-status.schema.json"
        ))
        .unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let errors: Vec<_> = validator
            .iter_errors(&value)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "status schema errors: {errors:?}; {value:#}"
        );
        assert_eq!(value["intake"]["state"], "not_configured");
        assert!(value["intake"]["port"].is_null());
        assert!(value["intake"]["last_admitted_age_ms"].is_null());
        assert_eq!(value["ownership"]["state"], "unknown");
        assert_eq!(value["session"]["phase"], "none");
        assert!(value["session"]["session_epoch"].is_null());
        assert_eq!(value["mode"]["state"], "unverified");
        assert!(value["mode"]["value"].is_null());
        assert_eq!(value["resource"]["cpu"]["state"], "unavailable");
        assert!(value["resource"]["cpu"]["last_sample_percent"].is_null());
        assert_eq!(value["resource"]["memory"]["state"], "unavailable");
        assert!(value["resource"]["memory"]["working_set_bytes"].is_null());
        assert_eq!(shared.snapshot()["availability"], "unknown");
    }

    #[test]
    fn cold_start_without_setup_binds_no_listener() {
        // GSI-INTAKE-SPEC 2.4: the port is chosen during first setup, so an empty
        // data directory must not open any socket and must report not_configured.
        let runtime = fresh_runtime();
        runtime.start();
        assert!(
            lock(&runtime.server).is_none(),
            "cold startup opened a listener before setup"
        );
        assert!(lock(&runtime.shared).port.is_none());
    }

    #[test]
    fn startup_source_and_direct_dependencies_have_a_bounded_network_surface() {
        let manifest = include_str!("../Cargo.toml");
        let allowed = [
            "kohbor-gsi",
            "tauri",
            "serde",
            "serde_json",
            "getrandom",
            "base64",
            "sha2",
            "tiny_http",
            "socket2",
            "winreg",
            "windows",
        ];
        let mut dependencies = false;
        for line in manifest.lines().map(str::trim) {
            if line.starts_with('[') {
                dependencies = line == "[dependencies]"
                    || (line.starts_with("[target.") && line.ends_with(".dependencies]"));
            } else if dependencies
                && !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with('"')
                && !line.starts_with(']')
            {
                if let Some((name, _)) = line.split_once('=') {
                    assert!(
                        allowed.contains(&name.trim()),
                        "review new host dependency: {name}"
                    );
                }
            }
        }
        let intake = include_str!("intake.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for source in [intake, include_str!("lib.rs"), include_str!("main.rs")] {
            for forbidden in [
                "TcpStream::connect",
                "UdpSocket::bind",
                "UdpSocket::connect",
                "reqwest::",
                "ureq::",
                "hyper::",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "review startup network operation: {forbidden}"
                );
            }
        }
    }
}
