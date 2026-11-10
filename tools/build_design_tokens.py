#!/usr/bin/env python3
"""Build Kohbor's bounded DTCG 2025.10 token profile with Python stdlib only.

This project importer intentionally does not implement all DTCG constructs.
See design/tokens/README.md for supported value types and import limitations.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
TOKEN_DIR = ROOT / "design" / "tokens"
SOURCE_DIR = TOKEN_DIR / "source"
TOKEN_SOURCES = ("primitives.tokens.json", "semantic.dark.tokens.json", "components.tokens.json")
ADAPTER_SOURCE = "css-adapters.json"
SUPPORTED_TYPES = {"color", "dimension", "fontFamily", "fontWeight", "duration", "cubicBezier", "number", "shadow", "typography"}
TOKEN_PROPERTIES = {"$value", "$type", "$description", "$extensions", "$deprecated"}
GROUP_PROPERTIES = {"$type", "$description", "$extensions", "$deprecated"}
NAME = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*\Z")
REFERENCE = re.compile(r"\{([a-z0-9.-]+)\}\Z")
HEX = re.compile(r"#[0-9a-fA-F]{6}\Z")
FONT_WEIGHT_VALUES = {"thin": 100, "hairline": 100, "extra-light": 200, "ultra-light": 200, "light": 300, "normal": 400, "regular": 400, "book": 400, "medium": 500, "semi-bold": 600, "demi-bold": 600, "bold": 700, "extra-bold": 800, "ultra-bold": 800, "black": 900, "heavy": 900, "extra-black": 950, "ultra-black": 950}
FONT_WEIGHTS = set(FONT_WEIGHT_VALUES)
GENERIC_FONTS = {"serif", "sans-serif", "monospace", "cursive", "fantasy", "system-ui", "ui-serif", "ui-sans-serif", "ui-monospace", "ui-rounded", "emoji", "math", "fangsong"}
TYPOGRAPHY_FIELDS = {"fontFamily": "fontFamily", "fontSize": "dimension", "fontWeight": "fontWeight", "letterSpacing": "dimension", "lineHeight": "number"}
SHADOW_FIELDS = {"color": "color", "offsetX": "dimension", "offsetY": "dimension", "blur": "dimension", "spread": "dimension"}
SYSTEM_COLORS = {"Canvas", "CanvasText", "LinkText", "ActiveText", "ButtonFace", "ButtonText", "ButtonBorder", "GrayText", "Highlight", "HighlightText"}


class TokenError(ValueError):
    """A source violates this project's declared token profile."""


def unique_object(pairs: list[tuple[str, Any]]) -> dict:
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise TokenError(f"Duplicate JSON key: {key}")
        obj[key] = value
    return obj


def load_json(path: Path) -> dict:
    def reject_constant(value: str) -> None:
        raise TokenError(f"Non-finite JSON number: {value}")
    try:
        data = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object, parse_constant=reject_constant)
    except (OSError, json.JSONDecodeError) as exc:
        raise TokenError(f"{path}: {exc}") from exc
    if not isinstance(data, dict):
        raise TokenError(f"{path}: root must be an object")
    return data


def json_bytes(value: Any) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def source_manifest(source_dir: Path = SOURCE_DIR) -> dict[str, str]:
    return {f"design/tokens/source/{name}": sha256((source_dir / name).read_bytes()) for name in (*TOKEN_SOURCES, ADAPTER_SOURCE)}


def manifest_hash(manifest: dict[str, str]) -> str:
    return sha256(json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode("utf-8"))


def merge_sources(source_dir: Path = SOURCE_DIR) -> dict:
    merged: dict[str, Any] = {"$description": "Kohbor design tokens v1.0.0 — authored proposal under DEC-VISUALS; DTCG 2025.10 value forms."}
    for filename in TOKEN_SOURCES:
        document = load_json(source_dir / filename)
        for key, value in document.items():
            if key == "$description":
                continue
            if key in merged:
                raise TokenError(f"Duplicate source namespace: {key}")
            merged[key] = value
    merged["$extensions"] = {"org.kohbor.design": {"designVersion": "1.0.0", "formatVersion": "2025.10", "sourceSha256": manifest_hash(source_manifest(source_dir)), "status": "design-proposal", "evidenceClass": "static", "namespaceScope": "Design metadata only; not an application/package/trust identifier."}}
    return merged


def reference(value: Any) -> str | None:
    if isinstance(value, str):
        match = REFERENCE.fullmatch(value)
        if match:
            return match.group(1)
        if "{" in value or "}" in value:
            raise TokenError(f"Malformed or embedded reference: {value!r}")
    return None


def number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise TokenError(message)


class TokenSet:
    def __init__(self, document: dict):
        self.document = document
        self.tokens: dict[str, dict] = {}
        self.types: dict[str, str | None] = {}
        self.resolved: dict[str, Any] = {}
        self.references: dict[str, list[str]] = {}
        self._walk(document)
        require(bool(self.tokens), "Token document is empty")
        for path in self.tokens:
            self.resolve(path)
        for path, node in self.tokens.items():
            dtype = self.type_of(path)
            self._check_reference_types(node["$value"], dtype, path)
            self.validate_value(self.resolved[path], dtype, path)
            self.references[path] = sorted(self._references(node["$value"]))
        names = [css_name(path) for path in self.tokens]
        require(len(names) == len(set(names)), "CSS name collision after flattening paths")

    def _walk(self, node: dict, path: str = "", inherited_type: str | None = None) -> None:
        require(isinstance(node, dict), f"{path or '<root>'}: group/token must be an object")
        is_token = "$value" in node
        allowed = TOKEN_PROPERTIES if is_token else GROUP_PROPERTIES
        for key in node:
            if key.startswith("$"):
                require(key in allowed, f"{path or '<root>'}: unsupported property {key}; see importer limitations")
            elif is_token:
                raise TokenError(f"{path}: token cannot also contain child {key}")
            else:
                require(bool(NAME.fullmatch(key)), f"{path}: invalid project token/group name {key!r}")
        if "$description" in node:
            require(isinstance(node["$description"], str), f"{path}: description must be a string")
        if "$extensions" in node:
            require(isinstance(node["$extensions"], dict), f"{path}: extensions must be an object")
        if "$deprecated" in node:
            require(isinstance(node["$deprecated"], (bool, str)), f"{path}: invalid deprecated value")
        dtype = node.get("$type", inherited_type)
        require(dtype is None or (isinstance(dtype, str) and dtype in SUPPORTED_TYPES), f"{path}: unsupported token type {dtype!r}")
        if is_token:
            require(bool(path), "A root token needs a name")
            self.tokens[path] = node
            self.types[path] = dtype
        else:
            for key, child in node.items():
                if not key.startswith("$"):
                    self._walk(child, f"{path}.{key}" if path else key, dtype)

    def type_of(self, path: str, visiting: tuple[str, ...] = ()) -> str:
        require(path in self.tokens, f"Unresolved token reference: {path}")
        require(path not in visiting, f"Cyclic type reference: {' -> '.join((*visiting, path))}")
        dtype = self.types[path]
        if dtype is None:
            target = reference(self.tokens[path]["$value"])
            require(target is not None, f"{path}: explicit, inherited or referenced type is required")
            dtype = self.type_of(target, (*visiting, path))
        return dtype

    def resolve(self, path: str, visiting: tuple[str, ...] = ()) -> Any:
        require(path in self.tokens, f"Unresolved token reference: {path}")
        require(path not in visiting, f"Cyclic token reference: {' -> '.join((*visiting, path))}")
        if path not in self.resolved:
            self.resolved[path] = self.resolve_value(self.tokens[path]["$value"], (*visiting, path))
        return copy.deepcopy(self.resolved[path])

    def resolve_value(self, value: Any, visiting: tuple[str, ...] = ()) -> Any:
        target = reference(value)
        if target is not None:
            return self.resolve(target, visiting)
        if isinstance(value, dict):
            require("$ref" not in value, "JSON Pointer $ref is outside this project's importer profile")
            return {key: self.resolve_value(child, visiting) for key, child in value.items()}
        if isinstance(value, list):
            return [self.resolve_value(child, visiting) for child in value]
        return value

    def _references(self, value: Any) -> set[str]:
        target = reference(value)
        if target is not None:
            return {target}
        if isinstance(value, dict):
            return set().union(*(self._references(v) for v in value.values())) if value else set()
        if isinstance(value, list):
            return set().union(*(self._references(v) for v in value)) if value else set()
        return set()

    def _check_reference_types(self, value: Any, dtype: str, path: str) -> None:
        target = reference(value)
        if target is not None:
            actual = self.type_of(target)
            require(actual == dtype, f"{path}: expected {dtype} alias, got {actual} from {target}")
            return
        fields = TYPOGRAPHY_FIELDS if dtype == "typography" else SHADOW_FIELDS if dtype == "shadow" else None
        if fields and isinstance(value, dict):
            for field, expected in fields.items():
                if field in value:
                    self._check_reference_types(value[field], expected, f"{path}.{field}")
        elif isinstance(value, (dict, list)):
            require(not self._references(value), f"{path}: property-level aliases are supported only in typography/shadow members")

    @staticmethod
    def validate_value(value: Any, dtype: str, path: str) -> None:
        def check(condition: bool, message: str) -> None:
            require(condition, f"{path} ({dtype}): {message}")
        if dtype == "color":
            check(isinstance(value, dict), "expected color object")
            check(set(value) <= {"colorSpace", "components", "alpha", "hex"} and {"colorSpace", "components"} <= set(value), "invalid color fields")
            check(value["colorSpace"] == "srgb", "only numeric sRGB colors are supported")
            components = value["components"]
            check(isinstance(components, list) and len(components) == 3 and all(number(v) and 0 <= v <= 1 for v in components), "components must be three numbers in [0, 1]")
            check(number(value.get("alpha", 1)) and 0 <= value.get("alpha", 1) <= 1, "alpha must be in [0, 1]")
            if "hex" in value:
                check(isinstance(value["hex"], str) and bool(HEX.fullmatch(value["hex"])), "fallback must be six-digit hex")
                check(all(abs(c - int(value["hex"][i:i + 2], 16) / 255) < 1e-8 for c, i in zip(components, (1, 3, 5))), "hex fallback must agree with sRGB components")
        elif dtype in {"dimension", "duration"}:
            check(isinstance(value, dict) and set(value) == {"value", "unit"}, "expected value/unit object")
            check(number(value["value"]), "value must be a finite number")
            check(value["unit"] in ({"px", "rem"} if dtype == "dimension" else {"ms", "s"}), "unsupported unit")
            if dtype == "duration":
                check(value["value"] >= 0, "CSS duration cannot be negative")
        elif dtype == "fontFamily":
            values = value if isinstance(value, list) else [value]
            check(bool(values) and all(isinstance(v, str) and bool(v.strip()) for v in values), "expected a name or nonempty list of names")
        elif dtype == "fontWeight":
            check((number(value) and 1 <= value <= 1000) or (isinstance(value, str) and value in FONT_WEIGHTS), "invalid OpenType weight or DTCG weight alias")
        elif dtype == "number":
            check(number(value), "expected finite JSON number")
        elif dtype == "cubicBezier":
            check(isinstance(value, list) and len(value) == 4 and all(number(v) for v in value), "expected four finite numbers")
            check(0 <= value[0] <= 1 and 0 <= value[2] <= 1, "x coordinates must be in [0, 1]")
        elif dtype in {"typography", "shadow"}:
            fields = TYPOGRAPHY_FIELDS if dtype == "typography" else SHADOW_FIELDS
            check(isinstance(value, dict) and set(value) == set(fields), "composite requires exactly " + ", ".join(fields))
            for field, subtype in fields.items():
                TokenSet.validate_value(value[field], subtype, f"{path}.{field}")
            if dtype == "shadow":
                check(value["blur"]["value"] >= 0, "shadow blur cannot be negative")
            else:
                check(value["fontSize"]["value"] > 0 and value["lineHeight"] > 0, "font size and line height must be positive")
        else:
            raise TokenError(f"{path}: unsupported type {dtype}")


def css_name(path: str) -> str:
    parts = path.split(".")
    if parts[0] == "semantic":
        parts = parts[1:]
    return "--kb-" + "-".join(parts)


def css_number(value: int | float) -> str:
    return str(int(value)) if float(value).is_integer() else format(value, ".15g")


def css_value(value: Any, dtype: str) -> str:
    target = reference(value)
    if target is not None:
        return f"var({css_name(target)})"
    if dtype == "color":
        rgb = [c * 255 for c in value["components"]]
        alpha = value.get("alpha", 1)
        if alpha == 1 and all(abs(c - round(c)) < 1e-7 for c in rgb):
            return "#" + "".join(f"{round(c):02X}" for c in rgb)
        return f"rgb({' '.join(css_number(c) for c in rgb)} / {css_number(alpha)})"
    if dtype in {"dimension", "duration"}:
        return f"{css_number(value['value'])}{value['unit']}"
    if dtype == "fontWeight":
        return css_number(FONT_WEIGHT_VALUES[value] if isinstance(value, str) else value)
    if dtype == "number":
        return css_number(value)
    if dtype == "fontFamily":
        names = value if isinstance(value, list) else [value]
        return ", ".join(name if name in GENERIC_FONTS else json.dumps(name, ensure_ascii=False) for name in names)
    if dtype == "cubicBezier":
        return "cubic-bezier(" + ", ".join(css_number(v) for v in value) + ")"
    if dtype == "shadow":
        return " ".join(css_value(value[field], SHADOW_FIELDS[field]) for field in ("offsetX", "offsetY", "blur", "spread", "color"))
    if dtype == "typography":
        part = {key: css_value(value[key], sub) for key, sub in TYPOGRAPHY_FIELDS.items()}
        return f"{part['fontWeight']} {part['fontSize']}/{part['lineHeight']} {part['fontFamily']}"
    raise TokenError(f"No CSS serializer for {dtype}")


def validate_adapters(adapters: dict, token_set: TokenSet) -> None:
    require(set(adapters) == {"description", "highContrast", "forcedColors"}, "Invalid CSS adapter fields")
    for mode in ("highContrast", "forcedColors"):
        require(isinstance(adapters[mode], dict), f"{mode}: expected override object")
        for path, value in adapters[mode].items():
            require(path in token_set.tokens, f"{mode}: unknown target {path}")
            if mode == "highContrast":
                target = reference(value)
                require(target is not None and target in token_set.tokens, f"{mode}: override requires a known token alias")
                require(token_set.type_of(path) == token_set.type_of(target), f"{mode}: override type mismatch for {path}")
            else:
                require((token_set.type_of(path) == "color" and value in SYSTEM_COLORS) or (token_set.type_of(path) == "shadow" and value == "none"), f"{mode}: unsupported system override for {path}")


def render_css(token_set: TokenSet, adapters: dict, source_hash: str) -> bytes:
    lines = ["/* GENERATED by python tools/build_design_tokens.py; do not edit.", f" * Source SHA-256: {source_hash}", " * Proposed authored tokens. This file does not certify native UI behavior.", " * Set accessibility attributes on <html> so aliases resolve at the same scope.", " */", ":root {", "  color-scheme: dark;"]
    for layer in ("primitive", "semantic", "component"):
        lines.append(f"\n  /* {layer.title()} layer */")
        for path, node in token_set.tokens.items():
            if not path.startswith(layer + "."):
                continue
            dtype = token_set.type_of(path)
            lines.append(f"  {css_name(path)}: {css_value(node['$value'], dtype)};")
            if dtype == "typography":
                target = reference(node["$value"])
                tracking = f"var({css_name(target)}-letter-spacing)" if target else css_value(node["$value"]["letterSpacing"], "dimension")
                lines.append(f"  {css_name(path)}-letter-spacing: {tracking};")
    lines.append("}")

    def override_block(selector: str, values: dict, indent: str = "", important: bool = False) -> None:
        lines.append(f"{indent}{selector} {{")
        for path, value in values.items():
            output = css_value(value, token_set.type_of(path)) if reference(value) is not None else value
            lines.append(f"{indent}  {css_name(path)}: {output}{' !important' if important else ''};")
        lines.append(f"{indent}}}")

    lines.extend(["", "/* Optional explicit preference; no settings persistence is implied. */"])
    override_block(':root[data-kb-contrast="more"]', adapters["highContrast"])
    lines.extend(["", "@media (prefers-contrast: more) {"])
    override_block(':root:not([data-kb-contrast="standard"])', adapters["highContrast"], "  ")
    lines.extend(["}", "", ":root[data-kb-motion=\"reduced\"] {"])
    for path in token_set.tokens:
        if path.startswith("semantic.motion.duration."):
            lines.append(f"  {css_name(path)}: 0ms;")
    lines.extend(["}", "", "@media (prefers-reduced-motion: reduce) {", "  :root {"])
    for path in token_set.tokens:
        if path.startswith("semantic.motion.duration."):
            lines.append(f"    {css_name(path)}: 0ms;")
    lines.extend(["  }", "}", "", "/* System colors override app contrast preferences; retain user color adjustment. */", "@media (forced-colors: active) {"])
    override_block(":root", adapters["forcedColors"], "  ", important=True)
    lines.extend(["}", ""])
    return "\n".join(lines).encode("utf-8")


def build_outputs(source_dir: Path = SOURCE_DIR) -> tuple[TokenSet, dict[str, bytes]]:
    merged = merge_sources(source_dir)
    token_set = TokenSet(merged)
    adapters = load_json(source_dir / ADAPTER_SOURCE)
    validate_adapters(adapters, token_set)
    resolved_document = copy.deepcopy(merged)
    for path in token_set.tokens:
        node = resolved_document
        for part in path.split("."):
            node = node[part]
        node["$value"] = token_set.resolve(path)
        node["$type"] = token_set.type_of(path)
    output = {"kohbor.tokens.json": json_bytes(merged), "tokens.resolved.json": json_bytes(resolved_document), "tokens.css": render_css(token_set, adapters, manifest_hash(source_manifest(source_dir)))}
    return token_set, output


def check_outputs(outputs: dict[str, bytes], token_dir: Path = TOKEN_DIR) -> list[str]:
    return [filename for filename, expected in outputs.items() if not (token_dir / filename).is_file() or (token_dir / filename).read_bytes() != expected]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail on stale/missing outputs without writing")
    args = parser.parse_args()
    try:
        token_set, outputs = build_outputs()
        if args.check:
            stale = check_outputs(outputs)
            require(not stale, "Stale/missing generated tokens: " + ", ".join(stale))
            print(f"PASS: {len(token_set.tokens)} tokens; references and types valid; {len(outputs)} generated outputs match.")
        else:
            TOKEN_DIR.mkdir(parents=True, exist_ok=True)
            for filename, content in outputs.items():
                (TOKEN_DIR / filename).write_bytes(content)
            print(f"Built {len(token_set.tokens)} tokens into {', '.join(outputs)}. Run python tools/validate_design.py to refresh the static receipt.")
        return 0
    except (TokenError, OSError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
