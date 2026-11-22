"""DEM -> neutral match model (allow-listed, no personal data).

Runs only under the pinned gem-dota (see pin.py). The output is built field by
field from an allow-list: hero names, teams, roster slot keys, positions,
death ticks, ward/smoke/visibility events and per-minute values. Player names,
Steam IDs, account IDs and chat are never read into the model.

Coordinates: gem reports world units offset by +16384 per axis; the model
stores centred world units (the same space as GSI and the Lab viewer).
"""

from __future__ import annotations

import bisect
import hashlib
from pathlib import Path

OFFSET = 16384.0
TEAM = {2: "Radiant", 3: "Dire"}
LANE = {1: "bot", 2: "mid", 3: "top"}
NEUTRAL_SCHEMA = "kohbor.replay-neutral.v1"


def _hero_display(name: str) -> str:
    from gem.constants import hero_display

    return hero_display(name)


def _readable_source(name: str | None, heroes: set[str]) -> str | None:
    if not name:
        return None
    if name in heroes:
        return _hero_display(name)
    label = name.removeprefix("npc_dota_").replace("_", " ").strip()
    return label[:80] or None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_dem(path: Path):
    import gem

    return gem.parse(str(path))


def own_player(parsed, hero: str):
    wanted = hero if hero.startswith("npc_dota_hero_") else f"npc_dota_hero_{hero}"
    found = [p for p in parsed.players if p.hero_name == wanted]
    if len(found) != 1:
        raise ValueError(f"expected exactly one player on {wanted}, found {len(found)}")
    return found[0]


def neutral_model(parsed, hero: str, patch: str, dem_sha256: str | None = None) -> dict:
    """Build the neutral model of one parsed match for the owner's hero."""
    if parsed.parse_error or parsed.truncated_at_tick is not None or parsed.post_game_tick is None:
        raise ValueError("incomplete parse: refusing a partial match")
    clock = parsed.game_clock.game_time_at
    me = own_player(parsed, hero)
    own_team = me.team
    if own_team not in TEAM:
        raise ValueError("own team unknown")
    players = sorted(parsed.players, key=lambda p: p.player_id)
    allies = [p for p in players if p.team == own_team and p is not me]
    enemies = [p for p in players if p.team != own_team and p.team in TEAM]
    if len(allies) != 4 or len(enemies) != 5:
        raise ValueError("expected a 5v5 roster")
    keys = {me.hero_name: "self"}
    keys.update({p.hero_name: f"ally-{i + 1}" for i, p in enumerate(allies)})
    keys.update({p.hero_name: f"enemy-{i + 1}" for i, p in enumerate(enemies)})
    hero_names = set(keys)
    duration = int(parsed.duration)

    deaths_by_hero: dict[str, list[dict]] = {name: [] for name in hero_names}
    self_deaths: list[dict] = []
    damage: list[tuple[int, str]] = []
    for entry in parsed.combat_log:
        kind = str(getattr(entry.log_type, "value", entry.log_type))
        source = entry.damage_source_name or entry.attacker_name
        if kind == "DAMAGE" and entry.target_name == me.hero_name and source in hero_names:
            if keys[source].startswith("enemy-"):
                damage.append((entry.tick, keys[source]))
        if kind != "DEATH" or not entry.target_is_hero or entry.target_is_illusion or entry.will_reincarnate:
            continue
        if entry.target_name not in hero_names:
            continue
        deaths_by_hero[entry.target_name].append({"tick": int(entry.tick), "killer": _readable_source(source, hero_names)})

    heroes = []
    for p in [me, *allies, *enemies]:
        log = p.position_log
        ticks = [row[0] for row in log]
        hp_ticks = list(p.times)
        times = [clock(t) for t in ticks]

        def hp_at(tick: int, _hp=p.hp_t, _ticks=hp_ticks):
            i = bisect.bisect_right(_ticks, tick) - 1
            return _hp[i] if 0 <= i < len(_hp) else None

        deaths = []
        for d in deaths_by_hero[p.hero_name]:
            i = bisect.bisect_right(ticks, d["tick"]) - 1
            if i < 0:
                continue
            st, x, y = log[i]
            deaths.append({"tick": d["tick"], "t": round(clock(d["tick"]), 3), "x": round(x - OFFSET, 1),
                           "y": round(y - OFFSET, 1), "age": round((d["tick"] - st) / 30, 3), "killer": d["killer"]})
        deaths.sort(key=lambda d: d["tick"])
        start = min([0] + [int(d["t"]) - 1 for d in deaths if d["t"] < 0])
        points = []
        for second in range(start, duration + 1):
            i = bisect.bisect_right(times, second) - 1
            if i < 0 or second - times[i] > 1.5:
                continue
            hp = hp_at(ticks[i])
            if hp is None or hp <= 0:
                continue
            points.append([second, round(log[i][1] - OFFSET, 1), round(log[i][2] - OFFSET, 1)])
        minute = {}
        gmin = list(p.game_times_min or [])
        if 600 in gmin:
            idx = gmin.index(600)
            minute = {"nw10": _at(p.net_worth_t_min, idx), "lh10": _at(p.lh_t_min, idx)}
        heroes.append({
            "key": keys[p.hero_name], "hero": _hero_display(p.hero_name), "team": TEAM[p.team],
            "points": points, "deaths": deaths, "lane": LANE.get(p.lane), "roaming": bool(p.is_roaming),
            "nw10": minute.get("nw10"), "lh10": minute.get("lh10"),
        })

    damage.sort()
    damage_ticks = [t for t, _ in damage]
    for d in heroes[0]["deaths"]:
        lo = bisect.bisect_left(damage_ticks, d["tick"] - 300)
        hi = bisect.bisect_right(damage_ticks, d["tick"])
        self_deaths.append({"tick": d["tick"], "damagers": sorted({k for _, k in damage[lo:hi]})})

    wards = []
    for w in parsed.wards:
        if w.team not in TEAM or w.ward_type not in ("observer", "sentry"):
            continue
        end = w.killed_tick if w.killed_tick is not None else w.expires_tick
        wards.append({"team": TEAM[w.team], "type": w.ward_type, "x": round(w.x - OFFSET, 1), "y": round(w.y - OFFSET, 1),
                      "t0": round(clock(w.tick), 3), "t1": round(clock(end), 3) if end is not None else None})

    smokes = [{"team": TEAM[s.team], "t": round(clock(s.tick), 3), "heroes": len(s.smoked)}
              for s in parsed.smoke_events if s.team in TEAM]

    enemy_side = "dire_state" if own_team == 2 else "radiant_state"
    visibility = []
    for v in parsed.hero_visibility_events:
        if v.hero_name != me.hero_name:
            continue
        state = getattr(v, enemy_side)
        visible = 1 if str(getattr(state, "value", state)) == "visible" else 0
        if not visibility or visibility[-1][1] != visible:
            visibility.append([round(clock(v.tick), 3), visible])

    return {
        "schema": NEUTRAL_SCHEMA,
        "match": str(parsed.match_id),
        "patch": patch,
        "team": TEAM[own_team],
        "duration": duration,
        "heroes": heroes,
        "selfDeaths": self_deaths,
        "wards": wards,
        "smokes": smokes,
        "selfVisibility": visibility,
        "provenance": {"parser": "gem-dota 0.14.0", "demSha256": dem_sha256},
    }


def _at(series, idx):
    try:
        value = series[idx]
    except (IndexError, TypeError):
        return None
    return int(value) if value is not None else None
