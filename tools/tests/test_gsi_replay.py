"""Synthetic tests for the local-only recorded GSI replay tool."""

from __future__ import annotations

import base64
import contextlib
import http.server
import io
import json
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import gsi_replay as replay  # noqa: E402


TEST_TOKEN = "FAKE_TEMP_TOKEN_123456789"
RECORDED_TOKEN = "FAKE_RECORDED_TOKEN_DO_NOT_SEND"
PRIVATE_VALUES = ("FAKE_STEAMID_", "FAKE_PLAYER_", "FAKE_HERO_", RECORDED_TOKEN)
STATES = (
    "DOTA_GAMERULES_STATE_HERO_SELECTION",
    "DOTA_GAMERULES_STATE_STRATEGY_TIME",
    "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS",
)


def fake_payload(index: int, state: str, clock_time: int) -> dict[str, Any]:
    return {
        "auth": {"token": RECORDED_TOKEN},
        "provider": {"appid": 999999999},
        "map": {"game_state": state, "clock_time": clock_time},
        "player": {"steamid": f"FAKE_STEAMID_{index}", "name": f"FAKE_PLAYER_{index}"},
        "hero": {"name": f"FAKE_HERO_{index}"},
    }


class SessionFixture:
    def __init__(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="gsi-replay-test-")
        self.root = Path(self.temp.name)
        self.session = self.root / "fake-session"
        (self.session / "raw").mkdir(parents=True)
        self.records_path = self.session / "raw" / "000001.jsonl"
        self.write_default_records()

    def cleanup(self) -> None:
        self.temp.cleanup()

    def write_default_records(self) -> None:
        lines = []
        receipts = []
        for index, (state, clock_time, timestamp) in enumerate(zip(STATES, (0, 5, 10), (1_000_000_000, 2_000_000_000, 4_000_000_000))):
            body = json.dumps(fake_payload(index, state, clock_time), separators=(",", ":")).encode()
            lines.append(self.raw_record(index, timestamp, base64.b64encode(body).decode()))
            receipts.append({"body_complete_monotonic_ns": str(timestamp), "game_state": state, "phase_group": "pre_live" if index < 2 else "live", "sequence": index})
        self.records_path.write_text("".join(json.dumps(line) + "\n" for line in lines), encoding="utf-8")
        (self.session / "manifest.json").write_text(json.dumps({
            "requested_data_blocks": ["provider", "map", "player", "hero", "abilities", "items"],
            "phase_receipts": receipts,
            "phase_coverage": {},
            "raw_files": [{"path": "raw/000001.jsonl", "record_count": 3}],
        }), encoding="utf-8")

    @staticmethod
    def raw_record(sequence: int, timestamp: int, body: str, parse_status: str = "ok") -> dict[str, Any]:
        return {
            "raw_body_base64": body,
            "body_sha256": "0" * 64,
            "body_complete_monotonic_ns": str(timestamp),
            "received_first_byte_monotonic_ns": str(timestamp - 10),
            "sequence": sequence,
            "content_type": "application/json",
            "parse_status": parse_status,
            "body_bytes": 1,
        }

    def append_raw(self, raw: dict[str, Any] | str) -> None:
        with self.records_path.open("a", encoding="utf-8") as stream:
            stream.write((raw if isinstance(raw, str) else json.dumps(raw)) + "\n")


class FakeClock:
    def __init__(self, oversleep: float = 0.0) -> None:
        self.now = 100.0
        self.oversleep = oversleep
        self.sleeps: list[float] = []

    def monotonic(self) -> float:
        return self.now

    def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds + self.oversleep


class LocalIntake:
    def __init__(self, statuses: list[int] | None = None) -> None:
        owner = self

        class Handler(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_POST(self) -> None:  # noqa: N802
                size = int(self.headers.get("Content-Length", "0"))
                owner.bodies.append(self.rfile.read(size))
                owner.connection_ids.add(id(self.connection))
                self.send_response(owner.statuses[min(len(owner.bodies) - 1, len(owner.statuses) - 1)])
                self.send_header("Content-Length", "0")
                self.end_headers()

            def log_message(self, _format: str, *args: Any) -> None:
                return

        self.bodies: list[bytes] = []
        self.connection_ids: set[int] = set()
        self.statuses = statuses or [204]
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = True
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    @property
    def port(self) -> int:
        return self.server.server_address[1]

    def __enter__(self) -> "LocalIntake":
        self.thread.start()
        return self

    def __exit__(self, *_args: Any) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


class GsiReplayTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = SessionFixture()

    def tearDown(self) -> None:
        self.fixture.cleanup()

    def invoke(self, args: list[str], *, env: dict[str, str] | None = None, clock: FakeClock | None = None,
               sleep=None, connection_factory=None) -> tuple[int, str, str]:
        stdout = io.StringIO()
        stderr = io.StringIO()
        fake_clock = clock or FakeClock()
        code = replay.main(args, environ=env or {}, stdout=stdout, stderr=stderr,
                           clock=fake_clock.monotonic, sleep=sleep or fake_clock.sleep,
                           connection_factory=connection_factory)
        return code, stdout.getvalue(), stderr.getvalue()

    def replay_args(self, port: int, *extra: str) -> list[str]:
        return ["replay", str(self.fixture.session), "--port", str(port), "--demo", *extra]

    def test_manifest_and_phase_receipts_are_loaded(self) -> None:
        session = replay.load_session(self.fixture.session)
        self.assertEqual(len(session.records), 3)
        self.assertEqual(session.skipped, 0)
        self.assertEqual(session.records[2].phase, STATES[2])
        self.assertEqual(session.records[1].clock_time, 5)

    def test_list_prints_counts_and_phases_without_payload_values(self) -> None:
        code, stdout, stderr = self.invoke(["--list", "--root", str(self.fixture.root)])
        self.assertEqual((code, stderr), (0, ""))
        self.assertIn("fake-session", stdout)
        self.assertIn("records=3", stdout)
        self.assertIn("duration=3.000s", stdout)
        self.assertIn("sequence=2", stdout)
        self.assertNotIn("FAKE_", stdout)
        for private_value in PRIVATE_VALUES:
            self.assertNotIn(private_value, stdout)

    def test_invalid_outer_jsonl_record_is_counted_as_skipped(self) -> None:
        self.fixture.append_raw("{not-json")
        session = replay.load_session(self.fixture.session)
        self.assertEqual(len(session.records), 3)
        self.assertEqual(session.skipped, 1)

    def test_non_success_parse_status_is_skipped(self) -> None:
        self.fixture.append_raw(SessionFixture.raw_record(3, 5_000_000_000, "e30=", "error"))
        session = replay.load_session(self.fixture.session)
        self.assertEqual(session.skipped, 1)

    def test_recorder_json_parse_status_is_a_success_marker(self) -> None:
        body = base64.b64encode(json.dumps(fake_payload(3, STATES[2], 15)).encode()).decode()
        self.fixture.append_raw(SessionFixture.raw_record(3, 5_000_000_000, body, "json"))
        session = replay.load_session(self.fixture.session)
        self.assertEqual(len(session.records), 4)
        self.assertEqual(session.skipped, 0)

    def test_invalid_base64_body_is_skipped(self) -> None:
        self.fixture.append_raw(SessionFixture.raw_record(3, 5_000_000_000, "***"))
        session = replay.load_session(self.fixture.session)
        self.assertEqual(session.skipped, 1)

    def test_valid_base64_with_invalid_json_body_is_skipped(self) -> None:
        encoded = base64.b64encode(b"{broken").decode()
        self.fixture.append_raw(SessionFixture.raw_record(3, 5_000_000_000, encoded))
        session = replay.load_session(self.fixture.session)
        self.assertEqual(session.skipped, 1)

    def test_non_object_json_body_is_skipped(self) -> None:
        encoded = base64.b64encode(b"[]").decode()
        self.fixture.append_raw(SessionFixture.raw_record(3, 5_000_000_000, encoded))
        session = replay.load_session(self.fixture.session)
        self.assertEqual(session.skipped, 1)

    def test_phase_sequence_and_clock_points_parse(self) -> None:
        phase_point = replay.parse_point("phase:GAME_IN_PROGRESS")
        self.assertEqual(phase_point.kind, "phase")
        self.assertEqual(phase_point.value, STATES[2])
        self.assertEqual(replay.parse_point("seq:2").value, 2)
        self.assertEqual(replay.parse_point("clock:12.5").value, 12.5)

    def test_invalid_point_forms_are_rejected(self) -> None:
        for value in ("phase:", "seq:-1", "clock:NaN", "clock:inf", "other:2"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                replay.parse_point(value)

    def test_start_at_phase_selects_first_matching_record(self) -> None:
        selected = replay.select_records(replay.load_session(self.fixture.session), "phase:GAME_IN_PROGRESS", None)
        self.assertEqual([record.sequence for record in selected], [2])

    def test_start_at_sequence_selects_first_record_at_or_after(self) -> None:
        selected = replay.select_records(replay.load_session(self.fixture.session), "seq:1", None)
        self.assertEqual([record.sequence for record in selected], [1, 2])

    def test_start_at_clock_selects_first_record_at_or_after(self) -> None:
        selected = replay.select_records(replay.load_session(self.fixture.session), "clock:6", None)
        self.assertEqual([record.sequence for record in selected], [2])

    def test_stop_at_phase_includes_boundary_record(self) -> None:
        selected = replay.select_records(replay.load_session(self.fixture.session), None, "phase:" + STATES[1])
        self.assertEqual([record.sequence for record in selected], [0, 1])

    def test_stop_at_sequence_includes_boundary_record(self) -> None:
        selected = replay.select_records(replay.load_session(self.fixture.session), None, "seq:1")
        self.assertEqual([record.sequence for record in selected], [0, 1])

    def test_stop_at_clock_includes_boundary_record(self) -> None:
        selected = replay.select_records(replay.load_session(self.fixture.session), None, "clock:5")
        self.assertEqual([record.sequence for record in selected], [0, 1])

    def test_missing_phase_or_clock_point_is_an_error(self) -> None:
        session = replay.load_session(self.fixture.session)
        for value in ("phase:DOTA_GAMERULES_STATE_POST_GAME", "clock:500"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                replay.select_records(session, value, None)

    def test_speed_bounds_are_checked(self) -> None:
        for speed in ("0.5", "61", "nan"):
            with self.subTest(speed=speed):
                code, _, _ = self.invoke(self.replay_args(1, "--speed", speed), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
                self.assertEqual(code, 2)

    def test_non_local_host_is_refused_before_connecting(self) -> None:
        code, stdout, stderr = self.invoke(self.replay_args(1, "--host", "example.invalid"), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 2)
        self.assertIn("refused", stdout.lower())
        self.assertEqual(stderr, "")

    def test_missing_test_token_is_refused(self) -> None:
        code, stdout, _ = self.invoke(self.replay_args(1))
        self.assertEqual(code, 2)
        self.assertIn("token", stdout.lower())

    def test_short_test_token_is_refused(self) -> None:
        code, stdout, _ = self.invoke(self.replay_args(1), env={"KOHBOR_TEST_TOKEN": "short"})
        self.assertEqual(code, 2)
        self.assertIn("token", stdout.lower())

    def test_demo_acknowledgement_is_required_even_with_unsafe_override(self) -> None:
        args = ["replay", str(self.fixture.session), "--port", "1", "--i-know-this-is-not-the-real-app"]
        code, stdout, _ = self.invoke(args, env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 2)
        self.assertIn("--demo", stdout)
        self.assertNotIn(TEST_TOKEN, stdout)

    def test_dry_run_prints_plan_and_sends_nothing(self) -> None:
        with LocalIntake() as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port, "--dry-run", "--speed", "20"), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertIn("records=3", stdout)
        self.assertIn("duration=0.150s", stdout)
        self.assertEqual(intake.bodies, [])
        self.assertNotIn(TEST_TOKEN, stdout)
        self.assertNotIn("FAKE_", stdout)
        for private_value in PRIVATE_VALUES:
            self.assertNotIn(private_value, stdout)

    def test_loop_dry_run_reports_unbounded_total_and_one_loop_duration(self) -> None:
        with LocalIntake() as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port, "--dry-run", "--loop", "--speed", "20"),
                                          env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertIn("records=unbounded", stdout)
        self.assertIn("per_loop=3", stdout)
        self.assertIn("one_loop_duration=0.150s", stdout)
        self.assertEqual(intake.bodies, [])

    def test_recorded_token_is_replaced_before_local_post(self) -> None:
        with LocalIntake() as intake:
            code, stdout, stderr = self.invoke(self.replay_args(intake.port, "--speed", "60"), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual((code, stderr), (0, ""))
        self.assertEqual(len(intake.bodies), 3)
        self.assertEqual(len(intake.connection_ids), 1)
        for body in intake.bodies:
            decoded = json.loads(body)
            self.assertEqual(decoded["auth"]["token"], TEST_TOKEN)
            self.assertNotIn(RECORDED_TOKEN.encode(), body)
        self.assertNotIn(TEST_TOKEN, stdout)
        self.assertNotIn(RECORDED_TOKEN, stdout)

    def test_auth_rejection_stops_after_first_request_with_exit_three(self) -> None:
        with LocalIntake([401]) as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 3)
        self.assertEqual(len(intake.bodies), 1)
        self.assertIn("refused: target did not accept the test token", stdout)
        self.assertNotIn(TEST_TOKEN, stdout)
        self.assertNotIn("FAKE_", stdout)

    def test_unsafe_override_allows_different_auth_response(self) -> None:
        with LocalIntake([401, 200, 200]) as intake:
            code, stdout, _ = self.invoke(["replay", str(self.fixture.session), "--port", str(intake.port),
                                          "--demo", "--i-know-this-is-not-the-real-app", "--speed", "60"],
                                         env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertEqual(len(intake.bodies), 3)
        self.assertIn("http_statuses={200:2,401:1}", stdout)

    def test_http_status_histogram_counts_responses(self) -> None:
        with LocalIntake([204, 200, 202]) as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port, "--speed", "60"), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertIn("http_statuses={200:1,202:1,204:1}", stdout)

    def test_schedule_uses_absolute_monotonic_targets_without_drift(self) -> None:
        clock = FakeClock(oversleep=0.1)
        code, _, _ = self.invoke(self.replay_args(1, "--dry-run"), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN}, clock=clock)
        self.assertEqual(code, 0)
        self.assertEqual(clock.sleeps, [])
        with LocalIntake() as intake:
            clock = FakeClock(oversleep=0.1)
            code, _, _ = self.invoke(self.replay_args(intake.port), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN}, clock=clock)
        self.assertEqual(code, 0)
        self.assertAlmostEqual(clock.sleeps[0], 1.0)
        self.assertAlmostEqual(clock.sleeps[1], 1.9)

    def test_lateness_summary_reports_mean_p95_and_max(self) -> None:
        with LocalIntake() as intake:
            clock = FakeClock(oversleep=0.1)
            code, stdout, _ = self.invoke(self.replay_args(intake.port), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN}, clock=clock)
        self.assertEqual(code, 0)
        self.assertIn("late_ms_mean=66.7", stdout)
        self.assertIn("late_ms_p95=100.0", stdout)
        self.assertIn("late_ms_max=100.0", stdout)

    def test_loop_repeats_selected_records_until_max_record_limit(self) -> None:
        with LocalIntake() as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port, "--loop", "--max-records", "5", "--speed", "60"),
                                          env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertEqual(len(intake.bodies), 5)
        self.assertIn("records_sent=5", stdout)

    def test_max_records_caps_attempted_posts(self) -> None:
        with LocalIntake() as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port, "--max-records", "2", "--speed", "60"),
                                          env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertEqual(len(intake.bodies), 2)
        self.assertIn("records_sent=2", stdout)

    def test_connection_error_retries_and_reconnects(self) -> None:
        class Connection:
            calls = 0
            closed = False

            def request(self, *_args: Any, **_kwargs: Any) -> None:
                Connection.calls += 1
                if Connection.calls == 1:
                    raise OSError("FAKE_TRANSPORT_ERROR")

            def getresponse(self):
                return type("Response", (), {"status": 204, "read": lambda self: b""})()

            def close(self) -> None:
                Connection.closed = True

        made: list[Connection] = []

        def factory(*_args: Any, **_kwargs: Any) -> Connection:
            connection = Connection()
            made.append(connection)
            return connection

        code, stdout, _ = self.invoke(self.replay_args(1, "--speed", "60", "--max-records", "1"),
                                      env={"KOHBOR_TEST_TOKEN": TEST_TOKEN}, connection_factory=factory)
        self.assertEqual(code, 0)
        self.assertEqual(Connection.calls, 2)
        self.assertTrue(Connection.closed)
        self.assertIn("records_sent=1", stdout)

    def test_connection_error_stops_after_three_retries_without_leaking_error(self) -> None:
        class Connection:
            def request(self, *_args: Any, **_kwargs: Any) -> None:
                raise OSError("FAKE_TRANSPORT_ERROR")

            def close(self) -> None:
                return

        made = []

        def factory(*_args: Any, **_kwargs: Any) -> Connection:
            connection = Connection()
            made.append(connection)
            return connection

        code, stdout, _ = self.invoke(self.replay_args(1, "--speed", "60", "--max-records", "1"),
                                      env={"KOHBOR_TEST_TOKEN": TEST_TOKEN}, connection_factory=factory)
        self.assertEqual(code, 1)
        self.assertEqual(len(made), 4)
        self.assertNotIn("FAKE_TRANSPORT_ERROR", stdout)
        self.assertIn("stop_reason=error", stdout)

    def test_keyboard_interrupt_is_reported_as_interrupt(self) -> None:
        with LocalIntake() as intake:
            calls = 0

            def interrupt_on_first_wait(seconds: float) -> None:
                nonlocal calls
                calls += 1
                raise KeyboardInterrupt

            code, stdout, _ = self.invoke(self.replay_args(intake.port), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN},
                                          sleep=interrupt_on_first_wait)
        self.assertEqual(code, 130)
        self.assertEqual(len(intake.bodies), 1)
        self.assertIn("stop_reason=interrupt", stdout)

    def test_stdout_never_contains_test_tokens_or_synthetic_identity_values(self) -> None:
        with LocalIntake() as intake:
            code, stdout, stderr = self.invoke(self.replay_args(intake.port, "--speed", "60"), env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual((code, stderr), (0, ""))
        for secret_or_payload in (*PRIVATE_VALUES, TEST_TOKEN, "FAKE_TRANSPORT_ERROR"):
            self.assertNotIn(secret_or_payload, stdout)
        self.assertNotIn("FAKE_", stdout)

    def test_local_host_spellings_are_accepted_by_safety_check(self) -> None:
        for host in ("127.0.0.1", "localhost", "::1"):
            with self.subTest(host=host):
                self.assertTrue(replay.is_local_host(host))

    def test_stop_at_cli_reports_stop_reason(self) -> None:
        with LocalIntake() as intake:
            code, stdout, _ = self.invoke(self.replay_args(intake.port, "--speed", "60", "--stop-at", "seq:1"),
                                          env={"KOHBOR_TEST_TOKEN": TEST_TOKEN})
        self.assertEqual(code, 0)
        self.assertEqual(len(intake.bodies), 2)
        self.assertIn("stop_reason=--stop-at", stdout)


if __name__ == "__main__":
    unittest.main()
