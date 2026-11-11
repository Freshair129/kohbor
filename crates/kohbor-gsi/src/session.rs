//! Session start and continuity (GSI-INTAKE-SPEC §6). Raw match and platform
//! identifiers are reduced to keyed hashes before they are stored.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

fn keyed(key: &[u8; 32], label: &str, value: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("any key length");
    mac.update(label.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentSession {
    pub session_id: String,
    pub match_key: String,
    pub owner_key: String,
    pub epoch: u64,
}

/// Persisted runtime state (`<data_directory>/runtime-state.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedSessions {
    pub schema_version: String,
    pub last_session_epoch: u64,
    pub current_session: Option<CurrentSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionChange {
    /// No usable match evidence in this payload; identity and watermark are kept.
    NoEvidence,
    Unchanged {
        epoch: u64,
    },
    /// First payload after an app restart that matches the persisted session.
    Resumed {
        epoch: u64,
    },
    Started {
        epoch: u64,
    },
    OwnerChanged {
        epoch: u64,
    },
}

pub struct SessionTracker {
    key: [u8; 32],
    last_epoch: u64,
    current: Option<CurrentSession>,
    restored: bool,
}

impl SessionTracker {
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            last_epoch: 0,
            current: None,
            restored: false,
        }
    }

    /// Restore persisted state. Epochs never move backwards.
    pub fn restore(key: [u8; 32], state: PersistedSessions) -> Self {
        let last = state
            .current_session
            .as_ref()
            .map_or(state.last_session_epoch, |c| {
                c.epoch.max(state.last_session_epoch)
            });
        Self {
            key,
            last_epoch: last,
            current: state.current_session,
            restored: true,
        }
    }

    pub fn persisted(&self) -> PersistedSessions {
        PersistedSessions {
            schema_version: "1.0.0".into(),
            last_session_epoch: self.last_epoch,
            current_session: self.current.clone(),
        }
    }

    pub fn current(&self) -> Option<&CurrentSession> {
        self.current.as_ref()
    }

    /// Observe one admitted payload with verified ownership.
    /// `new_session_id` supplies an opaque 128-bit identifier when a session starts.
    pub fn observe(
        &mut self,
        match_id: Option<&str>,
        owner_steamid64: u64,
        new_session_id: impl FnOnce() -> String,
    ) -> SessionChange {
        let Some(match_id) = match_id else {
            return SessionChange::NoEvidence;
        };
        let match_key = keyed(&self.key, "match", match_id);
        let owner_key = keyed(&self.key, "owner", &owner_steamid64.to_string());
        let restored = std::mem::replace(&mut self.restored, false);
        if let Some(cur) = &self.current {
            if cur.match_key == match_key && cur.owner_key == owner_key {
                return if restored {
                    SessionChange::Resumed { epoch: cur.epoch }
                } else {
                    SessionChange::Unchanged { epoch: cur.epoch }
                };
            }
            if cur.match_key == match_key {
                self.last_epoch += 1;
                self.current = Some(CurrentSession {
                    session_id: new_session_id(),
                    match_key,
                    owner_key,
                    epoch: self.last_epoch,
                });
                return SessionChange::OwnerChanged {
                    epoch: self.last_epoch,
                };
            }
        }
        self.last_epoch += 1;
        self.current = Some(CurrentSession {
            session_id: new_session_id(),
            match_key,
            owner_key,
            epoch: self.last_epoch,
        });
        SessionChange::Started {
            epoch: self.last_epoch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];
    const OWNER: u64 = 76_561_197_960_265_729;

    fn sid(n: &str) -> impl FnOnce() -> String + '_ {
        move || n.to_owned()
    }

    #[test]
    fn new_match_starts_session_and_same_match_continues() {
        let mut t = SessionTracker::new(KEY);
        assert_eq!(
            t.observe(Some("8000000001"), OWNER, sid("a")),
            SessionChange::Started { epoch: 1 }
        );
        assert_eq!(
            t.observe(Some("8000000001"), OWNER, sid("b")),
            SessionChange::Unchanged { epoch: 1 }
        );
        assert_eq!(t.observe(None, OWNER, sid("c")), SessionChange::NoEvidence);
        assert_eq!(
            t.observe(Some("8000000002"), OWNER, sid("d")),
            SessionChange::Started { epoch: 2 }
        );
    }

    #[test]
    fn owner_change_bumps_epoch() {
        let mut t = SessionTracker::new(KEY);
        t.observe(Some("8000000001"), OWNER, sid("a"));
        assert_eq!(
            t.observe(Some("8000000001"), OWNER + 1, sid("b")),
            SessionChange::OwnerChanged { epoch: 2 }
        );
    }

    #[test]
    fn restart_resumes_same_session_and_epochs_never_regress() {
        let mut t = SessionTracker::new(KEY);
        t.observe(Some("8000000001"), OWNER, sid("a"));
        let saved = t.persisted();
        assert!(!serde_json::to_string(&saved)
            .unwrap()
            .contains("8000000001"));
        let mut r = SessionTracker::restore(KEY, saved.clone());
        assert_eq!(
            r.observe(Some("8000000001"), OWNER, sid("x")),
            SessionChange::Resumed { epoch: 1 }
        );
        assert_eq!(r.current().unwrap().session_id, "a");
        let mut r2 = SessionTracker::restore(KEY, saved);
        assert_eq!(
            r2.observe(Some("8000000009"), OWNER, sid("y")),
            SessionChange::Started { epoch: 2 }
        );
    }
}
