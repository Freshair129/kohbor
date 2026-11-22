"""Neutral match model -> Lab Replay report v3 (Death review v3).

Standard library only, so the rules can be tested without the DEM parser.

Every value produced here is DERIVED by a named, versioned rule in RULES. The
rule values are provisional drafts (DEC-COACH-REVIEW point 4): the owner
confirms them after 3-5 matches and they are not tuned to make a result look
better. Death types use enemy positions from the full DEM, which the player
could not see during play; the viewer labels them that way.
"""

from __future__ import annotations

import bisect
import math

REPORT_FORMAT = "kohbor.post-match-replay.v3"
OWN_KEYS = ("self", "ally-1", "ally-2", "ally-3", "ally-4")

RULES = {
    "version": "death-review-0.1",
    "provisional": True,
    "enemyPositions": "full-map",
    "deathType": {"radius": 1500, "windowSeconds": 10, "allyRadius": 1500,
                  "teamfightDeaths": 3, "teamfightSeconds": 15, "teamfightRadius": 2500},
    "vision": {"observerRadius": 1600, "visibleWindowSeconds": 10},
    "smoke": {"windowSeconds": 30},
    "lane": {"minute": 10, "threshold": 500},
    "notes": {"max": 3, "seenSeconds": 8},
}


def clock(seconds: float) -> str:
    s = int(math.floor(abs(seconds)))
    return ("-" if seconds < 0 else "") + f"{s // 60}:{s % 60:02d}"


def _dist(ax: float, ay: float, bx: float, by: float) -> float:
    return math.hypot(ax - bx, ay - by)


class _Track:
    def __init__(self, points):
        self.seconds = [p[0] for p in points]
        self.points = points

    def at(self, second: int):
        i = bisect.bisect_left(self.seconds, second)
        if i < len(self.seconds) and self.seconds[i] == second:
            return self.points[i][1], self.points[i][2]
        return None


def classify_death(death: dict, damagers: list[str], heroes: list[dict], rules: dict = RULES) -> dict:
    r = rules["deathType"]
    t, x, y = death["t"], death["x"], death["y"]
    tracks = {h["key"]: _Track(h["points"]) for h in heroes}
    me = tracks["self"]
    first, last = math.ceil(t - r["windowSeconds"]), math.floor(t)
    enemies = set(damagers)
    for h in heroes:
        if not h["key"].startswith("enemy-"):
            continue
        for second in range(first, last + 1):
            pos = tracks[h["key"]].at(second)
            mine = me.at(second) or (x, y)
            if pos and _dist(*pos, *mine) <= r["radius"]:
                enemies.add(h["key"])
                break
    allies = 0
    for h in heroes:
        if not h["key"].startswith("ally-"):
            continue
        pos = tracks[h["key"]].at(last) or tracks[h["key"]].at(last - 1)
        if pos and _dist(*pos, x, y) <= r["allyRadius"]:
            allies += 1
    nearby_deaths = sum(1 for h in heroes for d in h["deaths"]
                        if abs(d["t"] - t) <= r["teamfightSeconds"] and _dist(d["x"], d["y"], x, y) <= r["teamfightRadius"])
    if nearby_deaths >= r["teamfightDeaths"]:
        kind = "teamfight"
    elif allies == 0 and len(enemies) >= 2:
        kind = "gank"
    elif allies == 0:
        kind = "solo"
    else:
        kind = "other"
    return {"type": kind, "enemiesInvolved": len(enemies), "alliesNear": allies, "nearbyDeaths": nearby_deaths}


def visible_seconds(t: float, changes: list, window: int) -> int | None:
    if not changes:
        return None
    times = [c[0] for c in changes]
    count = 0
    for second in range(math.floor(t) - window + 1, math.floor(t) + 1):
        i = bisect.bisect_right(times, second) - 1
        if i >= 0 and changes[i][1] == 1:
            count += 1
    return count


def ward_cover(t: float, x: float, y: float, wards: list[dict], team: str, radius: float) -> bool:
    return any(w["team"] == team and w["type"] == "observer" and w["t0"] <= t
               and (w["t1"] is None or t < w["t1"]) and _dist(w["x"], w["y"], x, y) <= radius for w in wards)


def enemy_smoke_before(t: float, smokes: list[dict], team: str, window: float) -> float | None:
    gaps = [t - s["t"] for s in smokes if s["team"] != team and 0 <= t - s["t"] <= window]
    return round(min(gaps), 1) if gaps else None


def lane_outcome(model: dict, rules: dict = RULES) -> dict | None:
    r = rules["lane"]
    if model["duration"] < r["minute"] * 60:
        return None
    own = [h for h in model["heroes"] if h["key"] in OWN_KEYS]
    enemy = [h for h in model["heroes"] if h["key"] not in OWN_KEYS]
    lanes = []
    for lane in ("top", "mid", "bot"):
        ours = [h for h in own if h["lane"] == lane and not h["roaming"]]
        theirs = [h for h in enemy if h["lane"] == lane and not h["roaming"]]
        if not ours:
            continue
        values = [h["nw10"] for h in ours + theirs]
        diff = None if any(v is None for v in values) or not theirs else sum(h["nw10"] for h in ours) - sum(h["nw10"] for h in theirs)
        outcome = None if diff is None else "behind" if diff <= -r["threshold"] else "ahead" if diff >= r["threshold"] else "even"
        deaths = sum(1 for h in ours for d in h["deaths"] if d["t"] < r["minute"] * 60)
        lanes.append({"lane": lane, "heroes": [h["key"] for h in ours], "opponents": len(theirs),
                      "nwDiff": diff, "deathsBefore": deaths, "outcome": outcome})
    roaming = [h["key"] for h in own if h["roaming"] or h["lane"] is None]
    return {"lanes": lanes, "roaming": roaming}


def _times(deaths: list[dict]) -> str:
    shown = [clock(d["t"]) for d in deaths[:4]]
    return ", ".join(shown) + (" …" if len(deaths) > 4 else "")


def coach_notes(deaths: list[dict], lane10: dict | None, rules: dict = RULES) -> list[dict]:
    """Template notes, each citing its evidence. At most rules['notes']['max']."""
    n = len(deaths)
    notes = []
    radius = rules["vision"]["observerRadius"]
    pick = [d for d in deaths if d["type"] in ("solo", "gank") and not d["ownWard"]]
    if pick:
        notes.append({"kind": "pickoff-no-vision", "evidence": [d["tick"] for d in pick],
                      "en": f"{len(pick)} of {n} deaths: caught alone or ganked with no allied observer ward within {radius} units ({_times(pick)}). Review the path that led into each spot.",
                      "th": f"ตาย {len(pick)} จาก {n} ครั้งแบบโดนจับคนเดียวหรือโดนแก๊ง โดยไม่มีวอร์ดฝั่งเราในระยะ {radius} หน่วย ({_times(pick)}) ลองย้อนดูเส้นทางที่เดินเข้าไปจุดนั้น"})
    seen_limit = rules["notes"]["seenSeconds"]
    seen = [d for d in deaths if d["type"] in ("solo", "gank") and (d["enemyVisibleSeconds"] or 0) >= seen_limit]
    if seen:
        notes.append({"kind": "seen-before-pickoff", "evidence": [d["tick"] for d in seen],
                      "en": f"{len(seen)} of these deaths: the enemy team could see you for at least {seen_limit} of the last 10 seconds ({_times(seen)}).",
                      "th": f"ก่อนโดนจับ {len(seen)} ครั้ง ทีมศัตรูมองเห็นคุณอย่างน้อย {seen_limit} จาก 10 วินาทีสุดท้าย ({_times(seen)})"})
    smoked = [d for d in deaths if d["enemySmoke"] is not None]
    if smoked:
        window = rules["smoke"]["windowSeconds"]
        notes.append({"kind": "enemy-smoke", "evidence": [d["tick"] for d in smoked],
                      "en": f"{len(smoked)} deaths came within {window} seconds after an enemy Smoke of Deceit ({_times(smoked)}). The link is timing only.",
                      "th": f"ตาย {len(smoked)} ครั้งภายใน {window} วินาทีหลังศัตรูใช้ Smoke ({_times(smoked)}) เชื่อมโยงจากเวลาเท่านั้น"})
    if lane10:
        mine = next((lane for lane in lane10["lanes"] if "self" in lane["heroes"]), None)
        if mine and mine["outcome"] == "behind":
            notes.append({"kind": "lane-behind", "evidence": [],
                          "en": f"Your lane ({mine['lane']}) was {abs(mine['nwDiff'])} net worth behind its opponents at 10:00, with {mine['deathsBefore']} deaths before 10:00. Lane assignment is derived.",
                          "th": f"เลนของคุณ ({mine['lane']}) ตามหลังคู่เลน {abs(mine['nwDiff'])} net worth ที่นาที 10 และตายก่อนนาที 10 รวม {mine['deathsBefore']} ครั้ง การแบ่งเลนเป็นค่าที่คำนวณ"})
    return notes[: rules["notes"]["max"]]


def build_match(model: dict, rules: dict = RULES) -> dict:
    if model.get("schema") != "kohbor.replay-neutral.v1":
        raise ValueError("unknown neutral model schema")
    heroes = model["heroes"]
    me = next(h for h in heroes if h["key"] == "self")
    team = model["team"]
    duration = int(model["duration"])
    earliest = min([0] + [math.floor(d["t"]) for h in heroes for d in h["deaths"]] + [p[0] for h in heroes for p in h["points"][:1]])
    start = max(-600, min(0, earliest))
    damagers = {d["tick"]: d["damagers"] for d in model["selfDeaths"]}
    analysed = []
    for d in me["deaths"]:
        kind = classify_death(d, damagers.get(d["tick"], []), heroes, rules)
        analysed.append({
            "tick": d["tick"], "t": d["t"], **kind,
            "sampleAge": d["age"],
            "enemyVisibleSeconds": visible_seconds(d["t"], model["selfVisibility"], rules["vision"]["visibleWindowSeconds"]),
            "ownWard": ward_cover(d["t"], d["x"], d["y"], model["wards"], team, rules["vision"]["observerRadius"]),
            "enemyWard": ward_cover(d["t"], d["x"], d["y"], model["wards"], "Dire" if team == "Radiant" else "Radiant",
                                    rules["vision"]["observerRadius"]),
            "enemySmoke": enemy_smoke_before(d["t"], model["smokes"], team, rules["smoke"]["windowSeconds"]),
        })
    lane10 = lane_outcome(model, rules)
    roster = []
    for h in heroes:
        actor = {"key": h["key"], "hero": h["hero"], "team": h["team"],
                 "lifeEnds": [{"t": d["t"], "x": d["x"], "y": d["y"]} for d in h["deaths"]]}
        if h["key"] != "self":
            actor["points"] = h["points"]
        roster.append(actor)
    return {
        "id": model["match"], "hero": me["hero"], "team": team, "patch": model["patch"],
        "start": start, "duration": duration, "points": me["points"],
        "deaths": [{"t": d["t"], "x": d["x"], "y": d["y"], "tick": d["tick"], "killer": d["killer"]} for d in me["deaths"]],
        "roster": roster,
        "coach": {"deaths": analysed, "lane10": lane10, "notes": coach_notes(analysed, lane10, rules)},
    }


def build_report(models: list[dict], maps: list[dict], rules: dict = RULES) -> dict:
    if not 1 <= len(models) <= 12:
        raise ValueError("a report holds 1-12 matches")
    return {"format": REPORT_FORMAT, "source": "DEM", "rules": rules, "maps": maps,
            "matches": [build_match(m, rules) for m in models]}
