//! Source admission and ownership (GSI-INTAKE-SPEC §3–§4).
//!
//! Field paths are assumptions A-5/A-6 until the M0 capture spike confirms them.

use serde_json::Value;

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
pub struct Admitted {
    pub phase: Phase,
    pub ownership: Ownership,
    pub team: Option<Team>,
    /// Raw match identifier; keep in native admission state only, never persist (DDD).
    pub match_id: Option<String>,
    /// Raw SteamID64 from the payload; never persist, log or display.
    pub steamid64: Option<u64>,
    pub hero: HeroFields,
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
        | Some("DOTA_GAMERULES_STATE_WAIT_FOR_PLAYERS_TO_LOAD") => Phase::Draft,
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
    let hero = hero_fields(body.get("hero"));
    Outcome::Admitted(Box::new(Admitted {
        phase,
        ownership,
        team,
        match_id,
        steamid64,
        hero,
    }))
}

fn int_field(hero: Option<&Value>, key: &str, min: i64, max: i64) -> Field<i64> {
    match hero.and_then(|h| h.get(key)) {
        None => Field::Missing,
        Some(v) => match v.as_i64() {
            Some(n) if (min..=max).contains(&n) => Field::Value(n),
            _ => Field::Invalid,
        },
    }
}

fn hero_fields(hero: Option<&Value>) -> HeroFields {
    let hero_id = match hero.and_then(|h| h.get("name")) {
        None => Field::Missing,
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
