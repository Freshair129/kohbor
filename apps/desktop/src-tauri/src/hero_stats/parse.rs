use super::request::LaneRole;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    TooLarge,
    InvalidBody,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    pub(crate) games: u64,
    pub(crate) wins: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HeroStatsRow {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) localized_name: String,
    pub(crate) roles: Vec<String>,
    pub(crate) brackets: BTreeMap<String, Counts>,
}

pub(crate) type HeroStatsTable = BTreeMap<u32, HeroStatsRow>;
pub(crate) type LaneRoleTable = BTreeMap<u32, Counts>;
pub(crate) type DurationTable = BTreeMap<u32, Counts>;

pub(crate) fn valid_hero_stats_table(table: &HeroStatsTable) -> bool {
    !table.is_empty()
        && table.iter().all(|(id, hero)| {
            *id > 0
                && hero.id == *id
                && valid_hero_key(&hero.name)
                && !hero.localized_name.trim().is_empty()
                && hero.roles.iter().all(|role| !role.trim().is_empty())
                && hero.brackets.len() == 9
                && (1..=8)
                    .map(|bracket| bracket.to_string())
                    .chain(std::iter::once("pub".to_owned()))
                    .all(|bracket| {
                        hero.brackets
                            .get(&bracket)
                            .is_some_and(|counts| counts.wins <= counts.games)
                    })
        })
}

pub(crate) fn valid_lane_role_table(table: &LaneRoleTable) -> bool {
    table
        .iter()
        .all(|(hero_id, counts)| *hero_id > 0 && counts.wins <= counts.games)
}

pub(crate) fn valid_duration_table(table: &DurationTable) -> bool {
    table.values().all(|counts| counts.wins <= counts.games)
}

fn parse_root(body: &[u8]) -> Result<Value, ParseError> {
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(ParseError::TooLarge);
    }
    serde_json::from_slice(body).map_err(|_| ParseError::InvalidBody)
}

fn required_u64(row: &Value, field: &str) -> Result<u64, ParseError> {
    row.get(field)
        .and_then(Value::as_u64)
        .ok_or(ParseError::InvalidBody)
}

fn add_checked(left: u64, right: u64) -> Result<u64, ParseError> {
    left.checked_add(right).ok_or(ParseError::InvalidBody)
}

fn valid_hero_key(name: &str) -> bool {
    name.strip_prefix("npc_dota_hero_").is_some_and(|tail| {
        !tail.is_empty()
            && tail
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    })
}

fn parse_brackets(row: &Value) -> Result<BTreeMap<String, Counts>, ParseError> {
    let mut result = BTreeMap::new();
    for bracket in (1..=8)
        .map(|value| value.to_string())
        .chain(std::iter::once("pub".to_owned()))
    {
        let games = required_u64(row, &format!("{bracket}_pick"))?;
        let wins = required_u64(row, &format!("{bracket}_win"))?;
        if wins > games {
            return Err(ParseError::InvalidBody);
        }
        result.insert(bracket, Counts { games, wins });
    }
    Ok(result)
}

/// Strictly parse the public hero table. Duplicate hero rows are accepted only when each carries
/// a distinct provider `time` partition; the bracket counts are then summed with checked arithmetic.
pub(crate) fn parse_hero_stats(body: &[u8]) -> Result<HeroStatsTable, ParseError> {
    let root = parse_root(body)?;
    let rows = root.as_array().ok_or(ParseError::InvalidBody)?;
    if rows.is_empty() {
        return Err(ParseError::InvalidBody);
    }
    let mut result = HeroStatsTable::new();
    let mut times = HashMap::<u32, HashSet<u64>>::new();
    let mut partitioned = HashMap::<u32, bool>::new();
    for row in rows {
        let id = u32::try_from(required_u64(row, "id")?).map_err(|_| ParseError::InvalidBody)?;
        if id == 0 {
            return Err(ParseError::InvalidBody);
        }
        let name = row
            .get("name")
            .and_then(Value::as_str)
            .ok_or(ParseError::InvalidBody)?;
        let localized_name = row
            .get("localized_name")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or(ParseError::InvalidBody)?;
        if !valid_hero_key(name) {
            return Err(ParseError::InvalidBody);
        }
        let roles = row
            .get("roles")
            .and_then(Value::as_array)
            .ok_or(ParseError::InvalidBody)?
            .iter()
            .map(|role| {
                role.as_str()
                    .filter(|role| !role.trim().is_empty())
                    .map(str::to_owned)
                    .ok_or(ParseError::InvalidBody)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let brackets = parse_brackets(row)?;
        let time = row
            .get("time")
            .map(|value| value.as_u64().ok_or(ParseError::InvalidBody))
            .transpose()?;
        let next = HeroStatsRow {
            id,
            name: name.to_owned(),
            localized_name: localized_name.to_owned(),
            roles,
            brackets,
        };
        if let Some(previous) = result.get_mut(&id) {
            let Some(time) = time else {
                return Err(ParseError::InvalidBody);
            };
            if partitioned.get(&id) != Some(&true)
                || !times.entry(id).or_default().insert(time)
                || previous.name != next.name
                || previous.localized_name != next.localized_name
                || previous.roles != next.roles
            {
                return Err(ParseError::InvalidBody);
            }
            for (bracket, counts) in &next.brackets {
                let prior = previous
                    .brackets
                    .get_mut(bracket)
                    .ok_or(ParseError::InvalidBody)?;
                prior.games = add_checked(prior.games, counts.games)?;
                prior.wins = add_checked(prior.wins, counts.wins)?;
                if prior.wins > prior.games {
                    return Err(ParseError::InvalidBody);
                }
            }
        } else {
            if let Some(time) = time {
                times.entry(id).or_default().insert(time);
            }
            partitioned.insert(id, time.is_some());
            result.insert(id, next);
        }
    }
    Ok(result)
}

fn decimal_string(row: &Value, field: &str) -> Result<u64, ParseError> {
    let value = row
        .get(field)
        .and_then(Value::as_str)
        .ok_or(ParseError::InvalidBody)?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ParseError::InvalidBody);
    }
    value.parse().map_err(|_| ParseError::InvalidBody)
}

/// Parse one fixed lane-role route. Rows split by distinct `time` values are summed; duplicate
/// `(hero_id, time)` rows and rows for a different requested lane invalidate the whole body.
pub(crate) fn parse_lane_roles(
    body: &[u8],
    requested: LaneRole,
) -> Result<LaneRoleTable, ParseError> {
    let root = parse_root(body)?;
    let rows = root.as_array().ok_or(ParseError::InvalidBody)?;
    let mut result = LaneRoleTable::new();
    let mut times = HashMap::<u32, HashSet<u64>>::new();
    for row in rows {
        let hero_id =
            u32::try_from(required_u64(row, "hero_id")?).map_err(|_| ParseError::InvalidBody)?;
        if hero_id == 0 || required_u64(row, "lane_role")? != u64::from(requested.api_number()) {
            return Err(ParseError::InvalidBody);
        }
        let time = required_u64(row, "time")?;
        let games = decimal_string(row, "games")?;
        let wins = decimal_string(row, "wins")?;
        if wins > games || !times.entry(hero_id).or_default().insert(time) {
            return Err(ParseError::InvalidBody);
        }
        let current = result.entry(hero_id).or_default();
        current.games = add_checked(current.games, games)?;
        current.wins = add_checked(current.wins, wins)?;
        if current.wins > current.games {
            return Err(ParseError::InvalidBody);
        }
    }
    Ok(result)
}

/// The durations response shape remains unverified against a live provider read. Unknown types,
/// wins above games and duplicate bins without distinct `time` partitions fail as invalid bodies.
pub(crate) fn parse_durations(body: &[u8]) -> Result<DurationTable, ParseError> {
    let root = parse_root(body)?;
    let rows = root.as_array().ok_or(ParseError::InvalidBody)?;
    let mut result = DurationTable::new();
    let mut times = HashMap::<u32, HashSet<u64>>::new();
    let mut unpartitioned = HashSet::<u32>::new();
    for row in rows {
        let bin = u32::try_from(required_u64(row, "duration_bin")?)
            .map_err(|_| ParseError::InvalidBody)?;
        let games = required_u64(row, "games_played")?;
        let wins = required_u64(row, "wins")?;
        let time = row
            .get("time")
            .map(|value| value.as_u64().ok_or(ParseError::InvalidBody))
            .transpose()?;
        if wins > games {
            return Err(ParseError::InvalidBody);
        }
        if result.contains_key(&bin) {
            match time {
                Some(time) if !unpartitioned.contains(&bin) => {
                    if !times.entry(bin).or_default().insert(time) {
                        return Err(ParseError::InvalidBody);
                    }
                }
                _ => return Err(ParseError::InvalidBody),
            }
        } else if let Some(time) = time {
            if !times.entry(bin).or_default().insert(time) {
                return Err(ParseError::InvalidBody);
            }
        } else {
            unpartitioned.insert(bin);
        }
        let current = result.entry(bin).or_default();
        current.games = add_checked(current.games, games)?;
        current.wins = add_checked(current.wins, wins)?;
        if current.wins > current.games {
            return Err(ParseError::InvalidBody);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{parse_durations, parse_hero_stats, parse_lane_roles, Counts, ParseError};
    use crate::hero_stats::request::LaneRole;
    use serde_json::json;

    fn hero_row(id: u32) -> serde_json::Value {
        let mut row = json!({
            "id": id,
            "name": format!("npc_dota_hero_test_{id}"),
            "localized_name": format!("Test {id}"),
            "roles": ["Carry"],
            "pub_pick": 1000,
            "pub_win": 500
        });
        for bracket in 1..=8 {
            row[format!("{bracket}_pick")] = json!(1000);
            row[format!("{bracket}_win")] = json!(500);
        }
        row
    }

    #[test]
    fn parsers_reject_missing_fields_wrong_lane_role_and_wins_above_games() {
        let mut hero = hero_row(1);
        hero.as_object_mut().unwrap().remove("localized_name");
        assert_eq!(
            parse_hero_stats(&json!([hero]).to_string().into_bytes()),
            Err(ParseError::InvalidBody)
        );
        assert_eq!(
            parse_lane_roles(
                br#"[{"hero_id":1,"lane_role":1,"time":900,"games":"12","wins":"13"}]"#,
                LaneRole::Safe
            ),
            Err(ParseError::InvalidBody)
        );
        assert_eq!(
            parse_lane_roles(
                br#"[{"hero_id":1,"lane_role":2,"time":900,"games":"12","wins":"2"}]"#,
                LaneRole::Safe
            ),
            Err(ParseError::InvalidBody)
        );
        assert_eq!(
            parse_durations(br#"[{"duration_bin":900,"games_played":20,"wins":21}]"#),
            Err(ParseError::InvalidBody)
        );
    }

    #[test]
    fn parsers_accept_decimal_lane_counts_and_sum_only_distinct_time_partitions() {
        let lane = br#"[
            {"hero_id":1,"lane_role":1,"time":900,"games":"60","wins":"30"},
            {"hero_id":1,"lane_role":1,"time":1800,"games":"40","wins":"20"}
        ]"#;
        let parsed = parse_lane_roles(lane, LaneRole::Safe).unwrap();
        assert_eq!(
            parsed[&1],
            Counts {
                games: 100,
                wins: 50
            }
        );
        let mut first = hero_row(1);
        first["time"] = json!(900);
        let mut second = hero_row(1);
        second["time"] = json!(1800);
        let parsed_heroes =
            parse_hero_stats(&json!([first, second]).to_string().into_bytes()).unwrap();
        assert_eq!(
            parsed_heroes[&1].brackets["pub"],
            Counts {
                games: 2000,
                wins: 1000
            }
        );
        assert_eq!(
            parse_lane_roles(
                br#"[{"hero_id":1,"lane_role":1,"time":900,"games":"12x","wins":"2"}]"#,
                LaneRole::Safe
            ),
            Err(ParseError::InvalidBody)
        );
    }

    #[test]
    fn parsers_reject_truncated_and_oversized_bodies_without_using_partial_rows() {
        assert_eq!(
            parse_hero_stats(br#"[{"id":1"#),
            Err(ParseError::InvalidBody)
        );
        assert_eq!(
            parse_durations(br#"[{"duration_bin":900,"games_played":20,"wins":10"#),
            Err(ParseError::InvalidBody)
        );
        let too_large = vec![b' '; super::MAX_RESPONSE_BYTES + 1];
        assert_eq!(
            parse_lane_roles(&too_large, LaneRole::Safe),
            Err(ParseError::TooLarge)
        );
    }
}
