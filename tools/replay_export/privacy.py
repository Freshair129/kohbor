"""Privacy scan for exporter output (DEC-COACH-REVIEW point 1).

A report or neutral model must never contain player names, Steam IDs,
account IDs or chat. The extractor builds output from an allow-list; this
scan is the second, independent check, and the exporter refuses to write a
file that fails it.

Standard library only.
"""

from __future__ import annotations

import re

FORBIDDEN_KEYS = {"player_name", "playername", "persona", "personaname", "steam_id", "steamid",
                  "account_id", "accountid", "chat", "name"}
STEAM_ID64 = re.compile(r"7656119\d{10}")


def _walk(value, path="$"):
    if isinstance(value, dict):
        for key, item in value.items():
            yield path, "key", key
            yield from _walk(item, f"{path}.{key}")
    elif isinstance(value, list):
        for i, item in enumerate(value):
            yield from _walk(item, f"{path}[{i}]")
    else:
        yield path, "value", value


def scan(document, forbidden_strings=(), forbidden_numbers=(), allowed_strings=()) -> list[str]:
    """Return findings (paths only, never the matched personal value)."""
    allowed = {s.casefold() for s in allowed_strings}
    strings = [s for s in {str(s) for s in forbidden_strings if s}
               if len(s.strip()) >= 3 and s.casefold() not in allowed]
    numbers = {int(n) for n in forbidden_numbers if n not in (None, 0)}
    findings = []
    for path, kind, item in _walk(document):
        if kind == "key":
            if str(item).casefold() in FORBIDDEN_KEYS:
                findings.append(f"{path}: forbidden key")
            continue
        if isinstance(item, bool):
            continue
        if isinstance(item, (int, float)):
            if isinstance(item, int) and item in numbers:
                findings.append(f"{path}: player identifier number")
            if STEAM_ID64.fullmatch(str(item)):
                findings.append(f"{path}: SteamID64-shaped number")
            continue
        if isinstance(item, str):
            if STEAM_ID64.search(item):
                findings.append(f"{path}: SteamID64-shaped text")
            text = item.casefold()
            if any(s.casefold() in text for s in strings):
                findings.append(f"{path}: contains a player name or chat text")
            if any(str(n) in item for n in numbers if n >= 100000):
                findings.append(f"{path}: contains a player identifier")
    return findings


def personal_values(parsed) -> tuple[list[str], list[int]]:
    """Collect the personal values of a parsed DEM, for scanning only (never written)."""
    strings, numbers = [], []
    for p in parsed.players:
        strings.append(getattr(p, "player_name", None))
        numbers += [getattr(p, "steam_id", None), getattr(p, "account_id", None)]
    for entry in getattr(parsed, "chat", None) or []:
        for attr in ("text", "message", "msg"):
            value = getattr(entry, attr, None)
            # Short chat lines ("gg", "mid") collide with report vocabulary and
            # cannot be told apart from template text; longer lines are scanned.
            if isinstance(value, str) and len(value.strip()) >= 12:
                strings.append(value)
    clean_numbers = []
    for n in numbers:
        try:
            clean_numbers.append(int(n))
        except (TypeError, ValueError):
            pass
    return [s for s in strings if isinstance(s, str)], clean_numbers
