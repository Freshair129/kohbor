"""Kohbor Lab DEM exporter: local .dem files -> kohbor.post-match-replay.v3 JSON.

Lab/dev tool only (DEC-COACH-REVIEW, DEC-LAB-GATE). It reads replay files the
user picked, runs the pinned gem-dota parser that is already installed on the
machine, and writes one local JSON report for the Lab Replay tab. It never
downloads replays, never contacts a network service and never writes player
names, Steam IDs, account IDs or chat.

Usage (run with the interpreter that holds the pinned gem-dota):

  python tools/replay_export/export.py --hero earthshaker --patch 7.41f \
      --layout layout-7.41f.json --dem C:/path/match.dem [--dem ...] --out report.json

  layout JSON: {"patch": "7.41f", "width": 1000, "height": 959,
                "calibration": {"sx": .., "sy": .., "ox": .., "oy": ..},
                "mapFile": "map-7.41f.png"}   # map image supplied by the user, kept local

Offline re-analysis without the parser: --neutral-in model.json (repeatable).
"""

from __future__ import annotations

import argparse
import base64
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import analyze  # noqa: E402
import privacy  # noqa: E402

MAX_REPORT_BYTES = 16 * 1024 * 1024
MIME = {".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".webp": "image/webp"}


def load_layout(path: Path) -> dict:
    spec = json.loads(path.read_text(encoding="utf-8"))
    image = (path.parent / spec["mapFile"]).resolve()
    mime = MIME.get(image.suffix.lower())
    if mime is None:
        raise ValueError(f"unsupported map image type: {image.suffix}")
    data = f"data:{mime};base64," + base64.b64encode(image.read_bytes()).decode("ascii")
    if len(data) > 1_500_000:
        raise ValueError("map image is larger than the viewer accepts")
    return {"patch": spec["patch"], "width": int(spec["width"]), "height": int(spec["height"]),
            "calibration": spec["calibration"], "map": data}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dem", action="append", default=[], type=Path, help="a .dem file the user picked")
    ap.add_argument("--neutral-in", action="append", default=[], type=Path)
    ap.add_argument("--neutral-out", type=Path, help="folder for neutral models (local only)")
    ap.add_argument("--hero", help="the owner's hero, e.g. earthshaker (required with --dem)")
    ap.add_argument("--patch", help="gameplay patch of the --dem files, e.g. 7.41f (required with --dem)")
    ap.add_argument("--layout", action="append", default=[], type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    if not args.dem and not args.neutral_in:
        ap.error("give --dem or --neutral-in")

    models, personal_strings, personal_numbers = [], [], []
    if args.dem:
        if not args.hero or not args.patch:
            ap.error("--hero and --patch are required with --dem")
        import pin
        problems = pin.verify_installed()
        if problems:
            print("refusing: the installed gem-dota does not match PIN.json:", *problems, sep="\n  ", file=sys.stderr)
            return 2
        import extract
        for dem in args.dem:
            print(f"parsing {dem.name} …", flush=True)
            parsed = extract.parse_dem(dem)
            strings, numbers = privacy.personal_values(parsed)
            personal_strings += strings
            personal_numbers += numbers
            models.append(extract.neutral_model(parsed, args.hero, args.patch, extract.sha256_file(dem)))
    for path in args.neutral_in:
        models.append(json.loads(path.read_text(encoding="utf-8")))

    maps = [load_layout(p) for p in args.layout]
    report = analyze.build_report(models, maps)
    heroes = {h["hero"] for m in models for h in m["heroes"]}
    findings = privacy.scan(report, personal_strings, personal_numbers, allowed_strings=heroes)
    for m in models:
        findings += privacy.scan(m, personal_strings, personal_numbers, allowed_strings=heroes)
    if findings:
        print("refusing: privacy scan failed:", *findings[:20], sep="\n  ", file=sys.stderr)
        return 3
    data = json.dumps(report, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(data) > MAX_REPORT_BYTES:
        print("refusing: report exceeds 16 MiB; export fewer matches", file=sys.stderr)
        return 4
    if args.neutral_out:
        args.neutral_out.mkdir(parents=True, exist_ok=True)
        for m in models:
            (args.neutral_out / f"neutral-{m['match']}.json").write_text(
                json.dumps(m, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    args.out.write_bytes(data)
    deaths = sum(len(m["coach"]["deaths"]) for m in report["matches"])
    print(f"wrote {args.out} ({len(data)} bytes): {len(report['matches'])} matches, {deaths} own deaths; privacy scan clean")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
