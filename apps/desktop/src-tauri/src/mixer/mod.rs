//! Per-application session volume (MEDIA-009) behind the orb sound mixer (UI-011).
//!
//! Kohbor changes only the Windows Core Audio shared-mode session volume and mute of the Dota 2 process and the
//! applications the user placed in a mixer slot, on the default render device. It never targets a renderer-supplied
//! process id: the host resolves sessions itself from their executable base name. Before the first change of an
//! application the original level and mute are persisted locally (one entry for the application when all its sessions
//! agree, one entry per session when they differ); they are restored on exit, on disable, on Reset and on the next
//! start after an abnormal exit. Every change re-reads the sessions first, so a session that has not been read is
//! never set and a changed default device is noticed before use. Logs carry channel, executable base name and level
//! only.
//!
//! Mixer slots (owner request 2026-11-22): the panel shows at most four channels. Game (Dota 2) is fixed first; up
//! to three slots follow, each the Kohbor announcer, the Music channel (a chosen app, or the first running default)
//! or an application chosen by executable name. An application with several sessions moves them all together and
//! shows their common value, or a mixed state when they differ.
//!
//! This file is platform independent: the Core Audio backend lives in `native.rs`, the COM thread in `service.rs`,
//! and the tests below drive the same logic through a fake session manager.

#[cfg(windows)]
pub(crate) mod native;
pub(crate) mod service;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) const GAME_EXE: &str = "dota2.exe";
/// Default music applications, in the owner-approved order: the first one with a session wins.
pub(crate) const MUSIC_DEFAULTS: [&str; 4] =
    ["chrome.exe", "msedge.exe", "firefox.exe", "spotify.exe"];
pub(crate) const DEFAULT_FOCUS_PERCENT: u8 = 20;
/// Slots after the fixed Game channel: the panel shows at most four channels.
pub(crate) const MAX_SLOTS: usize = 3;
const STORE_SCHEMA_VERSION: u8 = 1;
pub(crate) const ORIGINALS_FILE: &str = "mixer-originals.json";

// ---------------------------------------------------------------------------------------------------------------
// Slots

/// One configurable mixer slot after the fixed Game channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The Kohbor announcer volume (the existing announcer path, not a Windows session).
    Kohbor,
    /// The Music channel: the chosen application, or `None` for the first running default.
    Music(Option<String>),
    /// An application chosen by executable base name (lower case), for example `discord.exe`.
    App(String),
}

impl Slot {
    /// The channel id used by the renderer and the commands: `kohbor`, `music` or `app:<exe>`.
    pub(crate) fn id(&self) -> String {
        match self {
            Self::Kohbor => "kohbor".to_owned(),
            Self::Music(_) => "music".to_owned(),
            Self::App(exe) => format!("app:{exe}"),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Kohbor => "kohbor",
            Self::Music(_) => "music",
            Self::App(_) => "app",
        }
    }

    pub(crate) fn to_json(&self) -> Value {
        match self {
            Self::Kohbor => json!({"kind": "kohbor"}),
            Self::Music(exe) => json!({"kind": "music", "exe": exe}),
            Self::App(exe) => json!({"kind": "app", "exe": exe}),
        }
    }

    /// Parse one slot; executables are lower-cased. Unknown kinds or fields are refused.
    pub(crate) fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let exe = |required: bool| -> Option<Option<String>> {
            match object.get("exe") {
                None | Some(Value::Null) if !required => Some(None),
                Some(Value::String(exe)) => {
                    let exe = exe.to_ascii_lowercase();
                    valid_exe(&exe).then_some(Some(exe))
                }
                _ => None,
            }
        };
        let allowed = |keys: &[&str]| object.keys().all(|key| keys.contains(&key.as_str()));
        match object.get("kind")?.as_str()? {
            "kohbor" if allowed(&["kind"]) => Some(Self::Kohbor),
            "music" if allowed(&["kind", "exe"]) => Some(Self::Music(exe(false)?)),
            "app" if allowed(&["kind", "exe"]) => Some(Self::App(exe(true)??)),
            _ => None,
        }
    }

    fn fixed_exe(&self) -> Option<&str> {
        match self {
            Self::Kohbor | Self::Music(None) => None,
            Self::Music(Some(exe)) | Self::App(exe) => Some(exe),
        }
    }
}

/// Default slots: Kohbor and Music (the behaviour before slots existed), the fourth row left empty.
pub(crate) fn default_slots() -> Vec<Slot> {
    vec![Slot::Kohbor, Slot::Music(None)]
}

/// Executable base names accepted from settings and requests: no path, `.exe`, a conservative character set.
pub(crate) fn valid_exe(exe: &str) -> bool {
    (5..=64).contains(&exe.len())
        && exe.ends_with(".exe")
        && exe
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b' '))
}

/// At most three slots, at most one Kohbor and one Music slot, no Dota, no executable twice.
pub(crate) fn valid_slots(slots: &[Slot]) -> bool {
    if slots.len() > MAX_SLOTS
        || slots.iter().filter(|slot| **slot == Slot::Kohbor).count() > 1
        || slots
            .iter()
            .filter(|slot| matches!(slot, Slot::Music(_)))
            .count()
            > 1
    {
        return false;
    }
    let mut seen: Vec<&str> = Vec::new();
    for exe in slots.iter().filter_map(Slot::fixed_exe) {
        if exe == GAME_EXE
            || !valid_exe(exe)
            || exe != exe.to_ascii_lowercase()
            || seen.contains(&exe)
        {
            return false;
        }
        seen.push(exe);
    }
    true
}

pub(crate) fn slots_json(slots: &[Slot]) -> Value {
    Value::Array(slots.iter().map(Slot::to_json).collect())
}

// ---------------------------------------------------------------------------------------------------------------
// Backend

/// One audio session as read from the default render device.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SessionRead {
    Readable {
        exe: String,
        /// Session instance identifier (stable while the session lives), when Windows provides one. Stored locally
        /// with per-session originals, never logged.
        id: Option<String>,
        level: f32,
        muted: bool,
    },
    /// The session exists but its owner or volume could not be read. `exe` is known when only the volume failed.
    Unreadable { exe: Option<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeviceRead {
    /// Opaque endpoint id of the default render device. Stored locally with originals, never logged.
    pub(crate) device: String,
    pub(crate) sessions: Vec<SessionRead>,
}

impl DeviceRead {
    /// Readable sessions of `exe`: (session id, exact level, mute).
    fn readable(&self, exe: &str) -> Vec<(Option<&str>, f32, bool)> {
        self.sessions
            .iter()
            .filter_map(|session| match session {
                SessionRead::Readable {
                    exe: session_exe,
                    id,
                    level,
                    muted,
                } if session_exe == exe => Some((id.as_deref(), *level, *muted)),
                _ => None,
            })
            .collect()
    }

    fn has_session(&self, exe: &str, id: &str) -> bool {
        self.readable(exe)
            .iter()
            .any(|(session, ..)| *session == Some(id))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackendError {
    /// Core Audio (or the default device) is not available.
    Unavailable,
    /// The default device is not the one that was read before the set.
    DeviceChanged,
    /// No readable session of the executable (or the named session) exists any more.
    Missing,
    /// A session refused the new level or mute.
    SetFailed,
}

impl BackendError {
    fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "MIXER_AUDIO_UNAVAILABLE",
            Self::DeviceChanged => "MIXER_DEVICE_CHANGED",
            Self::Missing => "MIXER_SESSION_MISSING",
            Self::SetFailed => "MIXER_SET_FAILED",
        }
    }
}

/// The session manager seen by the mixer: the Core Audio adapter in production, a fake in tests.
pub(crate) trait SessionBackend {
    /// Enumerate the sessions of the current default render device.
    fn read(&mut self) -> Result<DeviceRead, BackendError>;
    /// Set level and/or mute on every session of `exe` (or only the session `session` of `exe`) that the backend
    /// reads again during this call on `device`. Must fail with `DeviceChanged` when the default device is no longer
    /// `device`. Returns the sessions changed.
    fn apply(
        &mut self,
        device: &str,
        exe: &str,
        session: Option<&str>,
        level: Option<f32>,
        muted: Option<bool>,
    ) -> Result<usize, BackendError>;
}

// ---------------------------------------------------------------------------------------------------------------
// Recovery data

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Original {
    pub(crate) device: String,
    pub(crate) exe: String,
    /// `None`: every session of `exe` had this value. `Some`: the value of that one session (mixed sessions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) session: Option<String>,
    pub(crate) level: f32,
    pub(crate) muted: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOriginals {
    schema_version: u8,
    entries: Vec<Original>,
}

/// Local persistence of the original session values (recovery data).
pub(crate) trait OriginalStore {
    fn load(&self) -> Result<Vec<Original>, String>;
    /// Persist the full list; an empty list removes the recovery data.
    fn save(&self, entries: &[Original]) -> Result<(), String>;
}

pub(crate) struct JsonOriginalStore {
    path: PathBuf,
}

impl JsonOriginalStore {
    pub(crate) fn in_data_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(ORIGINALS_FILE),
        }
    }
}

impl OriginalStore for JsonOriginalStore {
    fn load(&self) -> Result<Vec<Original>, String> {
        match std::fs::read(&self.path) {
            Ok(bytes) => parse_originals(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err("MIXER_RECOVERY_UNREADABLE".to_owned()),
        }
    }

    fn save(&self, entries: &[Original]) -> Result<(), String> {
        if entries.is_empty() {
            return match std::fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err("MIXER_RECOVERY_WRITE_FAILED".to_owned()),
            };
        }
        let bytes = serde_json::to_vec(&StoredOriginals {
            schema_version: STORE_SCHEMA_VERSION,
            entries: entries.to_vec(),
        })
        .map_err(|_| "MIXER_RECOVERY_WRITE_FAILED".to_owned())?;
        crate::atomic::write(&self.path, &bytes)
            .map_err(|_| "MIXER_RECOVERY_WRITE_FAILED".to_owned())
    }
}

fn parse_originals(bytes: &[u8]) -> Result<Vec<Original>, String> {
    let stored: StoredOriginals =
        serde_json::from_slice(bytes).map_err(|_| "MIXER_RECOVERY_INVALID".to_owned())?;
    if stored.schema_version != STORE_SCHEMA_VERSION
        || stored.entries.iter().any(|entry| {
            !entry.level.is_finite()
                || !(0.0..=1.0).contains(&entry.level)
                || entry.exe.is_empty()
                || entry.session.as_deref().is_some_and(str::is_empty)
        })
    {
        return Err("MIXER_RECOVERY_INVALID".to_owned());
    }
    Ok(stored.entries)
}

// ---------------------------------------------------------------------------------------------------------------
// Channel state

/// What a channel looks like after a fresh read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ChannelState {
    /// Every readable session of the application has this level and mute.
    Present {
        exe: String,
        percent: u8,
        muted: bool,
    },
    /// Several sessions with different levels or mutes: their range is shown, never one invented value.
    Mixed {
        exe: String,
        low: u8,
        high: u8,
        /// `Some` when the mute agrees across sessions.
        muted: Option<bool>,
    },
    /// No session for the executable (Dota not playing sound yet, app not running, or no music app found).
    Missing {
        exe: Option<String>,
    },
    Unreadable {
        exe: Option<String>,
    },
}

impl ChannelState {
    fn to_json(&self, id: &str, kind: &str) -> Value {
        match self {
            Self::Present {
                exe,
                percent,
                muted,
            } => json!({"id": id, "kind": kind, "state": "present", "exe": exe, "percent": percent,
                "muted": muted, "range": null}),
            Self::Mixed {
                exe,
                low,
                high,
                muted,
            } => json!({"id": id, "kind": kind, "state": "mixed", "exe": exe, "percent": null,
                "muted": muted, "range": [low, high]}),
            Self::Missing { exe } => json!({"id": id, "kind": kind, "state": "missing", "exe": exe,
                "percent": null, "muted": null, "range": null}),
            Self::Unreadable { exe } => {
                json!({"id": id, "kind": kind, "state": "unreadable", "exe": exe,
                "percent": null, "muted": null, "range": null})
            }
        }
    }

    fn controllable(&self) -> bool {
        matches!(self, Self::Present { .. } | Self::Mixed { .. })
    }
}

/// Outcome of a restore pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RestoreSummary {
    pub(crate) restored: usize,
    pub(crate) pending: usize,
}

/// The production session manager: Core Audio on Windows, nothing elsewhere (every read is "unavailable").
pub(crate) fn platform_backend() -> Box<dyn SessionBackend> {
    #[cfg(windows)]
    {
        Box::new(native::CoreAudioBackend::default())
    }
    #[cfg(not(windows))]
    {
        Box::new(UnavailableBackend)
    }
}

#[cfg(not(windows))]
struct UnavailableBackend;

#[cfg(not(windows))]
impl SessionBackend for UnavailableBackend {
    fn read(&mut self) -> Result<DeviceRead, BackendError> {
        Err(BackendError::Unavailable)
    }
    fn apply(
        &mut self,
        _: &str,
        _: &str,
        _: Option<&str>,
        _: Option<f32>,
        _: Option<bool>,
    ) -> Result<usize, BackendError> {
        Err(BackendError::Unavailable)
    }
}

pub(crate) fn percent_of(level: f32) -> u8 {
    if !level.is_finite() {
        return 0;
    }
    (level.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn level_of(percent: u8) -> f32 {
    f32::from(percent.min(100)) / 100.0
}

/// A channel lowered by Focus mode and the exact values to put back.
#[derive(Clone, Debug, PartialEq)]
struct FocusEntry {
    channel: String,
    exe: String,
    /// `(None, level)` for the whole application, `(Some(session), level)` per session when they differed.
    prior: Vec<(Option<String>, f32)>,
}

/// A resolved application channel: its id, kind (for logs: `game`, `music` or `app`) and executable.
struct Target {
    id: String,
    kind: &'static str,
    exe: Option<String>,
}

pub(crate) struct Mixer {
    backend: Box<dyn SessionBackend>,
    store: Box<dyn OriginalStore>,
    originals: Vec<Original>,
    /// Set when a restore was requested (disable, Reset, recovery) but some session was not present: every later
    /// read retries until all originals are back.
    restore_owed: bool,
    slots: Vec<Slot>,
    focus_percent: u8,
    /// Channels lowered by Focus mode, while Focus mode is on.
    focus: Option<Vec<FocusEntry>>,
    last_device: Option<String>,
    own_exe: Option<String>,
}

impl Mixer {
    pub(crate) fn new(backend: Box<dyn SessionBackend>, store: Box<dyn OriginalStore>) -> Self {
        let (originals, restore_owed) = match store.load() {
            Ok(entries) => {
                let owed = !entries.is_empty();
                (entries, owed)
            }
            Err(code) => {
                crate::log_warn!(
                    "mixer",
                    "recovery_unreadable",
                    "mixer recovery data unreadable",
                    code = code
                );
                (Vec::new(), false)
            }
        };
        if restore_owed {
            crate::log_info!(
                "mixer",
                "recovery_pending",
                "original session levels from an earlier run are pending restore",
                sessions = originals.len()
            );
        }
        let own_exe = std::env::current_exe().ok().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
        });
        Self {
            backend,
            store,
            originals,
            restore_owed,
            slots: default_slots(),
            focus_percent: DEFAULT_FOCUS_PERCENT,
            focus: None,
            last_device: None,
            own_exe,
        }
    }

    /// Apply the saved slots and Focus level. Invalid slots are refused (the previous slots stay). Focus entries of
    /// channels that are gone or now point to another application are dropped.
    pub(crate) fn configure(&mut self, slots: Vec<Slot>, focus_percent: u8) -> bool {
        self.focus_percent = focus_percent.min(100);
        if !valid_slots(&slots) {
            return false;
        }
        if let Some(entries) = &mut self.focus {
            entries.retain(|entry| {
                slots.iter().any(|slot| {
                    slot.id() == entry.channel
                        && slot.fixed_exe().is_none_or(|exe| exe == entry.exe)
                })
            });
            if entries.is_empty() {
                self.focus = None;
            }
        }
        self.slots = slots;
        true
    }

    pub(crate) fn focus_active(&self) -> bool {
        self.focus.is_some()
    }

    pub(crate) fn recovery_pending(&self) -> usize {
        self.originals.len()
    }

    fn read(&mut self) -> Result<DeviceRead, BackendError> {
        let read = self.backend.read()?;
        if self
            .last_device
            .as_ref()
            .is_some_and(|previous| previous != &read.device)
        {
            // Never log the endpoint id. Focus mode refers to sessions on the old device.
            crate::log_info!(
                "mixer",
                "device_changed",
                "default render device changed; sessions re-read"
            );
            self.focus = None;
        }
        self.last_device = Some(read.device.clone());
        Ok(read)
    }

    /// The executable of a Music slot: the chosen one, or the first running default not used by another slot.
    fn music_exe_in(&self, chosen: Option<&String>, read: &DeviceRead) -> Option<String> {
        if let Some(exe) = chosen {
            return Some(exe.clone());
        }
        MUSIC_DEFAULTS
            .iter()
            .find(|candidate| {
                !self
                    .slots
                    .iter()
                    .any(|slot| slot.fixed_exe() == Some(**candidate))
                    && !read.readable(candidate).is_empty()
            })
            .map(|exe| (*exe).to_owned())
    }

    /// Application channels in panel order (Kohbor excluded; the host composes it).
    fn targets(&self, read: Option<&DeviceRead>) -> Vec<Target> {
        let mut targets = vec![Target {
            id: "game".to_owned(),
            kind: "game",
            exe: Some(GAME_EXE.to_owned()),
        }];
        for slot in &self.slots {
            let exe = match slot {
                Slot::Kohbor => continue,
                Slot::Music(chosen) => match read {
                    Some(read) => self.music_exe_in(chosen.as_ref(), read),
                    None => chosen.clone(),
                },
                Slot::App(exe) => Some(exe.clone()),
            };
            targets.push(Target {
                id: slot.id(),
                kind: slot.kind(),
                exe,
            });
        }
        targets
    }

    fn target(&self, channel: &str, read: &DeviceRead) -> Option<Target> {
        self.targets(Some(read))
            .into_iter()
            .find(|target| target.id == channel)
    }

    fn state_in(read: &DeviceRead, exe: Option<&str>) -> ChannelState {
        let Some(exe) = exe else {
            return ChannelState::Missing { exe: None };
        };
        let sessions = read.readable(exe);
        let Some((_, first_level, first_muted)) = sessions.first().copied() else {
            let unreadable = read.sessions.iter().any(
                |session| matches!(session, SessionRead::Unreadable { exe: Some(other) } if other == exe),
            );
            return if unreadable {
                ChannelState::Unreadable {
                    exe: Some(exe.to_owned()),
                }
            } else {
                ChannelState::Missing {
                    exe: Some(exe.to_owned()),
                }
            };
        };
        let percents: Vec<u8> = sessions
            .iter()
            .map(|(_, level, _)| percent_of(*level))
            .collect();
        let low = percents.iter().copied().min().unwrap_or(0);
        let high = percents.iter().copied().max().unwrap_or(0);
        let same_mute = sessions.iter().all(|(_, _, muted)| *muted == first_muted);
        if low == high && same_mute {
            ChannelState::Present {
                exe: exe.to_owned(),
                percent: percent_of(first_level),
                muted: first_muted,
            }
        } else {
            ChannelState::Mixed {
                exe: exe.to_owned(),
                low,
                high,
                muted: same_mute.then_some(first_muted),
            }
        }
    }

    /// Applications the user may put in a slot or choose for Music: readable sessions other than Dota and Kohbor
    /// itself, plus the applications already configured.
    fn choices_in(&self, read: &DeviceRead) -> Vec<String> {
        let mut choices: Vec<String> = Vec::new();
        for session in &read.sessions {
            if let SessionRead::Readable { exe, .. } = session {
                if exe != GAME_EXE
                    && self.own_exe.as_deref() != Some(exe.as_str())
                    && !choices.contains(exe)
                {
                    choices.push(exe.clone());
                }
            }
        }
        for exe in self.slots.iter().filter_map(Slot::fixed_exe) {
            if !choices.iter().any(|choice| choice == exe) {
                choices.push(exe.to_owned());
            }
        }
        choices.sort();
        choices
    }

    fn focus_json(&self) -> Value {
        let channels: Vec<Value> = self
            .focus
            .iter()
            .flatten()
            .map(|entry| {
                let prior = match entry.prior.as_slice() {
                    [(None, level)] => json!(percent_of(*level)),
                    _ => Value::Null,
                };
                json!({"id": entry.channel, "prior_percent": prior})
            })
            .collect();
        json!({"active": self.focus_active(), "percent": self.focus_percent, "channels": channels})
    }

    fn music_selected(&self) -> Value {
        match self
            .slots
            .iter()
            .find(|slot| matches!(slot, Slot::Music(_)))
        {
            Some(Slot::Music(exe)) => json!(exe),
            _ => Value::Null,
        }
    }

    /// Fresh read of every application channel. Retries an owed restore first.
    pub(crate) fn snapshot(&mut self) -> Value {
        if self.restore_owed {
            let _ = self.restore_all("retry");
        }
        match self.read() {
            Ok(read) => self.snapshot_from(&read, None),
            Err(error) => self.unavailable_snapshot(error),
        }
    }

    /// Channels in panel order; the Kohbor slot is a placeholder `{"id": "kohbor", "kind": "kohbor"}` that the
    /// host fills from the announcer path.
    fn channels_json(&self, read: Option<&DeviceRead>) -> Vec<Value> {
        let mut targets = self.targets(read).into_iter();
        let mut channels = Vec::new();
        if let Some(game) = targets.next() {
            channels.push(self.channel_json(&game, read));
        }
        for slot in &self.slots {
            if *slot == Slot::Kohbor {
                channels.push(json!({"id": "kohbor", "kind": "kohbor"}));
            } else if let Some(target) = targets.next() {
                channels.push(self.channel_json(&target, read));
            }
        }
        channels
    }

    fn channel_json(&self, target: &Target, read: Option<&DeviceRead>) -> Value {
        let state = match read {
            Some(read) => Self::state_in(read, target.exe.as_deref()),
            None => ChannelState::Unreadable {
                exe: target.exe.clone(),
            },
        };
        state.to_json(&target.id, target.kind)
    }

    fn snapshot_from(&self, read: &DeviceRead, reason: Option<&str>) -> Value {
        json!({
            "available": true,
            "reason": reason,
            "channels": self.channels_json(Some(read)),
            "slots": slots_json(&self.slots),
            "app_choices": self.choices_in(read),
            "music_selected": self.music_selected(),
            "focus": self.focus_json(),
            "restore_pending": if self.restore_owed { self.originals.len() } else { 0 },
            "touched": self.originals.len(),
        })
    }

    fn unavailable_snapshot(&self, error: BackendError) -> Value {
        json!({
            "available": false,
            "reason": error.code(),
            "channels": self.channels_json(None),
            "slots": slots_json(&self.slots),
            "app_choices": [],
            "music_selected": self.music_selected(),
            "focus": self.focus_json(),
            "restore_pending": if self.restore_owed { self.originals.len() } else { 0 },
            "touched": self.originals.len(),
        })
    }

    fn snapshot_after_change(&mut self, reason: Option<&str>) -> Value {
        match self.read() {
            Ok(read) => self.snapshot_from(&read, reason),
            Err(error) => self.unavailable_snapshot(error),
        }
    }

    /// Persist the originals of `exe` before its first change: one entry when its sessions agree, one per session
    /// when they differ (a differing session without an id cannot be recorded, so nothing changes).
    fn ensure_originals(&mut self, read: &DeviceRead, exe: &str, kind: &str) -> Result<(), String> {
        let sessions = read.readable(exe);
        let existing: Vec<&Original> = self
            .originals
            .iter()
            .filter(|entry| entry.device == read.device && entry.exe == exe)
            .collect();
        if existing.iter().any(|entry| entry.session.is_none()) {
            return Ok(());
        }
        let uniform = matches!(
            Self::state_in(read, Some(exe)),
            ChannelState::Present { .. }
        );
        let mut additions = Vec::new();
        if existing.is_empty() && uniform {
            if let Some((_, level, muted)) = sessions.first() {
                additions.push(Original {
                    device: read.device.clone(),
                    exe: exe.to_owned(),
                    session: None,
                    level: *level,
                    muted: *muted,
                });
            }
        } else {
            for (id, level, muted) in &sessions {
                let Some(id) = id else {
                    if uniform {
                        continue;
                    }
                    return Err("MIXER_SESSION_UNREADABLE".to_owned());
                };
                if existing
                    .iter()
                    .any(|entry| entry.session.as_deref() == Some(*id))
                {
                    continue;
                }
                additions.push(Original {
                    device: read.device.clone(),
                    exe: exe.to_owned(),
                    session: Some((*id).to_owned()),
                    level: *level,
                    muted: *muted,
                });
            }
        }
        if additions.is_empty() {
            return Ok(());
        }
        let mut next = self.originals.clone();
        next.extend(additions);
        if let Err(code) = self.store.save(&next) {
            crate::log_warn!(
                "mixer",
                "set_refused",
                "original level could not be stored; nothing changed",
                channel = kind,
                exe = exe
            );
            return Err(code);
        }
        self.originals = next;
        Ok(())
    }

    fn change(
        &mut self,
        channel: &str,
        percent: Option<u8>,
        muted: Option<bool>,
    ) -> Result<Value, (String, Value)> {
        let read = self
            .read()
            .map_err(|error| (error.code().to_owned(), self.unavailable_snapshot(error)))?;
        let Some(target) = self.target(channel, &read) else {
            let code = "MIXER_CHANNEL_UNKNOWN";
            return Err((code.to_owned(), self.snapshot_from(&read, Some(code))));
        };
        let state = Self::state_in(&read, target.exe.as_deref());
        let (true, Some(exe)) = (state.controllable(), target.exe.clone()) else {
            let code = match state {
                ChannelState::Unreadable { .. } => "MIXER_SESSION_UNREADABLE",
                _ => BackendError::Missing.code(),
            };
            return Err((code.to_owned(), self.snapshot_from(&read, Some(code))));
        };
        if let Err(code) = self.ensure_originals(&read, &exe, target.kind) {
            return Err((code.clone(), self.snapshot_from(&read, Some(&code))));
        }
        match self
            .backend
            .apply(&read.device, &exe, None, percent.map(level_of), muted)
        {
            Ok(_) => {
                if let Some(percent) = percent {
                    crate::log_info!(
                        "mixer",
                        "level_set",
                        "session level set",
                        channel = target.kind,
                        exe = exe.as_str(),
                        level = percent
                    );
                }
                if let Some(muted) = muted {
                    crate::log_info!(
                        "mixer",
                        "mute_set",
                        "session mute set",
                        channel = target.kind,
                        exe = exe.as_str(),
                        muted = muted
                    );
                }
                Ok(self.snapshot_after_change(None))
            }
            Err(error) => {
                crate::log_warn!(
                    "mixer",
                    "set_failed",
                    "session change failed; showing the last read value",
                    channel = target.kind,
                    exe = exe.as_str(),
                    code = error.code()
                );
                // Re-read so the reply shows the real level, never the requested one.
                Err((
                    error.code().to_owned(),
                    self.snapshot_after_change(Some(error.code())),
                ))
            }
        }
    }

    /// Set a channel level (`game`, `music` or `app:<exe>`). Moving a fader un-mutes it and ends Focus mode for it.
    pub(crate) fn set_level(
        &mut self,
        channel: &str,
        percent: u8,
    ) -> Result<Value, (String, Value)> {
        let percent = percent.min(100);
        if let Some(entries) = &mut self.focus {
            entries.retain(|entry| entry.channel != channel);
            if entries.is_empty() {
                self.focus = None;
            }
        }
        self.change(channel, Some(percent), Some(false))
    }

    pub(crate) fn set_muted(
        &mut self,
        channel: &str,
        muted: bool,
    ) -> Result<Value, (String, Value)> {
        self.change(channel, None, Some(muted))
    }

    /// Focus on: every non-game application channel goes to the Focus level (never raised; each session on its
    /// own when they differ). Focus off: every lowered channel back to its exact earlier value.
    pub(crate) fn focus_toggle(&mut self) -> Result<Value, (String, Value)> {
        if self.focus.is_some() {
            return self.focus_off();
        }
        let read = self
            .read()
            .map_err(|error| (error.code().to_owned(), self.unavailable_snapshot(error)))?;
        let focus = self.focus_percent;
        let mut entries = Vec::new();
        let mut failure: Option<String> = None;
        for target in self.targets(Some(&read)).into_iter().skip(1) {
            let Some(exe) = target.exe.clone() else {
                continue;
            };
            let state = Self::state_in(&read, Some(&exe));
            if !state.controllable() {
                if matches!(state, ChannelState::Unreadable { .. }) {
                    failure = Some("MIXER_SESSION_UNREADABLE".to_owned());
                }
                continue;
            }
            if let Err(code) = self.ensure_originals(&read, &exe, target.kind) {
                failure = Some(code);
                continue;
            }
            let mut prior = Vec::new();
            if let ChannelState::Present { .. } = state {
                let level = read.readable(&exe)[0].1;
                let next = level.min(level_of(focus));
                match self
                    .backend
                    .apply(&read.device, &exe, None, Some(next), None)
                {
                    Ok(_) => prior.push((None, level)),
                    Err(error) => failure = Some(error.code().to_owned()),
                }
            } else {
                for (id, level, _) in read.readable(&exe) {
                    let Some(id) = id else {
                        failure = Some("MIXER_SESSION_UNREADABLE".to_owned());
                        continue;
                    };
                    let next = level.min(level_of(focus));
                    match self
                        .backend
                        .apply(&read.device, &exe, Some(id), Some(next), None)
                    {
                        Ok(_) => prior.push((Some(id.to_owned()), level)),
                        Err(error) => failure = Some(error.code().to_owned()),
                    }
                }
            }
            if !prior.is_empty() {
                crate::log_info!(
                    "mixer",
                    "focus_on",
                    "focus mode lowered a channel",
                    channel = target.kind,
                    exe = exe.as_str(),
                    level = focus
                );
                entries.push(FocusEntry {
                    channel: target.id.clone(),
                    exe,
                    prior,
                });
            }
        }
        if entries.is_empty() {
            let code = failure.unwrap_or_else(|| BackendError::Missing.code().to_owned());
            return Err((code.clone(), self.snapshot_from(&read, Some(&code))));
        }
        self.focus = Some(entries);
        Ok(self.snapshot_after_change(failure.as_deref()))
    }

    fn focus_off(&mut self) -> Result<Value, (String, Value)> {
        let entries = self.focus.take().unwrap_or_default();
        let read = match self.read() {
            Ok(read) => read,
            Err(error) => {
                self.focus = Some(entries);
                return Err((error.code().to_owned(), self.unavailable_snapshot(error)));
            }
        };
        let mut remaining = Vec::new();
        let mut failure: Option<String> = None;
        for mut entry in entries {
            entry.prior.retain(|(session, level)| {
                // A session that ended has nothing left to restore; the next start-up restore covers originals.
                if let Some(id) = session {
                    if !read.has_session(&entry.exe, id) {
                        return false;
                    }
                }
                match self.backend.apply(
                    &read.device,
                    &entry.exe,
                    session.as_deref(),
                    Some(*level),
                    None,
                ) {
                    Ok(_) => false,
                    Err(BackendError::Missing) => false,
                    Err(error) => {
                        failure = Some(error.code().to_owned());
                        true
                    }
                }
            });
            if entry.prior.is_empty() {
                crate::log_info!(
                    "mixer",
                    "focus_off",
                    "focus mode restored a channel",
                    exe = entry.exe.as_str()
                );
            } else {
                remaining.push(entry);
            }
        }
        self.focus = (!remaining.is_empty()).then_some(remaining);
        match failure {
            None => Ok(self.snapshot_after_change(None)),
            Some(code) => Err((code.clone(), self.snapshot_after_change(Some(&code)))),
        }
    }

    /// Choose the Music application. Only an executable with a readable session right now is accepted, never Dota
    /// or an application that already has its own slot. Returns the new slots for the host to persist.
    pub(crate) fn select_music(
        &mut self,
        exe: &str,
    ) -> Result<(Value, Vec<Slot>), (String, Value)> {
        let exe = exe.to_ascii_lowercase();
        let read = self
            .read()
            .map_err(|error| (error.code().to_owned(), self.unavailable_snapshot(error)))?;
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| matches!(slot, Slot::Music(_)))
        else {
            let code = "MIXER_CHANNEL_UNKNOWN";
            return Err((code.to_owned(), self.snapshot_from(&read, Some(code))));
        };
        let taken = self
            .slots
            .iter()
            .any(|slot| matches!(slot, Slot::App(other) if *other == exe));
        if exe == GAME_EXE || taken || read.readable(&exe).is_empty() || !valid_exe(&exe) {
            let code = "MIXER_APP_NOT_AVAILABLE";
            return Err((code.to_owned(), self.snapshot_from(&read, Some(code))));
        }
        if let Some(entries) = &self.focus {
            if entries.iter().any(|entry| entry.channel == "music") {
                let _ = self.focus_off();
            }
        }
        self.slots[index] = Slot::Music(Some(exe.clone()));
        crate::log_info!(
            "mixer",
            "music_selected",
            "music application selected",
            channel = "music",
            exe = exe.as_str()
        );
        Ok((self.snapshot_after_change(None), self.slots.clone()))
    }

    /// Put every touched session back to its original value. Entries that cannot be restored now (application not
    /// present, other device, set failure) stay persisted for the next attempt and the next start. A per-session
    /// entry whose session ended while its application still runs has nothing left to restore and is dropped.
    pub(crate) fn restore_all(&mut self, reason: &'static str) -> RestoreSummary {
        self.focus = None;
        if self.originals.is_empty() {
            self.restore_owed = false;
            return RestoreSummary::default();
        }
        let read = match self.read() {
            Ok(read) => read,
            Err(error) => {
                self.restore_owed = true;
                crate::log_warn!(
                    "mixer",
                    "restore_deferred",
                    "original levels kept for a later restore",
                    reason = reason,
                    code = error.code(),
                    pending = self.originals.len()
                );
                return RestoreSummary {
                    restored: 0,
                    pending: self.originals.len(),
                };
            }
        };
        let mut remaining = Vec::new();
        let mut restored = 0usize;
        for entry in std::mem::take(&mut self.originals) {
            let present = !read.readable(&entry.exe).is_empty();
            if entry.device != read.device || !present {
                remaining.push(entry);
                continue;
            }
            if let Some(id) = &entry.session {
                if !read.has_session(&entry.exe, id) {
                    crate::log_info!(
                        "mixer",
                        "restore_session_ended",
                        "a session ended before its original level could be restored",
                        exe = entry.exe.as_str(),
                        reason = reason
                    );
                    continue;
                }
            }
            match self.backend.apply(
                &read.device,
                &entry.exe,
                entry.session.as_deref(),
                Some(entry.level),
                Some(entry.muted),
            ) {
                Ok(_) => {
                    restored += 1;
                    crate::log_info!(
                        "mixer",
                        "level_restored",
                        "original session level restored",
                        exe = entry.exe.as_str(),
                        level = percent_of(entry.level),
                        reason = reason
                    );
                }
                Err(error) => {
                    crate::log_warn!(
                        "mixer",
                        "restore_failed",
                        "original session level not restored; kept for later",
                        exe = entry.exe.as_str(),
                        code = error.code(),
                        reason = reason
                    );
                    remaining.push(entry);
                }
            }
        }
        if let Err(code) = self.store.save(&remaining) {
            crate::log_warn!(
                "mixer",
                "recovery_write_failed",
                "mixer recovery data not updated",
                code = code
            );
        }
        let pending = remaining.len();
        self.originals = remaining;
        self.restore_owed = pending > 0;
        if pending > 0 {
            crate::log_info!(
                "mixer",
                "restore_pending",
                "some original levels wait for their session",
                reason = reason,
                pending = pending
            );
        }
        RestoreSummary { restored, pending }
    }
}

#[cfg(test)]
mod tests;
