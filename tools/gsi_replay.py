#!/usr/bin/env python3
"""Replay a recorded Dota GSI session into an explicitly acknowledged loopback demo."""

from __future__ import annotations

import argparse
import base64
import binascii
import collections
import http.client
import json
import math
import os
import re
import socket
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, TextIO


DEFAULT_ROOT = r"F:\gsi-log"
SUCCESS_PARSE_STATUSES = {"ok", "json"}
GAME_STATE_RE = re.compile(r"^DOTA_GAMERULES_STATE_[A-Z0-9_]+$")
TOKEN_MIN_LENGTH = 16
REQUEST_TIMEOUT_SECONDS = 2.0
MAX_RETRIES = 3


class ReplayError(Exception):
    """A sanitized input, selection, or delivery failure."""


@dataclass(frozen=True)
class ReplayPoint:
    kind: str
    value: str | int | float


@dataclass
class ReplayRecord:
    sequence: int
    timestamp_ns: int
    body: dict[str, Any]
    phase: str | None
    clock_time: float | None


@dataclass
class ReplaySession:
    path: Path
    manifest: dict[str, Any]
    records: list[ReplayRecord]
    total_records: int
    skipped: int

    @property
    def duration_ns(self) -> int:
        if len(self.records) < 2:
            return 0
        return self.records[-1].timestamp_ns - self.records[0].timestamp_ns


@dataclass
class ReplayResult:
    sent: int = 0
    skipped: int = 0
    statuses: collections.Counter[int] | None = None
    lateness_ms: list[float] | None = None
    stop_reason: str = "finished"
    error_code: int = 0
    started_at: float = 0.0
    finished_at: float = 0.0

    def __post_init__(self) -> None:
        if self.statuses is None:
            self.statuses = collections.Counter()
        if self.lateness_ms is None:
            self.lateness_ms = []


def _safe_game_state(value: Any) -> str | None:
    if isinstance(value, str) and GAME_STATE_RE.fullmatch(value):
        return value
    return None


def _normalize_phase(value: str) -> str | None:
    if value.startswith("DOTA_GAMERULES_STATE_"):
        return _safe_game_state(value)
    if re.fullmatch(r"[A-Z0-9_]+", value):
        return _safe_game_state("DOTA_GAMERULES_STATE_" + value)
    return None


def _load_manifest(session_path: Path) -> dict[str, Any]:
    try:
        value = json.loads((session_path / "manifest.json").read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        raise ReplayError("session manifest could not be read") from None
    if not isinstance(value, dict):
        raise ReplayError("session manifest has an invalid shape")
    return value


def _phase_receipts(manifest: dict[str, Any]) -> tuple[dict[int, str], list[tuple[int, str, int]]]:
    by_sequence: dict[int, str] = {}
    receipts: list[tuple[int, str, int]] = []
    values = manifest.get("phase_receipts", [])
    if not isinstance(values, list):
        return by_sequence, receipts
    for receipt in values:
        if not isinstance(receipt, dict):
            continue
        state = _safe_game_state(receipt.get("game_state"))
        try:
            sequence = int(receipt["sequence"])
            timestamp_ns = int(receipt["body_complete_monotonic_ns"])
        except (KeyError, TypeError, ValueError):
            continue
        if state is None:
            continue
        by_sequence.setdefault(sequence, state)
        receipts.append((timestamp_ns, state, sequence))
    return by_sequence, receipts


def _raw_paths(session_path: Path, manifest: dict[str, Any]) -> list[Path]:
    entries = manifest.get("raw_files")
    if not isinstance(entries, list) or not entries:
        return sorted((session_path / "raw").glob("*.jsonl"))
    root = session_path.resolve()
    paths: list[Path] = []
    for entry in entries:
        relative = entry if isinstance(entry, str) else entry.get("path") if isinstance(entry, dict) else None
        if not isinstance(relative, str):
            raise ReplayError("manifest raw-file entry is invalid")
        candidate = Path(relative)
        if candidate.is_absolute() or ".." in candidate.parts or not candidate.parts or candidate.parts[0].lower() != "raw":
            raise ReplayError("manifest raw-file path is invalid")
        resolved = (session_path / candidate).resolve()
        try:
            resolved.relative_to(root)
        except ValueError:
            raise ReplayError("manifest raw-file path is invalid") from None
        if resolved.suffix.lower() == ".jsonl":
            paths.append(resolved)
    return sorted(paths)


def _decode_record(raw: Any, phase_by_sequence: dict[int, str]) -> ReplayRecord | None:
    if not isinstance(raw, dict):
        return None
    parse_status = raw.get("parse_status")
    if not isinstance(parse_status, str) or parse_status.lower() not in SUCCESS_PARSE_STATUSES:
        return None
    encoded = raw.get("raw_body_base64")
    try:
        body_bytes = base64.b64decode(encoded, validate=True)
        body = json.loads(body_bytes.decode("utf-8"))
        sequence = int(raw["sequence"])
        timestamp_ns = int(raw["body_complete_monotonic_ns"])
    except (binascii.Error, UnicodeError, json.JSONDecodeError, KeyError, TypeError, ValueError):
        return None
    if not isinstance(body, dict):
        return None
    if timestamp_ns < 0:
        return None
    map_body = body.get("map")
    if not isinstance(map_body, dict):
        map_body = {}
    phase = _safe_game_state(map_body.get("game_state")) or phase_by_sequence.get(sequence)
    clock_time_value = map_body.get("clock_time")
    clock_time: float | None = None
    if isinstance(clock_time_value, (int, float)) and not isinstance(clock_time_value, bool):
        if math.isfinite(float(clock_time_value)):
            clock_time = float(clock_time_value)
    return ReplayRecord(sequence, timestamp_ns, body, phase, clock_time)


def load_session(session_path: str | Path) -> ReplaySession:
    """Read a recording into memory; never write its bodies or metadata to disk."""
    path = Path(session_path)
    manifest = _load_manifest(path)
    phase_by_sequence, _ = _phase_receipts(manifest)
    try:
        raw_paths = _raw_paths(path, manifest)
    except (OSError, ReplayError):
        raise ReplayError("session records could not be read") from None
    if not raw_paths:
        raise ReplayError("session contains no raw record files")

    records: list[ReplayRecord] = []
    total_records = 0
    skipped = 0
    for raw_path in raw_paths:
        try:
            with raw_path.open("r", encoding="utf-8") as stream:
                for line in stream:
                    if not line.strip():
                        continue
                    total_records += 1
                    try:
                        raw = json.loads(line)
                    except (json.JSONDecodeError, UnicodeError):
                        skipped += 1
                        continue
                    record = _decode_record(raw, phase_by_sequence)
                    if record is None:
                        skipped += 1
                        continue
                    records.append(record)
        except (OSError, UnicodeError):
            raise ReplayError("session records could not be read") from None
    if not records:
        raise ReplayError("session has no decodable records")
    if any(right.timestamp_ns < left.timestamp_ns for left, right in zip(records, records[1:])):
        raise ReplayError("record timestamps are not monotonic")
    return ReplaySession(path, manifest, records, total_records, skipped)


def _metadata_for_list(session_path: Path) -> tuple[int, int | None, int | None, list[tuple[int, str, int]]]:
    manifest = _load_manifest(session_path)
    _, receipts = _phase_receipts(manifest)
    count = 0
    timestamps: list[int] = []
    for raw_path in _raw_paths(session_path, manifest):
        try:
            with raw_path.open("r", encoding="utf-8") as stream:
                for line in stream:
                    if not line.strip():
                        continue
                    count += 1
                    try:
                        raw = json.loads(line)
                        timestamp_ns = int(raw["body_complete_monotonic_ns"])
                    except (json.JSONDecodeError, TypeError, ValueError, KeyError):
                        continue
                    timestamps.append(timestamp_ns)
        except (OSError, UnicodeError):
            raise ReplayError("session records could not be read") from None
    return count, (min(timestamps) if timestamps else None), (max(timestamps) if timestamps else None), receipts


def list_sessions(root: str | Path, output: TextIO) -> int:
    root_path = Path(root)
    try:
        sessions = sorted(path for path in root_path.iterdir() if path.is_dir() and (path / "manifest.json").is_file())
    except OSError:
        output.write("gsi_replay: session root could not be read\n")
        return 2
    for session_path in sessions:
        try:
            count, first_ns, last_ns, receipts = _metadata_for_list(session_path)
        except ReplayError:
            output.write(f"session: {session_path.name} records=unknown duration=unknown\n")
            continue
        duration = (last_ns - first_ns) / 1_000_000_000 if first_ns is not None and last_ns is not None else 0.0
        output.write(f"session: {session_path.name} records={count} duration={duration:.3f}s\n")
        base_ns = first_ns if first_ns is not None else 0
        for timestamp_ns, state, sequence in receipts:
            offset = (timestamp_ns - base_ns) / 1_000_000_000
            output.write(f"  phase: {state} offset={offset:.3f}s sequence={sequence}\n")
    return 0


def parse_point(value: str | None) -> ReplayPoint | None:
    if value is None:
        return None
    if value.startswith("phase:"):
        phase = value[len("phase:"):]
        normalized_phase = _normalize_phase(phase)
        if normalized_phase is None:
            raise ValueError("invalid phase point")
        return ReplayPoint("phase", normalized_phase)
    if value.startswith("seq:"):
        try:
            sequence = int(value[len("seq:"):])
        except ValueError:
            raise ValueError("invalid sequence point") from None
        if sequence < 0:
            raise ValueError("invalid sequence point")
        return ReplayPoint("seq", sequence)
    if value.startswith("clock:"):
        try:
            clock_value = float(value[len("clock:"):])
        except ValueError:
            raise ValueError("invalid clock point") from None
        if not math.isfinite(clock_value):
            raise ValueError("invalid clock point")
        return ReplayPoint("clock", clock_value)
    raise ValueError("invalid replay point")


def _point_matches(record: ReplayRecord, point: ReplayPoint) -> bool:
    if point.kind == "phase":
        return record.phase == point.value
    if point.kind == "seq":
        return record.sequence >= point.value
    if point.kind == "clock":
        return record.clock_time is not None and record.clock_time >= point.value
    return False


def select_records(session: ReplaySession, start_at: str | None, stop_at: str | None,
                   max_records: int | None = None) -> list[ReplayRecord]:
    start_point = parse_point(start_at)
    stop_point = parse_point(stop_at)
    records = session.records
    start_index = 0
    if start_point is not None:
        start_index = next((index for index, record in enumerate(records) if _point_matches(record, start_point)), -1)
        if start_index < 0:
            raise ValueError("start point is not present in this session")
    end_index = len(records) - 1
    if stop_point is not None:
        end_index = next((index for index in range(start_index, len(records)) if _point_matches(records[index], stop_point)), -1)
        if end_index < 0:
            raise ValueError("stop point is not present after the selected start")
    if end_index < start_index:
        raise ValueError("stop point is before the selected start")
    selected = records[start_index:end_index + 1]
    if max_records is not None:
        selected = selected[:max_records]
    if not selected:
        raise ValueError("selection has no records")
    return selected


def is_local_host(host: str) -> bool:
    return host.lower() in {"127.0.0.1", "localhost", "::1"}


def _speed_argument(value: str) -> float:
    try:
        speed = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError("speed must be between 1 and 60") from None
    if not math.isfinite(speed) or speed < 1 or speed > 60:
        raise argparse.ArgumentTypeError("speed must be between 1 and 60")
    return speed


def _positive_int_argument(value: str) -> int:
    try:
        number = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("value must be a positive integer") from None
    if number < 1:
        raise argparse.ArgumentTypeError("value must be a positive integer")
    return number


class _Parser(argparse.ArgumentParser):
    def error(self, _message: str) -> None:
        raise ValueError("invalid command line")


def _build_parser() -> argparse.ArgumentParser:
    parser = _Parser(description="Replay a recorded GSI session into a local Kohbor demo.")
    parser.add_argument("--list", action="store_true", help="list session names, counts, durations and phase offsets")
    parser.add_argument("--root", default=DEFAULT_ROOT, help="session root used with --list")
    parser.add_argument("command", nargs="?", choices=("replay",))
    parser.add_argument("session_folder", nargs="?")
    parser.add_argument("--port", type=int)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--speed", type=_speed_argument, default=1.0)
    parser.add_argument("--start-at")
    parser.add_argument("--stop-at")
    parser.add_argument("--loop", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--max-records", type=_positive_int_argument)
    parser.add_argument("--demo", action="store_true", help="acknowledge a demo instance with temporary data")
    parser.add_argument("--i-know-this-is-not-the-real-app", action="store_true",
                        help="unsafe override for a nonstandard local target or auth response")
    return parser


def _replace_auth_token(body: dict[str, Any], token: str) -> bytes:
    auth = body.get("auth")
    if not isinstance(auth, dict):
        auth = {}
        body["auth"] = auth
    auth["token"] = token
    return json.dumps(body, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


class _HttpPoster:
    def __init__(self, host: str, port: int, connection_factory: Callable[..., Any] | None = None) -> None:
        self.host = host
        self.port = port
        self.connection_factory = connection_factory or http.client.HTTPConnection
        self.connection: Any | None = None

    def _close_connection(self) -> None:
        if self.connection is not None:
            try:
                self.connection.close()
            except Exception:
                pass
            self.connection = None

    def post(self, body: bytes) -> int:
        retries = 0
        while True:
            try:
                if self.connection is None:
                    self.connection = self.connection_factory(self.host, self.port, timeout=REQUEST_TIMEOUT_SECONDS)
                self.connection.request("POST", "/", body=body, headers={"Content-Type": "application/json"})
                response = self.connection.getresponse()
                status = int(response.status)
                response.read()
                return status
            except (OSError, http.client.HTTPException, socket.error, TimeoutError):
                self._close_connection()
                if retries >= MAX_RETRIES:
                    raise ReplayError("local HTTP delivery failed after retries") from None
                retries += 1

    def close(self) -> None:
        self._close_connection()


def _percentile_95(values: list[float]) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = max(0, math.ceil(0.95 * len(ordered)) - 1)
    return ordered[index]


def _format_statuses(statuses: collections.Counter[int]) -> str:
    return "{" + ",".join(f"{status}:{statuses[status]}" for status in sorted(statuses)) + "}"


def _format_summary(result: ReplayResult) -> str:
    now = result.finished_at
    elapsed = max(0.0, now - result.started_at)
    rate = result.sent / elapsed if elapsed > 0 else 0.0
    late = result.lateness_ms or []
    mean_late = sum(late) / len(late) if late else 0.0
    p95_late = _percentile_95(late)
    max_late = max(late, default=0.0)
    return (f"summary: records_sent={result.sent} skipped={result.skipped} effective_rate={rate:.2f}/s "
            f"late_ms_mean={mean_late:.1f} late_ms_p95={p95_late:.1f} late_ms_max={max_late:.1f} "
            f"http_statuses={_format_statuses(result.statuses or collections.Counter())} "
            f"stop_reason={result.stop_reason}")


def _edge_phase(records: list[ReplayRecord], reverse: bool = False) -> str:
    candidates = reversed(records) if reverse else iter(records)
    return next((record.phase for record in candidates if record.phase is not None), "unknown")


def _dry_run_plan(records: list[ReplayRecord], speed: float, loop: bool,
                  max_records: int | None) -> tuple[str, str, str, str]:
    first_ns = records[0].timestamp_ns
    span = (records[-1].timestamp_ns - first_ns) / 1_000_000_000 / speed
    if not loop:
        return str(len(records)), f"{span:.3f}s", _edge_phase(records), _edge_phase(records, reverse=True)
    if max_records is None:
        return "unbounded", "unbounded", _edge_phase(records), _edge_phase(records, reverse=True)
    first_gap = ((records[1].timestamp_ns - first_ns) / 1_000_000_000 / speed) if len(records) > 1 else 0.0
    cycle_duration = span + first_gap
    final_number = max_records - 1
    final_cycle, final_index = divmod(final_number, len(records))
    final_record_offset = (records[final_index].timestamp_ns - first_ns) / 1_000_000_000 / speed
    duration = final_cycle * cycle_duration + final_record_offset
    last_phase = _edge_phase(records[:final_index + 1], reverse=True)
    if last_phase == "unknown":
        last_phase = _edge_phase(records, reverse=True)
    return str(max_records), f"{duration:.3f}s", _edge_phase(records), last_phase


def _write_progress(output: TextIO, sent: int, skipped: int, start: float, now: float) -> None:
    output.write(f"progress: records_sent={sent} skipped={skipped} elapsed={max(0.0, now - start):.1f}s\n")


def _run_records(records: list[ReplayRecord], skipped: int, *, host: str, port: int, speed: float,
                 loop: bool, max_records: int | None, unsafe_override: bool, token: str,
                 stop_at_used: bool,
                 output: TextIO, clock: Callable[[], float], sleep: Callable[[float], None],
                 connection_factory: Callable[..., Any] | None) -> ReplayResult:
    result = ReplayResult(skipped=skipped)
    poster = _HttpPoster(host, port, connection_factory)
    result.started_at = clock()
    anchor = result.started_at
    last_progress = anchor
    first_ns = records[0].timestamp_ns
    span_seconds = (records[-1].timestamp_ns - first_ns) / 1_000_000_000 / speed
    first_gap = ((records[1].timestamp_ns - first_ns) / 1_000_000_000 / speed) if len(records) > 1 else 0.0
    cycle_seconds = span_seconds + first_gap
    cycle = 0
    reason = "finished"
    error_code = 0
    try:
        while True:
            for record in records:
                if max_records is not None and result.sent >= max_records:
                    reason = "finished"
                    break
                target = anchor + cycle * cycle_seconds + ((record.timestamp_ns - first_ns) / 1_000_000_000 / speed)
                while True:
                    now = clock()
                    if now >= target:
                        break
                    until_target = target - now
                    until_progress = last_progress + 10.0 - now
                    delay = min(until_target, until_progress) if until_progress > 0 else until_target
                    sleep(delay)
                    now = clock()
                    if now - last_progress >= 10.0:
                        _write_progress(output, result.sent, skipped, anchor, now)
                        last_progress = now
                now = clock()
                result.lateness_ms.append(max(0.0, now - target) * 1000.0)  # type: ignore[union-attr]
                try:
                    body = _replace_auth_token(record.body, token)
                    status = poster.post(body)
                except ReplayError:
                    reason = "error"
                    error_code = 1
                    break
                result.sent += 1
                result.statuses[status] += 1  # type: ignore[index]
                now = clock()
                if now - last_progress >= 10.0:
                    _write_progress(output, result.sent, skipped, anchor, now)
                    last_progress = now
                if status in (401, 403) and not unsafe_override:
                    reason = "refused: target did not accept the test token - this is not a demo instance started with KOHBOR_TEST_TOKEN"
                    error_code = 3
                    break
            else:
                if loop:
                    cycle += 1
                    continue
                if result.sent < len(records) and max_records is not None and result.sent >= max_records:
                    reason = "finished"
                else:
                    reason = "--stop-at" if stop_at_used else "finished"
                break
            break
    except KeyboardInterrupt:
        reason = "interrupt"
        error_code = 130
    finally:
        poster.close()
        result.finished_at = clock()
    result.stop_reason = reason
    result.error_code = error_code
    return result


def main(argv: list[str] | None = None, *, environ: dict[str, str] | None = None,
         stdout: TextIO | None = None, stderr: TextIO | None = None,
         clock: Callable[[], float] | None = None, sleep: Callable[[float], None] | None = None,
         connection_factory: Callable[..., Any] | None = None) -> int:
    output = stdout or sys.stdout
    monotonic = clock or time.monotonic
    sleeper = sleep or time.sleep
    env = os.environ if environ is None else environ
    parser = _build_parser()
    try:
        args = parser.parse_args(argv)
    except ValueError:
        output.write("gsi_replay: refused: invalid command line\n")
        return 2
    except SystemExit as exit_signal:
        return int(exit_signal.code or 0)

    if args.list:
        if args.command is not None or args.session_folder is not None:
            output.write("gsi_replay: refused: --list cannot be combined with replay\n")
            return 2
        return list_sessions(args.root, output)
    if args.command != "replay" or not args.session_folder or args.port is None:
        output.write("gsi_replay: refused: replay requires a session folder and --port\n")
        return 2
    if args.port < 1 or args.port > 65535:
        output.write("gsi_replay: refused: port is outside the valid range\n")
        return 2
    if not is_local_host(args.host):
        output.write("gsi_replay: refused: target host is not loopback\n")
        return 2
    unsafe_override = bool(args.i_know_this_is_not_the_real_app)
    if not args.demo:
        output.write("gsi_replay: refused: pass --demo to acknowledge a temporary-data DEMO instance\n")
        return 2
    token = env.get("KOHBOR_TEST_TOKEN", "")
    if len(token) < TOKEN_MIN_LENGTH:
        output.write("gsi_replay: refused: KOHBOR_TEST_TOKEN is missing or too short\n")
        return 2
    try:
        session = load_session(args.session_folder)
        complete_selection = select_records(session, args.start_at, args.stop_at)
    except (ReplayError, ValueError):
        output.write("gsi_replay: refused: session could not be decoded or selected\n")
        return 2
    stop_at_reached = bool(args.stop_at) and (args.max_records is None or args.max_records >= len(complete_selection))
    records = complete_selection if args.loop or args.max_records is None else complete_selection[:args.max_records]
    for record in records:
        _replace_auth_token(record.body, token)
    if args.dry_run:
        planned_count, duration, first_phase, last_phase = _dry_run_plan(records, args.speed, args.loop, args.max_records)
        loop_info = ""
        if args.loop and args.max_records is None:
            one_loop_duration = (records[-1].timestamp_ns - records[0].timestamp_ns) / 1_000_000_000 / args.speed
            loop_info = f" per_loop={len(records)} one_loop_duration={one_loop_duration:.3f}s"
        output.write(f"plan: records={planned_count}{loop_info} skipped={session.skipped} duration={duration} speed={args.speed:g}x "
                     f"first_phase={first_phase} last_phase={last_phase}\n")
        return 0

    result = _run_records(records, session.skipped, host=args.host, port=args.port, speed=args.speed,
                          loop=args.loop, max_records=args.max_records, unsafe_override=unsafe_override,
                          token=token, stop_at_used=stop_at_reached,
                          output=output, clock=monotonic, sleep=sleeper,
                          connection_factory=connection_factory)
    output.write(_format_summary(result) + "\n")
    return result.error_code


if __name__ == "__main__":
    raise SystemExit(main())
