//! Per-application session volume (MEDIA-009) behind the orb sound mixer (UI-011).
//!
//! Kohbor changes only the Windows Core Audio shared-mode session volume and mute of the Dota 2 process and the
//! music application the user selected, on the default render device. It never targets a renderer-supplied process
//! id: the host resolves sessions itself from their executable base name. Before the first change of a session the
//! original level and mute are persisted locally; they are restored on exit, on disable, on Reset and on the next
//! start after an abnormal exit. Every change re-reads the sessions first, so a session that has not been read is
//! never set and a changed default device is noticed before use. Logs carry channel, executable base name and level
//! only.
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
const STORE_SCHEMA_VERSION: u8 = 1;
pub(crate) const ORIGINALS_FILE: &str = "mixer-originals.json";

/// One audio session as read from the default render device.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SessionRead {
    Readable {
        exe: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackendError {
    /// Core Audio (or the default device) is not available.
    Unavailable,
    /// The default device is not the one that was read before the set.
    DeviceChanged,
    /// No readable session of the executable exists any more.
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
    /// Set level and/or mute on every session of `exe` that the backend reads again during this call on `device`.
    /// Must fail with `DeviceChanged` when the default device is no longer `device`. Returns the sessions changed.
    fn apply(
        &mut self,
        device: &str,
        exe: &str,
        level: Option<f32>,
        muted: Option<bool>,
    ) -> Result<usize, BackendError>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Original {
    pub(crate) device: String,
    pub(crate) exe: String,
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
            !entry.level.is_finite() || !(0.0..=1.0).contains(&entry.level) || entry.exe.is_empty()
        })
    {
        return Err("MIXER_RECOVERY_INVALID".to_owned());
    }
    Ok(stored.entries)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AppChannel {
    Game,
    Music,
}

impl AppChannel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Game => "game",
            Self::Music => "music",
        }
    }
}

/// What a channel looks like after a fresh read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ChannelState {
    Present {
        exe: String,
        percent: u8,
        muted: bool,
    },
    /// No session for the executable (Dota not playing sound yet, music app not running, or no app chosen).
    Missing {
        exe: Option<String>,
    },
    Unreadable {
        exe: Option<String>,
    },
}

impl ChannelState {
    pub(crate) fn to_json(&self, channel: &str) -> Value {
        match self {
            Self::Present {
                exe,
                percent,
                muted,
            } => {
                json!({"channel": channel, "state": "present", "exe": exe, "percent": percent, "muted": muted})
            }
            Self::Missing { exe } => {
                json!({"channel": channel, "state": "missing", "exe": exe, "percent": null, "muted": null})
            }
            Self::Unreadable { exe } => {
                json!({"channel": channel, "state": "unreadable", "exe": exe, "percent": null, "muted": null})
            }
        }
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

pub(crate) struct Mixer {
    backend: Box<dyn SessionBackend>,
    store: Box<dyn OriginalStore>,
    originals: Vec<Original>,
    /// Set when a restore was requested (disable, Reset, recovery) but some session was not present: every later
    /// read retries until all originals are back.
    restore_owed: bool,
    /// Executable chosen by the user for the music channel; `None` picks the first running default.
    music_exe: Option<String>,
    focus_percent: u8,
    /// Music level before Focus mode, while Focus mode is on.
    focus_prior: Option<u8>,
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
            music_exe: None,
            focus_percent: DEFAULT_FOCUS_PERCENT,
            focus_prior: None,
            last_device: None,
            own_exe,
        }
    }

    pub(crate) fn configure(&mut self, music_exe: Option<String>, focus_percent: u8) {
        let music_exe = music_exe.map(|exe| exe.to_ascii_lowercase());
        if music_exe != self.music_exe {
            self.focus_prior = None;
        }
        self.music_exe = music_exe;
        self.focus_percent = focus_percent.min(100);
    }

    pub(crate) fn focus_active(&self) -> bool {
        self.focus_prior.is_some()
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
            // Never log the endpoint id. Focus mode refers to a session on the old device.
            crate::log_info!(
                "mixer",
                "device_changed",
                "default render device changed; sessions re-read"
            );
            self.focus_prior = None;
        }
        self.last_device = Some(read.device.clone());
        Ok(read)
    }

    fn music_exe_in(&self, read: &DeviceRead) -> Option<String> {
        if let Some(exe) = &self.music_exe {
            return Some(exe.clone());
        }
        MUSIC_DEFAULTS
            .iter()
            .find(|candidate| {
                read.sessions
                    .iter()
                    .any(|session| matches!(session, SessionRead::Readable { exe, .. } if exe == *candidate))
            })
            .map(|exe| (*exe).to_owned())
    }

    fn exe_for(&self, channel: AppChannel, read: &DeviceRead) -> Option<String> {
        match channel {
            AppChannel::Game => Some(GAME_EXE.to_owned()),
            AppChannel::Music => self.music_exe_in(read),
        }
    }

    fn state_in(read: &DeviceRead, exe: Option<String>) -> ChannelState {
        let Some(exe) = exe else {
            return ChannelState::Missing { exe: None };
        };
        let mut unreadable = false;
        for session in &read.sessions {
            match session {
                SessionRead::Readable {
                    exe: session_exe,
                    level,
                    muted,
                } if session_exe == &exe => {
                    return ChannelState::Present {
                        exe,
                        percent: percent_of(*level),
                        muted: *muted,
                    };
                }
                SessionRead::Unreadable {
                    exe: Some(session_exe),
                } if session_exe == &exe => unreadable = true,
                _ => {}
            }
        }
        if unreadable {
            ChannelState::Unreadable { exe: Some(exe) }
        } else {
            ChannelState::Missing { exe: Some(exe) }
        }
    }

    /// Music applications the user may choose from: readable sessions other than Dota and Kohbor itself.
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
        if let Some(chosen) = &self.music_exe {
            if !choices.contains(chosen) {
                choices.push(chosen.clone());
            }
        }
        choices.sort();
        choices
    }

    /// Fresh read of both application channels. Retries an owed restore first.
    pub(crate) fn snapshot(&mut self) -> Value {
        if self.restore_owed {
            let _ = self.restore_all("retry");
        }
        match self.read() {
            Ok(read) => self.snapshot_from(&read, None),
            Err(error) => json!({
                "available": false,
                "reason": error.code(),
                "game": ChannelState::Unreadable { exe: Some(GAME_EXE.to_owned()) }.to_json("game"),
                "music": ChannelState::Unreadable { exe: self.music_exe.clone() }.to_json("music"),
                "music_choices": [],
                "music_selected": self.music_exe,
                "focus": {"active": self.focus_active(), "percent": self.focus_percent, "prior_percent": self.focus_prior},
                "restore_pending": if self.restore_owed { self.originals.len() } else { 0 },
                "touched": self.originals.len(),
            }),
        }
    }

    fn snapshot_from(&self, read: &DeviceRead, reason: Option<&str>) -> Value {
        let game = Self::state_in(read, self.exe_for(AppChannel::Game, read));
        let music = Self::state_in(read, self.exe_for(AppChannel::Music, read));
        json!({
            "available": true,
            "reason": reason,
            "game": game.to_json("game"),
            "music": music.to_json("music"),
            "music_choices": self.choices_in(read),
            "music_selected": self.music_exe,
            "focus": {"active": self.focus_active(), "percent": self.focus_percent, "prior_percent": self.focus_prior},
            "restore_pending": if self.restore_owed { self.originals.len() } else { 0 },
                "touched": self.originals.len(),
        })
    }

    fn change(
        &mut self,
        channel: AppChannel,
        percent: Option<u8>,
        muted: Option<bool>,
    ) -> Result<Value, (String, Value)> {
        let read = self
            .read()
            .map_err(|error| (error.code().to_owned(), self.unavailable_snapshot(error)))?;
        let exe = self.exe_for(channel, &read);
        let state = Self::state_in(&read, exe.clone());
        let ChannelState::Present { exe, .. } = &state else {
            let code = match state {
                ChannelState::Unreadable { .. } => "MIXER_SESSION_UNREADABLE",
                _ => BackendError::Missing.code(),
            };
            return Err((code.to_owned(), self.snapshot_from(&read, Some(code))));
        };
        let exe = exe.clone();
        let exact_level = read.sessions.iter().find_map(|session| match session {
            SessionRead::Readable {
                exe: session_exe,
                level,
                ..
            } if *session_exe == exe => Some(*level),
            _ => None,
        });
        if !self
            .originals
            .iter()
            .any(|entry| entry.device == read.device && entry.exe == exe)
        {
            let ChannelState::Present {
                muted: was_muted,
                percent: was,
                ..
            } = &state
            else {
                unreachable!("checked above");
            };
            let mut next = self.originals.clone();
            next.push(Original {
                device: read.device.clone(),
                exe: exe.clone(),
                level: exact_level.unwrap_or(level_of(*was)),
                muted: *was_muted,
            });
            if let Err(code) = self.store.save(&next) {
                crate::log_warn!(
                    "mixer",
                    "set_refused",
                    "original level could not be stored; nothing changed",
                    channel = channel.as_str(),
                    exe = exe.as_str()
                );
                return Err((code.clone(), self.snapshot_from(&read, Some(&code))));
            }
            self.originals = next;
        }
        match self
            .backend
            .apply(&read.device, &exe, percent.map(level_of), muted)
        {
            Ok(_) => {
                if let Some(percent) = percent {
                    crate::log_info!(
                        "mixer",
                        "level_set",
                        "session level set",
                        channel = channel.as_str(),
                        exe = exe.as_str(),
                        level = percent
                    );
                }
                if let Some(muted) = muted {
                    crate::log_info!(
                        "mixer",
                        "mute_set",
                        "session mute set",
                        channel = channel.as_str(),
                        exe = exe.as_str(),
                        muted = muted
                    );
                }
                Ok(self.snapshot_after_change())
            }
            Err(error) => {
                crate::log_warn!(
                    "mixer",
                    "set_failed",
                    "session change failed; showing the last read value",
                    channel = channel.as_str(),
                    exe = exe.as_str(),
                    code = error.code()
                );
                // Re-read so the reply shows the real level, never the requested one.
                let snapshot = match self.read() {
                    Ok(read) => self.snapshot_from(&read, Some(error.code())),
                    Err(read_error) => self.unavailable_snapshot(read_error),
                };
                Err((error.code().to_owned(), snapshot))
            }
        }
    }

    fn snapshot_after_change(&mut self) -> Value {
        match self.read() {
            Ok(read) => self.snapshot_from(&read, None),
            Err(error) => self.unavailable_snapshot(error),
        }
    }

    fn unavailable_snapshot(&self, error: BackendError) -> Value {
        json!({
            "available": false,
            "reason": error.code(),
            "game": ChannelState::Unreadable { exe: Some(GAME_EXE.to_owned()) }.to_json("game"),
            "music": ChannelState::Unreadable { exe: self.music_exe.clone() }.to_json("music"),
            "music_choices": [],
            "music_selected": self.music_exe,
            "focus": {"active": self.focus_active(), "percent": self.focus_percent, "prior_percent": self.focus_prior},
            "restore_pending": if self.restore_owed { self.originals.len() } else { 0 },
                "touched": self.originals.len(),
        })
    }

    pub(crate) fn set_level(
        &mut self,
        channel: AppChannel,
        percent: u8,
    ) -> Result<Value, (String, Value)> {
        let percent = percent.min(100);
        // Moving the music level by hand ends Focus mode and keeps the new level.
        if channel == AppChannel::Music {
            self.focus_prior = None;
        }
        // Moving a fader un-mutes the channel (prototype rule 8).
        self.change(channel, Some(percent), Some(false))
    }

    pub(crate) fn set_muted(
        &mut self,
        channel: AppChannel,
        muted: bool,
    ) -> Result<Value, (String, Value)> {
        self.change(channel, None, Some(muted))
    }

    /// Focus on: music to the focus level (never raised). Focus off: back to the level before Focus mode.
    pub(crate) fn focus_toggle(&mut self) -> Result<Value, (String, Value)> {
        if let Some(prior) = self.focus_prior {
            let result = self.change(AppChannel::Music, Some(prior), None);
            if result.is_ok() {
                self.focus_prior = None;
                crate::log_info!(
                    "mixer",
                    "focus_off",
                    "focus mode off",
                    channel = "music",
                    level = prior
                );
                return Ok(self.snapshot_after_change());
            }
            return result;
        }
        let read = self
            .read()
            .map_err(|error| (error.code().to_owned(), self.unavailable_snapshot(error)))?;
        let state = Self::state_in(&read, self.exe_for(AppChannel::Music, &read));
        let ChannelState::Present { percent, .. } = state else {
            let code = if matches!(state, ChannelState::Unreadable { .. }) {
                "MIXER_SESSION_UNREADABLE"
            } else {
                BackendError::Missing.code()
            };
            return Err((code.to_owned(), self.snapshot_from(&read, Some(code))));
        };
        let target = percent.min(self.focus_percent);
        self.change(AppChannel::Music, Some(target), None)?;
        self.focus_prior = Some(percent);
        crate::log_info!(
            "mixer",
            "focus_on",
            "focus mode on",
            channel = "music",
            level = target
        );
        Ok(self.snapshot_after_change())
    }

    /// Choose the music application. Only an executable with a readable session right now is accepted.
    pub(crate) fn select_music(&mut self, exe: &str) -> Result<Value, (String, Value)> {
        let exe = exe.to_ascii_lowercase();
        let read = self
            .read()
            .map_err(|error| (error.code().to_owned(), self.unavailable_snapshot(error)))?;
        if !self.choices_in(&read).contains(&exe) || exe == GAME_EXE {
            return Err((
                "MIXER_APP_NOT_AVAILABLE".to_owned(),
                self.snapshot_from(&read, Some("MIXER_APP_NOT_AVAILABLE")),
            ));
        }
        if self.focus_prior.is_some() {
            let _ = self.focus_toggle();
        }
        self.music_exe = Some(exe.clone());
        crate::log_info!(
            "mixer",
            "music_selected",
            "music application selected",
            channel = "music",
            exe = exe.as_str()
        );
        Ok(self.snapshot_after_change())
    }

    /// Put every touched session back to its original value. Entries that cannot be restored now (session not
    /// present, other device, set failure) stay persisted for the next attempt and the next start.
    pub(crate) fn restore_all(&mut self, reason: &'static str) -> RestoreSummary {
        self.focus_prior = None;
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
            let present = read.sessions.iter().any(
                |session| matches!(session, SessionRead::Readable { exe, .. } if *exe == entry.exe),
            );
            if entry.device != read.device || !present {
                remaining.push(entry);
                continue;
            }
            match self.backend.apply(
                &read.device,
                &entry.exe,
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
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct World {
        device: String,
        sessions: Vec<(String, f32, bool, bool)>, // exe, level, muted, readable
        unavailable: bool,
        fail_set: bool,
        /// Device switch that happens between `read` and `apply`.
        switch_before_apply: Option<String>,
        applied: Vec<(String, Option<f32>, Option<bool>)>,
        reads: usize,
    }

    #[derive(Clone)]
    struct FakeBackend(Rc<RefCell<World>>);

    impl SessionBackend for FakeBackend {
        fn read(&mut self) -> Result<DeviceRead, BackendError> {
            let mut world = self.0.borrow_mut();
            world.reads += 1;
            if world.unavailable {
                return Err(BackendError::Unavailable);
            }
            Ok(DeviceRead {
                device: world.device.clone(),
                sessions: world
                    .sessions
                    .iter()
                    .map(|(exe, level, muted, readable)| {
                        if *readable {
                            SessionRead::Readable {
                                exe: exe.clone(),
                                level: *level,
                                muted: *muted,
                            }
                        } else {
                            SessionRead::Unreadable {
                                exe: Some(exe.clone()),
                            }
                        }
                    })
                    .collect(),
            })
        }

        fn apply(
            &mut self,
            device: &str,
            exe: &str,
            level: Option<f32>,
            muted: Option<bool>,
        ) -> Result<usize, BackendError> {
            let mut world = self.0.borrow_mut();
            if let Some(next) = world.switch_before_apply.take() {
                world.device = next;
            }
            if world.device != device {
                return Err(BackendError::DeviceChanged);
            }
            if world.fail_set {
                return Err(BackendError::SetFailed);
            }
            let mut changed = 0;
            for session in world
                .sessions
                .iter_mut()
                .filter(|session| session.0 == exe && session.3)
            {
                if let Some(level) = level {
                    session.1 = level;
                }
                if let Some(muted) = muted {
                    session.2 = muted;
                }
                changed += 1;
            }
            if changed == 0 {
                return Err(BackendError::Missing);
            }
            world.applied.push((exe.to_owned(), level, muted));
            Ok(changed)
        }
    }

    #[derive(Clone, Default)]
    struct MemStore {
        entries: Rc<RefCell<Option<Vec<Original>>>>,
        fail: Rc<RefCell<bool>>,
    }

    impl OriginalStore for MemStore {
        fn load(&self) -> Result<Vec<Original>, String> {
            Ok(self.entries.borrow().clone().unwrap_or_default())
        }
        fn save(&self, entries: &[Original]) -> Result<(), String> {
            if *self.fail.borrow() {
                return Err("MIXER_RECOVERY_WRITE_FAILED".to_owned());
            }
            *self.entries.borrow_mut() = (!entries.is_empty()).then(|| entries.to_vec());
            Ok(())
        }
    }

    fn world() -> Rc<RefCell<World>> {
        Rc::new(RefCell::new(World {
            device: "dev-a".into(),
            sessions: vec![
                ("dota2.exe".into(), 0.8, false, true),
                ("chrome.exe".into(), 0.65, false, true),
                ("chrome.exe".into(), 0.65, false, true),
                ("discord.exe".into(), 1.0, false, true),
            ],
            ..World::default()
        }))
    }

    fn mixer(world: &Rc<RefCell<World>>, store: &MemStore) -> Mixer {
        Mixer::new(
            Box::new(FakeBackend(world.clone())),
            Box::new(store.clone()),
        )
    }

    fn level(world: &Rc<RefCell<World>>, exe: &str) -> Vec<f32> {
        world
            .borrow()
            .sessions
            .iter()
            .filter(|session| session.0 == exe)
            .map(|session| session.1)
            .collect()
    }

    #[test]
    fn snapshot_reports_present_channels_and_default_music_app() {
        let world = world();
        let store = MemStore::default();
        let snapshot = mixer(&world, &store).snapshot();
        assert_eq!(snapshot["game"]["state"], "present");
        assert_eq!(snapshot["game"]["percent"], 80);
        assert_eq!(snapshot["music"]["exe"], "chrome.exe");
        assert_eq!(snapshot["music"]["percent"], 65);
        assert_eq!(
            snapshot["music_choices"],
            json!(["chrome.exe", "discord.exe"])
        );
    }

    #[test]
    fn default_music_follows_the_approved_order() {
        let world = world();
        world
            .borrow_mut()
            .sessions
            .push(("spotify.exe".into(), 0.5, false, true));
        world
            .borrow_mut()
            .sessions
            .retain(|session| session.0 != "chrome.exe");
        world
            .borrow_mut()
            .sessions
            .push(("firefox.exe".into(), 0.4, false, true));
        let store = MemStore::default();
        assert_eq!(
            mixer(&world, &store).snapshot()["music"]["exe"],
            "firefox.exe"
        );
    }

    #[test]
    fn missing_and_unreadable_sessions_are_never_controllable() {
        let world = world();
        world.borrow_mut().sessions = vec![("dota2.exe".into(), 0.8, false, false)];
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        let snapshot = mixer.snapshot();
        assert_eq!(snapshot["game"]["state"], "unreadable");
        assert_eq!(snapshot["game"]["percent"], Value::Null);
        assert_eq!(snapshot["music"]["state"], "missing");
        let (code, _) = mixer.set_level(AppChannel::Game, 20).unwrap_err();
        assert_eq!(code, "MIXER_SESSION_UNREADABLE");
        let (code, _) = mixer.set_level(AppChannel::Music, 20).unwrap_err();
        assert_eq!(code, "MIXER_SESSION_MISSING");
        assert!(world.borrow().applied.is_empty());
        assert!(
            store.entries.borrow().is_none(),
            "nothing stored for an unchanged session"
        );
    }

    #[test]
    fn first_change_persists_the_original_before_applying() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.set_level(AppChannel::Game, 30).unwrap();
        let stored = store.entries.borrow().clone().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].exe, "dota2.exe");
        assert!((stored[0].level - 0.8).abs() < 1e-6);
        assert_eq!(level(&world, "dota2.exe"), vec![0.3]);
        // A second change keeps the first original.
        mixer.set_level(AppChannel::Game, 10).unwrap();
        assert!((store.entries.borrow().clone().unwrap()[0].level - 0.8).abs() < 1e-6);
    }

    #[test]
    fn a_failed_recovery_write_changes_nothing() {
        let world = world();
        let store = MemStore::default();
        *store.fail.borrow_mut() = true;
        let mut mixer = mixer(&world, &store);
        let (code, snapshot) = mixer.set_level(AppChannel::Game, 30).unwrap_err();
        assert_eq!(code, "MIXER_RECOVERY_WRITE_FAILED");
        assert_eq!(snapshot["game"]["percent"], 80);
        assert!(world.borrow().applied.is_empty());
    }

    #[test]
    fn music_change_touches_every_session_of_the_selected_app_only() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.set_level(AppChannel::Music, 40).unwrap();
        assert_eq!(level(&world, "chrome.exe"), vec![0.4, 0.4]);
        assert_eq!(level(&world, "discord.exe"), vec![1.0]);
        assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
    }

    #[test]
    fn set_failure_reports_the_reread_value() {
        let world = world();
        world.borrow_mut().fail_set = true;
        let store = MemStore::default();
        let (code, snapshot) = mixer(&world, &store)
            .set_level(AppChannel::Game, 5)
            .unwrap_err();
        assert_eq!(code, "MIXER_SET_FAILED");
        assert_eq!(snapshot["game"]["percent"], 80);
        assert_eq!(snapshot["reason"], "MIXER_SET_FAILED");
    }

    #[test]
    fn device_switch_between_read_and_set_is_refused() {
        let world = world();
        world.borrow_mut().switch_before_apply = Some("dev-b".into());
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        let (code, _) = mixer.set_level(AppChannel::Game, 5).unwrap_err();
        assert_eq!(code, "MIXER_DEVICE_CHANGED");
        assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
        // The next change reads the new device first and stores a separate original for it.
        mixer.set_level(AppChannel::Game, 50).unwrap();
        let stored = store.entries.borrow().clone().unwrap();
        assert!(stored.iter().any(|entry| entry.device == "dev-b"));
    }

    #[test]
    fn every_change_rereads_sessions_first() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        let before = world.borrow().reads;
        mixer.set_muted(AppChannel::Game, true).unwrap();
        assert!(world.borrow().reads > before);
        // A session that appeared after the last read is found on the next change.
        world
            .borrow_mut()
            .sessions
            .push(("spotify.exe".into(), 0.9, false, true));
        mixer.select_music("spotify.exe").unwrap();
        mixer.set_level(AppChannel::Music, 10).unwrap();
        assert_eq!(level(&world, "spotify.exe"), vec![0.1]);
    }

    #[test]
    fn focus_lowers_music_and_restores_the_prior_level() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        let on = mixer.focus_toggle().unwrap();
        assert_eq!(on["focus"]["active"], true);
        assert_eq!(on["music"]["percent"], 20);
        let off = mixer.focus_toggle().unwrap();
        assert_eq!(off["focus"]["active"], false);
        assert_eq!(off["music"]["percent"], 65);
    }

    #[test]
    fn focus_never_raises_a_level_already_below_the_focus_level() {
        let world = world();
        world.borrow_mut().sessions[1].1 = 0.1;
        world.borrow_mut().sessions[2].1 = 0.1;
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.configure(None, 20);
        assert_eq!(mixer.focus_toggle().unwrap()["music"]["percent"], 10);
    }

    #[test]
    fn focus_uses_the_configured_level_and_a_manual_move_ends_it() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.configure(None, 35);
        assert_eq!(mixer.focus_toggle().unwrap()["music"]["percent"], 35);
        mixer.set_level(AppChannel::Music, 50).unwrap();
        assert!(!mixer.focus_active());
    }

    #[test]
    fn focus_without_a_music_session_is_honest() {
        let world = world();
        world
            .borrow_mut()
            .sessions
            .retain(|session| session.0 == "dota2.exe");
        let store = MemStore::default();
        let (code, snapshot) = mixer(&world, &store).focus_toggle().unwrap_err();
        assert_eq!(code, "MIXER_SESSION_MISSING");
        assert_eq!(snapshot["music"]["state"], "missing");
    }

    #[test]
    fn music_choice_must_be_a_running_session_and_never_dota() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        assert!(mixer.select_music("dota2.exe").is_err());
        assert!(mixer.select_music("notepad.exe").is_err());
        assert_eq!(
            mixer.select_music("Discord.exe").unwrap()["music"]["exe"],
            "discord.exe"
        );
    }

    #[test]
    fn restore_puts_originals_back_and_clears_recovery_data() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.set_level(AppChannel::Game, 10).unwrap();
        mixer.set_muted(AppChannel::Music, true).unwrap();
        let summary = mixer.restore_all("reset");
        assert_eq!(
            summary,
            RestoreSummary {
                restored: 2,
                pending: 0
            }
        );
        assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
        assert!(world.borrow().sessions.iter().all(|session| !session.2));
        assert!(store.entries.borrow().is_none());
    }

    #[test]
    fn crash_restart_restores_persisted_originals() {
        let world = world();
        let store = MemStore::default();
        {
            let mut first_run = mixer(&world, &store);
            first_run.set_level(AppChannel::Game, 20).unwrap();
            first_run.set_level(AppChannel::Music, 5).unwrap();
            // Simulated crash: the mixer is dropped without restore_all.
        }
        assert_eq!(level(&world, "dota2.exe"), vec![0.2]);
        let mut second_run = mixer(&world, &store);
        assert_eq!(second_run.recovery_pending(), 2);
        assert_eq!(
            second_run.restore_all("startup"),
            RestoreSummary {
                restored: 2,
                pending: 0
            }
        );
        assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
        assert_eq!(level(&world, "chrome.exe"), vec![0.65, 0.65]);
        assert!(store.entries.borrow().is_none());
    }

    #[test]
    fn restore_waits_for_an_absent_session_and_retries_on_the_next_read() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.set_level(AppChannel::Game, 20).unwrap();
        let dota = world.borrow().sessions[0].clone();
        world.borrow_mut().sessions.remove(0);
        assert_eq!(
            mixer.restore_all("disable"),
            RestoreSummary {
                restored: 0,
                pending: 1
            }
        );
        assert!(store.entries.borrow().is_some(), "recovery data kept");
        world.borrow_mut().sessions.insert(0, dota);
        let snapshot = mixer.snapshot();
        assert_eq!(snapshot["restore_pending"], 0);
        assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
    }

    #[test]
    fn restore_on_another_device_waits_instead_of_touching_it() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.set_level(AppChannel::Game, 20).unwrap();
        world.borrow_mut().device = "dev-b".into();
        world.borrow_mut().sessions[0].1 = 0.55;
        assert_eq!(
            mixer.restore_all("exit"),
            RestoreSummary {
                restored: 0,
                pending: 1
            }
        );
        assert_eq!(level(&world, "dota2.exe"), vec![0.55]);
    }

    #[test]
    fn restore_failure_keeps_recovery_data() {
        let world = world();
        let store = MemStore::default();
        let mut mixer = mixer(&world, &store);
        mixer.set_level(AppChannel::Game, 20).unwrap();
        world.borrow_mut().fail_set = true;
        assert_eq!(
            mixer.restore_all("exit"),
            RestoreSummary {
                restored: 0,
                pending: 1
            }
        );
        assert_eq!(store.entries.borrow().clone().unwrap().len(), 1);
    }

    #[test]
    fn unavailable_audio_is_reported_not_guessed() {
        let world = world();
        world.borrow_mut().unavailable = true;
        let store = MemStore::default();
        let snapshot = mixer(&world, &store).snapshot();
        assert_eq!(snapshot["available"], false);
        assert_eq!(snapshot["game"]["state"], "unreadable");
        assert_eq!(snapshot["game"]["percent"], Value::Null);
    }

    #[test]
    fn json_store_round_trips_and_rejects_invalid_data() {
        let dir = std::env::temp_dir().join(format!("kohbor-mixer-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = JsonOriginalStore::in_data_dir(&dir);
        assert_eq!(store.load().unwrap(), Vec::new());
        let entry = Original {
            device: "d".into(),
            exe: "dota2.exe".into(),
            level: 0.5,
            muted: true,
        };
        store.save(std::slice::from_ref(&entry)).unwrap();
        assert_eq!(store.load().unwrap(), vec![entry]);
        store.save(&[]).unwrap();
        assert!(!dir.join(ORIGINALS_FILE).exists());
        assert!(parse_originals(br#"{"schema_version":1,"entries":[{"device":"d","exe":"x","level":2.0,"muted":false}]}"#).is_err());
        assert!(parse_originals(br#"{"schema_version":2,"entries":[]}"#).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
