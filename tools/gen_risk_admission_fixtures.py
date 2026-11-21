#!/usr/bin/env python3
"""Build deterministic synthetic risk-admission fixtures.

Interpretations
---------------
* An interruption is applied before evaluating that tick. A non-valid coverage
  tick, a non-advancing or over-limit frame time, or an unknown/true pause clears
  visible and missing state and discards sightings on that tick. This makes the
  new baseline start only on later uninterrupted frames.
* Missing own GSI, unknown alive, unknown pause, unknown kills/assists, and
  unknown media readiness fail closed using the closest named gate reason:
  not_in_match, own_dead_or_grace, paused_or_unknown, kill_data_unknown, and
  media_unready respectively.
* A same-tick duplicate certified hero sighting is resolved deterministically:
  choose the lexicographically smallest (track, x, y), so duplicates cannot
  manufacture heading samples.
* Own-death intervals use only fresh, in-match own state. They start on the
  first such dead tick and end at the first subsequent such alive tick plus the
  configured grace. A vanish timestamp inside that half-open interval
  permanently excludes that absence episode.
* An unknown kill/assist tick remains in the evidence history. Counter increases
  compare against the last tick on which both counters were known. Unknown data
  in the active vanish window blocks admission.
* A latch interruption clears the whole latch silently. Confirmed reappearance
  removes that hero immediately; a correction is considered only when the last
  latched member leaves, and guards 1-5 are checked on that tick.
* Table input "NaN" is a JSON-safe spelling for a non-finite float input; the
  evaluator parses it as IEEE NaN and emits JSON null for the result.
* Trace risk is evaluated after state transitions. Therefore interruption
  ticks have zero contributors (or unavailable score for invalid coverage),
  even when their pre-interruption state had a larger score.

This is a standalone Python reference model. It intentionally does not import or
call the Rust engine under test.
"""

from __future__ import annotations

import argparse
import itertools
import json
import math
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "fixtures" / "risk-admission"

DEFAULT_PARAMS: dict[str, Any] = {
    "ramp_start_s": 5.0,
    "peak_s": 12.0,
    "peak_risk": 0.70,
    "decay_per_s": 0.03,
    "floor": 0.10,
    "multi_boost": 1.15,
    "heading_amp": 0.22,
    "danger": 0.65,
    "revision": 0.40,
    "global_cooldown_ms": 30000,
    "contributor_cooldown_ms": 60000,
    "observation_gap_ms": 2000,
    "gsi_fresh_ms": 2000,
    "reappear_min_frames": 3,
    "reappear_min_ms": 500,
    "heading_window_ms": 1500,
    "heading_min_sightings": 3,
    "heading_min_span_ms": 500,
    "heading_min_displacement": 20.0,
    "center_deadzone": 20.0,
    "own_respawn_grace_ms": 5000,
    "kill_participation_window_ms": 3000,
    "coord_min_members": 2,
    "coord_vanish_window_ms": 3000,
    "coord_radius": 120.0,
    "coord_heading_cos": 0.5,
    "coord_max_age_ms": 15000,
    "player_relative_gate": True,
    "toward_player_cos": 0.5,
    "proximity_radius": 250.0,
    "hysteresis_low_danger": 0.85,
    "hysteresis_low_revision": 0.50,
    "hysteresis_high_danger": 0.50,
    "hysteresis_high_revision": 0.30,
    "history_ms": 300000,
}

PARAM_ALIASES = {
    "OBSERVATION_GAP_MS": "observation_gap_ms",
    "GSI_FRESH_MS": "gsi_fresh_ms",
    "REAPPEAR_MIN_FRAMES": "reappear_min_frames",
    "REAPPEAR_MIN_MS": "reappear_min_ms",
    "HEADING_WINDOW_MS": "heading_window_ms",
    "HEADING_MIN_SIGHTINGS": "heading_min_sightings",
    "HEADING_MIN_SPAN_MS": "heading_min_span_ms",
    "HEADING_MIN_DISPLACEMENT": "heading_min_displacement",
    "CENTER_DEADZONE": "center_deadzone",
    "OWN_RESPAWN_GRACE_MS": "own_respawn_grace_ms",
    "KILL_PARTICIPATION_WINDOW_MS": "kill_participation_window_ms",
    "COORD_MIN_MEMBERS": "coord_min_members",
    "COORD_VANISH_WINDOW_MS": "coord_vanish_window_ms",
    "COORD_RADIUS": "coord_radius",
    "COORD_HEADING_COS": "coord_heading_cos",
    "COORD_MAX_AGE_MS": "coord_max_age_ms",
    "PLAYER_RELATIVE_GATE": "player_relative_gate",
    "TOWARD_PLAYER_COS": "toward_player_cos",
    "PROXIMITY_RADIUS": "proximity_radius",
}


def apply_param_overrides(params: dict[str, Any], overrides: dict[str, Any]) -> dict[str, Any]:
    result = dict(params)
    for name, value in overrides.items():
        canonical = PARAM_ALIASES.get(name, name)
        if canonical not in result:
            raise ValueError(f"unknown risk parameter override: {name}")
        result[canonical] = value
    if not params_are_valid(result):
        raise ValueError("risk parameter override is out of range")
    return result


def finite_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value))


def params_are_valid(params: dict[str, Any]) -> bool:
    integer_names = {
        "global_cooldown_ms",
        "contributor_cooldown_ms",
        "observation_gap_ms",
        "gsi_fresh_ms",
        "reappear_min_frames",
        "reappear_min_ms",
        "heading_window_ms",
        "heading_min_sightings",
        "heading_min_span_ms",
        "own_respawn_grace_ms",
        "kill_participation_window_ms",
        "coord_min_members",
        "coord_vanish_window_ms",
        "coord_max_age_ms",
        "history_ms",
    }
    floats = [
        value
        for value in params.values()
        if isinstance(value, (int, float)) and not isinstance(value, bool)
    ]
    thresholds = [
        "danger",
        "revision",
        "coord_heading_cos",
        "toward_player_cos",
        "hysteresis_low_danger",
        "hysteresis_low_revision",
        "hysteresis_high_danger",
        "hysteresis_high_revision",
    ]
    return (
        all(math.isfinite(float(value)) for value in floats)
        and isinstance(params["player_relative_gate"], bool)
        and all(
            isinstance(params[name], int) and not isinstance(params[name], bool) and params[name] >= 0
            for name in integer_names
        )
        and all(0.0 <= float(params[name]) <= 1.0 for name in thresholds)
        and params["revision"] < params["danger"]
        and params["hysteresis_low_revision"] < params["hysteresis_low_danger"]
        and params["hysteresis_high_revision"] < params["hysteresis_high_danger"]
        and params["ramp_start_s"] >= 0.0
        and params["peak_s"] >= 0.0
        and params["peak_s"] > params["ramp_start_s"]
        and params["peak_risk"] >= 0.0
        and params["floor"] >= 0.0
        and params["multi_boost"] >= 0.0
        and params["heading_min_displacement"] >= 0.0
        and params["center_deadzone"] >= 0.0
        and params["coord_radius"] >= 0.0
        and params["proximity_radius"] >= 0.0
    )


def params_set_accepts(name: str, value: Any) -> bool:
    canonical = PARAM_ALIASES.get(name, name.lower())
    if canonical not in DEFAULT_PARAMS:
        return False
    if canonical == "player_relative_gate":
        if not isinstance(value, bool):
            return False
        candidate = dict(DEFAULT_PARAMS)
        candidate[canonical] = value
        return params_are_valid(candidate)
    integer_names = {
        "global_cooldown_ms",
        "contributor_cooldown_ms",
        "observation_gap_ms",
        "gsi_fresh_ms",
        "reappear_min_frames",
        "reappear_min_ms",
        "heading_window_ms",
        "heading_min_sightings",
        "heading_min_span_ms",
        "own_respawn_grace_ms",
        "kill_participation_window_ms",
        "coord_min_members",
        "coord_vanish_window_ms",
        "coord_max_age_ms",
        "history_ms",
    }
    if canonical in integer_names:
        if not isinstance(value, int) or isinstance(value, bool) or value < 0:
            return False
    elif not finite_number(value):
        return False
    candidate = dict(DEFAULT_PARAMS)
    candidate[canonical] = value
    return params_are_valid(candidate)


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


def distance(a: tuple[float, float], b: tuple[float, float]) -> float:
    return math.hypot(a[0] - b[0], a[1] - b[1])


def dot_cos(a: tuple[float, float], b: tuple[float, float]) -> float | None:
    norm_a = math.hypot(*a)
    norm_b = math.hypot(*b)
    if norm_a == 0.0 or norm_b == 0.0:
        return None
    return clamp((a[0] * b[0] + a[1] * b[1]) / (norm_a * norm_b), -1.0, 1.0)


def base_risk(seconds: float, params: dict[str, Any]) -> float | None:
    if not finite_number(seconds) or seconds < 0.0:
        return None
    start = float(params["ramp_start_s"])
    peak = float(params["peak_s"])
    peak_risk = float(params["peak_risk"])
    if seconds < start:
        return 0.0
    if seconds <= peak:
        return ((seconds - start) / (peak - start)) * peak_risk
    return max(peak_risk - (seconds - peak) * float(params["decay_per_s"]), float(params["floor"]))


def heading_from_history(history: list[dict[str, Any]], vanish_ms: int, params: dict[str, Any]) -> dict[str, Any]:
    recent = [
        sighting
        for sighting in history
        if vanish_ms - int(params["heading_window_ms"]) <= sighting["t_ms"] <= vanish_ms
    ]
    if not recent:
        return {"valid": False, "vector": None, "last": None, "multiplier": 1.0}
    earliest = recent[0]
    latest = recent[-1]
    first_point = (float(earliest["x"]), float(earliest["y"]))
    last_point = (float(latest["x"]), float(latest["y"]))
    vector = (last_point[0] - first_point[0], last_point[1] - first_point[1])
    span = int(latest["t_ms"]) - int(earliest["t_ms"])
    displacement = math.hypot(*vector)
    valid = (
        len(recent) >= int(params["heading_min_sightings"])
        and span >= int(params["heading_min_span_ms"])
        and displacement >= float(params["heading_min_displacement"])
    )
    if not valid:
        return {"valid": False, "vector": None, "last": [latest["x"], latest["y"]], "multiplier": 1.0}
    toward_center = (500.0 - last_point[0], 500.0 - last_point[1])
    center_distance = math.hypot(*toward_center)
    if center_distance < float(params["center_deadzone"]):
        multiplier = 1.0
    else:
        cosine = dot_cos(vector, toward_center)
        multiplier = 1.0 + (cosine if cosine is not None else 0.0) * float(params["heading_amp"])
    return {
        "valid": True,
        "vector": [vector[0], vector[1]],
        "last": [latest["x"], latest["y"]],
        "multiplier": clamp(multiplier, 0.0, 2.0),
    }


def aggregate(scores: list[float | None], params: dict[str, Any]) -> float:
    contributors = [float(score) for score in scores if score is not None and score > 0.0]
    if not contributors:
        return 0.0
    combined = 1.0
    for score in contributors:
        combined *= 1.0 - score
    result = 1.0 - combined
    if len(contributors) >= 2:
        result *= float(params["multi_boost"])
    return min(result, 1.0)


def eta(seconds: float) -> float | None:
    if not finite_number(seconds) or seconds < 0.0:
        return None
    return max(12.0 - seconds, 1.0)


def normalize_sighting(sighting: dict[str, Any]) -> dict[str, Any]:
    return {
        "track": str(sighting["track"]),
        "hero": sighting.get("hero"),
        "x": int(sighting["x"]),
        "y": int(sighting["y"]),
    }


def state_template() -> dict[str, Any]:
    return {
        "state": "NeverSeen",
        "history": [],
        "vanish_ms": None,
        "heading": None,
        "notified": False,
        "reappearance": [],
    }


def _apply_observations(
    states: dict[str, dict[str, Any]],
    sightings: list[dict[str, Any]],
    t_ms: int,
    params: dict[str, Any],
    reappeared: set[str],
) -> None:
    chosen: dict[str, dict[str, Any]] = {}
    for raw in sightings:
        sighting = normalize_sighting(raw)
        hero = sighting["hero"]
        if hero is None or hero not in states:
            continue
        key = (sighting["track"], sighting["x"], sighting["y"])
        current = chosen.get(hero)
        current_key = (current["track"], current["x"], current["y"]) if current else None
        if current is None or key < current_key:
            chosen[hero] = sighting

    for hero, state in states.items():
        sighting = chosen.get(hero)
        if state["state"] == "NeverSeen":
            if sighting is not None:
                state["state"] = "Visible"
                state["history"] = [{"t_ms": t_ms, "x": sighting["x"], "y": sighting["y"]}]
            continue

        if state["state"] == "Visible":
            if sighting is None:
                state["state"] = "Missing"
                state["vanish_ms"] = state["history"][-1]["t_ms"]
                state["heading"] = heading_from_history(state["history"], state["vanish_ms"], params)
                state["reappearance"] = []
            else:
                state["history"].append({"t_ms": t_ms, "x": sighting["x"], "y": sighting["y"]})
                state["history"] = [
                    item for item in state["history"] if item["t_ms"] >= t_ms - int(params["history_ms"])
                ]
            continue

        if state["state"] in ("Missing", "Interrupted"):
            if sighting is None:
                state["reappearance"] = []
                continue
            run = state["reappearance"]
            run.append({"t_ms": t_ms, "x": sighting["x"], "y": sighting["y"]})
            state["reappearance"] = run
            if (
                len(run) >= int(params["reappear_min_frames"])
                and run[-1]["t_ms"] - run[0]["t_ms"] >= int(params["reappear_min_ms"])
            ):
                if state["state"] == "Missing":
                    reappeared.add(hero)
                state["state"] = "Visible"
                state["history"] = list(run)
                state["vanish_ms"] = None
                state["heading"] = None
                state["notified"] = False
                state["reappearance"] = []


def _inside_own_death(vanish_ms: int, death_windows: list[tuple[int, int | None]]) -> bool:
    return any(start <= vanish_ms and (end is None or vanish_ms < end) for start, end in death_windows)


def _choose_group(
    states: dict[str, dict[str, Any]],
    risks: dict[str, float | None],
    t_ms: int,
    death_windows: list[tuple[int, int | None]],
    params: dict[str, Any],
) -> tuple[list[str] | None, float | None]:
    candidates = []
    for hero, state in states.items():
        heading = state["heading"]
        vanish = state["vanish_ms"]
        if (
            state["state"] == "Missing"
            and heading is not None
            and heading["valid"]
            and not state["notified"]
            and vanish is not None
            and not _inside_own_death(int(vanish), death_windows)
        ):
            candidates.append(hero)
    groups: list[tuple[int, float, tuple[str, ...]]] = []
    for size in range(int(params["coord_min_members"]), len(candidates) + 1):
        for group in itertools.combinations(sorted(candidates), size):
            if t_ms - max(int(states[hero]["vanish_ms"]) for hero in group) > int(params["coord_max_age_ms"]):
                continue
            valid = True
            for left, right in itertools.combinations(group, 2):
                left_state, right_state = states[left], states[right]
                if abs(int(left_state["vanish_ms"]) - int(right_state["vanish_ms"])) > int(
                    params["coord_vanish_window_ms"]
                ):
                    valid = False
                    break
                if distance(tuple(left_state["heading"]["last"]), tuple(right_state["heading"]["last"])) > float(
                    params["coord_radius"]
                ):
                    valid = False
                    break
                cosine = dot_cos(tuple(left_state["heading"]["vector"]), tuple(right_state["heading"]["vector"]))
                if cosine is None or cosine < float(params["coord_heading_cos"]):
                    valid = False
                    break
            if valid:
                groups.append((len(group), aggregate([risks[hero] for hero in group], params), group))
    if not groups:
        return None, None
    groups.sort(key=lambda item: (-item[0], -item[1], item[2]))
    selected = list(groups[0][2])
    return selected, groups[0][1]


def _player_gate(group: list[str], states: dict[str, dict[str, Any]], own: dict[str, Any], params: dict[str, Any]) -> str | None:
    if not params["player_relative_gate"]:
        return None
    player = own.get("map_pos")
    if not isinstance(player, dict) or not finite_number(player.get("x")) or not finite_number(player.get("y")):
        return "own_position_unknown"
    unit_sum = [0.0, 0.0]
    positions = []
    for hero in group:
        heading = states[hero]["heading"]
        vector = tuple(float(value) for value in heading["vector"])
        norm = math.hypot(*vector)
        unit_sum[0] += vector[0] / norm
        unit_sum[1] += vector[1] / norm
        positions.append(tuple(float(value) for value in heading["last"]))
    if math.hypot(*unit_sum) == 0.0:
        return "not_toward_player"
    centroid = (sum(point[0] for point in positions) / len(positions), sum(point[1] for point in positions) / len(positions))
    direction = (float(player["x"]) - centroid[0], float(player["y"]) - centroid[1])
    direction_distance = math.hypot(*direction)
    if direction_distance >= 20.0:
        cosine = dot_cos(tuple(unit_sum), direction)
        if cosine is None or cosine < float(params["toward_player_cos"]):
            return "not_toward_player"
    if direction_distance > float(params["proximity_radius"]):
        return "too_far"
    return None


def evaluate_trace(fixture: dict[str, Any]) -> dict[str, Any]:
    params = apply_param_overrides(DEFAULT_PARAMS, fixture.get("params_override", {}))
    states = {f"enemy_{index}": state_template() for index in range(1, 6)}
    last_tick_ms: int | None = None
    death_windows: list[tuple[int, int | None]] = []
    respawn_until: int | None = None
    previous_kill_total: int | None = None
    kill_change_times: list[int] = []
    kill_unknown_times: list[int] = []
    last_admitted_ms: int | None = None
    contributor_admitted: dict[str, int] = {}
    latch: set[str] = set()
    events: list[dict[str, Any]] = []
    probes: list[dict[str, Any]] = []
    per_tick: list[dict[str, Any]] = []
    previous_reason: str | None = None

    for tick in fixture["ticks"]:
        t_ms = int(tick["t_ms"])
        own = tick.get("own")
        gap = last_tick_ms is not None and (
            t_ms <= last_tick_ms or t_ms - last_tick_ms > int(params["observation_gap_ms"])
        )
        coverage_valid = tick.get("coverage") == "valid"
        paused_bad = own is None or own.get("paused") is not False
        interruption = (not coverage_valid) or gap or paused_bad

        if own is not None and own.get("fresh") is True and own.get("in_match") is True:
            alive = own.get("alive")
            open_death = any(end is None for _, end in death_windows)
            if alive is False and not open_death:
                death_windows.append((t_ms, None))
                respawn_until = None
            elif alive is True and open_death:
                for window_index in range(len(death_windows) - 1, -1, -1):
                    start, end = death_windows[window_index]
                    if end is None:
                        respawn_until = t_ms + int(params["own_respawn_grace_ms"])
                        death_windows[window_index] = (start, respawn_until)
                        break
        kills = own.get("kills") if own is not None else None
        assists = own.get("assists") if own is not None else None
        if (
            isinstance(kills, int)
            and not isinstance(kills, bool)
            and isinstance(assists, int)
            and not isinstance(assists, bool)
        ):
            total = kills + assists
            if previous_kill_total is not None and total > previous_kill_total:
                kill_change_times.append(t_ms)
            previous_kill_total = total
        else:
            kill_unknown_times.append(t_ms)

        interrupted_latched = False
        reappeared: set[str] = set()
        if interruption:
            for hero, state in states.items():
                if state["state"] in ("Visible", "Missing"):
                    if hero in latch:
                        interrupted_latched = True
                    state["state"] = "Interrupted"
                    state["history"] = []
                    state["heading"] = None
                    state["vanish_ms"] = None
                    state["reappearance"] = []
                elif state["state"] == "Interrupted":
                    state["reappearance"] = []
        else:
            _apply_observations(states, tick.get("sightings", []), t_ms, params, reappeared)

        if own is not None and own.get("alive") is True and respawn_until is not None and t_ms >= respawn_until:
            respawn_until = None

        risks: dict[str, float | None] = {}
        missing_seconds: dict[str, float] = {}
        for hero, state in states.items():
            if state["state"] == "Missing" and state["vanish_ms"] is not None:
                seconds = (t_ms - int(state["vanish_ms"])) / 1000.0
                missing_seconds[hero] = seconds
                base = base_risk(seconds, params)
                heading = state["heading"] or {"valid": False, "multiplier": 1.0}
                risks[hero] = None if base is None else clamp(base * float(heading["multiplier"]), 0.0, 1.0)
            else:
                risks[hero] = None
        score_all = None if not coverage_valid else aggregate(list(risks.values()), params)

        latch_event: dict[str, Any] | None = None
        if latch and interrupted_latched:
            latch_members = sorted(latch)
            latch.clear()
            latch_event = {
                "t_ms": t_ms,
                "kind": "latch_cleared_silent",
                "members": latch_members,
                "score": score_all,
                "eta_s": None,
            }
        elif latch and (cleared_members := reappeared.intersection(latch)):
            latch.difference_update(cleared_members)
            if not latch:
                latch_members = sorted(cleared_members)
                base_reason = _base_gate_reason(tick, t_ms, params, respawn_until)
                if base_reason is None and score_all is not None and score_all < float(params["revision"]):
                    latch_event = {
                        "t_ms": t_ms,
                        "kind": "correction_would_admit",
                        "members": latch_members,
                        "score": score_all,
                        "eta_s": None,
                    }
                else:
                    latch_event = {
                        "t_ms": t_ms,
                        "kind": "latch_cleared_silent",
                        "members": latch_members,
                        "score": score_all,
                        "eta_s": None,
                    }
        if latch_event is not None:
            events.append(latch_event)

        reason, group, group_score = _evaluate_gate(
            tick=tick,
            t_ms=t_ms,
            states=states,
            risks=risks,
            death_windows=death_windows,
            kill_change_times=kill_change_times,
            kill_unknown_times=kill_unknown_times,
            last_admitted_ms=last_admitted_ms,
            contributor_admitted=contributor_admitted,
            params=params,
            respawn_until=respawn_until,
        )
        if not probes or reason != previous_reason:
            probes.append({"t_ms": t_ms, "reason": reason, "score_all": score_all})
        previous_reason = reason

        if reason == "admit" and group is not None and group_score is not None:
            group_eta = min(eta(missing_seconds[hero]) for hero in group)
            event = {
                "t_ms": t_ms,
                "kind": "would_admit",
                "members": sorted(group),
                "score": group_score,
                "eta_s": group_eta,
            }
            events.append(event)
            for hero in group:
                states[hero]["notified"] = True
                contributor_admitted[hero] = t_ms
            last_admitted_ms = t_ms
            latch.update(group)

        per_tick.append({
            "t_ms": t_ms,
            "reason": reason,
            "score_all": score_all,
            "group": group,
            "group_score": group_score,
            "states": {hero: state["state"] for hero, state in states.items()},
        })
        last_tick_ms = t_ms

    return {
        "events": events,
        "probes": probes,
        "per_tick": per_tick,
        "kill_change_times": kill_change_times,
    }


def _base_gate_reason(tick: dict[str, Any], t_ms: int, params: dict[str, Any], respawn_until: int | None) -> str | None:
    own = tick.get("own")
    if own is None or own.get("in_match") is not True:
        return "not_in_match"
    if own.get("fresh") is not True:
        return "gsi_stale"
    if own.get("alive") is not True or (respawn_until is not None and t_ms < respawn_until):
        return "own_dead_or_grace"
    if own.get("paused") is not False:
        return "paused_or_unknown"
    if tick.get("coverage") != "valid":
        return "coverage_invalid"
    return None


def _evaluate_gate(
    *,
    tick: dict[str, Any],
    t_ms: int,
    states: dict[str, dict[str, Any]],
    risks: dict[str, float | None],
    death_windows: list[tuple[int, int | None]],
    kill_change_times: list[int],
    kill_unknown_times: list[int],
    last_admitted_ms: int | None,
    contributor_admitted: dict[str, int],
    params: dict[str, Any],
    respawn_until: int | None,
) -> tuple[str, list[str] | None, float | None]:
    own = tick.get("own")
    reason = _base_gate_reason(tick, t_ms, params, respawn_until)
    if reason is not None:
        return reason, None, None
    assert own is not None
    if params["player_relative_gate"]:
        pos = own.get("map_pos")
        if not isinstance(pos, dict) or not finite_number(pos.get("x")) or not finite_number(pos.get("y")):
            return "own_position_unknown", None, None
    kills, assists = own.get("kills"), own.get("assists")
    if not (isinstance(kills, int) and not isinstance(kills, bool) and isinstance(assists, int) and not isinstance(assists, bool)):
        return "kill_data_unknown", None, None
    group, group_score = _choose_group(states, risks, t_ms, death_windows, params)
    if group is None or group_score is None:
        return "no_group", None, None
    earliest_vanish = min(int(states[hero]["vanish_ms"]) for hero in group)
    unknown_after = max(0, earliest_vanish - int(params["kill_participation_window_ms"]))
    if any(unknown_after <= unknown_ms <= t_ms for unknown_ms in kill_unknown_times):
        return "kill_data_unknown", None, None
    player_reason = _player_gate(group, states, own, params)
    if player_reason is not None:
        return player_reason, group, group_score
    if any(
        abs(change_ms - int(states[hero]["vanish_ms"])) <= int(params["kill_participation_window_ms"])
        for change_ms in kill_change_times
        for hero in group
    ):
        return "kill_participation", group, group_score
    if group_score < float(params["danger"]):
        return "below_danger", group, group_score
    if last_admitted_ms is not None and t_ms - last_admitted_ms < int(params["global_cooldown_ms"]):
        return "global_cooldown", group, group_score
    if any(
        hero in contributor_admitted
        and t_ms - contributor_admitted[hero] < int(params["contributor_cooldown_ms"])
        for hero in group
    ):
        return "contributor_cooldown", group, group_score
    if tick.get("media_ready") is not True:
        return "media_unready", group, group_score
    return "admit", group, group_score


def _rounded(value: Any) -> Any:
    if isinstance(value, float):
        if not math.isfinite(value):
            return None
        return round(value, 9)
    if isinstance(value, list):
        return [_rounded(item) for item in value]
    if isinstance(value, tuple):
        return [_rounded(item) for item in value]
    if isinstance(value, dict):
        return {key: _rounded(item) for key, item in value.items()}
    return value


def _table_fixtures() -> list[dict[str, Any]]:
    score_values: list[tuple[Any, float | None]] = [
        (0, 0.0),
        (4, 0.0),
        (4.999, 0.0),
        (5.0, 0.0),
        (8.5, 0.35),
        (11.999, ((11.999 - 5.0) / 7.0) * 0.70),
        (12, 0.70),
        (20, 0.46),
        (30, 0.16),
        (32, 0.10),
        (40, 0.10),
        (100, 0.10),
        (-0.001, None),
        ("NaN", None),
    ]
    score_rows = []
    for value, declared in score_values:
        seconds = float("nan") if value == "NaN" else float(value)
        computed = base_risk(seconds, DEFAULT_PARAMS)
        assert computed == declared or (computed is not None and declared is not None and math.isclose(computed, declared, abs_tol=1e-12))
        score_rows.append({"missing_seconds": value, "expected_score": computed})

    histories = [
        ("toward-centre", [{"t_ms": 0, "x": 500, "y": 300}, {"t_ms": 400, "x": 500, "y": 350}, {"t_ms": 800, "x": 500, "y": 400}]),
        ("away-from-centre", [{"t_ms": 0, "x": 500, "y": 500}, {"t_ms": 400, "x": 500, "y": 450}, {"t_ms": 800, "x": 500, "y": 400}]),
        ("perpendicular", [{"t_ms": 0, "x": 400, "y": 400}, {"t_ms": 400, "x": 450, "y": 400}, {"t_ms": 800, "x": 500, "y": 400}]),
        ("too-few-sightings", [{"t_ms": 0, "x": 500, "y": 300}, {"t_ms": 800, "x": 500, "y": 400}]),
        ("span-too-short", [{"t_ms": 400, "x": 500, "y": 350}, {"t_ms": 600, "x": 500, "y": 375}, {"t_ms": 800, "x": 500, "y": 400}]),
        ("displacement-too-small", [{"t_ms": 0, "x": 500, "y": 390}, {"t_ms": 400, "x": 500, "y": 395}, {"t_ms": 800, "x": 500, "y": 400}]),
        ("last-point-centre-deadzone", [{"t_ms": 0, "x": 500, "y": 450}, {"t_ms": 400, "x": 500, "y": 480}, {"t_ms": 800, "x": 500, "y": 510}]),
    ]
    heading_rows = []
    for name, history in histories:
        result = heading_from_history(history, 800, DEFAULT_PARAMS)
        heading_rows.append({
            "id": name,
            "vanish_ms": 800,
            "history": history,
            "valid": result["valid"],
            "multiplier": result["multiplier"],
        })
    assert [row["valid"] for row in heading_rows] == [True, True, True, False, False, False, True]

    aggregate_rows = []
    for name, scores in [
        ("single-hero-no-boost", [0.70]),
        ("two-heroes-at-12-seconds", [0.70, 0.70]),
        ("two-with-one-zero", [0.70, 0.0]),
        ("three-heroes", [0.40, 0.40, 0.40]),
        ("empty", []),
    ]:
        aggregate_rows.append({"id": name, "scores": scores, "expected_score": aggregate(scores, DEFAULT_PARAMS)})
    assert math.isclose(aggregate_rows[0]["expected_score"], 0.70, abs_tol=1e-12)
    assert math.isclose(aggregate_rows[1]["expected_score"], 1.0, abs_tol=1e-12)
    assert math.isclose(aggregate_rows[2]["expected_score"], 0.70, abs_tol=1e-12)
    assert math.isclose(aggregate_rows[3]["expected_score"], 0.9016, abs_tol=1e-12)
    assert aggregate_rows[4]["expected_score"] == 0.0

    eta_rows = []
    for name, values in [
        ("per-hero-before-ramp", [4.0]),
        ("per-hero-near-peak", [11.0]),
        ("per-hero-late", [20.0]),
        ("group-min", [4.0, 10.5, 20.0]),
    ]:
        estimates = [eta(value) for value in values]
        eta_rows.append({"id": name, "missing_seconds": values, "per_hero_eta_s": estimates, "group_eta_s": min(estimates)})

    param_rows = [
        {"name": "DANGER", "value": -1, "accepted": False},
        {"name": "REVISION", "value": 2, "accepted": False},
        {"name": "REVISION", "value": DEFAULT_PARAMS["danger"], "accepted": False},
        {"name": "COORD_RADIUS", "value": -1, "accepted": False},
        {"name": "DANGER", "value": 0.70, "accepted": True},
    ]
    for row in param_rows:
        accepted = params_set_accepts(row["name"], row["value"])
        assert accepted is row["accepted"], f"params-reject-table {row}: accepted={accepted}"

    return [
        {"id": "score-table", "file": "score-table.json", "kind": "table", "rule": "4.1", "expect": "table", "rows": score_rows},
        {"id": "heading-table", "file": "heading-table.json", "kind": "table", "rule": "4.2", "expect": "table", "rows": heading_rows},
        {"id": "aggregate-table", "file": "aggregate-table.json", "kind": "table", "rule": "4.3", "expect": "table", "rows": aggregate_rows},
        {"id": "eta-table", "file": "eta-table.json", "kind": "table", "rule": "4.4", "expect": "table", "rows": eta_rows},
        {"id": "params-reject-table", "file": "params-reject-table.json", "kind": "table", "rule": "14", "expect": "table", "rows": param_rows},
    ]


def _hero(
    hero: str | None,
    vanish_ms: int,
    start: tuple[int, int],
    end: tuple[int, int],
    *,
    track: str | None = None,
    segments: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    return {
        "hero": hero,
        "track": track or hero or "anonymous_track",
        "vanish_ms": vanish_ms,
        "segments": segments or [{"start_ms": 0, "end_ms": vanish_ms, "from": list(start), "to": list(end)}],
    }


def _pair(
    vanish_ms: int = 4000,
    *,
    starts: tuple[tuple[int, int], tuple[int, int]] = ((480, 300), (520, 300)),
    ends: tuple[tuple[int, int], tuple[int, int]] = ((480, 390), (520, 410)),
    heroes: tuple[str | None, str | None] = ("enemy_1", "enemy_2"),
    vanish_ms_2: int | None = None,
) -> list[dict[str, Any]]:
    return [
        _hero(heroes[0], vanish_ms, starts[0], ends[0]),
        _hero(heroes[1], vanish_ms if vanish_ms_2 is None else vanish_ms_2, starts[1], ends[1]),
    ]


def _own_default() -> dict[str, Any]:
    return {
        "fresh": True,
        "in_match": True,
        "alive": True,
        "paused": False,
        "kills": 0,
        "assists": 0,
        "map_pos": {"x": 500, "y": 600},
    }


def _times(end_ms: int, *, gap: tuple[int, int] | None = None) -> list[int]:
    values = list(range(0, end_ms + 1, 200))
    if gap is not None:
        last_before, resume_at = gap
        values = [value for value in values if value <= last_before or value >= resume_at]
        if resume_at not in values:
            values.append(resume_at)
        values = sorted(set(values))
    if end_ms not in values:
        values.append(end_ms)
    return sorted(set(values))


def _active_override(changes: list[dict[str, Any]], t_ms: int, key: str, default: Any) -> Any:
    result = default
    for change in changes:
        if int(change["start_ms"]) <= t_ms <= int(change.get("end_ms", change["start_ms"])):
            result = change[key]
    return result


def _position_in_segment(segment: dict[str, Any], t_ms: int) -> tuple[int, int]:
    start_ms, end_ms = int(segment["start_ms"]), int(segment["end_ms"])
    ratio = 0.0 if end_ms == start_ms else (t_ms - start_ms) / (end_ms - start_ms)
    source, target = segment["from"], segment["to"]
    return (
        int(round(source[0] + (target[0] - source[0]) * ratio)),
        int(round(source[1] + (target[1] - source[1]) * ratio)),
    )


def _build_trace(config: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    params = apply_param_overrides(DEFAULT_PARAMS, config.get("params_override", {}))
    ticks = []
    times = _times(config["end_ms"], gap=config.get("gap"))
    duplicate_tick_ms = config.get("duplicate_tick_ms")
    if duplicate_tick_ms is not None:
        if duplicate_tick_ms not in times:
            raise AssertionError(f"{config['id']}: duplicate tick {duplicate_tick_ms} is not on the time grid")
        times.insert(times.index(duplicate_tick_ms) + 1, duplicate_tick_ms)
    for t_ms in times:
        own = _own_default()
        for change in config.get("own_changes", []):
            if int(change["start_ms"]) <= t_ms <= int(change.get("end_ms", change["start_ms"])):
                own.update(change["values"])
        coverage = _active_override(config.get("coverage_changes", []), t_ms, "coverage", "valid")
        media_ready = _active_override(config.get("media_changes", []), t_ms, "media_ready", True)
        sightings = []
        for hero in config.get("heroes", []):
            for segment in hero["segments"]:
                if int(segment["start_ms"]) <= t_ms <= int(segment["end_ms"]):
                    x, y = _position_in_segment(segment, t_ms)
                    sightings.append({"track": hero["track"], "hero": hero["hero"], "x": x, "y": y})
                    break
        ticks.append({"t_ms": t_ms, "coverage": coverage, "own": own, "media_ready": media_ready, "sightings": sightings})

    fixture: dict[str, Any] = {
        "id": config["id"],
        "kind": "trace",
        "description": config["description"],
        "spec_sections": config["spec_sections"],
        "ticks": ticks,
    }
    if config.get("params_override"):
        fixture["params_override"] = config["params_override"]
    result = evaluate_trace(fixture)
    events = result["events"]
    admits = [event for event in events if event["kind"] == "would_admit"]
    if config.get("must_admit") and not admits:
        raise AssertionError(f"{config['id']}: expected at least one would_admit")
    if config.get("must_not_admit") and admits:
        raise AssertionError(f"{config['id']}: unexpected would_admit at {admits[0]['t_ms']}")
    exact_admits = config.get("exact_admits")
    if exact_admits is not None and len(admits) != exact_admits:
        raise AssertionError(f"{config['id']}: expected {exact_admits} would_admit events, got {len(admits)}")
    expected_members = config.get("first_members")
    if expected_members is not None and (not admits or admits[0]["members"] != expected_members):
        raise AssertionError(f"{config['id']}: first event members differ: {admits[0]['members'] if admits else None}")
    expected_types = config.get("event_types")
    if expected_types is not None and [event["kind"] for event in events] != expected_types:
        raise AssertionError(f"{config['id']}: event kinds differ: {[event['kind'] for event in events]}")
    for expected in config.get("exact_event_times", []):
        kind, expected_t = expected
        matches = [event for event in events if event["kind"] == kind]
        if not any(event["t_ms"] == expected_t for event in matches):
            raise AssertionError(f"{config['id']}: expected {kind} at {expected_t}, got {matches}")
    for expected in config.get("event_assertions", []):
        event = next(
            (
                event
                for event in events
                if event["kind"] == expected["kind"] and event["t_ms"] == expected["t_ms"]
            ),
            None,
        )
        if event is None:
            raise AssertionError(f"{config['id']}: missing event {expected}")
        if "members" in expected and event["members"] != expected["members"]:
            raise AssertionError(
                f"{config['id']}: {expected['kind']} members at {expected['t_ms']} "
                f"were {event['members']}, expected {expected['members']}"
            )
        if expected.get("score_below_revision") and not (
            event["score"] is not None and event["score"] < float(params["revision"])
        ):
            raise AssertionError(
                f"{config['id']}: {expected['kind']} score {event['score']} "
                f"was not below revision {params['revision']}"
            )
    expected_kill_change_times = config.get("expected_kill_change_times")
    if expected_kill_change_times is not None and result["kill_change_times"] != expected_kill_change_times:
        raise AssertionError(
            f"{config['id']}: kill counter increases at {result['kill_change_times']}, "
            f"expected {expected_kill_change_times}"
        )

    peak_candidates = [item for item in result["per_tick"] if item["score_all"] is not None]
    peak_score = max(item["score_all"] for item in peak_candidates)
    peak_ticks = [item for item in peak_candidates if math.isclose(item["score_all"], peak_score, abs_tol=1e-12)]
    peak_reason = config.get("peak_reason")
    if peak_reason is not None and not any(item["reason"] == peak_reason for item in peak_ticks):
        raise AssertionError(
            f"{config['id']}: peak score {peak_score} at {[item['t_ms'] for item in peak_ticks]} "
            f"did not have expected reason {peak_reason}: {[item['reason'] for item in peak_ticks]}"
        )
    reasons = {item["reason"] for item in result["per_tick"]}
    for required_reason in config.get("must_reasons", []):
        if required_reason not in reasons:
            raise AssertionError(f"{config['id']}: expected reason {required_reason}, got {sorted(reasons)}")

    expected = {"events": events, "probes": result["probes"]}
    fixture["expected"] = expected
    info = {
        "id": config["id"],
        "peak_score": peak_score,
        "peak_t_ms": peak_ticks[0]["t_ms"],
        "peak_reason": peak_ticks[0]["reason"],
        "event_summary": [(event["kind"], event["t_ms"], event["score"]) for event in events],
    }
    return _rounded(fixture), info


def _scenario_definitions() -> list[dict[str, Any]]:
    def config(
        fixture_id: str,
        description: str,
        end_ms: int,
        heroes: list[dict[str, Any]],
        sections: list[str],
        *,
        peak_reason: str | None = None,
        must_reasons: list[str] | None = None,
        must_admit: bool = False,
        must_not_admit: bool = False,
        exact_admits: int | None = None,
        first_members: list[str] | None = None,
        event_types: list[str] | None = None,
        exact_event_times: list[tuple[str, int]] | None = None,
        event_assertions: list[dict[str, Any]] | None = None,
        expected_kill_change_times: list[int] | None = None,
        own_changes: list[dict[str, Any]] | None = None,
        coverage_changes: list[dict[str, Any]] | None = None,
        media_changes: list[dict[str, Any]] | None = None,
        params_override: dict[str, Any] | None = None,
        gap: tuple[int, int] | None = None,
        duplicate_tick_ms: int | None = None,
    ) -> dict[str, Any]:
        return {
            "id": fixture_id,
            "description": description,
            "end_ms": end_ms,
            "heroes": heroes,
            "spec_sections": sections,
            "peak_reason": peak_reason,
            "must_reasons": must_reasons or [],
            "must_admit": must_admit,
            "must_not_admit": must_not_admit,
            "exact_admits": exact_admits,
            "first_members": first_members,
            "event_types": event_types,
            "exact_event_times": exact_event_times or [],
            "event_assertions": event_assertions or [],
            "expected_kill_change_times": expected_kill_change_times,
            "own_changes": own_changes or [],
            "coverage_changes": coverage_changes or [],
            "media_changes": media_changes or [],
            "params_override": params_override or {},
            "gap": gap,
            "duplicate_tick_ms": duplicate_tick_ms,
        }

    def moved_pair(vanish_ms: int = 4000) -> list[dict[str, Any]]:
        return _pair(vanish_ms)

    two = moved_pair()
    three = two + [_hero("enemy_3", 4000, (500, 300), (500, 400))]
    single = [_hero("enemy_1", 4000, (500, 300), (500, 400))]

    far = _pair(
        4000,
        starts=((280, 300), (500, 300)),
        ends=((280, 390), (500, 390)),
    )
    opposite = [
        _hero("enemy_1", 4000, (480, 300), (480, 390)),
        _hero("enemy_2", 4000, (520, 500), (520, 410)),
    ]
    stationary = [
        _hero("enemy_1", 4000, (480, 390), (480, 390)),
        _hero("enemy_2", 4000, (520, 400), (520, 400)),
    ]
    away = [
        _hero("enemy_1", 4000, (480, 300), (480, 390)),
        _hero("enemy_2", 4000, (520, 300), (520, 410)),
    ]
    owner_away = [{"start_ms": 0, "end_ms": 100000, "values": {"map_pos": {"x": 500, "y": 200}}}]
    too_far_own = [{"start_ms": 0, "end_ms": 100000, "values": {"map_pos": {"x": 500, "y": 900}}}]
    unknown_pos = [{"start_ms": 0, "end_ms": 100000, "values": {"map_pos": None}}]

    # Long-lived visible episodes use a fresh final movement segment so the
    # second absence still has a valid 1.5 s pre-vanish heading.
    def reappearing_pair(second_vanish: int | None = None) -> list[dict[str, Any]]:
        records = []
        for index, x, y0, y1 in [(1, 480, 390, 460), (2, 520, 410, 480)]:
            segments = [
                {"start_ms": 0, "end_ms": 4000, "from": [x, 300], "to": [x, y0]},
                {"start_ms": 13000, "end_ms": 13600, "from": [x, y0], "to": [x, y0]},
            ]
            if second_vanish is None:
                segments.append({"start_ms": 13600, "end_ms": 13600, "from": [x, y0], "to": [x, y0]})
            else:
                move_start = second_vanish - 600
                segments.extend([
                    {"start_ms": 13600, "end_ms": move_start, "from": [x, y0], "to": [x, y0]},
                    {"start_ms": move_start, "end_ms": second_vanish, "from": [x, y0], "to": [x, y1]},
                ])
            records.append(_hero(f"enemy_{index}", second_vanish or 4000, (x, 300), (x, y0), segments=segments))
        return records

    configs = [
        config("pos-two-group", "Two certified enemies move together toward the player and disappear together.", 16000, two, ["3.1", "4.1", "4.2", "4.3", "6.2", "6.3", "6.4"], must_admit=True, exact_admits=1, first_members=["enemy_1", "enemy_2"]),
        config("pos-three-group", "Three certified enemies move together toward the player and disappear together.", 16000, three, ["3.1", "4.1", "4.2", "4.3", "6.2", "6.3", "6.4"], must_admit=True, exact_admits=1, first_members=["enemy_1", "enemy_2", "enemy_3"]),
        config("neg-single-hero", "One previously seen enemy is missing for a long time with no coordinated partner.", 20000, single, ["3.1", "3.3", "4.1", "6.2", "6.3"], peak_reason="no_group", must_not_admit=True),
        config("neg-vanish-apart", "Two headed enemies disappear 3200 ms apart and never form an eligible group.", 18000, _pair(4000, vanish_ms_2=800), ["6.2", "6.3"], peak_reason="no_group", must_not_admit=True),
        config("bnd-vanish-3000", "Two headed enemies disappearing exactly 3000 ms apart satisfy the inclusive time bound.", 16000, _pair(4000, vanish_ms_2=7000), ["6.2", "6.3"], must_admit=True, first_members=["enemy_1", "enemy_2"]),
        config("neg-far-apart", "Two headed enemies disappear together but their final sightings are farther apart than the group radius.", 18000, far, ["6.2", "6.3"], peak_reason="no_group", must_not_admit=True),
        config("neg-opposite-heading", "Two nearby enemies disappear together while moving in opposite directions.", 18000, opposite, ["4.2", "6.2", "6.3"], peak_reason="no_group", must_not_admit=True),
        config("neg-stationary", "Two nearby standing enemies have no valid pre-vanish heading.", 18000, stationary, ["4.2", "6.1", "6.2", "6.3"], peak_reason="no_group", must_not_admit=True),
        config("neg-away-from-player", "A coordinated group moves away from the known player position.", 18000, away, ["6.2", "6.3", "6.4"], peak_reason="not_toward_player", must_not_admit=True, own_changes=owner_away),
        config("neg-too-far", "A coordinated group heads toward the player but remains more than 250 map units away.", 18000, two, ["6.2", "6.3", "6.4"], peak_reason="too_far", must_not_admit=True, own_changes=too_far_own),
        config("neg-own-pos-unknown", "A coordinated group is eligible but the local player position is unknown.", 18000, two, ["2", "6.3", "6.4", "8"], peak_reason="own_position_unknown", must_not_admit=True, own_changes=unknown_pos),
        config("opt-b-own-pos-unknown", "The same coordinated group admits when option B disables the player-relative position gate.", 16000, two, ["6.3", "6.4"], must_admit=True, params_override={"PLAYER_RELATIVE_GATE": False}, own_changes=unknown_pos),
        config("opt-b-away-from-player", "A group moving away from the player admits when option B disables the player-relative direction gate.", 16000, away, ["6.3", "6.4"], must_admit=True, params_override={"PLAYER_RELATIVE_GATE": False}, own_changes=owner_away),
        config("neg-anonymous", "Two moving anonymous tracks disappear together but cannot contribute without certified hero ids.", 18000, _pair(4000, heroes=(None, None)), ["2", "3.1", "6.1", "6.2", "6.3"], peak_reason="no_group", must_not_admit=True),
        config("neg-coverage-gap", "A 2400 ms capture-frame gap interrupts the absence baseline before danger and the enemies remain unseen.", 18000, two, ["3.4", "6.3", "8"], peak_reason="below_danger", must_reasons=["no_group"], must_not_admit=True, gap=(11800, 14200)),
        config("neg-stale-coverage", "Several stale-coverage ticks interrupt the absence baseline and later valid frames do not restore it.", 18000, two, ["3.4", "6.3", "8"], peak_reason="below_danger", must_reasons=["coverage_invalid", "no_group"], must_not_admit=True, coverage_changes=[{"start_ms": 11800, "end_ms": 12200, "coverage": "stale"}]),
        config("neg-paused", "The own-player pause flag becomes true during a coordinated absence and interrupts its baseline.", 18000, two, ["3.4", "6.3", "8"], peak_reason="below_danger", must_reasons=["paused_or_unknown", "no_group"], must_not_admit=True, own_changes=[{"start_ms": 11800, "end_ms": 12200, "values": {"paused": True}}]),
        config("neg-paused-unknown", "An unknown pause flag during the absence interrupts the baseline and fails closed.", 18000, two, ["3.4", "6.3", "8"], peak_reason="below_danger", must_reasons=["paused_or_unknown", "no_group"], must_not_admit=True, own_changes=[{"start_ms": 11800, "end_ms": 12200, "values": {"paused": None}}]),
        config("neg-own-dead", "The player dies just before the group vanishes, respawns later, and the death-window episode stays ineligible.", 22000, moved_pair(6000), ["5.1", "6.2", "6.3"], peak_reason="own_dead_or_grace", must_reasons=["no_group"], must_not_admit=True, own_changes=[{"start_ms": 5600, "end_ms": 14800, "values": {"alive": False}}]),
        config("neg-stale-alive", "A fresh dead tick opens a death window; stale alive state cannot close it before fresh in-match alive state starts grace, keeping the later vanish ineligible.", 26000, moved_pair(10000), ["5.1", "6.2", "6.3"], peak_reason="no_group", must_reasons=["own_dead_or_grace", "gsi_stale", "no_group"], must_not_admit=True, own_changes=[{"start_ms": 2000, "end_ms": 4000, "values": {"alive": False}}, {"start_ms": 4200, "end_ms": 9600, "values": {"fresh": False, "alive": True}}, {"start_ms": 9800, "end_ms": 26000, "values": {"fresh": True, "alive": True}}]),
        config("neg-kill-participation", "Own kills increase 1000 ms after the group vanishes, blocking the matching group window.", 18000, two, ["5.2", "6.3"], peak_reason="kill_participation", must_not_admit=True, own_changes=[{"start_ms": 5000, "end_ms": 100000, "values": {"kills": 1}}]),
        config("neg-kill-unknown", "Unknown own kill and assist counters keep the cue unavailable.", 18000, two, ["5.2", "6.3", "8"], peak_reason="kill_data_unknown", must_not_admit=True, own_changes=[{"start_ms": 0, "end_ms": 100000, "values": {"kills": None}}]),
        config("neg-kill-unknown-gap", "Unknown counters around the vanish block admission; the first known pair compares against the last fully known counters and detects the increase.", 18000, two, ["5.2", "6.3", "8"], peak_reason="kill_data_unknown", must_not_admit=True, expected_kill_change_times=[4400], own_changes=[{"start_ms": 3800, "end_ms": 4200, "values": {"kills": None}}, {"start_ms": 4400, "end_ms": 100000, "values": {"kills": 1}}]),
        config("neg-kill-unknown-gap-clean", "Unknown counters end more than 3000 ms before the earliest vanish and stay unchanged, so the clean coordinated group admits.", 18000, moved_pair(6000), ["5.2", "6.3", "8"], must_admit=True, exact_admits=1, first_members=["enemy_1", "enemy_2"], expected_kill_change_times=[], own_changes=[{"start_ms": 0, "end_ms": 2400, "values": {"kills": None}}]),
        config("neg-time-non-advancing", "A repeated timestamp interrupts the visible baseline before vanish; the discarded frame cannot later become a coordinated absence.", 16000, two, ["14"], peak_reason="no_group", must_reasons=["no_group"], must_not_admit=True, duplicate_tick_ms=4000),
        config("neg-media-unready", "Critical media stays unready while the coordinated group score crosses danger.", 18000, two, ["6.3", "7", "8"], peak_reason="media_unready", must_not_admit=True, media_changes=[{"start_ms": 0, "end_ms": 100000, "media_ready": False}]),
        config("neg-media-late", "Critical media becomes ready only after the coordinated group exceeds its maximum age.", 21000, two, ["6.2", "6.3", "7"], peak_reason="media_unready", must_reasons=["no_group"], must_not_admit=True, media_changes=[{"start_ms": 0, "end_ms": 19000, "media_ready": False}, {"start_ms": 19200, "end_ms": 100000, "media_ready": True}]),
        config("pos-media-recovers", "Critical media readiness recovers while the coordinated group is fresh and admits on that recovery tick.", 16000, two, ["6.2", "6.3", "7"], must_admit=True, exact_event_times=[("would_admit", 12200)], media_changes=[{"start_ms": 0, "end_ms": 12000, "media_ready": False}, {"start_ms": 12200, "end_ms": 100000, "media_ready": True}]),
        config("neg-pregame", "The coordinated absence is tracked but the session remains outside an active match.", 18000, two, ["2", "6.3"], peak_reason="not_in_match", must_not_admit=True, own_changes=[{"start_ms": 0, "end_ms": 100000, "values": {"in_match": False}}]),
        config("neg-gsi-stale", "The coordinated absence is tracked while the own-player GSI freshness flag is stale.", 18000, two, ["2", "6.3"], peak_reason="gsi_stale", must_not_admit=True, own_changes=[{"start_ms": 0, "end_ms": 100000, "values": {"fresh": False}}]),
        config("dedup-stays-missing", "One admitted absence remains missing beyond 90 seconds without a second cue or decay correction.", 96000, two, ["4.1", "6.2", "7"], must_admit=True, exact_admits=1, event_types=["would_admit"]),
        config("rearm-after-reappearance", "Both admitted members confirm reappearance, clear the latch, then form a second group after both cooldowns.", 90000, reappearing_pair(80000), ["3.2", "6.2", "7"], must_admit=True, exact_admits=2, event_types=["would_admit", "correction_would_admit", "would_admit"], exact_event_times=[("correction_would_admit", 13600), ("would_admit", 88000)]),
        config("blip-no-rearm", "One missing member is sighted for a single frame, which neither re-arms it nor clears the latch.", 18000, [
            _hero("enemy_1", 4000, (480, 300), (480, 390), segments=[{"start_ms": 0, "end_ms": 4000, "from": [480, 300], "to": [480, 390]}, {"start_ms": 13000, "end_ms": 13000, "from": [480, 390], "to": [480, 390]}]),
            _hero("enemy_2", 4000, (520, 300), (520, 410), segments=[{"start_ms": 0, "end_ms": 4000, "from": [520, 300], "to": [520, 410]}]),
        ], ["3.2", "7"], must_admit=True, exact_admits=1, event_types=["would_admit"]),
        config("bnd-reappear-short", "Three consecutive reappearance frames span only 400 ms and do not confirm.", 18000, [
            _hero("enemy_1", 4000, (480, 300), (480, 390), segments=[{"start_ms": 0, "end_ms": 4000, "from": [480, 300], "to": [480, 390]}, {"start_ms": 13000, "end_ms": 13400, "from": [480, 390], "to": [480, 390]}]),
            _hero("enemy_2", 4000, (520, 300), (520, 410), segments=[{"start_ms": 0, "end_ms": 4000, "from": [520, 300], "to": [520, 410]}, {"start_ms": 13000, "end_ms": 13400, "from": [520, 410], "to": [520, 410]}]),
        ], ["3.2", "7"], must_admit=True, exact_admits=1, event_types=["would_admit"]),
        config("bnd-reappear-ok", "Four consecutive reappearance frames span 600 ms and confirm the members, clearing the latch with a correction.", 18000, reappearing_pair(), ["3.2", "7"], must_admit=True, exact_admits=1, event_types=["would_admit", "correction_would_admit"], exact_event_times=[("correction_would_admit", 13600)]),
        config("latch-partial-reappear", "After A confirms reappearance it leaves the latch and does not rejoin when it vanishes alone; B's confirmation empties the latch at score_all 0.0, below revision 0.40, so the correction occurs then.", 16600, [
            _hero("enemy_1", 4000, (480, 300), (480, 390), segments=[
                {"start_ms": 0, "end_ms": 4000, "from": [480, 300], "to": [480, 390]},
                {"start_ms": 13000, "end_ms": 14000, "from": [480, 390], "to": [480, 390]},
                {"start_ms": 14000, "end_ms": 14600, "from": [480, 390], "to": [480, 460]},
            ]),
            _hero("enemy_2", 4000, (520, 300), (520, 410), segments=[
                {"start_ms": 0, "end_ms": 4000, "from": [520, 300], "to": [520, 410]},
                {"start_ms": 15000, "end_ms": 15600, "from": [520, 410], "to": [520, 410]},
            ]),
        ], ["3.2", "7"], must_admit=True, exact_admits=1, event_types=["would_admit", "correction_would_admit"], exact_event_times=[("correction_would_admit", 15600)], event_assertions=[{"kind": "correction_would_admit", "t_ms": 15600, "members": ["enemy_2"], "score_below_revision": True}]),
        config("global-cooldown-defers", "A distinct second group clears danger within 30 seconds and admits exactly when the global cooldown expires.", 46000, [
            _hero("enemy_1", 3800, (350, 300), (350, 400)),
            _hero("enemy_2", 3800, (390, 300), (390, 410)),
            _hero("enemy_3", 30000, (550, 390), (550, 420), segments=[{"start_ms": 0, "end_ms": 29400, "from": [550, 390], "to": [550, 390]}, {"start_ms": 29400, "end_ms": 30000, "from": [550, 390], "to": [550, 420]}]),
            _hero("enemy_4", 30000, (590, 410), (590, 440), segments=[{"start_ms": 0, "end_ms": 29400, "from": [590, 410], "to": [590, 410]}, {"start_ms": 29400, "end_ms": 30000, "from": [590, 410], "to": [590, 440]}]),
        ], ["6.2", "6.3", "7"], must_admit=True, exact_admits=2, first_members=["enemy_1", "enemy_2"], event_types=["would_admit", "would_admit"], exact_event_times=[("would_admit", 12000), ("would_admit", 42000)], must_reasons=["global_cooldown"]),
        config("contributor-cooldown", "A confirmed reappeared contributor joins a new group after global cooldown but before its per-contributor cooldown expires.", 52000, [
            _hero("enemy_1", 34000, (480, 300), (480, 460), segments=[{"start_ms": 0, "end_ms": 4000, "from": [480, 300], "to": [480, 390]}, {"start_ms": 13000, "end_ms": 13600, "from": [480, 390], "to": [480, 390]}, {"start_ms": 13600, "end_ms": 33400, "from": [480, 390], "to": [480, 390]}, {"start_ms": 33400, "end_ms": 34000, "from": [480, 390], "to": [480, 460]}]),
            _hero("enemy_2", 4000, (520, 300), (520, 410), segments=[{"start_ms": 0, "end_ms": 4000, "from": [520, 300], "to": [520, 410]}, {"start_ms": 13000, "end_ms": 13600, "from": [520, 410], "to": [520, 410]}, {"start_ms": 13600, "end_ms": 52000, "from": [520, 410], "to": [520, 410]}]),
            _hero("enemy_3", 34000, (520, 390), (520, 460), segments=[{"start_ms": 0, "end_ms": 33400, "from": [520, 390], "to": [520, 390]}, {"start_ms": 33400, "end_ms": 34000, "from": [520, 390], "to": [520, 460]}]),
        ], ["3.2", "6.2", "6.3", "7"], must_admit=True, exact_admits=1, event_types=["would_admit", "correction_would_admit"], must_reasons=["contributor_cooldown"]),
        config("correction-suppressed", "Latched members confirm reappearance while a separate missing contributor keeps aggregate risk above the revision threshold.", 18000, [
            *_pair(4000),
            _hero("enemy_3", 4000, (500, 390), (500, 390)),
        ][:3], ["3.2", "4.3", "7"], must_admit=True, exact_admits=1, event_types=["would_admit", "latch_cleared_silent"], exact_event_times=[("latch_cleared_silent", 13600)]),
        config("latch-gap", "A capture coverage loss after admission interrupts the latched heroes and clears the latch silently.", 15000, two, ["3.4", "7"], must_admit=True, exact_admits=1, event_types=["would_admit", "latch_cleared_silent"], exact_event_times=[("latch_cleared_silent", 13000)], coverage_changes=[{"start_ms": 13000, "end_ms": 13000, "coverage": "unavailable"}]),
    ]

    # Correction-suppressed needs only the initial two moving contributors and
    # the third stationary missing contributor, then the first two reappear.
    suppressed = next(item for item in configs if item["id"] == "correction-suppressed")
    suppressed["heroes"] = [
        _hero("enemy_1", 4000, (480, 300), (480, 390), segments=[{"start_ms": 0, "end_ms": 4000, "from": [480, 300], "to": [480, 390]}, {"start_ms": 13000, "end_ms": 13600, "from": [480, 390], "to": [480, 390]}]),
        _hero("enemy_2", 4000, (520, 300), (520, 410), segments=[{"start_ms": 0, "end_ms": 4000, "from": [520, 300], "to": [520, 410]}, {"start_ms": 13000, "end_ms": 13600, "from": [520, 410], "to": [520, 410]}]),
        _hero("enemy_3", 4000, (500, 390), (500, 390)),
    ]
    return configs


def build_documents() -> tuple[dict[str, bytes], list[dict[str, Any]]]:
    files: dict[str, bytes] = {}
    index_entries: list[dict[str, Any]] = []
    event_summaries = []
    for table in _table_fixtures():
        content = _rounded({key: value for key, value in table.items() if key not in ("file", "rule", "expect")})
        files[table["file"]] = _encode_json(content)
        index_entries.append({"id": table["id"], "file": table["file"], "kind": table["kind"], "rule": table["rule"], "expect": table["expect"]})

    for config in _scenario_definitions():
        fixture, info = _build_trace(config)
        file_name = f"{config['id']}.json"
        files[file_name] = _encode_json(fixture)
        index_entries.append({
            "id": config["id"],
            "file": file_name,
            "kind": "trace",
            "rule": ",".join(config["spec_sections"]),
            "expect": "admit" if config["must_admit"] else "silent",
        })
        event_summaries.append(info)

    index = {
        "status": "synthetic_frozen_draft",
        "approval": "not_approved",
        "spec": "docs/design/RISK-ADMISSION-SPEC.md",
        "generator": "tools/gen_risk_admission_fixtures.py",
        "fixtures": sorted(index_entries, key=lambda item: item["id"]),
    }
    files["index.json"] = _encode_json(index)
    assert all(entry["file"] in files for entry in index["fixtures"])
    assert "current-engine-replay.json" not in files

    by_id = {item["id"]: item for item in event_summaries}
    assert by_id["pos-three-group"]["event_summary"][0][1] < by_id["pos-two-group"]["event_summary"][0][1]
    assert by_id["rearm-after-reappearance"]["event_summary"][-1][1] > 13600 + DEFAULT_PARAMS["contributor_cooldown_ms"]
    assert len(index_entries) == 45, f"expected 45 indexed files, got {len(index_entries)}"
    return {name: files[name] for name in sorted(files)}, event_summaries


def _encode_json(value: Any) -> bytes:
    return (json.dumps(_rounded(value), ensure_ascii=False, sort_keys=True, indent=1, allow_nan=False) + "\n").encode("utf-8")


def _write_or_check(mode: str) -> int:
    files, summaries = build_documents()
    mismatches = []
    for name, content in files.items():
        path = OUT_DIR / name
        if mode == "write":
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        elif not path.is_file() or path.read_bytes() != content:
            mismatches.append(name)
    if mode == "check" and OUT_DIR.exists():
        allowed = set(files) | {"current-engine-replay.json"}
        unexpected = sorted(
            path.relative_to(OUT_DIR).as_posix()
            for path in OUT_DIR.rglob("*")
            if path.is_file() and path.relative_to(OUT_DIR).as_posix() not in allowed
        )
        mismatches.extend(f"unexpected:{name}" for name in unexpected)
    if mode == "check" and mismatches:
        print("fixture mismatch: " + ", ".join(mismatches), file=sys.stderr)
        return 1
    action = "wrote" if mode == "write" else "checked"
    print(f"{action} {len(files)} deterministic synthetic fixtures")
    for item in summaries:
        events = item["event_summary"]
        if events:
            rendered = ", ".join(
                f"{kind}@{t_ms}ms score={score:.9f}" if score is not None else f"{kind}@{t_ms}ms score=null"
                for kind, t_ms, score in events
            )
        else:
            rendered = f"none; peak={item['peak_reason']}@{item['peak_t_ms']}ms score={item['peak_score']:.9f}"
        print(f"{item['id']}: {rendered}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--write", action="store_true", help="write deterministic fixture files")
    modes.add_argument("--check", action="store_true", help="compare generated fixture bytes to disk")
    args = parser.parse_args(argv)
    return _write_or_check("write" if args.write else "check")


if __name__ == "__main__":
    raise SystemExit(main())


