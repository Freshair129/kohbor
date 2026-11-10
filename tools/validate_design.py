#!/usr/bin/env python3
"""Validate Kohbor token structure, intended contrast and authored geometry.

This receipt is static evidence only. Browser interaction, Windows/DPI, live
gameplay, acoustic timing and resource acceptance remain separate procedures.
"""

from __future__ import annotations

import argparse
import copy
import itertools
import json
import sys
from collections import Counter

sys.dont_write_bytecode = True

from build_design_tokens import (
    ADAPTER_SOURCE, ROOT, SOURCE_DIR, SUPPORTED_TYPES, TOKEN_DIR, TokenError,
    TokenSet, build_outputs, check_outputs, css_value, json_bytes, load_json,
    manifest_hash, require, sha256, source_manifest, unique_object,
)

REPORT = TOKEN_DIR / "token-validation.json"


def luminance(color: dict) -> float:
    require(color["colorSpace"] == "srgb" and color.get("alpha", 1) == 1, "Contrast receipt requires opaque sRGB colors; composited scenes need separate evidence")
    linear = [channel / 12.92 if channel <= .04045 else ((channel + .055) / 1.055) ** 2.4 for channel in color["components"]]
    return sum(weight * channel for weight, channel in zip((.2126, .7152, .0722), linear))


def contrast(a: dict, b: dict) -> float:
    first, second = luminance(a), luminance(b)
    return (max(first, second) + .05) / (min(first, second) + .05)


def intended_pairs() -> list[tuple[str, str, str, float]]:
    pairs = []
    color = "semantic.color."
    surfaces = ("canvas", "panel", "raised", "hover")
    for foreground, surface in itertools.product(("primary", "secondary", "muted"), surfaces):
        pairs.append(("normal-text", color + "text." + foreground, color + "surface." + surface, 4.5))
    for state in ("success", "warning", "danger", "info"):
        for surface in ("surface.canvas", "surface.panel", "surface.raised", state + "-surface"):
            pairs.append(("status-text-with-label", color + state, color + surface, 4.5))
        for text in ("primary", "secondary"):
            pairs.append(("status-body-text", color + "text." + text, color + state + "-surface", 4.5))
    for state, surface in itertools.product(("unknown", "stale", "estimated", "catalog", "unavailable"), ("panel", "raised")):
        pairs.append(("provenance-label", color + "state." + state, color + "surface." + surface, 4.5))
    for surface in (*surfaces, "selected"):
        pairs.append(("accent-text", color + "accent", color + "surface." + surface, 4.5))
        pairs.append(("focus-ring", color + "focus", color + "surface." + surface, 3))
    for state in ("accent", "accent-hover", "accent-pressed"):
        pairs.append(("filled-action-text", color + "on-accent", color + state, 4.5))
    for surface in ("canvas", "panel", "raised"):
        pairs.append(("interactive-boundary", color + "border.strong", color + "surface." + surface, 3))
        pairs.append(("selected-boundary", color + "border.selected", color + "surface." + surface, 3))
    for component in ("secondary-button", "input", "badge", "navigation", "overlay", "tooltip"):
        pairs.append(("component-text", f"component.{component}.foreground", f"component.{component}.background", 4.5))
    pairs.append(("component-placeholder", "component.input.placeholder", "component.input.background", 4.5))
    pairs.append(("component-selected-text", "component.navigation.selected-foreground", "component.navigation.selected-background", 4.5))
    return pairs


def contrast_rows(token_set: TokenSet, theme: str) -> list[dict]:
    rows = []
    for purpose, foreground, background, minimum in intended_pairs():
        fg, bg = token_set.resolve(foreground), token_set.resolve(background)
        ratio = contrast(fg, bg)
        # Compare the unrounded ratio. Rounding cannot turn a failure into a pass.
        require(ratio >= minimum, f"{theme} contrast failure: {foreground} / {background} = {ratio:.6f}, requires {minimum}")
        rows.append({"theme": theme, "purpose": purpose, "foreground": foreground, "background": background, "foregroundHex": fg.get("hex"), "backgroundHex": bg.get("hex"), "minimum": minimum, "ratio": round(ratio, 6), "status": "PASS"})
    return rows


def overridden_set(token_set: TokenSet, overrides: dict) -> TokenSet:
    document = copy.deepcopy(token_set.document)
    for path, value in overrides.items():
        node = document
        for part in path.split("."):
            node = node[part]
        node["$value"] = value
    return TokenSet(document)


def geometry_receipt(token_set: TokenSet) -> dict:
    def value(path: str) -> float:
        result = token_set.resolve("semantic.layout." + path)
        require(result["unit"] == "px", f"Geometry must use authored px: {path}")
        return result["value"]

    def box(path: str) -> dict:
        return {dimension: value(path + "." + dimension) for dimension in ("x", "y", "width", "height")}

    def contains(container: dict, child: dict) -> bool:
        return child["x"] >= 0 and child["y"] >= 0 and child["width"] > 0 and child["height"] > 0 and child["x"] + child["width"] <= container["width"] and child["y"] + child["height"] <= container["height"]

    def overlap(a: dict, b: dict) -> bool:
        return min(a["x"] + a["width"], b["x"] + b["width"]) > max(a["x"], b["x"]) and min(a["y"] + a["height"], b["y"] + b["height"]) > max(a["y"], b["y"])

    stage = {"width": value("stage.width"), "height": value("stage.height")}
    panel, nav = box("panel"), box("nav")
    require(stage == {"width": 1420, "height": 760}, "Inherited authored stage must remain 1420 × 760")
    require(panel["width"] == 1280 and panel["height"] == 720, "Inherited panel canvas must remain 1280 × 720")
    require(contains(stage, panel) and contains(stage, nav) and not overlap(panel, nav), "Stage panel/navigation containment or separation failed")
    regions = {name: box("live." + name) for name in ("own", "map", "enemies", "allies", "right")}
    header_bottom = value("live.header.y") + value("live.header.height")
    for name, region in regions.items():
        require(contains(panel, region), f"Live {name} escapes the panel canvas")
        require(region["y"] >= header_bottom, f"Live {name} overlaps header")
    for (name_a, a), (name_b, b) in itertools.combinations(regions.items(), 2):
        require(not overlap(a, b), f"Live regions overlap: {name_a}, {name_b}")
    map_area = regions["map"]["width"] * regions["map"]["height"]
    require(all(map_area > region["width"] * region["height"] for name, region in regions.items() if name != "map"), "Map must remain the largest authored live region by area")
    for name in ("enemies", "allies"):
        require(regions[name]["x"] == regions["map"]["x"] and regions[name]["width"] == regions["map"]["width"], f"{name} must align with map")
    counts = {name: token_set.resolve(f"semantic.layout.live.{name}.count") for name in ("enemies", "allies")}
    require(counts == {"enemies": 5, "allies": 4}, "Five enemy and four ally slots are required")
    return {"status": "PASS", "coordinateSystem": {"stage": "Authored CSS px", "liveRegions": "Panel-local authored CSS px"}, "stage": stage, "navigation": nav, "panel": panel, "liveRegions": regions, "slotCounts": counts, "mapAreaPx2": map_area, "rightAreaPx2": regions["right"]["width"] * regions["right"]["height"], "checks": ["Inherited canvases preserved", "Regions contained and non-overlapping", "Map largest by authored area", "Five enemy/four ally slots; allies exclude self"], "nativeValidation": "NOT_RUN", "runtimeMapInvariant": "NOT_RUN — static coordinates do not prove <=1 CSS px across interactions"}


def design_constraints(token_set: TokenSet) -> dict:
    for path in token_set.tokens:
        if path.startswith("semantic.font.size."):
            value = token_set.resolve(path)
            require(value["unit"] == "px" and value["value"] >= 14, f"Authored type below 14px: {path}")
    for style in ("body", "supporting", "label"):
        value = token_set.resolve("semantic.typography." + style)
        require(value["lineHeight"] >= 1.6 and value["letterSpacing"]["value"] == 0, f"Thai text spacing regression: {style}")
    require(token_set.resolve("semantic.target.default")["value"] == 44 and token_set.resolve("semantic.target.compact")["value"] == 36, "Authored target proposal changed without updating design checks")
    require(token_set.resolve("semantic.focus.width")["value"] == 2 and token_set.resolve("semantic.focus.offset")["value"] == 3, "Focus geometry proposal changed")
    require(token_set.resolve("semantic.motion.duration.reduced")["value"] == 0, "Reduced motion must be immediate")
    for index in range(13):
        require(token_set.resolve(f"semantic.space.{index}") == {"value": 4 * index, "unit": "px"}, "Four-pixel spacing scale drifted")
    return {"status": "PASS", "authoredMinimumFontPx": 14, "bodyFontPx": token_set.resolve("semantic.font.size.body")["value"], "bodyLineHeight": token_set.resolve("semantic.line-height.body"), "focusWidthPx": 2, "focusOffsetPx": 3, "defaultTargetPx": 44, "compactTargetPx": 36, "reducedMotionMs": 0, "nativeReadabilityAndTargetStatus": "NOT_RUN"}


def importer_negative_probes() -> list[dict]:
    def basic(value=1, kind="number") -> dict:
        return {"test": {"$type": kind, "$value": value}}

    cases = {
        "unresolved alias": {"test": {"$type": "number", "$value": "{missing}"}},
        "cyclic aliases": {"a": {"$type": "number", "$value": "{b}"}, "b": {"$type": "number", "$value": "{a}"}},
        "alias type mismatch": {"a": {"$type": "number", "$value": 400}, "b": {"$type": "fontWeight", "$value": "{a}"}},
        "invalid dimension unit": basic({"value": 16, "unit": "em"}, "dimension"),
        "invalid color components": basic({"colorSpace": "srgb", "components": [1.2, 0, 0]}, "color"),
        "invalid font weight": basic(1001, "fontWeight"),
        "invalid bezier x": basic([1.2, 0, 0, 1], "cubicBezier"),
        "invalid duration": basic({"value": -1, "unit": "ms"}, "duration"),
        "incomplete typography": basic({"fontFamily": "Tahoma"}, "typography"),
        "mixed group and token": {"test": {"$type": "number", "$value": 1, "child": {"$type": "number", "$value": 2}}},
        "missing type": {"test": {"$value": 1}},
        "unsupported JSON Pointer": {"test": {"$type": "number", "$value": {"$ref": "#/a/$value"}}},
    }
    results = []
    for name, case in cases.items():
        try:
            TokenSet(case)
        except TokenError:
            results.append({"probe": name, "status": "PASS", "observed": "Rejected as required"})
        else:
            raise TokenError(f"Validator negative probe accepted invalid data: {name}")
    try:
        json.loads('{"a":1,"a":2}', object_pairs_hook=unique_object)
    except TokenError:
        results.append({"probe": "duplicate JSON keys", "status": "PASS", "observed": "Rejected as required"})
    else:
        raise TokenError("Validator accepted duplicate JSON keys")
    return results


def exporter_regression_probes() -> list[dict]:
    results = []
    for name, expected in (("semi-bold", "600"), ("book", "400"), ("extra-light", "200")):
        actual = css_value(name, "fontWeight")
        require(actual == expected, f"DTCG font weight {name} must export as CSS weight {expected}")
        results.append({"probe": f"fontWeight {name}", "expectedCss": expected, "actualCss": actual, "status": "PASS"})
    fractional = {"colorSpace": "srgb", "components": [.123, .456, .789], "alpha": 1}
    actual = css_value(fractional, "color")
    require(actual == "rgb(31.365 116.28 201.195 / 1)", "Fractional sRGB must not be quantized to 8-bit")
    results.append({"probe": "fractional sRGB export", "expectedCss": "rgb(31.365 116.28 201.195 / 1)", "actualCss": actual, "status": "PASS"})
    return results


def make_report() -> dict:
    token_set, outputs = build_outputs()
    stale = check_outputs(outputs)
    require(not stale, "Stale/missing generated outputs: " + ", ".join(stale) + ". Run python tools/build_design_tokens.py")
    adapters = load_json(SOURCE_DIR / ADAPTER_SOURCE)
    high_contrast = overridden_set(token_set, adapters["highContrast"])
    rows = contrast_rows(token_set, "dark") + contrast_rows(high_contrast, "dark-more-contrast")
    sources = source_manifest()
    tools = {f"tools/{filename}": sha256((ROOT / "tools" / filename).read_bytes()) for filename in ("build_design_tokens.py", "validate_design.py")}
    ratios = [row["ratio"] for row in rows if row["minimum"] == 4.5]
    boundary_ratios = [row["ratio"] for row in rows if row["minimum"] == 3]
    exclusions = []
    for surface in ("panel", "raised"):
        exclusions.append({"foreground": "semantic.color.border.default", "background": "semantic.color.surface." + surface, "ratio": round(contrast(token_set.resolve("semantic.color.border.default"), token_set.resolve("semantic.color.surface." + surface)), 6), "reason": "Decorative divider only; never the sole control/state boundary", "acceptanceClaim": False})
    return {
        "reportVersion": "1.0.0",
        "status": "PASS",
        "evidenceClass": "static",
        "scope": "Token graph/value validation, generated-file byte equality, intended opaque-color pairs and proposed authored layout only",
        "sourceBinding": {"validationRevision": "sha256:" + manifest_hash({**sources, **tools}), "bindingRule": "Content-addressed revision of exact token sources and tools; independent of Git HEAD so committing an unchanged receipt preserves deterministic validation", "tokenSourceSha256": manifest_hash(sources), "sourceFiles": sources, "toolFiles": tools, "generatedFiles": {"design/tokens/" + name: sha256(data) for name, data in outputs.items()}},
        "profile": {"format": "DTCG 2025.10 value forms", "supportedTypes": sorted(SUPPORTED_TYPES), "validation": "Project structural/value profile, not complete DTCG importer conformance", "unsupported": ["JSON Pointer $ref", "group $extends", "$root tokens", "DTCG Resolver Module", "color spaces other than numeric sRGB", "array shadows", "gradient, border, transition and strokeStyle composites", "embedded references in strings"]},
        "inventory": {"totalTokens": len(token_set.tokens), "byLayer": dict(Counter(path.split(".")[0] for path in token_set.tokens)), "byType": dict(sorted(Counter(token_set.type_of(path) for path in token_set.tokens).items())), "tokensWithReferences": sum(bool(refs) for refs in token_set.references.values()), "referenceEdges": sum(len(refs) for refs in token_set.references.values()), "cssCustomProperties": len(token_set.tokens) + sum(token_set.type_of(path) == "typography" for path in token_set.tokens)},
        "checks": {"tokenStructureAndTypes": "PASS", "duplicateKeys": "PASS", "unresolvedReferences": "PASS", "cyclicReferences": "PASS", "referenceTypeAgreement": "PASS", "generatedOutputEquality": "PASS", "highContrastAdapterTypes": "PASS", "forcedColorsSystemMapping": "STATIC_MAPPING_ONLY", "reducedMotionOverride": "STATIC_MAPPING_ONLY"},
        "defensiveValidationProbes": importer_negative_probes(),
        "exporterRegressionProbes": exporter_regression_probes(),
        "contrastSummary": {"testedPairs": len(rows), "minimumTextRatio": min(ratios), "textThreshold": 4.5, "minimumNonTextRatio": min(boundary_ratios), "nonTextThreshold": 3, "method": "WCAG 2 relative luminance, sRGB 0.04045 breakpoint, unrounded threshold comparison", "claim": "Only listed opaque token pairs; not full-interface WCAG conformance"},
        "contrastPairs": rows,
        "contrastExclusions": exclusions,
        "designConstraints": design_constraints(token_set),
        "authoredGeometry": geometry_receipt(token_set),
        "notEstablished": ["Windows native safe area and click-through", "DPI/zoom-scaled Thai readability and interactive target sizes", "Actual host font availability and shaping", "Runtime map invariance during selection or entitlement transitions", "Forced-colors rendering in WebView2 or assistive technology behavior", "Contrast over transparent game imagery", "Live data admission, freshness, audio timing or resource/FPS compliance"],
        "references": ["https://www.designtokens.org/tr/2025.10/format/", "https://www.designtokens.org/tr/2025.10/color/", "https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html", "https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html", "https://www.w3.org/WAI/WCAG22/Techniques/general/G18"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="check the deterministic receipt without writing it")
    args = parser.parse_args()
    try:
        report = make_report()
        expected = json_bytes(report)
        if args.check:
            require(REPORT.is_file() and REPORT.read_bytes() == expected, "Static token receipt is stale/missing. Run python tools/validate_design.py")
        else:
            REPORT.write_bytes(expected)
        summary = report["contrastSummary"]
        print(f"PASS static: {report['inventory']['totalTokens']} tokens; {summary['testedPairs']} intended contrast pairs; {len(report['defensiveValidationProbes'])} invalid-input rejection probes; authored geometry valid.")
        print(f"Minimum text {summary['minimumTextRatio']:.3f}:1; non-text {summary['minimumNonTextRatio']:.3f}:1. Native, browser interaction, live, acoustic and resource evidence not established.")
        print(("Checked" if args.check else "Wrote") + " design/tokens/token-validation.json")
        return 0
    except (TokenError, OSError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
