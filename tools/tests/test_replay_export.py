"""Tests for the Lab DEM exporter (tools/replay_export), standard library only.

The fixture neutral-es-mid.json is derived from one of the owner's local DEM
files (trimmed, real match ID replaced, no personal data). The gem-dota parser
is not needed: extraction is exercised separately on the owner's machine.
"""

from __future__ import annotations

import base64
import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools" / "replay_export"))

import analyze  # noqa: E402
import export  # noqa: E402
import fixture  # noqa: E402
import pin  # noqa: E402
import privacy  # noqa: E402

FIXTURE = ROOT / "tools" / "tests" / "fixtures" / "replay_export" / "neutral-es-mid.json"
TS_FIXTURE = ROOT / "apps" / "desktop" / "tests" / "fixtures" / "replay" / "report-v3-fixture.json"
TEST_LAYOUT = {"patch": "7.41f", "width": 256, "height": 256,
               "calibration": {"sx": 0.0154, "sy": -0.0154, "ox": 128, "oy": 128}}


def neutral() -> dict:
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def build_test_report() -> dict:
    data = "data:image/png;base64," + base64.b64encode(fixture.blank_png()).decode("ascii")
    return analyze.build_report([neutral()], [dict(TEST_LAYOUT, map=data)])


def hero(key, team, points, deaths=()):
    return {"key": key, "hero": key, "team": team, "points": points, "deaths": list(deaths),
            "lane": None, "roaming": False, "nw10": None, "lh10": None}


def scene(enemy_points, ally_points=(), extra_deaths=()):
    death = {"tick": 3000, "t": 100.0, "x": 0.0, "y": 0.0, "age": 0.0, "killer": "e"}
    heroes = [hero("self", "Radiant", [[s, 0.0, 0.0] for s in range(90, 101)], [death])]
    heroes += [hero(f"ally-{i + 1}", "Radiant", list(ally_points[i]) if i < len(ally_points) else [[100, 9000.0, 9000.0]])
               for i in range(4)]
    heroes += [hero(f"enemy-{i + 1}", "Dire", list(enemy_points[i]) if i < len(enemy_points) else [[100, -9000.0, -9000.0]],
                    [d for d in extra_deaths if d.get("owner") == f"enemy-{i + 1}"]) for i in range(5)]
    return death, heroes


class DeathRules(unittest.TestCase):
    def test_solo_gank_teamfight_other(self):
        near = [[s, 800.0, 0.0] for s in range(90, 101)]
        death, heroes = scene([near])
        self.assertEqual(analyze.classify_death(death, [], heroes)["type"], "solo")
        death, heroes = scene([near, near])
        self.assertEqual(analyze.classify_death(death, [], heroes)["type"], "gank")
        # A damage credit counts as involvement even without a nearby sample.
        death, heroes = scene([near])
        self.assertEqual(analyze.classify_death(death, ["enemy-4"], heroes)["type"], "gank")
        death, heroes = scene([near, near], ally_points=[[[100, 300.0, 0.0]]])
        self.assertEqual(analyze.classify_death(death, [], heroes)["type"], "other")
        fight = [{"owner": "enemy-1", "tick": 1, "t": 95.0, "x": 500.0, "y": 0.0, "age": 0},
                 {"owner": "enemy-2", "tick": 2, "t": 104.0, "x": -500.0, "y": 0.0, "age": 0}]
        death, heroes = scene([near, near], extra_deaths=fight)
        result = analyze.classify_death(death, [], heroes)
        self.assertEqual((result["type"], result["nearbyDeaths"]), ("teamfight", 3))

    def test_enemy_outside_radius_or_window_is_not_counted(self):
        far = [[s, 1600.0, 0.0] for s in range(90, 101)]
        early = [[80, 100.0, 0.0]]
        death, heroes = scene([far, early])
        self.assertEqual(analyze.classify_death(death, [], heroes)["enemiesInvolved"], 0)

    def test_vision_ward_and_smoke_windows(self):
        wards = [{"team": "Radiant", "type": "observer", "x": 1000.0, "y": 0.0, "t0": 50.0, "t1": 100.0},
                 {"team": "Radiant", "type": "sentry", "x": 0.0, "y": 0.0, "t0": 50.0, "t1": 200.0},
                 {"team": "Dire", "type": "observer", "x": 1500.0, "y": 0.0, "t0": 90.0, "t1": None}]
        self.assertFalse(analyze.ward_cover(100.0, 0, 0, wards, "Radiant", 1600))  # expired at 100, sentries ignored
        self.assertTrue(analyze.ward_cover(99.9, 0, 0, wards, "Radiant", 1600))
        self.assertTrue(analyze.ward_cover(100.0, 0, 0, wards, "Dire", 1600))
        smokes = [{"team": "Dire", "t": 69.0}, {"team": "Dire", "t": 80.0}, {"team": "Radiant", "t": 99.0}]
        self.assertEqual(analyze.enemy_smoke_before(100.0, smokes, "Radiant", 30), 20.0)
        self.assertIsNone(analyze.enemy_smoke_before(100.0, smokes[:1], "Radiant", 30))
        self.assertIsNone(analyze.visible_seconds(100.0, [], 10))
        self.assertEqual(analyze.visible_seconds(100.0, [[0.0, 0], [95.0, 1]], 10), 6)

    def test_rules_are_provisional_and_versioned(self):
        self.assertTrue(analyze.RULES["provisional"])
        self.assertEqual(analyze.RULES["enemyPositions"], "full-map")
        self.assertEqual(analyze.RULES["notes"]["max"], 3)


class FixtureMatch(unittest.TestCase):
    def setUp(self):
        self.match = analyze.build_match(neutral())

    def test_death_review_on_dem_derived_fixture(self):
        coach = self.match["coach"]
        self.assertEqual([analyze.clock(d["t"]) for d in coach["deaths"]],
                         ["4:10", "9:50", "20:31", "31:31", "35:12", "37:27"])
        self.assertEqual([d["type"] for d in coach["deaths"]],
                         ["solo", "other", "teamfight", "teamfight", "other", "other"])
        self.assertEqual([d["tick"] for d in coach["deaths"]], [d["tick"] for d in self.match["deaths"]])
        mid = next(lane for lane in coach["lane10"]["lanes"] if lane["lane"] == "mid")
        self.assertEqual((mid["heroes"], mid["nwDiff"], mid["outcome"]), (["self"], -717, "behind"))

    def test_notes_are_templates_with_evidence(self):
        notes = self.match["coach"]["notes"]
        self.assertLessEqual(len(notes), 3)
        ticks = {d["tick"] for d in self.match["coach"]["deaths"]}
        for note in notes:
            self.assertTrue(set(note["evidence"]) <= ticks)
            self.assertTrue(note["en"] and note["th"])
            self.assertLessEqual(max(len(note["en"]), len(note["th"])), 480)
        self.assertEqual([n["kind"] for n in notes], ["pickoff-no-vision", "seen-before-pickoff", "lane-behind"])

    def test_committed_viewer_fixture_matches_the_exporter(self):
        built = json.loads(json.dumps(build_test_report(), ensure_ascii=False))
        self.assertEqual(json.loads(TS_FIXTURE.read_text(encoding="utf-8")), built)


class Privacy(unittest.TestCase):
    def test_fixture_and_report_are_clean(self):
        self.assertEqual(privacy.scan(neutral()), [])
        self.assertEqual(privacy.scan(build_test_report()), [])
        self.assertNotIn("9073867431", FIXTURE.read_text(encoding="utf-8"))

    def test_scan_finds_names_ids_and_chat_keys(self):
        doc = {"a": {"player_name": "x"}, "b": "hello SomePlayerName", "c": 76561198000000001, "d": [123456789],
               "e": "id 123456789 here", "chat": []}
        findings = privacy.scan(doc, ["SomePlayerName"], [123456789])
        joined = "\n".join(findings)
        for expected in ("$.a: forbidden key", "$.b: contains a player name", "$.c: SteamID64-shaped number",
                         "$.d[0]: player identifier number", "$.e: contains a player identifier", "$: forbidden key"):
            self.assertIn(expected, joined)
        self.assertNotIn("SomePlayerName", joined)

    def test_hero_names_are_not_mistaken_for_players(self):
        self.assertEqual(privacy.scan({"hero": "Pudge"}, ["Pudge"], allowed_strings=["Pudge"]), [])

    def test_export_refuses_a_model_with_personal_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            bad = neutral()
            bad["heroes"][1]["player_name"] = "Someone"
            (tmp / "bad.json").write_text(json.dumps(bad), encoding="utf-8")
            (tmp / "map.png").write_bytes(fixture.blank_png())
            (tmp / "layout.json").write_text(json.dumps(dict(TEST_LAYOUT, mapFile="map.png")), encoding="utf-8")
            with contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
                code = export.main(["--neutral-in", str(tmp / "bad.json"), "--layout", str(tmp / "layout.json"),
                                    "--out", str(tmp / "out.json")])
            self.assertEqual(code, 3)
            self.assertFalse((tmp / "out.json").exists())


class ExportCli(unittest.TestCase):
    def test_neutral_in_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / "map.png").write_bytes(fixture.blank_png())
            (tmp / "layout.json").write_text(json.dumps(dict(TEST_LAYOUT, mapFile="map.png")), encoding="utf-8")
            with contextlib.redirect_stdout(io.StringIO()):
                code = export.main(["--neutral-in", str(FIXTURE), "--layout", str(tmp / "layout.json"),
                                    "--out", str(tmp / "out.json")])
            self.assertEqual(code, 0)
            report = json.loads((tmp / "out.json").read_text(encoding="utf-8"))
            self.assertEqual(report, json.loads(json.dumps(build_test_report(), ensure_ascii=False)))

    def test_dem_requires_hero_and_patch(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            export.main(["--dem", "x.dem", "--layout", "l.json", "--out", "o.json"])


class Pin(unittest.TestCase):
    def test_pin_file_and_missing_distribution(self):
        data = pin.load_pin()
        self.assertEqual((data["distribution"], data["version"]), ("gem-dota", "0.14.0"))
        self.assertRegex(data["record_sha256"], r"^[0-9a-f]{64}$")
        problems = pin.verify_installed(dict(data, distribution="kohbor-no-such-distribution"))
        self.assertEqual(len(problems), 1)
        self.assertIn("not installed", problems[0])


if __name__ == "__main__":
    unittest.main()
