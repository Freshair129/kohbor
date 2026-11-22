use super::*;
use std::cell::RefCell;
use std::rc::Rc;

/// exe, session id, level, muted, readable
type FakeSession = (String, Option<String>, f32, bool, bool);
/// exe, session id, level, mute
type Applied = (String, Option<String>, Option<f32>, Option<bool>);

#[derive(Default)]
struct World {
    device: String,
    sessions: Vec<FakeSession>,
    unavailable: bool,
    fail_set: bool,
    /// Device switch that happens between `read` and `apply`.
    switch_before_apply: Option<String>,
    applied: Vec<Applied>,
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
                .map(|(exe, id, level, muted, readable)| {
                    if *readable {
                        SessionRead::Readable {
                            exe: exe.clone(),
                            id: id.clone(),
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
        session: Option<&str>,
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
        for entry in world.sessions.iter_mut().filter(|entry| {
            entry.0 == exe && entry.4 && session.is_none_or(|id| entry.1.as_deref() == Some(id))
        }) {
            if let Some(level) = level {
                entry.2 = level;
            }
            if let Some(muted) = muted {
                entry.3 = muted;
            }
            changed += 1;
        }
        if changed == 0 {
            return Err(BackendError::Missing);
        }
        world
            .applied
            .push((exe.to_owned(), session.map(str::to_owned), level, muted));
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

fn session(exe: &str, id: &str, level: f32) -> FakeSession {
    (exe.into(), Some(id.into()), level, false, true)
}

fn world() -> Rc<RefCell<World>> {
    Rc::new(RefCell::new(World {
        device: "dev-a".into(),
        sessions: vec![
            session("dota2.exe", "d1", 0.8),
            session("chrome.exe", "c1", 0.65),
            session("chrome.exe", "c2", 0.65),
            session("discord.exe", "x1", 1.0),
            session("discord.exe", "x2", 1.0),
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

fn with_discord(world: &Rc<RefCell<World>>, store: &MemStore) -> Mixer {
    let mut mixer = mixer(world, store);
    assert!(mixer.configure(
        vec![
            Slot::Kohbor,
            Slot::Music(None),
            Slot::App("discord.exe".into())
        ],
        20
    ));
    mixer
}

fn level(world: &Rc<RefCell<World>>, exe: &str) -> Vec<f32> {
    world
        .borrow()
        .sessions
        .iter()
        .filter(|entry| entry.0 == exe)
        .map(|entry| entry.2)
        .collect()
}

fn channel<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|channel| channel["id"] == id)
        .unwrap_or_else(|| panic!("no channel {id} in {snapshot}"))
}

fn ids(snapshot: &Value) -> Vec<String> {
    snapshot["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|channel| channel["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn snapshot_lists_game_first_then_the_slots_in_order() {
    let world = world();
    let store = MemStore::default();
    let snapshot = mixer(&world, &store).snapshot();
    assert_eq!(ids(&snapshot), ["game", "kohbor", "music"]);
    assert_eq!(channel(&snapshot, "game")["percent"], 80);
    assert_eq!(channel(&snapshot, "music")["exe"], "chrome.exe");
    assert_eq!(channel(&snapshot, "music")["percent"], 65);
    assert_eq!(channel(&snapshot, "kohbor")["kind"], "kohbor");
    assert_eq!(
        snapshot["app_choices"],
        json!(["chrome.exe", "discord.exe"])
    );
    let mut mixer = with_discord(&world, &store);
    let snapshot = mixer.snapshot();
    assert_eq!(
        ids(&snapshot),
        ["game", "kohbor", "music", "app:discord.exe"]
    );
    assert_eq!(channel(&snapshot, "app:discord.exe")["percent"], 100);
    assert_eq!(
        snapshot["slots"][2],
        json!({"kind": "app", "exe": "discord.exe"})
    );
}

#[test]
fn slot_rules_cap_four_channels_and_refuse_duplicates() {
    let app = |exe: &str| Slot::App(exe.into());
    assert!(valid_slots(&default_slots()));
    assert!(valid_slots(&[]));
    assert!(valid_slots(&[
        app("discord.exe"),
        Slot::Kohbor,
        app("spotify.exe")
    ]));
    assert!(!valid_slots(&[
        Slot::Kohbor,
        Slot::Music(None),
        app("a.exe"),
        app("b.exe")
    ]));
    assert!(!valid_slots(&[Slot::Kohbor, Slot::Kohbor]));
    assert!(!valid_slots(&[
        Slot::Music(None),
        Slot::Music(Some("spotify.exe".into()))
    ]));
    assert!(!valid_slots(&[app("dota2.exe")]));
    assert!(!valid_slots(&[app("discord.exe"), app("discord.exe")]));
    assert!(!valid_slots(&[
        Slot::Music(Some("discord.exe".into())),
        app("discord.exe")
    ]));
    assert!(
        !valid_slots(&[app("Discord.exe")]),
        "stored names are lower case"
    );
    assert!(!valid_slots(&[app("..\\x.exe")]));
    let world = world();
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    assert!(!mixer.configure(vec![Slot::Kohbor, Slot::Kohbor], 20));
    assert_eq!(
        ids(&mixer.snapshot()),
        ["game", "kohbor", "music"],
        "invalid slots leave the old ones"
    );
}

#[test]
fn slot_json_round_trips_and_rejects_unknown_shapes() {
    for slot in [
        Slot::Kohbor,
        Slot::Music(None),
        Slot::Music(Some("spotify.exe".into())),
        Slot::App("discord.exe".into()),
    ] {
        assert_eq!(Slot::from_json(&slot.to_json()), Some(slot));
    }
    assert_eq!(
        Slot::from_json(&json!({"kind": "app", "exe": "Discord.exe"})),
        Some(Slot::App("discord.exe".into()))
    );
    assert_eq!(Slot::from_json(&json!({"kind": "app"})), None);
    assert_eq!(Slot::from_json(&json!({"kind": "app", "exe": "x"})), None);
    assert_eq!(
        Slot::from_json(&json!({"kind": "kohbor", "exe": "a.exe"})),
        None
    );
    assert_eq!(Slot::from_json(&json!({"kind": "game"})), None);
    assert_eq!(
        Slot::from_json(&json!({"kind": "music", "exe": null, "x": 1})),
        None
    );
}

#[test]
fn default_music_follows_the_approved_order_and_skips_apps_with_their_own_slot() {
    let world = world();
    {
        let mut world = world.borrow_mut();
        world.sessions.retain(|entry| entry.0 != "chrome.exe");
        world.sessions.push(session("spotify.exe", "s1", 0.5));
        world.sessions.push(session("firefox.exe", "f1", 0.4));
    }
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    assert_eq!(channel(&mixer.snapshot(), "music")["exe"], "firefox.exe");
    assert!(mixer.configure(vec![Slot::Music(None), Slot::App("firefox.exe".into())], 20));
    assert_eq!(channel(&mixer.snapshot(), "music")["exe"], "spotify.exe");
}

#[test]
fn missing_and_unreadable_sessions_are_never_controllable() {
    let world = world();
    world.borrow_mut().sessions = vec![("dota2.exe".into(), None, 0.8, false, false)];
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    let snapshot = mixer.snapshot();
    assert_eq!(channel(&snapshot, "game")["state"], "unreadable");
    assert_eq!(channel(&snapshot, "game")["percent"], Value::Null);
    assert_eq!(channel(&snapshot, "music")["state"], "missing");
    let discord = channel(&snapshot, "app:discord.exe");
    assert_eq!(
        discord["state"], "missing",
        "a chosen app without a session stays honest"
    );
    assert_eq!(discord["exe"], "discord.exe");
    let (code, _) = mixer.set_level("game", 20).unwrap_err();
    assert_eq!(code, "MIXER_SESSION_UNREADABLE");
    let (code, _) = mixer.set_level("music", 20).unwrap_err();
    assert_eq!(code, "MIXER_SESSION_MISSING");
    let (code, _) = mixer.set_level("app:discord.exe", 20).unwrap_err();
    assert_eq!(code, "MIXER_SESSION_MISSING");
    let (code, _) = mixer.set_level("app:spotify.exe", 20).unwrap_err();
    assert_eq!(
        code, "MIXER_CHANNEL_UNKNOWN",
        "only configured slots can be set"
    );
    assert!(world.borrow().applied.is_empty());
    assert!(
        store.entries.borrow().is_none(),
        "nothing stored for an unchanged session"
    );
}

#[test]
fn an_app_with_several_sessions_moves_them_all_together() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    let reply = mixer.set_level("app:discord.exe", 40).unwrap();
    assert_eq!(level(&world, "discord.exe"), vec![0.4, 0.4]);
    assert_eq!(channel(&reply, "app:discord.exe")["percent"], 40);
    assert_eq!(level(&world, "chrome.exe"), vec![0.65, 0.65]);
    assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
    mixer.set_muted("app:discord.exe", true).unwrap();
    assert!(world
        .borrow()
        .sessions
        .iter()
        .filter(|entry| entry.0 == "discord.exe")
        .all(|entry| entry.3));
    // One recovery entry for the whole app because its sessions agreed.
    let stored = store.entries.borrow().clone().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].session, None);
}

#[test]
fn differing_sessions_show_a_mixed_range_never_one_value() {
    let world = world();
    world.borrow_mut().sessions[4].2 = 0.3;
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    let snapshot = mixer.snapshot();
    let discord = channel(&snapshot, "app:discord.exe");
    assert_eq!(discord["state"], "mixed");
    assert_eq!(discord["percent"], Value::Null);
    assert_eq!(discord["range"], json!([30, 100]));
    assert_eq!(discord["muted"], false, "mute agrees");
    world.borrow_mut().sessions[4].3 = true;
    assert_eq!(
        channel(&mixer.snapshot(), "app:discord.exe")["muted"],
        Value::Null
    );
}

#[test]
fn moving_a_mixed_app_records_each_session_and_restores_each_one() {
    let world = world();
    world.borrow_mut().sessions[4].2 = 0.3;
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    mixer.set_level("app:discord.exe", 50).unwrap();
    assert_eq!(level(&world, "discord.exe"), vec![0.5, 0.5]);
    let stored = store.entries.borrow().clone().unwrap();
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|entry| entry.session.is_some()));
    assert_eq!(
        mixer.restore_all("reset"),
        RestoreSummary {
            restored: 2,
            pending: 0
        }
    );
    assert_eq!(level(&world, "discord.exe"), vec![1.0, 0.3]);
    assert!(store.entries.borrow().is_none());
}

#[test]
fn a_mixed_session_without_an_id_is_not_changed() {
    let world = world();
    world.borrow_mut().sessions[4] = ("discord.exe".into(), None, 0.3, false, true);
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    let (code, _) = mixer.set_level("app:discord.exe", 50).unwrap_err();
    assert_eq!(code, "MIXER_SESSION_UNREADABLE");
    assert!(world.borrow().applied.is_empty());
}

#[test]
fn an_ended_session_is_dropped_from_recovery_not_kept_forever() {
    let world = world();
    world.borrow_mut().sessions[4].2 = 0.3;
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    mixer.set_level("app:discord.exe", 50).unwrap();
    world.borrow_mut().sessions.remove(4);
    assert_eq!(
        mixer.restore_all("exit"),
        RestoreSummary {
            restored: 1,
            pending: 0
        }
    );
    assert_eq!(level(&world, "discord.exe"), vec![1.0]);
    assert!(store.entries.borrow().is_none());
}

#[test]
fn first_change_persists_the_original_before_applying() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    mixer.set_level("game", 30).unwrap();
    let stored = store.entries.borrow().clone().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].exe, "dota2.exe");
    assert!((stored[0].level - 0.8).abs() < 1e-6);
    assert_eq!(level(&world, "dota2.exe"), vec![0.3]);
    // A second change keeps the first original.
    mixer.set_level("game", 10).unwrap();
    assert!((store.entries.borrow().clone().unwrap()[0].level - 0.8).abs() < 1e-6);
}

#[test]
fn a_failed_recovery_write_changes_nothing() {
    let world = world();
    let store = MemStore::default();
    *store.fail.borrow_mut() = true;
    let mut mixer = mixer(&world, &store);
    let (code, snapshot) = mixer.set_level("game", 30).unwrap_err();
    assert_eq!(code, "MIXER_RECOVERY_WRITE_FAILED");
    assert_eq!(channel(&snapshot, "game")["percent"], 80);
    assert!(world.borrow().applied.is_empty());
}

#[test]
fn set_failure_reports_the_reread_value() {
    let world = world();
    world.borrow_mut().fail_set = true;
    let store = MemStore::default();
    let (code, snapshot) = mixer(&world, &store).set_level("game", 5).unwrap_err();
    assert_eq!(code, "MIXER_SET_FAILED");
    assert_eq!(channel(&snapshot, "game")["percent"], 80);
    assert_eq!(snapshot["reason"], "MIXER_SET_FAILED");
}

#[test]
fn device_switch_between_read_and_set_is_refused() {
    let world = world();
    world.borrow_mut().switch_before_apply = Some("dev-b".into());
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    let (code, _) = mixer.set_level("game", 5).unwrap_err();
    assert_eq!(code, "MIXER_DEVICE_CHANGED");
    assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
    // The next change reads the new device first and stores a separate original for it.
    mixer.set_level("game", 50).unwrap();
    let stored = store.entries.borrow().clone().unwrap();
    assert!(stored.iter().any(|entry| entry.device == "dev-b"));
}

#[test]
fn every_change_rereads_sessions_first() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    let before = world.borrow().reads;
    mixer.set_muted("game", true).unwrap();
    assert!(world.borrow().reads > before);
    // A session that appeared after the last read is found on the next change.
    world
        .borrow_mut()
        .sessions
        .push(session("spotify.exe", "s1", 0.9));
    let (_, slots) = mixer.select_music("spotify.exe").unwrap();
    assert_eq!(slots[1], Slot::Music(Some("spotify.exe".into())));
    mixer.set_level("music", 10).unwrap();
    assert_eq!(level(&world, "spotify.exe"), vec![0.1]);
}

#[test]
fn focus_lowers_every_non_game_app_channel_and_restores_each() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    let on = mixer.focus_toggle().unwrap();
    assert_eq!(on["focus"]["active"], true);
    assert_eq!(channel(&on, "music")["percent"], 20);
    assert_eq!(channel(&on, "app:discord.exe")["percent"], 20);
    assert_eq!(channel(&on, "game")["percent"], 80, "Game is never lowered");
    assert_eq!(on["focus"]["channels"].as_array().unwrap().len(), 2);
    let off = mixer.focus_toggle().unwrap();
    assert_eq!(off["focus"]["active"], false);
    assert_eq!(channel(&off, "music")["percent"], 65);
    assert_eq!(channel(&off, "app:discord.exe")["percent"], 100);
}

#[test]
fn focus_on_a_mixed_app_lowers_and_restores_each_session() {
    let world = world();
    world.borrow_mut().sessions[4].2 = 0.1;
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    mixer.focus_toggle().unwrap();
    assert_eq!(level(&world, "discord.exe"), vec![0.2, 0.1], "never raised");
    mixer.focus_toggle().unwrap();
    assert_eq!(level(&world, "discord.exe"), vec![1.0, 0.1]);
}

#[test]
fn focus_never_raises_a_level_already_below_the_focus_level() {
    let world = world();
    world.borrow_mut().sessions[1].2 = 0.1;
    world.borrow_mut().sessions[2].2 = 0.1;
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    assert_eq!(
        channel(&mixer.focus_toggle().unwrap(), "music")["percent"],
        10
    );
}

#[test]
fn focus_uses_the_configured_level_and_a_manual_move_ends_it_for_that_channel() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    mixer.configure(
        vec![
            Slot::Kohbor,
            Slot::Music(None),
            Slot::App("discord.exe".into()),
        ],
        35,
    );
    assert_eq!(
        channel(&mixer.focus_toggle().unwrap(), "music")["percent"],
        35
    );
    mixer.set_level("music", 50).unwrap();
    assert!(mixer.focus_active(), "Discord is still lowered");
    let off = mixer.focus_toggle().unwrap();
    assert_eq!(
        channel(&off, "music")["percent"],
        50,
        "the manual level is kept"
    );
    assert_eq!(channel(&off, "app:discord.exe")["percent"], 100);
    mixer.focus_toggle().unwrap();
    mixer.set_level("music", 40).unwrap();
    mixer.set_level("app:discord.exe", 40).unwrap();
    assert!(!mixer.focus_active());
}

#[test]
fn focus_skips_missing_apps_and_is_honest_when_nothing_can_be_lowered() {
    let world = world();
    world
        .borrow_mut()
        .sessions
        .retain(|entry| entry.0 != "discord.exe");
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    let on = mixer.focus_toggle().unwrap();
    assert_eq!(channel(&on, "music")["percent"], 20);
    assert_eq!(channel(&on, "app:discord.exe")["state"], "missing");
    mixer.focus_toggle().unwrap();
    world
        .borrow_mut()
        .sessions
        .retain(|entry| entry.0 == "dota2.exe");
    let (code, snapshot) = mixer.focus_toggle().unwrap_err();
    assert_eq!(code, "MIXER_SESSION_MISSING");
    assert_eq!(channel(&snapshot, "music")["state"], "missing");
}

#[test]
fn removing_a_slot_drops_its_focus_entry() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    mixer.focus_toggle().unwrap();
    mixer.configure(default_slots(), 20);
    let snapshot = mixer.snapshot();
    assert_eq!(snapshot["focus"]["channels"].as_array().unwrap().len(), 1);
    assert_eq!(snapshot["focus"]["channels"][0]["id"], "music");
}

#[test]
fn music_choice_must_be_a_running_session_never_dota_nor_an_app_slot() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    assert!(mixer.select_music("dota2.exe").is_err());
    assert!(mixer.select_music("notepad.exe").is_err());
    assert!(
        mixer.select_music("discord.exe").is_err(),
        "Discord has its own slot"
    );
    let (snapshot, _) = mixer.select_music("Chrome.exe").unwrap();
    assert_eq!(channel(&snapshot, "music")["exe"], "chrome.exe");
    let mut no_music = mixer;
    no_music.configure(vec![Slot::Kohbor], 20);
    assert_eq!(
        no_music.select_music("chrome.exe").unwrap_err().0,
        "MIXER_CHANNEL_UNKNOWN"
    );
}

#[test]
fn restore_puts_originals_back_and_clears_recovery_data() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = with_discord(&world, &store);
    mixer.set_level("game", 10).unwrap();
    mixer.set_muted("music", true).unwrap();
    mixer.set_level("app:discord.exe", 5).unwrap();
    assert_eq!(
        mixer.restore_all("reset"),
        RestoreSummary {
            restored: 3,
            pending: 0
        }
    );
    assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
    assert_eq!(level(&world, "discord.exe"), vec![1.0, 1.0]);
    assert!(world.borrow().sessions.iter().all(|entry| !entry.3));
    assert!(store.entries.borrow().is_none());
}

#[test]
fn crash_restart_restores_persisted_originals() {
    let world = world();
    world.borrow_mut().sessions[4].2 = 0.3;
    let store = MemStore::default();
    {
        let mut first_run = with_discord(&world, &store);
        first_run.set_level("game", 20).unwrap();
        first_run.set_level("music", 5).unwrap();
        first_run.set_level("app:discord.exe", 60).unwrap();
        // Simulated crash: the mixer is dropped without restore_all.
    }
    assert_eq!(level(&world, "dota2.exe"), vec![0.2]);
    let mut second_run = mixer(&world, &store);
    assert_eq!(second_run.recovery_pending(), 4);
    assert_eq!(
        second_run.restore_all("startup"),
        RestoreSummary {
            restored: 4,
            pending: 0
        }
    );
    assert_eq!(level(&world, "dota2.exe"), vec![0.8]);
    assert_eq!(level(&world, "chrome.exe"), vec![0.65, 0.65]);
    assert_eq!(level(&world, "discord.exe"), vec![1.0, 0.3]);
    assert!(store.entries.borrow().is_none());
}

#[test]
fn restore_waits_for_an_absent_session_and_retries_on_the_next_read() {
    let world = world();
    let store = MemStore::default();
    let mut mixer = mixer(&world, &store);
    mixer.set_level("game", 20).unwrap();
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
    mixer.set_level("game", 20).unwrap();
    world.borrow_mut().device = "dev-b".into();
    world.borrow_mut().sessions[0].2 = 0.55;
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
    mixer.set_level("game", 20).unwrap();
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
    let snapshot = with_discord(&world, &store).snapshot();
    assert_eq!(snapshot["available"], false);
    assert_eq!(
        ids(&snapshot),
        ["game", "kohbor", "music", "app:discord.exe"]
    );
    assert_eq!(channel(&snapshot, "game")["state"], "unreadable");
    assert_eq!(channel(&snapshot, "app:discord.exe")["state"], "unreadable");
    assert_eq!(channel(&snapshot, "game")["percent"], Value::Null);
}

#[test]
fn json_store_round_trips_and_rejects_invalid_data() {
    let dir = std::env::temp_dir().join(format!("kohbor-mixer-store-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = JsonOriginalStore::in_data_dir(&dir);
    assert_eq!(store.load().unwrap(), Vec::new());
    let entries = vec![
        Original {
            device: "d".into(),
            exe: "dota2.exe".into(),
            session: None,
            level: 0.5,
            muted: true,
        },
        Original {
            device: "d".into(),
            exe: "discord.exe".into(),
            session: Some("x2".into()),
            level: 0.3,
            muted: false,
        },
    ];
    store.save(&entries).unwrap();
    assert_eq!(store.load().unwrap(), entries);
    let text = std::fs::read_to_string(dir.join(ORIGINALS_FILE)).unwrap();
    assert_eq!(
        text.matches("\"session\"").count(),
        1,
        "an app-wide entry keeps the old shape"
    );
    store.save(&[]).unwrap();
    assert!(!dir.join(ORIGINALS_FILE).exists());
    // Recovery data written before slots existed (no session field) still loads.
    assert_eq!(
        parse_originals(br#"{"schema_version":1,"entries":[{"device":"d","exe":"x.exe","level":0.4,"muted":false}]}"#)
            .unwrap()[0]
            .session,
        None
    );
    assert!(parse_originals(
        br#"{"schema_version":1,"entries":[{"device":"d","exe":"x","level":2.0,"muted":false}]}"#
    )
    .is_err());
    assert!(parse_originals(br#"{"schema_version":1,"entries":[{"device":"d","exe":"x","session":"","level":0.2,"muted":false}]}"#).is_err());
    assert!(parse_originals(br#"{"schema_version":2,"entries":[]}"#).is_err());
    let _ = std::fs::remove_dir_all(dir);
}
