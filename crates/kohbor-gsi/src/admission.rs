//! Source admission and ownership (GSI-INTAKE-SPEC §3–§4).
//!
//! Field paths are assumptions A-5/A-6 until the M0 capture spike confirms them.

use serde_json::Value;
use std::collections::BTreeMap;

/// SteamID64 = this base + 32-bit account ID (A-7).
pub const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

pub fn steamid64_from_account_id(account_id: u32) -> u64 {
    STEAMID64_BASE + u64::from(account_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Draft,
    Pregame,
    Live,
    PostGame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Radiant,
    Dire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Verified,
    /// Platform identity unavailable; never treated as verified.
    Unverified,
    Mismatch,
}

/// Why a request or payload was not admitted. Codes map to contracts/error-catalog.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    AuthRejected,
    BodyRejected,
    /// Wrong provider or malformed required structure.
    SourceInvalid,
}

impl Rejection {
    pub fn code(self) -> &'static str {
        match self {
            Rejection::AuthRejected => "GSI_AUTH_REJECTED",
            Rejection::BodyRejected => "GSI_BODY_REJECTED",
            Rejection::SourceInvalid => "SOURCE_INVALID",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedReason {
    Spectating,
    CustomGame,
    UnknownGameState,
}

/// A raw hero field: absent, present but invalid, or an admitted value.
#[derive(Debug, Clone, PartialEq)]
pub enum Field<T> {
    Missing,
    Invalid,
    Value(T),
}

impl<T> Field<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Field::Value(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeroFields {
    pub hero_id: Field<String>,
    pub level: Field<i64>,
    pub hp: Field<i64>,
    pub max_hp: Field<i64>,
    pub mana_percent: Field<i64>,
    pub alive: Field<bool>,
    pub respawn_seconds: Field<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnCounters {
    pub gold: Field<i64>,
    pub kills: Field<i64>,
    pub kill_streak: Field<i64>,
    pub deaths: Field<i64>,
    pub assists: Field<i64>,
    pub last_hits: Field<i64>,
    pub denies: Field<i64>,
    pub gpm: Field<i64>,
    pub xpm: Field<i64>,
    pub combined_score: Field<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Admitted {
    pub phase: Phase,
    pub ownership: Ownership,
    pub team: Option<Team>,
    /// Raw match identifier; keep in native admission state only, never persist (DDD).
    pub match_id: Option<String>,
    /// Raw SteamID64 from the payload; never persist, log or display.
    pub steamid64: Option<u64>,
    pub counters: OwnCounters,
    pub hero: HeroFields,
}

/// Separate local own-hero projection so adding these fields does not change the
/// existing `Admitted` contract consumed by event detection.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnLoadout {
    pub items: Field<Vec<ItemObservation>>,
    pub abilities: Field<Vec<AbilityObservation>>,
}

impl OwnLoadout {
    pub fn unknown() -> Self {
        Self {
            items: Field::Missing,
            abilities: Field::Missing,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSlot {
    Inventory(u8),
    Backpack(u8),
    Teleport,
    Neutral,
}

impl ItemSlot {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemSlot::Inventory(index) => match index {
                0 => "inventory0",
                1 => "inventory1",
                2 => "inventory2",
                3 => "inventory3",
                4 => "inventory4",
                _ => "inventory5",
            },
            ItemSlot::Backpack(index) => match index {
                0 => "backpack0",
                1 => "backpack1",
                _ => "backpack2",
            },
            ItemSlot::Teleport => "teleport",
            ItemSlot::Neutral => "neutral",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemObservation {
    pub slot: ItemSlot,
    pub name: Field<Option<String>>,
    pub cooldown: Field<i64>,
    pub charges: Field<i64>,
    pub can_cast: Field<bool>,
    pub passive: Field<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbilityObservation {
    /// The source `abilityN` index. Only the eight schema-defined slots are retained.
    pub slot: u8,
    pub name: Field<String>,
    pub level: Field<i64>,
    pub cooldown: Field<i64>,
    pub ultimate: Field<bool>,
    pub can_cast: Field<bool>,
    pub passive: Field<bool>,
    pub active: Field<bool>,
}

/// Parse only the bounded own-hero item and ability scalars from one admitted
/// payload. Callers must gate use of the result on verified ownership.
pub fn parse_own_loadout(body: &Value) -> OwnLoadout {
    OwnLoadout {
        items: item_fields(body.get("items")),
        abilities: ability_fields(body.get("abilities")),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Rejected(Rejection),
    /// Authenticated game data, but the player is not in active play (menu etc.).
    NotPlaying,
    Unsupported(UnsupportedReason),
    Admitted(Box<Admitted>),
}

/// Constant-time byte comparison for the intake token.
pub fn token_matches(expected: &str, presented: &str) -> bool {
    let (a, b) = (expected.as_bytes(), presented.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Evaluate one parsed, size-checked body.
///
/// `platform_steamid64` is the identity derived from the signed-in Steam account
/// (A-7); `None` means it could not be read and yields `Ownership::Unverified`.
/// `session_owner` is the SteamID64 that started the current session, if any.
pub fn admit(
    body: &Value,
    expected_token: &str,
    platform_steamid64: Option<u64>,
    session_owner: Option<u64>,
) -> Outcome {
    let token = body.pointer("/auth/token").and_then(Value::as_str);
    match token {
        Some(t) if token_matches(expected_token, t) => {}
        _ => return Outcome::Rejected(Rejection::AuthRejected),
    }
    if body.pointer("/provider/appid").and_then(Value::as_u64) != Some(570) {
        return Outcome::Rejected(Rejection::SourceInvalid);
    }
    let player = match body.get("player") {
        Some(Value::Object(p)) => p,
        _ => return Outcome::NotPlaying,
    };
    // A-6: spectator/replay payloads carry per-team collections.
    if player.contains_key("team2") || player.contains_key("team3") {
        return Outcome::Unsupported(UnsupportedReason::Spectating);
    }
    if player.get("activity").and_then(Value::as_str) != Some("playing") {
        return Outcome::NotPlaying;
    }
    if let Some(name) = body.pointer("/map/customgamename").and_then(Value::as_str) {
        if !name.is_empty() {
            return Outcome::Unsupported(UnsupportedReason::CustomGame);
        }
    }
    let phase = match body.pointer("/map/game_state").and_then(Value::as_str) {
        Some("DOTA_GAMERULES_STATE_HERO_SELECTION")
        | Some("DOTA_GAMERULES_STATE_STRATEGY_TIME")
        | Some("DOTA_GAMERULES_STATE_WAIT_FOR_PLAYERS_TO_LOAD")
        // Observed in the 2026-11-11 capture between strategy time and pre-game.
        | Some("DOTA_GAMERULES_STATE_TEAM_SHOWCASE")
        | Some("DOTA_GAMERULES_STATE_WAIT_FOR_MAP_TO_LOAD") => Phase::Draft,
        Some("DOTA_GAMERULES_STATE_PRE_GAME") => Phase::Pregame,
        Some("DOTA_GAMERULES_STATE_GAME_IN_PROGRESS") => Phase::Live,
        Some("DOTA_GAMERULES_STATE_POST_GAME") => Phase::PostGame,
        _ => return Outcome::Unsupported(UnsupportedReason::UnknownGameState),
    };
    let steamid64 = player
        .get("steamid")
        .and_then(Value::as_str)
        .filter(|s| s.len() == 17 && s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|s| s.parse::<u64>().ok());
    let ownership = match (steamid64, platform_steamid64) {
        (_, None) => Ownership::Unverified,
        (None, Some(_)) => Ownership::Mismatch,
        (Some(id), Some(platform)) if id != platform => Ownership::Mismatch,
        (Some(id), Some(_)) => match session_owner {
            Some(owner) if owner != id => Ownership::Mismatch,
            _ => Ownership::Verified,
        },
    };
    let team = match player.get("team_name").and_then(Value::as_str) {
        Some("radiant") => Some(Team::Radiant),
        Some("dire") => Some(Team::Dire),
        _ => None,
    };
    let match_id = body
        .pointer("/map/matchid")
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty() && *m != "0")
        .map(str::to_owned);
    let counters = own_counters(body.get("player"), body.get("map"));
    let hero = hero_fields(body.get("hero"));
    Outcome::Admitted(Box::new(Admitted {
        phase,
        ownership,
        team,
        match_id,
        steamid64,
        counters,
        hero,
    }))
}

fn item_fields(block: Option<&Value>) -> Field<Vec<ItemObservation>> {
    const SOURCE_SLOTS: [(&str, ItemSlot); 11] = [
        ("slot0", ItemSlot::Inventory(0)),
        ("slot1", ItemSlot::Inventory(1)),
        ("slot2", ItemSlot::Inventory(2)),
        ("slot3", ItemSlot::Inventory(3)),
        ("slot4", ItemSlot::Inventory(4)),
        ("slot5", ItemSlot::Inventory(5)),
        ("slot6", ItemSlot::Backpack(0)),
        ("slot7", ItemSlot::Backpack(1)),
        ("slot8", ItemSlot::Backpack(2)),
        ("teleport0", ItemSlot::Teleport),
        ("neutral0", ItemSlot::Neutral),
    ];
    let Some(block) = block else {
        return Field::Missing;
    };
    let Value::Object(object) = block else {
        return Field::Invalid;
    };
    let mut items = Vec::new();
    for (source_key, slot) in SOURCE_SLOTS {
        if let Some(item) = object.get(source_key) {
            if items.len() == 12 {
                break;
            }
            items.push(ItemObservation {
                slot,
                name: item_name_field(item),
                cooldown: cooldown_field(item, "cooldown"),
                charges: charges_field(item),
                can_cast: bool_field(item, "can_cast"),
                passive: bool_field(item, "passive"),
            });
        }
    }
    Field::Value(items)
}

fn ability_fields(block: Option<&Value>) -> Field<Vec<AbilityObservation>> {
    let Some(block) = block else {
        return Field::Missing;
    };
    let Value::Object(object) = block else {
        return Field::Invalid;
    };

    let mut sources = BTreeMap::<u8, &Value>::new();
    for (key, value) in object {
        let Some(slot) = key
            .strip_prefix("ability")
            .and_then(|digits| digits.parse::<u8>().ok())
            .filter(|slot| *slot <= 7)
        else {
            continue;
        };
        if ability_is_hidden(value) {
            continue;
        }
        sources.entry(slot).or_insert(value);
    }

    let abilities = sources
        .into_iter()
        .take(8)
        .map(|(slot, ability)| AbilityObservation {
            slot,
            name: identifier_field(ability, "name"),
            level: scalar_int_field(ability, "level", 0, 30),
            cooldown: cooldown_field(ability, "cooldown"),
            ultimate: bool_field(ability, "ultimate"),
            can_cast: bool_field(ability, "can_cast"),
            passive: bool_field(ability, "passive"),
            active: bool_field(ability, "ability_active"),
        })
        .collect();
    Field::Value(abilities)
}

fn ability_is_hidden(ability: &Value) -> bool {
    let Some(name) = ability.get("name").and_then(Value::as_str) else {
        return false;
    };
    name.starts_with("special_bonus_")
        || name.contains("generic_hidden")
        || name.contains("attribute_bonus")
}

fn item_name_field(item: &Value) -> Field<Option<String>> {
    match item {
        Value::Object(object) => match object.get("name") {
            None => Field::Missing,
            Some(Value::String(name)) if name == "empty" => Field::Value(None),
            Some(Value::String(name)) if valid_game_identifier(name) => {
                Field::Value(Some(name.clone()))
            }
            Some(_) => Field::Invalid,
        },
        _ => Field::Invalid,
    }
}

fn identifier_field(object: &Value, key: &str) -> Field<String> {
    match object {
        Value::Object(object) => match object.get(key) {
            None => Field::Missing,
            Some(Value::String(value)) if valid_game_identifier(value) => {
                Field::Value(value.clone())
            }
            Some(_) => Field::Invalid,
        },
        _ => Field::Invalid,
    }
}

fn valid_game_identifier(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn charges_field(object: &Value) -> Field<i64> {
    match object {
        Value::Object(object) => match object.get("charges") {
            Some(value) => bounded_integer(value, 0, 999),
            None => match object.get("item_charges") {
                Some(value) => bounded_integer(value, 0, 999),
                None => Field::Missing,
            },
        },
        _ => Field::Invalid,
    }
}

fn scalar_int_field(object: &Value, key: &str, min: i64, max: i64) -> Field<i64> {
    match object {
        Value::Object(object) => match object.get(key) {
            None => Field::Missing,
            Some(value) => bounded_integer(value, min, max),
        },
        _ => Field::Invalid,
    }
}

fn bounded_integer(value: &Value, min: i64, max: i64) -> Field<i64> {
    match value.as_i64() {
        Some(value) if (min..=max).contains(&value) => Field::Value(value),
        _ => Field::Invalid,
    }
}

fn cooldown_field(object: &Value, key: &str) -> Field<i64> {
    match object {
        Value::Object(object) => match object.get(key) {
            None => Field::Missing,
            Some(value) => match value.as_f64() {
                Some(seconds) if seconds.is_finite() && (0.0..=3600.0).contains(&seconds) => {
                    Field::Value(seconds.ceil() as i64)
                }
                _ => Field::Invalid,
            },
        },
        _ => Field::Invalid,
    }
}

fn bool_field(object: &Value, key: &str) -> Field<bool> {
    match object {
        Value::Object(object) => match object.get(key) {
            None => Field::Missing,
            Some(Value::Bool(value)) => Field::Value(*value),
            Some(_) => Field::Invalid,
        },
        _ => Field::Invalid,
    }
}

fn int_field(object: Option<&Value>, key: &str, min: i64, max: i64) -> Field<i64> {
    match object.and_then(|o| o.get(key)) {
        None => Field::Missing,
        Some(v) => match v.as_i64() {
            Some(n) if (min..=max).contains(&n) => Field::Value(n),
            _ => Field::Invalid,
        },
    }
}

fn own_counters(player: Option<&Value>, map: Option<&Value>) -> OwnCounters {
    let radiant_score = int_field(map, "radiant_score", 0, 100_000);
    let dire_score = int_field(map, "dire_score", 0, 100_000);
    let combined_score = match (radiant_score, dire_score) {
        (Field::Missing, _) | (_, Field::Missing) => Field::Missing,
        (Field::Invalid, _) | (_, Field::Invalid) => Field::Invalid,
        (Field::Value(radiant), Field::Value(dire)) => radiant
            .checked_add(dire)
            .map(Field::Value)
            .unwrap_or(Field::Invalid),
    };

    OwnCounters {
        gold: int_field(player, "gold", 0, 1_000_000),
        kills: int_field(player, "kills", 0, 100_000),
        kill_streak: int_field(player, "kill_streak", 0, 100_000),
        deaths: int_field(player, "deaths", 0, 100_000),
        assists: int_field(player, "assists", 0, 10_000),
        last_hits: int_field(player, "last_hits", 0, 10_000),
        denies: int_field(player, "denies", 0, 10_000),
        gpm: int_field(player, "gpm", 0, 10_000),
        xpm: int_field(player, "xpm", 0, 10_000),
        combined_score,
    }
}

fn hero_fields(hero: Option<&Value>) -> HeroFields {
    let hero_id = match hero.and_then(|h| h.get("name")) {
        None => Field::Missing,
        // Capture 2026-11-11: an empty name is sent during hero selection before a pick.
        Some(Value::String(s)) if s.is_empty() => Field::Missing,
        Some(Value::String(s)) if valid_hero_id(s) => Field::Value(s.clone()),
        Some(_) => Field::Invalid,
    };
    let alive = match hero.and_then(|h| h.get("alive")) {
        None => Field::Missing,
        Some(Value::Bool(b)) => Field::Value(*b),
        Some(_) => Field::Invalid,
    };
    let mut fields = HeroFields {
        hero_id,
        level: int_field(hero, "level", 1, 30),
        hp: int_field(hero, "health", 0, 100_000),
        max_hp: int_field(hero, "max_health", 1, 100_000),
        mana_percent: int_field(hero, "mana_percent", 0, 100),
        alive,
        respawn_seconds: int_field(hero, "respawn_seconds", 0, 600),
    };
    // Semantic rule (ICD): hp must not exceed max_hp.
    if let (Field::Value(hp), Field::Value(max)) = (&fields.hp, &fields.max_hp) {
        if hp > max {
            fields.hp = Field::Invalid;
        }
    }
    fields
}

fn valid_hero_id(s: &str) -> bool {
    s.strip_prefix("npc_dota_hero_").is_some_and(|rest| {
        (1..=100).contains(&rest.len())
            && rest
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    })
}
