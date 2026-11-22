"""Trim a neutral model into a small test fixture.

Keeps only the position samples near the owner's deaths (and a short window
around 10:00), all hero deaths, wards, smokes, visibility changes and the
10-minute values. The real match ID is replaced, because a public match ID
can be looked up to reveal the players. Standard library only.

  python tools/replay_export/fixture.py neutral.json out.json --match-id 1000000001
"""

from __future__ import annotations

import argparse
import json
import struct
import zlib
from pathlib import Path


def trim(model: dict, match_id: str, before: int = 12, after: int = 1) -> dict:
    me = next(h for h in model["heroes"] if h["key"] == "self")
    keep: set[int] = set(range(598, 603))
    for d in me["deaths"]:
        keep.update(range(int(d["t"]) - before, int(d["t"]) + after + 1))
    out = json.loads(json.dumps(model))
    out["match"] = match_id
    out["provenance"] = {**out.get("provenance", {}), "fixture": "trimmed; real match id replaced"}
    for h in out["heroes"]:
        h["points"] = [p for p in h["points"] if p[0] in keep] or h["points"][:1]
    return out


def blank_png(width: int = 256, height: int = 256) -> bytes:
    """A plain dark PNG, used as a stand-in map in tests (no game art)."""
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    row = bytes([0]) + bytes((24, 33, 35)) * width
    signature = bytes([0x89]) + b"PNG" + bytes([0x0D, 0x0A, 0x1A, 0x0A])
    return (signature + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(row * height, 9)) + chunk(b"IEND", b""))


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("neutral", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--match-id", required=True)
    a = ap.parse_args()
    data = trim(json.loads(a.neutral.read_text(encoding="utf-8")), a.match_id)
    a.out.write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    print(f"wrote {a.out} ({a.out.stat().st_size} bytes)")
