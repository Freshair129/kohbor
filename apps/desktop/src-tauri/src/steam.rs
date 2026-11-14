use crate::{capture_host, intake};
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_LOGINUSERS_BYTES: usize = 256 * 1024;
const MAX_VDF_DEPTH: usize = 32;
const MAX_PERSONA_CHARS: usize = 64;
const STEAMID64_BASE: u64 = 76_561_197_960_265_728;
const LAUNCH_COOLDOWN: Duration = Duration::from_secs(20);
const STATUS_INTERVAL: Duration = Duration::from_secs(3);
const STEAM_GAME_URL: &str = "steam://rungameid/570";

static STATUS_THREAD_STARTED: AtomicBool = AtomicBool::new(false);
static LAUNCH_PENDING_UNTIL: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static CURRENT_STATUS: OnceLock<Mutex<SteamStatus>> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn pending_slot() -> &'static Mutex<Option<Instant>> {
    LAUNCH_PENDING_UNTIL.get_or_init(|| Mutex::new(None))
}

fn status_slot() -> &'static Mutex<SteamStatus> {
    CURRENT_STATUS.get_or_init(|| Mutex::new(SteamStatus::unknown()))
}

fn launch_pending_at(now: Instant) -> bool {
    let mut pending = lock(pending_slot());
    if pending.is_some_and(|until| now >= until) {
        *pending = None;
    }
    pending.is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LaunchDecision {
    BlockedLiveMatch,
    AlreadyRunning,
    Cooldown,
    SteamNotFound,
    ProcessStatusUnknown,
    OpenShell,
}

fn launch_decision(
    steam_running: Option<bool>,
    dota_running: Option<bool>,
    game_phase: GamePhase,
    pending_until: Option<Instant>,
    steam_installed: bool,
    now: Instant,
) -> LaunchDecision {
    if dota_running.is_none() {
        return LaunchDecision::ProcessStatusUnknown;
    }
    if matches!(game_phase, GamePhase::Pregame | GamePhase::Live) {
        return LaunchDecision::BlockedLiveMatch;
    }
    if dota_running == Some(true) {
        return LaunchDecision::AlreadyRunning;
    }
    if pending_until.is_some_and(|until| now < until) {
        return LaunchDecision::Cooldown;
    }
    if steam_running == Some(false) && !steam_installed {
        return LaunchDecision::SteamNotFound;
    }
    LaunchDecision::OpenShell
}

enum VdfToken {
    Text(String),
    Open,
    Close,
}

impl std::fmt::Debug for VdfToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Text(_) => "Text",
            Self::Open => "Open",
            Self::Close => "Close",
        })
    }
}

fn tokenize_vdf(input: &str) -> Option<Vec<VdfToken>> {
    if input.len() > MAX_LOGINUSERS_BYTES {
        return None;
    }
    let mut chars = input.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for rest in chars.by_ref() {
                if rest == '\n' {
                    break;
                }
            }
            continue;
        }
        match ch {
            '{' => tokens.push(VdfToken::Open),
            '}' => tokens.push(VdfToken::Close),
            '"' => {
                let mut value = String::new();
                let mut closed = false;
                while let Some(next) = chars.next() {
                    match next {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' => {
                            let escaped = chars.next()?;
                            value.push(match escaped {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                other => other,
                            });
                        }
                        other => value.push(other),
                    }
                }
                if !closed {
                    return None;
                }
                tokens.push(VdfToken::Text(value));
            }
            other => {
                let mut value = String::from(other);
                while let Some(next) = chars.peek().copied() {
                    if next == '"' {
                        return None;
                    }
                    if next.is_whitespace() || matches!(next, '{' | '}') {
                        break;
                    }
                    if next == '/' {
                        let mut lookahead = chars.clone();
                        lookahead.next();
                        if lookahead.peek() == Some(&'/') {
                            break;
                        }
                    }
                    value.push(chars.next()?);
                }
                tokens.push(VdfToken::Text(value));
            }
        }
    }
    Some(tokens)
}

/// Validate all key/value pairs and braces before any selected value can leave
/// the parser. A complete early user block does not excuse a malformed tail.
fn vdf_structure_valid(tokens: &[VdfToken]) -> bool {
    fn object(tokens: &[VdfToken], cursor: &mut usize, depth: usize, nested: bool) -> bool {
        if depth > MAX_VDF_DEPTH {
            return false;
        }
        loop {
            match tokens.get(*cursor) {
                None => return !nested,
                Some(VdfToken::Close) => {
                    if nested {
                        *cursor += 1;
                        return true;
                    }
                    return false;
                }
                Some(VdfToken::Open) => return false,
                Some(VdfToken::Text(_)) => *cursor += 1,
            }
            match tokens.get(*cursor) {
                Some(VdfToken::Text(_)) => *cursor += 1,
                Some(VdfToken::Open) => {
                    *cursor += 1;
                    if !object(tokens, cursor, depth + 1, true) {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }
    let mut cursor = 0;
    object(tokens, &mut cursor, 0, false) && cursor == tokens.len()
}

fn persona_in_block(
    tokens: &[VdfToken],
    cursor: &mut usize,
    depth: usize,
    target_id: &str,
    in_users: bool,
    in_target: bool,
) -> Option<String> {
    if depth > MAX_VDF_DEPTH {
        return None;
    }
    loop {
        let key = match tokens.get(*cursor)? {
            VdfToken::Text(key) => key.as_str(),
            VdfToken::Close => {
                *cursor += 1;
                return None;
            }
            VdfToken::Open => return None,
        };
        *cursor += 1;
        match tokens.get(*cursor)? {
            VdfToken::Text(value) => {
                if in_target && key.eq_ignore_ascii_case("PersonaName") {
                    if let Some(name) = sanitize_persona_name(value) {
                        return Some(name);
                    }
                }
                *cursor += 1;
            }
            VdfToken::Open => {
                *cursor += 1;
                let child_in_users = in_users || key.eq_ignore_ascii_case("users");
                let child_in_target = in_users && key == target_id;
                if let Some(name) = persona_in_block(
                    tokens,
                    cursor,
                    depth + 1,
                    target_id,
                    child_in_users,
                    child_in_target,
                ) {
                    return Some(name);
                }
            }
            VdfToken::Close => return None,
        }
    }
}

fn sanitize_persona_name(value: &str) -> Option<String> {
    let name: String = value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_PERSONA_CHARS)
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Parse the local login-users file and return only the persona name. The
/// account ID and its derived SteamID64 stay inside this module.
fn parse_loginusers_vdf(input: &str, account_id: u32) -> Option<String> {
    let account_id_text = account_id.to_string();
    crate::logging::register_secret(&account_id_text);
    let tokens = tokenize_vdf(input)?;
    if !vdf_structure_valid(&tokens) {
        return None;
    }
    let target_id = (STEAMID64_BASE + u64::from(account_id)).to_string();
    crate::logging::register_secret(&target_id);
    // Display text is untrusted too: keep a platform identifier out of status
    // even when the user has embedded it in their local persona name.
    let public_persona = |name: String| {
        (!name.contains(&account_id_text)
            && !name.contains(&target_id)
            && !name
                .as_bytes()
                .windows(17)
                .any(|digits| digits.iter().all(u8::is_ascii_digit)))
        .then_some(name)
    };
    let mut cursor = 0;
    while cursor < tokens.len() {
        match tokens.get(cursor)? {
            VdfToken::Text(key) => {
                cursor += 1;
                if matches!(tokens.get(cursor), Some(VdfToken::Open)) {
                    cursor += 1;
                    if let Some(name) = persona_in_block(
                        &tokens,
                        &mut cursor,
                        1,
                        &target_id,
                        key.eq_ignore_ascii_case("users"),
                        false,
                    ) {
                        return public_persona(name);
                    }
                } else {
                    cursor = cursor.saturating_add(1);
                }
            }
            VdfToken::Open => {
                cursor += 1;
                if let Some(name) =
                    persona_in_block(&tokens, &mut cursor, 1, &target_id, false, false)
                {
                    return public_persona(name);
                }
            }
            VdfToken::Close => cursor += 1,
        }
    }
    None
}

/// States shared with the renderer's pure minimap placeholder function:
/// not running + pending → Launching; not running → NotRunning; draft →
/// HeroSelection; no/unknown phase → Loading; unsupported or unrecognized
/// capture → Unsupported; stale + occluded → Occluded; pregame/live without a
/// frame or while calibrating → FirstFrame; validated live capture → Live.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum MinimapPlaceholder {
    NotRunning,
    Launching,
    HeroSelection,
    Loading,
    FirstFrame,
    Live,
    Occluded,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum GamePhase {
    None,
    Draft,
    Pregame,
    Live,
    PostGame,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CapturePhase {
    Idle,
    Calibrating,
    Live,
    Stale,
    Unsupported,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum GsiState {
    Receiving,
    Waiting,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SteamStatus {
    steam_running: Option<bool>,
    dota_running: Option<bool>,
    persona_name: Option<String>,
    gsi: GsiState,
    game_phase: GamePhase,
    launch_pending: bool,
    minimap_state: MinimapPlaceholder,
}

impl SteamStatus {
    fn unknown() -> Self {
        Self {
            steam_running: None,
            dota_running: None,
            persona_name: None,
            gsi: GsiState::Unknown,
            game_phase: GamePhase::Unknown,
            launch_pending: false,
            minimap_state: MinimapPlaceholder::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum LaunchResultKind {
    Launched,
    SteamNotFound,
    AlreadyRunning,
    BlockedLiveMatch,
    Cooldown,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct LaunchResult {
    result: LaunchResultKind,
    detail: Option<String>,
}

fn minimap_placeholder(
    dota_running: Option<bool>,
    launch_pending: bool,
    game_phase: GamePhase,
    capture_phase: CapturePhase,
    capture_reason: Option<&str>,
    frame_age_ms: Option<u64>,
) -> MinimapPlaceholder {
    match dota_running {
        Some(false) if launch_pending => return MinimapPlaceholder::Launching,
        Some(false) => return MinimapPlaceholder::NotRunning,
        None => return MinimapPlaceholder::Unsupported,
        Some(true) => {}
    }
    if game_phase == GamePhase::Draft {
        return MinimapPlaceholder::HeroSelection;
    }
    if matches!(game_phase, GamePhase::None | GamePhase::Unknown) {
        return MinimapPlaceholder::Loading;
    }
    if capture_phase == CapturePhase::Unsupported {
        return MinimapPlaceholder::Unsupported;
    }
    if capture_phase == CapturePhase::Stale && capture_reason == Some("occluded") {
        return MinimapPlaceholder::Occluded;
    }
    let no_frame_yet = frame_age_ms.is_none()
        || (capture_phase == CapturePhase::Idle && capture_reason == Some("waiting_for_game"));
    if matches!(game_phase, GamePhase::Pregame | GamePhase::Live)
        && (capture_phase == CapturePhase::Calibrating || no_frame_yet)
    {
        return MinimapPlaceholder::FirstFrame;
    }
    if capture_phase == CapturePhase::Live {
        return MinimapPlaceholder::Live;
    }
    MinimapPlaceholder::Unsupported
}

fn phase_from_status(value: &Value) -> GamePhase {
    match value["session"]["phase"].as_str() {
        Some("none") => GamePhase::None,
        Some("draft") => GamePhase::Draft,
        Some("pregame") => GamePhase::Pregame,
        Some("live") => GamePhase::Live,
        Some("post_game") => GamePhase::PostGame,
        _ => GamePhase::Unknown,
    }
}

fn gsi_from_status(value: &Value) -> GsiState {
    match value["intake"]["state"].as_str() {
        Some("receiving") => GsiState::Receiving,
        Some("listening" | "stale") => GsiState::Waiting,
        _ => GsiState::Unknown,
    }
}

fn intake_status(runtime: &intake::Runtime) -> (GsiState, GamePhase) {
    let shared = intake::lock(&runtime.shared);
    let sample = crate::containment::Sample {
        cpu_percent: None,
        working_set_bytes: None,
        process_count: None,
    };
    let value = crate::status::build(&shared, &sample, true, 1, ("unavailable", None));
    (gsi_from_status(&value), phase_from_status(&value))
}

fn capture_phase(value: &Value) -> CapturePhase {
    match value["phase"].as_str() {
        Some("idle") => CapturePhase::Idle,
        Some("calibrating") => CapturePhase::Calibrating,
        Some("live") => CapturePhase::Live,
        Some("stale") => CapturePhase::Stale,
        Some("unsupported" | "coverage_unavailable" | "lost") => CapturePhase::Unsupported,
        _ => CapturePhase::Other,
    }
}

fn status_from_inputs(
    steam_running: Option<bool>,
    dota_running: Option<bool>,
    persona_name: Option<String>,
    gsi: GsiState,
    game_phase: GamePhase,
    launch_pending: bool,
    capture: &Value,
) -> SteamStatus {
    let capture_phase = capture_phase(capture);
    let capture_reason = capture["phase_reason"].as_str();
    let frame_age_ms = capture["frame_age_ms"].as_u64();
    let minimap_state = minimap_placeholder(
        dota_running,
        launch_pending,
        game_phase,
        capture_phase,
        capture_reason,
        frame_age_ms,
    );
    SteamStatus {
        steam_running,
        dota_running,
        persona_name,
        gsi,
        game_phase,
        launch_pending,
        minimap_state,
    }
}

fn latest_status() -> SteamStatus {
    let mut status = lock(status_slot()).clone();
    status.launch_pending = launch_pending_at(Instant::now());
    if status.dota_running == Some(false) {
        status.minimap_state = if status.launch_pending {
            MinimapPlaceholder::Launching
        } else {
            MinimapPlaceholder::NotRunning
        };
    }
    status
}

#[tauri::command]
pub(crate) fn steam_status() -> SteamStatus {
    latest_status()
}

#[tauri::command(async)]
pub(crate) fn launch_dota(state: tauri::State<'_, crate::AppState>) -> LaunchResult {
    let (_, game_phase) = intake_status(&state.runtime);
    let (steam_running, dota_running) = process_flags();
    let now = Instant::now();
    let mut pending_until = lock(pending_slot());
    let decision = launch_decision(
        steam_running,
        dota_running,
        game_phase,
        *pending_until,
        steam_install_key_exists(),
        now,
    );
    finish_launch(decision, &mut pending_until, now, open_steam_url)
}

fn finish_launch(
    decision: LaunchDecision,
    pending_until: &mut Option<Instant>,
    now: Instant,
    open: impl FnOnce() -> bool,
) -> LaunchResult {
    match decision {
        LaunchDecision::BlockedLiveMatch => LaunchResult {
            result: LaunchResultKind::BlockedLiveMatch,
            detail: None,
        },
        LaunchDecision::AlreadyRunning => LaunchResult {
            result: LaunchResultKind::AlreadyRunning,
            detail: None,
        },
        LaunchDecision::Cooldown => LaunchResult {
            result: LaunchResultKind::Cooldown,
            detail: None,
        },
        LaunchDecision::SteamNotFound => LaunchResult {
            result: LaunchResultKind::SteamNotFound,
            detail: None,
        },
        LaunchDecision::ProcessStatusUnknown => LaunchResult {
            result: LaunchResultKind::Failed,
            detail: Some("process_check_failed".to_owned()),
        },
        LaunchDecision::OpenShell if open() => {
            *pending_until = Some(now + LAUNCH_COOLDOWN);
            LaunchResult {
                result: LaunchResultKind::Launched,
                detail: None,
            }
        }
        LaunchDecision::OpenShell => LaunchResult {
            result: LaunchResultKind::Failed,
            detail: Some("SHELL_OPEN_FAILED".to_owned()),
        },
    }
}

pub(crate) fn start_status_probe(app: tauri::AppHandle, runtime: Arc<intake::Runtime>) {
    if STATUS_THREAD_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Err(_error) = std::thread::Builder::new()
        .name("steam-status".into())
        .spawn(move || {
            if !set_probe_thread_priority() {
                STATUS_THREAD_STARTED.store(false, Ordering::Release);
                crate::log_warn!(
                    "steam",
                    "status_priority_unavailable",
                    "Steam status probe could not lower its thread priority"
                );
                return;
            }
            let mut previous = None;
            loop {
                let (steam_running, dota_running) = process_flags();
                let (gsi, game_phase) = intake_status(&runtime);
                let current = status_from_inputs(
                    steam_running,
                    dota_running,
                    read_persona_name(),
                    gsi,
                    game_phase,
                    launch_pending_at(Instant::now()),
                    &capture_host::capture_status(),
                );
                if previous.as_ref() != Some(&current) {
                    *lock(status_slot()) = current.clone();
                    if tauri::Emitter::emit(&app, "steam://status", current.clone()).is_err() {
                        crate::log_warn!(
                            "steam",
                            "status_emit_failed",
                            "Steam status event could not be emitted"
                        );
                    }
                    previous = Some(current);
                }
                std::thread::sleep(STATUS_INTERVAL);
            }
        })
    {
        STATUS_THREAD_STARTED.store(false, Ordering::Release);
        crate::log_warn!(
            "steam",
            "status_probe_unavailable",
            "Steam status probe thread could not start"
        );
    }
}

#[cfg(windows)]
fn set_probe_thread_priority() -> bool {
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
    };
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL).is_ok() }
}

#[cfg(not(windows))]
fn set_probe_thread_priority() -> bool {
    true
}

#[cfg(windows)]
fn process_flags() -> (Option<bool>, Option<bool>) {
    use windows::core::HRESULT;
    use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return (None, None);
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut first = true;
        let flags = process_flags_from(|| {
            let result = if std::mem::take(&mut first) {
                Process32FirstW(snapshot, &mut entry)
            } else {
                Process32NextW(snapshot, &mut entry)
            };
            match result {
                Ok(()) => {}
                Err(error) if error.code() == HRESULT::from_win32(ERROR_NO_MORE_FILES.0) => {
                    return Ok(None)
                }
                Err(_) => return Err(()),
            }
            let len = entry
                .szExeFile
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExeFile.len());
            Ok(Some(String::from_utf16_lossy(&entry.szExeFile[..len])))
        });
        let _ = CloseHandle(snapshot);
        flags
    }
}

#[cfg(any(windows, test))]
fn process_flags_from(
    mut next: impl FnMut() -> Result<Option<String>, ()>,
) -> (Option<bool>, Option<bool>) {
    let (mut steam, mut dota) = (false, false);
    loop {
        match next() {
            Ok(Some(name)) => {
                steam |= name.eq_ignore_ascii_case("steam.exe");
                dota |= name.eq_ignore_ascii_case("dota2.exe");
            }
            Ok(None) => return (Some(steam), Some(dota)),
            Err(()) => return (None, None),
        }
    }
}

#[cfg(not(windows))]
fn process_flags() -> (Option<bool>, Option<bool>) {
    (None, None)
}

#[cfg(windows)]
fn steam_registry_info() -> (bool, Option<std::path::PathBuf>) {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey(r"Software\Valve\Steam") else {
        return (false, None);
    };
    let path = key
        .get_value::<String, _>("SteamPath")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from);
    (true, path)
}

#[cfg(not(windows))]
fn steam_registry_info() -> (bool, Option<std::path::PathBuf>) {
    (false, None)
}

fn steam_install_key_exists() -> bool {
    steam_registry_info().0
}

#[cfg(windows)]
fn active_user_account_id() -> Option<u32> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Valve\Steam\ActiveProcess")
        .ok()?;
    let account_id: u32 = key.get_value("ActiveUser").ok()?;
    crate::logging::register_secret(&account_id.to_string());
    (account_id != 0).then_some(account_id)
}

#[cfg(not(windows))]
fn active_user_account_id() -> Option<u32> {
    None
}

fn read_persona_name() -> Option<String> {
    let account_id = active_user_account_id()?;
    let (_, install_path) = steam_registry_info();
    let file = File::open(install_path?.join("config/loginusers.vdf")).ok()?;
    let mut bytes = Vec::new();
    file.take((MAX_LOGINUSERS_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > MAX_LOGINUSERS_BYTES {
        return None;
    }
    parse_loginusers_vdf(std::str::from_utf8(&bytes).ok()?, account_id)
}

#[cfg(windows)]
fn open_steam_url() -> bool {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let url: Vec<u16> = STEAM_GAME_URL
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
        .0 as isize
            > 32
    }
}

#[cfg(not(windows))]
fn open_steam_url() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::{
        launch_decision, minimap_placeholder, parse_loginusers_vdf, CapturePhase, GamePhase,
        LaunchDecision, LaunchResult, LaunchResultKind, MinimapPlaceholder, SteamStatus,
        LAUNCH_COOLDOWN,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn debug_of_tokens_and_parse_results_never_contains_platform_identifier_runs() {
        fn has_seventeen_digits(text: &str) -> bool {
            text.as_bytes()
                .windows(17)
                .any(|window| window.iter().all(u8::is_ascii_digit))
        }
        let raw = "76561198000000001";
        let token_debug = format!("{:?}", super::VdfToken::Text(raw.to_owned()));
        assert!(!has_seventeen_digits(&token_debug));
        assert!(
            !format!("{:?}", super::VdfToken::Text("private-account-name".into()))
                .contains("private-account-name")
        );
        let account_id = 123_456_789_u32;
        let steam_id = (76_561_197_960_265_728_u64 + u64::from(account_id)).to_string();
        for name in ["Player", "Player 99999999999999999"] {
            let vdf = format!("\"users\" {{ \"{steam_id}\" {{ \"PersonaName\" \"{name}\" }} }}");
            let parsed = parse_loginusers_vdf(&vdf, account_id);
            assert!(!has_seventeen_digits(&format!("{parsed:?}")));
            if name != "Player" {
                assert!(parsed.is_none());
            }
        }
    }

    #[test]
    fn vdf_requires_the_entire_structure_and_user_block_to_be_closed() {
        let account_id = 123_456_789_u32;
        let steam_id = (76_561_197_960_265_728_u64 + u64::from(account_id)).to_string();
        let valid = format!("\"users\" {{ \"{steam_id}\" {{ \"PersonaName\" \"Player\" }} }}");
        for end in 0..valid.len() {
            assert!(
                parse_loginusers_vdf(&valid[..end], account_id).is_none(),
                "accepted truncated VDF at byte {end}"
            );
        }
        for malformed in [
            format!("{valid} }}"),
            format!("{valid} \"orphan\""),
            format!("{valid} \"other\" {{"),
            format!("{valid} {{ }}"),
            format!("{valid} bad\"key value"),
        ] {
            assert!(parse_loginusers_vdf(&malformed, account_id).is_none());
        }
        assert_eq!(
            parse_loginusers_vdf(&valid, account_id).as_deref(),
            Some("Player")
        );
    }

    #[test]
    fn process_enumerator_returns_unknown_for_first_or_midway_errors() {
        for names in [
            vec![Err(())],
            vec![Ok(Some("steam.exe".to_owned())), Err(())],
            vec![
                Ok(Some("steam.exe".to_owned())),
                Ok(Some("dota2.exe".to_owned())),
                Err(()),
            ],
        ] {
            let mut fake = names.into_iter();
            assert_eq!(
                super::process_flags_from(|| fake.next().unwrap_or(Ok(None))),
                (None, None)
            );
        }
        let mut normal = [Ok(Some("steam.exe".to_owned())), Ok(None)].into_iter();
        assert_eq!(
            super::process_flags_from(|| normal.next().unwrap()),
            (Some(true), Some(false))
        );
        assert_eq!(
            super::process_flags_from(|| Ok(None)),
            (Some(false), Some(false))
        );
    }

    #[test]
    fn failed_process_check_never_opens_the_shell_and_has_the_required_detail() {
        let mut fake = [Ok(Some("steam.exe".to_owned())), Err(())].into_iter();
        let (steam, dota) = super::process_flags_from(|| fake.next().unwrap_or(Ok(None)));
        let now = Instant::now();
        for phase in [GamePhase::None, GamePhase::Live] {
            let mut pending = Some(now + LAUNCH_COOLDOWN);
            let decision = launch_decision(steam, dota, phase, pending, true, now);
            let mut opened = false;
            let result = super::finish_launch(decision, &mut pending, now, || {
                opened = true;
                true
            });
            assert_eq!(result.result, LaunchResultKind::Failed);
            assert_eq!(result.detail.as_deref(), Some("process_check_failed"));
            assert!(!opened);
        }
    }

    fn derive(
        dota_running: Option<bool>,
        launch_pending: bool,
        game_phase: GamePhase,
        capture_phase: CapturePhase,
        capture_reason: Option<&str>,
        frame_age_ms: Option<u64>,
    ) -> MinimapPlaceholder {
        minimap_placeholder(
            dota_running,
            launch_pending,
            game_phase,
            capture_phase,
            capture_reason,
            frame_age_ms,
        )
    }

    #[test]
    fn minimap_placeholder_covers_status_transitions_and_precedence() {
        let cases = [
            (
                Some(false),
                true,
                GamePhase::Unknown,
                CapturePhase::Idle,
                None,
                None,
                MinimapPlaceholder::Launching,
            ),
            (
                Some(false),
                false,
                GamePhase::Unknown,
                CapturePhase::Live,
                None,
                Some(10),
                MinimapPlaceholder::NotRunning,
            ),
            (
                Some(true),
                false,
                GamePhase::Draft,
                CapturePhase::Unsupported,
                Some("capture_disabled"),
                None,
                MinimapPlaceholder::HeroSelection,
            ),
            (
                Some(true),
                false,
                GamePhase::None,
                CapturePhase::Unsupported,
                Some("capture_unavailable"),
                None,
                MinimapPlaceholder::Loading,
            ),
            (
                Some(true),
                false,
                GamePhase::Unknown,
                CapturePhase::Stale,
                Some("occluded"),
                None,
                MinimapPlaceholder::Loading,
            ),
            (
                Some(true),
                false,
                GamePhase::Pregame,
                CapturePhase::Unsupported,
                Some("unsupported_resolution"),
                None,
                MinimapPlaceholder::Unsupported,
            ),
            (
                Some(true),
                false,
                GamePhase::Live,
                CapturePhase::Stale,
                Some("occluded"),
                None,
                MinimapPlaceholder::Occluded,
            ),
            (
                Some(true),
                false,
                GamePhase::Pregame,
                CapturePhase::Calibrating,
                Some("calibrating"),
                None,
                MinimapPlaceholder::FirstFrame,
            ),
            (
                Some(true),
                false,
                GamePhase::Live,
                CapturePhase::Idle,
                Some("waiting_for_game"),
                None,
                MinimapPlaceholder::FirstFrame,
            ),
            (
                Some(true),
                false,
                GamePhase::Live,
                CapturePhase::Live,
                None,
                Some(33),
                MinimapPlaceholder::Live,
            ),
            (
                Some(true),
                false,
                GamePhase::PostGame,
                CapturePhase::Live,
                None,
                Some(33),
                MinimapPlaceholder::Live,
            ),
            (
                Some(true),
                false,
                GamePhase::Pregame,
                CapturePhase::Other,
                Some("future_phase"),
                Some(500),
                MinimapPlaceholder::Unsupported,
            ),
            (
                None,
                false,
                GamePhase::Unknown,
                CapturePhase::Idle,
                Some("waiting_for_game"),
                None,
                MinimapPlaceholder::Unsupported,
            ),
        ];

        for (dota, pending, phase, capture, reason, age, expected) in cases {
            assert_eq!(
                derive(dota, pending, phase, capture, reason, age),
                expected,
                "dota={dota:?}, pending={pending}, phase={phase:?}, capture={capture:?}, reason={reason:?}, age={age:?}"
            );
        }
    }

    #[test]
    fn launch_decision_blocks_live_pending_and_missing_steam_only() {
        let now = Instant::now();
        let cases = [
            (
                Some(false),
                Some(false),
                GamePhase::Live,
                false,
                false,
                LaunchDecision::BlockedLiveMatch,
            ),
            (
                Some(false),
                Some(false),
                GamePhase::Pregame,
                false,
                false,
                LaunchDecision::BlockedLiveMatch,
            ),
            (
                Some(true),
                Some(true),
                GamePhase::None,
                false,
                false,
                LaunchDecision::AlreadyRunning,
            ),
            (
                Some(false),
                Some(false),
                GamePhase::None,
                true,
                true,
                LaunchDecision::Cooldown,
            ),
            (
                Some(false),
                Some(false),
                GamePhase::None,
                false,
                false,
                LaunchDecision::SteamNotFound,
            ),
            (
                Some(false),
                Some(false),
                GamePhase::None,
                false,
                true,
                LaunchDecision::OpenShell,
            ),
            (
                None,
                Some(false),
                GamePhase::None,
                false,
                false,
                LaunchDecision::OpenShell,
            ),
            (
                None,
                None,
                GamePhase::None,
                false,
                false,
                LaunchDecision::ProcessStatusUnknown,
            ),
        ];
        for (steam_running, dota_running, phase, pending, installed, expected) in cases {
            assert_eq!(
                launch_decision(
                    steam_running,
                    dota_running,
                    phase,
                    pending.then_some(now + LAUNCH_COOLDOWN),
                    installed,
                    now,
                ),
                expected
            );
        }
        assert_eq!(LAUNCH_COOLDOWN, Duration::from_secs(20));
        assert_eq!(
            launch_decision(
                Some(false),
                Some(false),
                GamePhase::None,
                Some(now - Duration::from_secs(1)),
                true,
                now,
            ),
            LaunchDecision::OpenShell
        );
    }

    #[test]
    fn steam_status_payload_has_only_the_approved_public_fields() {
        let status = serde_json::to_value(SteamStatus::unknown()).unwrap();
        assert_eq!(
            status,
            serde_json::json!({
                "steam_running": null,
                "dota_running": null,
                "persona_name": null,
                "gsi": "unknown",
                "game_phase": "unknown",
                "launch_pending": false,
                "minimap_state": "unsupported"
            })
        );
        assert_eq!(
            serde_json::to_value(LaunchResult {
                result: LaunchResultKind::Launched,
                detail: None,
            })
            .unwrap(),
            serde_json::json!({"result": "launched", "detail": null})
        );
    }

    #[test]
    fn loginusers_parser_returns_only_sanitized_persona_and_logs_no_platform_id() {
        let account_id = 123_456_789_u32;
        let steam_id = (76_561_197_960_265_728_u64 + u64::from(account_id)).to_string();
        let vdf = format!(
            "\"users\" {{ \"{steam_id}\" {{ \"PersonaName\" \"Player\\nOne\" \"AccountName\" \"private-login\" }} }}"
        );

        let mut persona = None;
        let logs = crate::logging::collect_test_logs(|| {
            persona = parse_loginusers_vdf(&vdf, account_id);
            // Synthetic values exercise facade redaction in an in-memory logger.
            crate::log_info!(
                "steam",
                "persona_read",
                "Steam persona read",
                diagnostic = format!("account={account_id} platform={steam_id}")
            );
        });
        assert_eq!(persona.as_deref(), Some("PlayerOne"));
        let returned = serde_json::json!({"persona_name": persona}).to_string();
        assert!(!returned.contains(&steam_id));
        assert!(!returned.contains(&account_id.to_string()));
        assert_eq!(logs.len(), 1);
        let logs = serde_json::to_string(&logs).unwrap();
        assert!(!logs.contains(&steam_id));
        assert!(!logs.contains(&account_id.to_string()));
        assert!(logs.contains("[redacted]"));
    }

    #[test]
    fn loginusers_parser_bounds_input_and_persona_length() {
        let account_id = 7_u32;
        let steam_id = (76_561_197_960_265_728_u64 + u64::from(account_id)).to_string();
        let long_name = "x".repeat(80);
        let vdf = format!("\"users\" {{ \"{steam_id}\" {{ \"PersonaName\" \"{long_name}\" }} }}");
        assert_eq!(
            parse_loginusers_vdf(&vdf, account_id).as_deref(),
            Some("x".repeat(64).as_str())
        );
        assert!(parse_loginusers_vdf(&" ".repeat(256 * 1024 + 1), account_id).is_none());
    }

    #[test]
    fn persona_cannot_project_the_signed_in_platform_identifiers() {
        let account_id = 123_456_789_u32;
        let steam_id = (76_561_197_960_265_728_u64 + u64::from(account_id)).to_string();
        for name in [format!("Player {steam_id}"), format!("Player {account_id}")] {
            let vdf = format!("\"users\" {{ \"{steam_id}\" {{ \"PersonaName\" \"{name}\" }} }}");
            assert!(parse_loginusers_vdf(&vdf, account_id).is_none());
        }
    }
}
