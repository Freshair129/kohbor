//! Synthetic tests for the local own-hero items and abilities projection.
//! These are shaped like GSI input; they are not captured game data.

use kohbor_gsi::admission::{parse_own_loadout, Field, OwnLoadout, Ownership, UnsupportedReason};
use kohbor_gsi::{
    admit, build_snapshot_with_loadout, steamid64_from_account_id, Outcome, SourceState,
};
use serde_json::{json, Value};

const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_";
const ACCOUNT: u32 = 12_345;

fn owner() -> u64 {
    steamid64_from_account_id(ACCOUNT)
}

fn payload() -> Value {
    json!({
        "provider": {"appid": 570},
        "map": {"game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "customgamename": ""},
        "player": {"steamid": owner().to_string(), "activity": "playing", "team_name": "radiant"},
        "hero": {"name": "npc_dota_hero_axe", "level": 12, "health": 850, "max_health": 1200,
                 "mana_percent": 43, "alive": true, "respawn_seconds": 0},
        "auth": {"token": TOKEN}
    })
}

fn source_item(name: &str) -> Value {
    json!({
        "name": name,
        "can_cast": true,
        "cooldown": 0.0,
        "passive": false,
        "charges": 0
    })
}

fn source_ability(name: &str) -> Value {
    json!({
        "name": name,
        "level": 1,
        "can_cast": true,
        "passive": false,
        "ability_active": false,
        "cooldown": 0.0,
        "ultimate": false
    })
}

fn gsi_payload() -> Value {
    let mut body = payload();
    body["items"] = json!({
        "slot0": source_item("item_blink"),
        "slot6": {"name": "empty"},
        "teleport0": source_item("item_tpscroll"),
        "neutral0": source_item("item_mind_breaker")
    });
    body["abilities"] = json!({
        "ability0": source_ability("axe_berserkers_call"),
        "ability1": source_ability("axe_battle_hunger"),
        "ability2": source_ability("special_bonus_strength_10")
    });
    body
}

fn admitted(p: &Value) -> Outcome {
    admit(p, TOKEN, Some(owner()), None)
}

fn own_loadout(p: &Value) -> OwnLoadout {
    parse_own_loadout(p)
}

fn source(age_ms: u64) -> SourceState {
    SourceState {
        generation: 4,
        sequence: Some(9),
        age_ms: Some(age_ms),
        session_epoch: Some(2),
        identity_revision: 1,
    }
}

fn item<'a>(loadout: &'a OwnLoadout, slot: &str) -> &'a kohbor_gsi::admission::ItemObservation {
    loadout
        .items
        .value()
        .expect("items block present")
        .iter()
        .find(|entry| entry.slot.as_str() == slot)
        .unwrap_or_else(|| panic!("missing item slot {slot}"))
}

fn ability(loadout: &OwnLoadout, slot: u8) -> &kohbor_gsi::admission::AbilityObservation {
    loadout
        .abilities
        .value()
        .expect("abilities block present")
        .iter()
        .find(|entry| entry.slot == slot)
        .unwrap_or_else(|| panic!("missing ability slot {slot}"))
}

fn schema_validator() -> jsonschema::Validator {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contracts/own-snapshot.schema.json"
    );
    let text = std::fs::read_to_string(path).expect("own-snapshot schema present");
    let schema: Value = serde_json::from_str(&text).unwrap();
    jsonschema::validator_for(&schema).expect("valid schema")
}

fn snapshot(p: &Value, age_ms: u64) -> Value {
    let outcome = admitted(p);
    build_snapshot_with_loadout(&outcome, &own_loadout(p), source(age_ms))
}

#[test]
fn missing_items_block_is_unknown_not_an_empty_inventory() {
    let p = payload();
    let a = own_loadout(&p);
    assert_eq!(a.items, Field::Missing);
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["items"],
        json!({"value": null, "availability": "unknown"})
    );
}

#[test]
fn non_object_items_block_is_invalid() {
    let mut p = payload();
    p["items"] = json!(false);
    let a = own_loadout(&p);
    assert_eq!(a.items, Field::Invalid);
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["items"],
        json!({"value": null, "availability": "invalid"})
    );
}

#[test]
fn item_slots_map_to_inventory_backpack_teleport_and_neutral() {
    let mut p = payload();
    let mut items = serde_json::Map::new();
    for slot in 0..=8 {
        items.insert(format!("slot{slot}"), source_item("item_blink"));
    }
    items.insert("teleport0".into(), source_item("item_tpscroll"));
    items.insert("neutral0".into(), source_item("item_mind_breaker"));
    p["items"] = Value::Object(items);
    let snapshot = snapshot(&p, 100);
    let slots: Vec<&str> = snapshot["own"]["fields"]["items"]["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["slot"].as_str().unwrap())
        .collect();
    assert_eq!(
        slots,
        [
            "inventory0",
            "inventory1",
            "inventory2",
            "inventory3",
            "inventory4",
            "inventory5",
            "backpack0",
            "backpack1",
            "backpack2",
            "teleport",
            "neutral"
        ]
    );
}

#[test]
fn empty_item_slot_has_a_null_name() {
    let p = gsi_payload();
    let a = own_loadout(&p);
    assert_eq!(item(&a, "backpack0").name, Field::Value(None));
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["items"]["value"][1]["name"],
        Value::Null
    );
    assert_ne!(
        snapshot["own"]["fields"]["items"]["value"][1]["name"],
        json!("empty")
    );
}

#[test]
fn unknown_or_invalid_item_name_makes_the_whole_field_invalid() {
    let mut p = payload();
    let mut bad = source_item("Item_Blink");
    bad["name"] = json!("Item_Blink");
    p["items"] = json!({
        "slot0": bad,
        "slot1": {"can_cast": true},
        "slot2": source_item("item_blink"),
    });
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["items"],
        json!({"value": null, "availability": "invalid"}),
        "a list with a dropped unknown entry is never exposed as a complete inventory"
    );
}

#[test]
fn item_names_must_be_lowercase_game_identifiers() {
    let mut p = payload();
    let mut bad = source_item("Item_Blink");
    bad["name"] = json!("Item_Blink");
    p["items"] = json!({"slot0": bad});
    let a = own_loadout(&p);
    assert_eq!(item(&a, "inventory0").name, Field::Invalid);
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["items"]["value"][0]["name"],
        Value::Null
    );
}

#[test]
fn item_values_keep_boolean_fields_and_ceil_cooldown_seconds() {
    let mut p = payload();
    let mut blink = source_item("item_blink");
    blink["can_cast"] = json!(false);
    blink["cooldown"] = json!(1.25);
    blink["passive"] = json!(true);
    blink["charges"] = json!(3);
    p["items"] = json!({"slot0": blink});
    let a = own_loadout(&p);
    let blink = item(&a, "inventory0");
    assert_eq!(blink.can_cast, Field::Value(false));
    assert_eq!(blink.cooldown, Field::Value(2));
    assert_eq!(blink.passive, Field::Value(true));
    assert_eq!(blink.charges, Field::Value(3));
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["items"]["value"][0]["cooldown"],
        2
    );
}

#[test]
fn omitted_item_scalars_remain_missing() {
    let mut p = payload();
    p["items"] = json!({"slot0": {}});
    let a = own_loadout(&p);
    let item = item(&a, "inventory0");
    assert_eq!(item.name, Field::Missing);
    assert_eq!(item.cooldown, Field::Missing);
    assert_eq!(item.charges, Field::Missing);
    assert_eq!(item.can_cast, Field::Missing);
    assert_eq!(item.passive, Field::Missing);
}

#[test]
fn wrong_typed_item_scalars_are_invalid() {
    let mut p = payload();
    p["items"] = json!({"slot0": {
        "name": 7,
        "cooldown": "2.5",
        "charges": "3",
        "can_cast": 1,
        "passive": "false"
    }});
    let a = own_loadout(&p);
    let item = item(&a, "inventory0");
    assert_eq!(item.name, Field::Invalid);
    assert_eq!(item.cooldown, Field::Invalid);
    assert_eq!(item.charges, Field::Invalid);
    assert_eq!(item.can_cast, Field::Invalid);
    assert_eq!(item.passive, Field::Invalid);
}

#[test]
fn item_cooldown_accepts_zero_and_3600_seconds() {
    let mut p = payload();
    p["items"] = json!({
        "slot0": {"name": "item_blink", "cooldown": 0.0},
        "slot1": {"name": "item_tpscroll", "cooldown": 3600.0}
    });
    let a = own_loadout(&p);
    assert_eq!(item(&a, "inventory0").cooldown, Field::Value(0));
    assert_eq!(item(&a, "inventory1").cooldown, Field::Value(3600));
}

#[test]
fn item_cooldown_rejects_negative_absurd_and_wrong_typed_values() {
    for invalid in [
        json!(-0.01),
        json!(3600.1),
        json!(u64::MAX),
        json!("1"),
        json!(true),
    ] {
        let mut p = payload();
        p["items"] = json!({"slot0": {"name": "item_blink", "cooldown": invalid}});
        assert_eq!(
            item(&own_loadout(&p), "inventory0").cooldown,
            Field::Invalid,
            "cooldown {invalid} must be invalid"
        );
    }
}

#[test]
fn item_charges_accept_inclusive_zero_and_999() {
    let mut p = payload();
    p["items"] = json!({
        "slot0": {"name": "item_blink", "charges": 0},
        "slot1": {"name": "item_flask", "charges": 999}
    });
    let a = own_loadout(&p);
    assert_eq!(item(&a, "inventory0").charges, Field::Value(0));
    assert_eq!(item(&a, "inventory1").charges, Field::Value(999));
}

#[test]
fn item_charges_reject_negative_overflow_and_wrong_type() {
    for invalid in [json!(-1), json!(1000), json!("9")] {
        let mut p = payload();
        p["items"] = json!({"slot0": {"name": "item_flask", "charges": invalid}});
        assert_eq!(
            item(&own_loadout(&p), "inventory0").charges,
            Field::Invalid,
            "charges {invalid} must be invalid"
        );
    }
}

#[test]
fn item_charges_use_item_charges_only_when_charges_is_missing() {
    let mut p = payload();
    p["items"] = json!({"slot0": {"name": "item_magic_stick", "item_charges": 4}});
    assert_eq!(
        item(&own_loadout(&p), "inventory0").charges,
        Field::Value(4)
    );

    p["items"]["slot0"]["charges"] = json!("invalid");
    assert_eq!(
        item(&own_loadout(&p), "inventory0").charges,
        Field::Invalid,
        "a present invalid primary value is not hidden by the alias"
    );
}

#[test]
fn item_parser_ignores_unknown_keys_and_keeps_at_most_twelve_entries() {
    let mut p = payload();
    let mut items = serde_json::Map::new();
    for slot in 0..=30 {
        items.insert(format!("slot{slot}"), source_item("item_blink"));
    }
    items.insert("teleport0".into(), source_item("item_tpscroll"));
    items.insert("neutral0".into(), source_item("item_mind_breaker"));
    p["items"] = Value::Object(items);
    let a = own_loadout(&p);
    let entries = a.items.value().unwrap();
    assert_eq!(entries.len(), 11);
    assert!(entries.len() <= 12);
    assert!(entries
        .iter()
        .all(|entry| entry.slot.as_str() != "inventory9"));
}

#[test]
fn missing_abilities_block_is_unknown_not_an_empty_spell_list() {
    let p = payload();
    let a = own_loadout(&p);
    assert_eq!(a.abilities, Field::Missing);
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["abilities"],
        json!({"value": null, "availability": "unknown"})
    );
}

#[test]
fn non_object_abilities_block_is_invalid() {
    let mut p = payload();
    p["abilities"] = json!(["axe_berserkers_call"]);
    let a = own_loadout(&p);
    assert_eq!(a.abilities, Field::Invalid);
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["abilities"],
        json!({"value": null, "availability": "invalid"})
    );
}

#[test]
fn ability_values_map_active_and_boolean_fields() {
    let mut p = payload();
    let mut call = source_ability("axe_berserkers_call");
    call["level"] = json!(4);
    call["can_cast"] = json!(false);
    call["passive"] = json!(true);
    call["ability_active"] = json!(true);
    call["cooldown"] = json!(1.01);
    call["ultimate"] = json!(false);
    p["abilities"] = json!({"ability0": call});
    let a = own_loadout(&p);
    let call = ability(&a, 0);
    assert_eq!(call.name, Field::Value("axe_berserkers_call".into()));
    assert_eq!(call.level, Field::Value(4));
    assert_eq!(call.can_cast, Field::Value(false));
    assert_eq!(call.passive, Field::Value(true));
    assert_eq!(call.active, Field::Value(true));
    assert_eq!(call.cooldown, Field::Value(2));
    assert_eq!(call.ultimate, Field::Value(false));
}

#[test]
fn omitted_ability_scalars_remain_missing() {
    let mut p = payload();
    p["abilities"] = json!({"ability0": {}});
    let a = own_loadout(&p);
    let ability = ability(&a, 0);
    assert_eq!(ability.name, Field::Missing);
    assert_eq!(ability.level, Field::Missing);
    assert_eq!(ability.cooldown, Field::Missing);
    assert_eq!(ability.ultimate, Field::Missing);
    assert_eq!(ability.can_cast, Field::Missing);
    assert_eq!(ability.passive, Field::Missing);
    assert_eq!(ability.active, Field::Missing);
}

#[test]
fn wrong_typed_ability_scalars_are_invalid() {
    let mut p = payload();
    p["abilities"] = json!({"ability0": {
        "name": false,
        "level": "2",
        "cooldown": "1.2",
        "ultimate": 0,
        "can_cast": "true",
        "passive": 1,
        "ability_active": "false"
    }});
    let a = own_loadout(&p);
    let ability = ability(&a, 0);
    assert_eq!(ability.name, Field::Invalid);
    assert_eq!(ability.level, Field::Invalid);
    assert_eq!(ability.cooldown, Field::Invalid);
    assert_eq!(ability.ultimate, Field::Invalid);
    assert_eq!(ability.can_cast, Field::Invalid);
    assert_eq!(ability.passive, Field::Invalid);
    assert_eq!(ability.active, Field::Invalid);
}

#[test]
fn ability_names_must_be_lowercase_game_identifiers() {
    let mut p = payload();
    p["abilities"] = json!({"ability0": source_ability("Axe_Berserkers_Call")});
    let a = own_loadout(&p);
    assert_eq!(ability(&a, 0).name, Field::Invalid);
    let snapshot = snapshot(&p, 100);
    assert_eq!(
        snapshot["own"]["fields"]["abilities"],
        json!({"value": null, "availability": "invalid"})
    );
}

#[test]
fn ability_parser_skips_talent_names() {
    let mut p = payload();
    p["abilities"] = json!({
        "ability0": source_ability("special_bonus_strength_10"),
        "ability1": source_ability("axe_berserkers_call")
    });
    let a = own_loadout(&p);
    assert_eq!(a.abilities.value().unwrap().len(), 1);
    assert_eq!(
        ability(&a, 1).name,
        Field::Value("axe_berserkers_call".into())
    );
}

#[test]
fn ability_parser_skips_generic_hidden_and_attribute_bonus_names() {
    let mut p = payload();
    p["abilities"] = json!({
        "ability0": source_ability("axe_generic_hidden_ability"),
        "ability1": source_ability("axe_attribute_bonus"),
        "ability2": source_ability("axe_berserkers_call")
    });
    let a = own_loadout(&p);
    assert_eq!(a.abilities.value().unwrap().len(), 1);
    assert_eq!(
        ability(&a, 2).name,
        Field::Value("axe_berserkers_call".into())
    );
}

#[test]
fn ability_parser_keeps_at_most_eight_source_slots() {
    let mut p = payload();
    let mut abilities = serde_json::Map::new();
    for slot in 0..=20 {
        abilities.insert(
            format!("ability{slot}"),
            source_ability(&format!("axe_ability_{slot}")),
        );
    }
    p["abilities"] = Value::Object(abilities);
    let a = own_loadout(&p);
    let entries = a.abilities.value().unwrap();
    assert_eq!(entries.len(), 8);
    assert!(entries.iter().all(|entry| entry.slot <= 7));
}

#[test]
fn ability_level_accepts_inclusive_zero_and_30() {
    let mut p = payload();
    let mut basic = source_ability("axe_berserkers_call");
    basic["level"] = json!(0);
    let mut ultimate = source_ability("axe_culling_blade");
    ultimate["level"] = json!(30);
    p["abilities"] = json!({"ability0": basic, "ability1": ultimate});
    let a = own_loadout(&p);
    assert_eq!(ability(&a, 0).level, Field::Value(0));
    assert_eq!(ability(&a, 1).level, Field::Value(30));
}

#[test]
fn ability_level_rejects_negative_overflow_and_wrong_type() {
    for invalid in [json!(-1), json!(31), json!(u64::MAX), json!("2")] {
        let mut spell = source_ability("axe_berserkers_call");
        spell["level"] = invalid.clone();
        let mut p = payload();
        p["abilities"] = json!({"ability0": spell});
        assert_eq!(
            ability(&own_loadout(&p), 0).level,
            Field::Invalid,
            "level {invalid} must be invalid"
        );
    }
}

#[test]
fn ability_cooldown_ceil_and_bounds_are_applied_in_seconds() {
    let mut p = payload();
    let mut rounded = source_ability("axe_berserkers_call");
    rounded["cooldown"] = json!(1.2);
    let mut upper = source_ability("axe_culling_blade");
    upper["cooldown"] = json!(3600.0);
    p["abilities"] = json!({"ability0": rounded, "ability1": upper});
    let a = own_loadout(&p);
    assert_eq!(ability(&a, 0).cooldown, Field::Value(2));
    assert_eq!(ability(&a, 1).cooldown, Field::Value(3600));
}

#[test]
fn ability_cooldown_rejects_negative_absurd_and_wrong_type() {
    for invalid in [
        json!(-0.1),
        json!(3600.1),
        json!(u64::MAX),
        json!("1"),
        json!(false),
    ] {
        let mut spell = source_ability("axe_berserkers_call");
        spell["cooldown"] = invalid.clone();
        let mut p = payload();
        p["abilities"] = json!({"ability0": spell});
        assert_eq!(
            ability(&own_loadout(&p), 0).cooldown,
            Field::Invalid,
            "cooldown {invalid} must be invalid"
        );
    }
}

#[test]
fn item_and_ability_availability_share_the_2000ms_freshness_boundary() {
    let p = gsi_payload();
    let fresh = snapshot(&p, 2000);
    assert_eq!(fresh["own"]["fields"]["items"]["availability"], "fresh");
    assert_eq!(fresh["own"]["fields"]["abilities"]["availability"], "fresh");
    let stale = snapshot(&p, 2001);
    assert_eq!(stale["own"]["fields"]["items"]["availability"], "stale");
    assert_eq!(stale["own"]["fields"]["abilities"]["availability"], "stale");
}

#[test]
fn unverified_ownership_exposes_no_items_or_abilities() {
    let p = gsi_payload();
    let outcome = admit(&p, TOKEN, None, None);
    match &outcome {
        Outcome::Admitted(a) => assert_eq!(a.ownership, Ownership::Unverified),
        other => panic!("expected admitted payload, got {other:?}"),
    }
    let snapshot = build_snapshot_with_loadout(&outcome, &own_loadout(&p), source(100));
    assert!(snapshot["own"].is_null());
}

#[test]
fn spectator_payload_exposes_no_items_or_abilities() {
    let mut p = gsi_payload();
    p["player"]["team2"] = json!({"player0": {"steamid": owner().to_string()}});
    let outcome = admitted(&p);
    assert_eq!(outcome, Outcome::Unsupported(UnsupportedReason::Spectating));
    let snapshot = build_snapshot_with_loadout(&outcome, &own_loadout(&p), source(100));
    assert!(snapshot["own"].is_null());
}

#[test]
fn synthetic_dota_gsi_items_and_abilities_match_the_own_snapshot_golden() {
    let mut p = payload();
    let mut blink = source_item("item_blink");
    blink["cooldown"] = json!(0.25);
    p["items"] = json!({
        "slot0": blink,
        "slot6": {"name": "empty"},
        "teleport0": {
            "name": "item_tpscroll", "can_cast": true, "cooldown": 10.01,
            "passive": false, "charges": 1
        },
        "neutral0": {
            "name": "item_mind_breaker", "can_cast": false, "cooldown": 0.0,
            "passive": true, "charges": 0
        }
    });
    let mut call = source_ability("axe_berserkers_call");
    call["level"] = json!(4);
    call["ability_active"] = json!(true);
    call["cooldown"] = json!(1.2);
    p["abilities"] = json!({
        "ability0": call,
        "ability1": source_ability("axe_battle_hunger"),
        "ability2": source_ability("special_bonus_strength_10")
    });

    let snapshot = snapshot(&p, 2000);
    assert_eq!(snapshot["schema_version"], "2.2.0");
    assert_eq!(
        snapshot["own"]["fields"]["items"],
        json!({
            "value": [
                {"slot":"inventory0", "name":"item_blink", "cooldown":1, "charges":0, "can_cast":true, "passive":false},
                {"slot":"backpack0", "name":null, "cooldown":null, "charges":null, "can_cast":null, "passive":null},
                {"slot":"teleport", "name":"item_tpscroll", "cooldown":11, "charges":1, "can_cast":true, "passive":false},
                {"slot":"neutral", "name":"item_mind_breaker", "cooldown":0, "charges":0, "can_cast":false, "passive":true}
            ],
            "availability": "fresh"
        })
    );
    assert_eq!(
        snapshot["own"]["fields"]["abilities"],
        json!({
            "value": [
                {"slot":0, "name":"axe_berserkers_call", "level":4, "cooldown":2, "ultimate":false, "can_cast":true, "passive":false, "active":true},
                {"slot":1, "name":"axe_battle_hunger", "level":1, "cooldown":0, "ultimate":false, "can_cast":true, "passive":false, "active":false}
            ],
            "availability": "fresh"
        })
    );
    let validator = schema_validator();
    let errors: Vec<String> = validator
        .iter_errors(&snapshot)
        .map(|error| error.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "snapshot violates schema: {errors:?}\n{snapshot:#}"
    );
}
