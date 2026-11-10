#!/usr/bin/env python3
"""Render the exact-token Kohbor technical design poster, without a browser.

This is deterministic drawing, not a screenshot or a generative-image render.
Requires Pillow with Raqm. Uses the repository's unmodified OFL Noto fonts and
original SVG shapes. Exact input hashes are embedded in PNG metadata. This
artifact supplies static design evidence only; native acceptance is NOT_RUN.

Run: python tools/render_design_poster.py
Check: python tools/render_design_poster.py --check
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import math
import re
import sys
import xml.etree.ElementTree as ET
from functools import lru_cache
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, PngImagePlugin, features

ROOT = Path(__file__).resolve().parents[1]
TOKENS_PATH = ROOT / "design/tokens/tokens.resolved.json"
ASSETS = ROOT / "design/assets"
FONTS = ASSETS / "fonts"
OUTPUT = ROOT / "design/board/kohbor-design-board.png"
WIDTH, HEIGHT, AA = 1800, 2300, 2
SCRIPT_RUNS = re.compile(r"[\u0E00-\u0E7F]+|[^\u0E00-\u0E7F]+")
PATH_PARTS = re.compile(r"[A-Za-z]|[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?")


def token(document: dict, path: str):
    node = document
    for part in path.split("."):
        node = node[part]
    return node["$value"]


def token_color(document: dict, path: str) -> str:
    value = token(document, path)
    if value["colorSpace"] != "srgb" or value.get("alpha", 1) != 1:
        raise ValueError(f"Poster requires opaque sRGB for {path}")
    return "#" + "".join(f"{round(component * 255):02X}" for component in value["components"])


def dimension(document: dict, path: str) -> float:
    value = token(document, path)
    if value["unit"] != "px":
        raise ValueError(f"Authored geometry must be CSS px: {path}")
    return value["value"]


@lru_cache(maxsize=256)
def font(pixel_size: int, bold: bool, thai: bool) -> ImageFont.FreeTypeFont:
    name = ("NotoSansThai" if thai else "NotoSans") + ("-Bold.ttf" if bold else "-Regular.ttf")
    return ImageFont.truetype(str(FONTS / name), pixel_size, layout_engine=ImageFont.Layout.RAQM)


class Canvas:
    def __init__(self, image: Image.Image, origin=(0.0, 0.0), scale=AA):
        self.image = image
        self.draw = ImageDraw.Draw(image)
        self.origin = origin
        self.scale = scale

    def point(self, x, y):
        return (round(self.origin[0] + x * self.scale), round(self.origin[1] + y * self.scale))

    def child(self, x, y, scale=1):
        return Canvas(self.image, (self.origin[0] + x * self.scale, self.origin[1] + y * self.scale), self.scale * scale)

    def rect(self, x, y, width, height, fill=None, outline=None, radius=0, stroke=1):
        bounds = [self.point(x, y), self.point(x + width, y + height)]
        self.draw.rounded_rectangle(bounds, radius=round(radius * self.scale), fill=fill, outline=outline, width=max(1, round(stroke * self.scale)))

    def ellipse(self, x, y, width, height, fill=None, outline=None, stroke=1):
        self.draw.ellipse([self.point(x, y), self.point(x + width, y + height)], fill=fill, outline=outline, width=max(1, round(stroke * self.scale)))

    def line(self, points, fill, width=1, round_caps=False):
        self.draw.line([self.point(x, y) for x, y in points], fill=fill, width=max(1, round(width * self.scale)), joint="curve")
        if round_caps:
            r = width / 2
            for x, y in (points[0], points[-1]):
                self.ellipse(x - r, y - r, width, width, fill)

    def arc(self, x, y, width, height, start, end, fill, stroke=1):
        self.draw.arc([self.point(x, y), self.point(x + width, y + height)], start, end, fill, max(1, round(stroke * self.scale)))

    def text_width(self, text: str, size: float, bold=False):
        total = 0
        for run in SCRIPT_RUNS.findall(text):
            face = font(max(1, round(size * self.scale)), bold, "\u0e00" <= run[0] <= "\u0e7f")
            total += face.getlength(run, direction="ltr")
        return total / self.scale

    def text(self, text: str, x, baseline, size, fill, bold=False, anchor="left", max_width=None):
        width = self.text_width(text, size, bold)
        if max_width is not None and width > max_width + .5:
            raise ValueError(f"Text exceeds intended column ({width:.1f}>{max_width}): {text}")
        if anchor == "center":
            x -= width / 2
        elif anchor == "right":
            x -= width
        cursor = self.origin[0] + x * self.scale
        for run in SCRIPT_RUNS.findall(text):
            thai = "\u0e00" <= run[0] <= "\u0e7f"
            face = font(max(1, round(size * self.scale)), bold, thai)
            self.draw.text((cursor, self.origin[1] + baseline * self.scale), run, font=face, fill=fill, anchor="ls", direction="ltr", language="th" if thai else "en")
            cursor += face.getlength(run, direction="ltr")
        return width

    def svg(self, filename, x, y, width, height=None):
        height = width if height is None else height
        image = render_svg(str(ASSETS / filename), max(1, round(width * self.scale)), max(1, round(height * self.scale)))
        self.image.alpha_composite(image, self.point(x, y))


def parse_svg_path(data: str):
    """Tessellate only the commands used by the repository's original SVGs."""
    parts = PATH_PARTS.findall(data)
    output, points = [], []
    index = 0
    command = None
    current = (0.0, 0.0)
    start = current
    previous_control = None
    previous_command = None
    counts = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4}

    def flush(closed=False):
        nonlocal points
        if points:
            output.append((points, closed))
            points = []

    while index < len(parts):
        if parts[index].isalpha():
            command = parts[index]
            index += 1
            if command.upper() == "Z":
                points.append(start)
                flush(True)
                current = start
                previous_control = None
                previous_command = "Z"
                continue
        if command is None or command.upper() not in counts:
            raise ValueError(f"Unsupported original SVG command: {command}")
        op, relative = command.upper(), command.islower()
        count = counts[op]
        values = list(map(float, parts[index:index + count]))
        index += count
        x0, y0 = current

        def xy(a, b):
            return (a + x0, b + y0) if relative else (a, b)

        if op == "M":
            flush()
            current = xy(*values)
            start = current
            points = [current]
            command = "l" if relative else "L"
        elif op == "L":
            current = xy(*values)
            points.append(current)
        elif op == "H":
            current = (values[0] + x0 if relative else values[0], y0)
            points.append(current)
        elif op == "V":
            current = (x0, values[0] + y0 if relative else values[0])
            points.append(current)
        elif op in {"C", "S"}:
            if op == "C":
                control1, control2, end = xy(values[0], values[1]), xy(values[2], values[3]), xy(values[4], values[5])
            else:
                control1 = (2 * x0 - previous_control[0], 2 * y0 - previous_control[1]) if previous_control is not None and previous_command in {"C", "S"} else current
                control2, end = xy(values[0], values[1]), xy(values[2], values[3])
            for step in range(1, 49):
                t = step / 48
                inverse = 1 - t
                points.append((inverse ** 3 * x0 + 3 * inverse ** 2 * t * control1[0] + 3 * inverse * t ** 2 * control2[0] + t ** 3 * end[0], inverse ** 3 * y0 + 3 * inverse ** 2 * t * control1[1] + 3 * inverse * t ** 2 * control2[1] + t ** 3 * end[1]))
            current = end
            previous_control = control2
        if op not in {"C", "S"}:
            previous_control = None
        previous_command = op
    flush()
    return output


@lru_cache(maxsize=32)
def render_svg(filename: str, width: int, height: int) -> Image.Image:
    root = ET.parse(filename).getroot()
    viewbox = list(map(float, root.attrib["viewBox"].split()))
    if viewbox[:2] != [0, 0] or abs(width / height - viewbox[2] / viewbox[3]) > .02:
        raise ValueError("Original SVG must retain its aspect ratio")
    image = Image.new("RGBA", (width, height))
    canvas = Canvas(image, scale=width / viewbox[2])

    def svg_color(value, opacity):
        if value is None or value == "none":
            return None
        return tuple(int(value[i:i + 2], 16) for i in (1, 3, 5)) + (round(255 * opacity),)

    for element in root:
        kind = element.tag.split("}")[-1]
        attributes = element.attrib
        opacity = float(attributes.get("opacity", 1))
        fill = svg_color(attributes.get("fill", root.attrib.get("fill", "none")), opacity)
        stroke = svg_color(attributes.get("stroke"), opacity)
        stroke_width = float(attributes.get("stroke-width", 1))
        if kind == "rect":
            canvas.rect(float(attributes.get("x", 0)), float(attributes.get("y", 0)), float(attributes["width"]), float(attributes["height"]), fill, stroke, float(attributes.get("rx", 0)), stroke_width)
        elif kind in {"ellipse", "circle"}:
            rx = float(attributes.get("rx", attributes.get("r")))
            ry = float(attributes.get("ry", attributes.get("r")))
            canvas.ellipse(float(attributes["cx"]) - rx, float(attributes["cy"]) - ry, rx * 2, ry * 2, fill, stroke, stroke_width)
        elif kind == "path":
            for points, closed in parse_svg_path(attributes["d"]):
                if fill is not None:
                    canvas.draw.polygon([canvas.point(x, y) for x, y in points], fill=fill)
                if stroke is not None:
                    canvas.line(points, stroke, stroke_width, round_caps=attributes.get("stroke-linecap") == "round" and not closed)
        elif kind not in {"title", "desc"}:
            raise ValueError(f"Unsupported original SVG element: {kind}")
    return image


def icon(canvas: Canvas, name, x, y, size, color, width=1.8):
    c = canvas.child(x, y, size / 24)
    if name == "user":
        c.ellipse(8, 3, 8, 8, outline=color, stroke=width)
        c.arc(4, 13, 16, 14, 180, 360, color, width)
        c.line([(4, 20), (4, 22), (20, 22), (20, 20)], color, width)
    elif name == "map":
        c.line([(2, 5), (8, 2), (16, 5), (22, 2), (22, 19), (16, 22), (8, 19), (2, 22), (2, 5)], color, width)
        c.line([(8, 2), (8, 19)], color, width)
        c.line([(16, 5), (16, 22)], color, width)
    elif name == "home":
        c.line([(2, 11), (12, 3), (22, 11)], color, width, True)
        c.line([(5, 10), (5, 22), (10, 22), (10, 15), (14, 15), (14, 22), (19, 22), (19, 10)], color, width)
    elif name == "live":
        for xx, yy in ((3, 3), (14, 3), (3, 14), (14, 14)):
            c.rect(xx, yy, 7, 7, outline=color, radius=1.5, stroke=width)
    elif name == "audio":
        c.line([(3, 9), (7, 9), (12, 4), (12, 20), (7, 15), (3, 15), (3, 9)], color, width)
        c.arc(10, 6, 10, 12, -60, 60, color, width)
        c.arc(8, 2, 16, 20, -55, 55, color, width)
    elif name == "store":
        c.rect(4, 8, 16, 14, outline=color, radius=2, stroke=width)
        c.arc(8, 2, 8, 12, 180, 360, color, width)
    elif name == "insights":
        c.line([(3, 3), (3, 21), (22, 21)], color, width)
        c.line([(7, 17), (7, 12)], color, width + 1, True)
        c.line([(13, 17), (13, 5)], color, width + 1, True)
        c.line([(19, 17), (19, 9)], color, width + 1, True)
    elif name == "settings":
        c.ellipse(5, 5, 14, 14, outline=color, stroke=width)
        c.ellipse(9, 9, 6, 6, outline=color, stroke=width)
        for angle in range(0, 360, 45):
            theta = math.radians(angle)
            c.line([(12 + math.cos(theta) * 8, 12 + math.sin(theta) * 8), (12 + math.cos(theta) * 11, 12 + math.sin(theta) * 11)], color, width, True)
    elif name == "clock":
        c.ellipse(2, 2, 20, 20, outline=color, stroke=width)
        c.line([(12, 6), (12, 12), (16, 14)], color, width, True)
    elif name == "check":
        c.line([(4, 12), (9, 17), (20, 6)], color, width + .4, True)
    elif name == "minus":
        c.line([(5, 12), (19, 12)], color, width + .4, True)
    elif name == "estimate":
        c.line([(3, 9), (7, 7), (12, 9), (17, 11), (21, 9)], color, width, True)
        c.line([(3, 15), (7, 13), (12, 15), (17, 17), (21, 15)], color, width, True)
    elif name == "mic-off":
        c.rect(8, 2, 8, 13, outline=color, radius=4, stroke=width)
        c.arc(4, 7, 16, 13, 0, 180, color, width)
        c.line([(12, 20), (12, 23)], color, width)
        c.line([(3, 2), (21, 22)], color, width, True)


def draw_stage(canvas: Canvas, document: dict, colors: dict):
    p = colors
    get = lambda path: dimension(document, "semantic.layout." + path)
    stage_width, stage_height = get("stage.width"), get("stage.height")
    canvas.rect(0, 0, stage_width, stage_height, p["canvas"], p["border"], 20, 1)
    nav = canvas.child(get("nav.x"), get("nav.y"))
    nav.rect(0, 0, get("nav.width"), get("nav.height"), p["panel"], p["border"], 16)
    nav.svg("kohbor-mark.svg", 24, 24, 40)
    routes = [("ภาพรวม", "home"), ("สนามสด", "live"), ("แพ็กเสียง", "audio"), ("ร้านค้า", "store"), ("สรุปการเล่น", "insights"), ("บัญชี", "user")]
    for index, (label, glyph) in enumerate(routes):
        y = 100 + index * 82
        active = index == 1
        if active:
            nav.rect(8, y - 8, 72, 68, p["selected"], radius=12)
            nav.rect(8, y + 6, 3, 25, p["accent"], radius=1.5)
        color = p["accent"] if active else p["secondary"]
        icon(nav, glyph, 33, y, 22, color)
        nav.text(label, 44, y + 45, 14, color, anchor="center", max_width=76)
    icon(nav, "settings", 33, 650, 22, p["secondary"])
    nav.text("ตั้งค่า", 44, 695, 14, p["secondary"], anchor="center")

    panel = canvas.child(get("panel.x"), get("panel.y"))
    pw, ph = get("panel.width"), get("panel.height")
    panel.rect(0, 0, pw, ph, p["panel"], p["border"], 16)
    panel.line([(0, get("live.header.height")), (pw, get("live.header.height"))], p["border"])
    panel.text("แมตช์ของคุณ", 24, 33, 20, p["primary"], True)
    panel.text("รอเข้าเกม", 24, 57, 14, p["secondary"])
    panel.rect(274, 18, 88, 36, p["selected"], radius=8)
    panel.text("สนาม", 318, 42, 14, p["accent"], anchor="center")
    panel.text("บิลด์", 398, 42, 14, p["secondary"])
    panel.text("ฮีโร่ที่ตรวจจับ", 478, 42, 14, p["secondary"])
    panel.text("แชร์กับทีม", 614, 42, 14, p["secondary"])
    panel.rect(pw - 100, 20, 76, 32, p["raised"], p["border"], 16)
    panel.text("BASE", pw - 62, 42, 14, p["secondary"], anchor="center")

    def region(name):
        return panel.child(get(f"live.{name}.x"), get(f"live.{name}.y")), get(f"live.{name}.width"), get(f"live.{name}.height")

    enemies, ew, eh = region("enemies")
    enemy_count = token(document, "semantic.layout.live.enemies.count")
    gap, cell = 8, (ew - 8 * (enemy_count - 1)) / enemy_count
    for index in range(enemy_count):
        x = index * (cell + gap)
        enemies.rect(x, 0, cell, eh, p["raised"], p["border"], 12)
        enemies.ellipse(x + 12, 16, 28, 28, p["canvas"], p["strong"])
        icon(enemies, "user", x + 18, 22, 16, p["muted"], 1.6)
        enemies.text(f"ศัตรู {index + 1}", x + 49, 30, 14, p["secondary"], max_width=cell - 57)
        enemies.text("—", x + 49, 53, 14, p["muted"])

    own, ow, oh = region("own")
    own.rect(0, 0, ow, oh, p["raised"], p["border"], 16)
    own.text("ตัวคุณ", 18, 29, 14, p["secondary"])
    own.rect((ow - 76) / 2, 52, 76, 76, p["canvas"], p["strong"], 16)
    icon(own, "user", ow / 2 - 21, 70, 42, p["muted"])
    own.text("ยังไม่มีข้อมูล", ow / 2, 162, 16, p["primary"], True, "center", ow - 24)
    own.text("รอข้อมูลจากเกม", ow / 2, 187, 14, p["secondary"], anchor="center", max_width=ow - 24)
    for y, label in ((231, "พลังชีวิต"), (268, "มานา")):
        own.text(label, 18, y, 14, p["secondary"])
        own.text("—", ow - 18, y, 16, p["muted"], anchor="right")
    own.line([(18, 293), (ow - 18, 293)], p["border"])
    own.svg("kohbor-companion.svg", (ow - 88) / 2, 304, 88)
    own.text("ไมโครโฟนปิด", ow / 2, 414, 14, p["secondary"], anchor="center")

    game_map, mw, mh = region("map")
    game_map.rect(0, 0, mw, mh, p["canvas"], p["strong"], 16)
    icon(game_map, "map", 18, 18, 20, p["secondary"])
    game_map.text("พื้นที่แผนที่", 48, 35, 16, p["secondary"])
    game_map.rect(mw - 126, 13, 110, 32, p["raised"], p["border"], 16)
    game_map.text("ยังไม่มีภาพ", mw - 71, 35, 14, p["secondary"], anchor="center")
    game_map.line([(18, 60), (mw - 18, 60)], p["border"])
    # A neutral symbol, not a map image, player position, terrain or live marker.
    game_map.rect(mw / 2 - 33, 122, 66, 66, p["panel"], p["border"], 18)
    icon(game_map, "map", mw / 2 - 18, 137, 36, p["strong"])
    game_map.text("รอภาพแผนที่จากเกม", mw / 2, 234, 20, p["primary"], True, "center")
    game_map.text("ไม่มีภาพที่ตรวจสอบได้ในตัวอย่างนี้", mw / 2, 266, 14, p["secondary"], anchor="center")
    game_map.text("ภาพจัดวางเท่านั้น · ไม่มีตำแหน่งผู้เล่น", mw / 2, 310, 14, p["muted"], anchor="center")
    game_map.line([(18, mh - 46), (mw - 18, mh - 46)], p["border"])
    game_map.text("แหล่งภาพ: ยังไม่พร้อม", 18, mh - 19, 14, p["secondary"])
    game_map.text("อายุข้อมูล —", mw - 18, mh - 19, 14, p["muted"], anchor="right")

    allies, aw, ah = region("allies")
    ally_count = token(document, "semantic.layout.live.allies.count")
    gap, cell = 12, (aw - 12 * (ally_count - 1)) / ally_count
    for index in range(ally_count):
        x = index * (cell + gap)
        allies.rect(x, 0, cell, ah, p["raised"], p["border"], 12)
        allies.ellipse(x + 12, 16, 30, 30, p["canvas"], p["strong"])
        icon(allies, "user", x + 19, 23, 16, p["muted"], 1.6)
        allies.text(f"เพื่อน {index + 1}", x + 51, 34, 14, p["secondary"], max_width=cell - 59)
        allies.text("ยังไม่ทราบ", x + 14, 74, 14, p["muted"])
        allies.text("—", x + cell - 14, 74, 14, p["muted"], anchor="right")

    right, rw, rh = region("right")
    right.rect(0, 0, rw, rh, p["raised"], p["border"], 16)
    right.rect(16, 16, 122, 36, p["selected"], radius=8)
    right.text("สรุป", 77, 40, 14, p["accent"], anchor="center")
    right.text("เหตุการณ์", 213, 40, 14, p["secondary"], anchor="center")
    right.line([(16, 68), (rw - 16, 68)], p["border"])
    right.ellipse(rw / 2 - 27, 139, 54, 54, p["canvas"], p["border"])
    icon(right, "user", rw / 2 - 14, 150, 28, p["strong"])
    right.text("ยังไม่ได้เลือกฮีโร่", rw / 2, 238, 20, p["primary"], True, "center")
    right.text("พื้นที่สำหรับข้อมูลที่ตรวจสอบได้", rw / 2, 270, 14, p["secondary"], anchor="center")
    right.text("เมื่อคุณเลือกจากการ์ด", rw / 2, 295, 14, p["secondary"], anchor="center")
    right.rect(24, 351, rw - 48, 110, p["canvas"], p["border"], 12)
    right.text("รอเข้าเกม", 44, 382, 16, p["primary"], True)
    right.text("ค่าที่ไม่ทราบใช้ “—”", 44, 413, 14, p["secondary"])
    right.text("ไม่เติมค่าที่ไม่ทราบ", 44, 438, 14, p["muted"])
    right.line([(24, rh - 94), (rw - 24, rh - 94)], p["border"])
    icon(right, "mic-off", 26, rh - 69, 22, p["secondary"])
    right.text("ไมโครโฟนปิด", 62, rh - 51, 14, p["secondary"])
    right.text("ตัวอย่างสถานะเริ่มต้น", 62, rh - 26, 14, p["muted"])


def make_poster() -> tuple[bytes, dict]:
    if not features.check("raqm"):
        raise RuntimeError("Raqm is required for the Thai specimen; do not silently render without shaping")
    document = json.loads(TOKENS_PATH.read_text(encoding="utf-8"))
    paths = {"canvas": "surface.canvas", "panel": "surface.panel", "raised": "surface.raised", "selected": "surface.selected", "primary": "text.primary", "secondary": "text.secondary", "muted": "text.muted", "accent": "accent", "on-accent": "on-accent", "border": "border.default", "strong": "border.strong", "focus": "focus", "success": "success", "success-surface": "success-surface", "warning": "warning", "warning-surface": "warning-surface", "info": "info", "info-surface": "info-surface"}
    p = {key: token_color(document, "semantic.color." + path) for key, path in paths.items()}
    image = Image.new("RGBA", (WIDTH * AA, HEIGHT * AA), p["canvas"])
    c = Canvas(image)
    margin, end, content = 88, 1712, 1624

    # Editorial masthead: identity and evidence class are both immediately visible.
    c.text("STATIC DESIGN STUDY", margin, 43, 16, p["muted"])
    c.text("DESIGN CANDIDATE 1.0  /  NATIVE NOT_RUN", end, 43, 16, p["secondary"], anchor="right")
    c.svg("kohbor-mark.svg", margin, 79, 80)
    title_width = c.text("Kohbor", 192, 143, 72, p["primary"], True)
    c.text("/", 192 + title_width + 22, 141, 48, p["strong"])
    c.text("เกาะเบาะ", 192 + title_width + 67, 144, 57, p["primary"], True)
    c.text("NIGHTGLASS", end, 143, 56, p["accent"], True, "right")
    c.text("อยู่ข้างเกม ไม่แย่งสายตา", margin, 243, 42, p["primary"], True)
    c.text("เพื่อนร่วมจอที่สงบ ชัดเจน และบอกตามข้อมูลที่มี", margin, 290, 24, p["secondary"])
    c.text("Quietly in your corner.", end, 230, 26, p["secondary"], anchor="right")
    c.text("Calm surfaces. Honest states.", end, 272, 20, p["muted"], anchor="right")
    c.line([(margin, 330), (end, 330)], p["border"])

    c.text("01 / PALETTE", margin, 371, 18, p["secondary"], True)
    c.text("Opaque foundations · one calm accent", end, 371, 18, p["muted"], anchor="right")
    swatches = [("CANVAS", "Graphite", p["canvas"], p["primary"]), ("PANEL", "Smoke", p["panel"], p["primary"]), ("ACCENT", "Jade", p["accent"], p["on-accent"]), ("TEXT", "Porcelain", p["primary"], p["canvas"])]
    swatch_width = (content - 60) / 4
    for index, (role, name, color, foreground) in enumerate(swatches):
        x = margin + index * (swatch_width + 20)
        c.rect(x, 397, swatch_width, 158, color, p["strong"] if index < 2 else None, 16)
        c.text(role, x + 24, 429, 16, foreground)
        c.text(name, x + 24, 480, 30, foreground, True)
        c.text(color, x + 24, 531, 22, foreground)

    c.text("02 / NEUTRAL LIVE FIELD", margin, 609, 18, p["secondary"], True)
    c.text("5 enemies  /  self  /  4 allies  /  fixed inspector", end, 609, 18, p["muted"], anchor="right")
    stage_width = dimension(document, "semantic.layout.stage.width")
    stage_height = dimension(document, "semantic.layout.stage.height")
    map_width = dimension(document, "semantic.layout.live.map.width")
    map_height = dimension(document, "semantic.layout.live.map.height")
    right_width = dimension(document, "semantic.layout.live.right.width")
    right_height = dimension(document, "semantic.layout.live.right.height")
    stage_scale = content / stage_width
    draw_stage(c.child(margin, 640, stage_scale), document, p)
    c.text(f"{stage_width:g} × {stage_height:g} AUTHORED STAGE", margin, 1543, 18, p["secondary"], True)
    c.text(f"MAP {map_width:g} × {map_height:g}", 915, 1543, 18, p["accent"], True, "center")
    c.text(f"RIGHT {right_width:g} × {right_height:g} · RESERVED", end, 1543, 18, p["secondary"], True, "right")
    c.text("Geometry study only · no admitted map image, player markers or selected hero", margin, 1576, 17, p["muted"])

    left_x, right_x, panel_width = margin, 922, 790
    for x in (left_x, right_x):
        c.rect(x, 1612, panel_width, 322, p["panel"], p["border"], 20)
    c.text("03 / TYPE — THAI FIRST", left_x + 28, 1653, 18, p["secondary"], True)
    c.text("กขค Aa", left_x + 28, 1750, 74, p["primary"], True)
    c.text("ค่อย ๆ อ่านเกม แล้วเลือกจังหวะของคุณ", left_x + 28, 1804, 27, p["secondary"], max_width=734)
    c.text("Noto Sans Thai + Noto Sans", left_x + 28, 1855, 20, p["accent"])
    c.text("16 / 1.6 body     14 / 1.6 supporting", left_x + 28, 1897, 18, p["muted"])
    c.text("48 / 32 / 24 / 20", left_x + panel_width - 28, 1897, 18, p["secondary"], anchor="right")

    c.text("04 / TRUTHFUL STATES", right_x + 28, 1653, 18, p["secondary"], True)
    c.text("Semantic samples · no live values", right_x + 28, 1684, 16, p["muted"])
    states = [("ปัจจุบัน", "Current", "check", p["success"], p["success-surface"]), ("ยังไม่ทราบ", "Unknown", "minus", p["secondary"], p["raised"]), ("ข้อมูลเก่า", "Stale", "clock", p["warning"], p["warning-surface"]), ("ค่าประมาณ", "Estimated", "estimate", p["info"], p["info-surface"])]
    for index, (thai, english, glyph, foreground, backing) in enumerate(states):
        x = right_x + 28 + (index % 2) * 378
        y = 1710 + (index // 2) * 93
        c.rect(x, y, 356, 76, backing, p["border"], 12)
        icon(c, glyph, x + 18, y + 24, 27, foreground, 2)
        c.text(thai, x + 60, y + 32, 22, foreground, True)
        c.text(english, x + 60, y + 60, 17, p["secondary"])
    c.text("ข้อความ + สัญลักษณ์ + สี", right_x + 28, 1909, 20, p["secondary"])

    for x in (left_x, right_x):
        c.rect(x, 1960, panel_width, 236, p["panel"], p["border"], 20)
    c.text("05 / RHYTHM + SHAPE", left_x + 28, 2001, 18, p["secondary"], True)
    c.text("4px rhythm", left_x + 28, 2037, 17, p["muted"])
    for index, value in enumerate((4, 8, 12, 16, 24, 32)):
        x = left_x + 28 + index * 57
        c.rect(x, 2110 - value * 1.5, max(8, value * 1.5), value * 1.5, p["accent"], radius=2)
        c.text(str(value), x + max(8, value * 1.5) / 2, 2136, 16, p["secondary"], anchor="center")
    c.text("Corner radii", left_x + 428, 2037, 17, p["muted"])
    for index, value in enumerate((8, 12, 16, 24)):
        x = left_x + 428 + index * 80
        c.rect(x, 2059, 52, 52, p["raised"], p["strong"], value)
        c.text(str(value), x + 26, 2093, 18, p["secondary"], anchor="center")
    c.text("Focus 2 + 3px   ·   Targets 44 / 36px   ·   Motion 0 / 100 / 160 / 240ms", left_x + 28, 2172, 16, p["muted"], max_width=734)

    c.text("06 / QUIET OVERLAY", right_x + 28, 2001, 18, p["secondary"], True)
    c.rect(right_x + 28, 2025, 734, 56, p["canvas"], p["strong"], 12)
    c.text("HP —  MP —", right_x + 46, 2061, 18, p["secondary"])
    c.text("ยังประเมินไม่ได้", right_x + 380, 2061, 18, p["secondary"], anchor="center")
    icon(c, "mic-off", right_x + 565, 2043, 20, p["muted"])
    c.text("ไมโครโฟนปิด", right_x + 596, 2061, 17, p["secondary"])
    c.text("อยู่ในเกม โดยไม่ขวางเกม", right_x + 28, 2130, 26, p["primary"], True)
    c.text("Passive placement study · native HUD clearance pending", right_x + 28, 2172, 17, p["muted"], max_width=734)

    c.line([(margin, 2230), (end, 2230)], p["border"])
    c.text("EXACT TOKEN COLOURS · AUTHORED GEOMETRY · NOT AN APP CAPTURE", margin, 2265, 16, p["secondary"])
    c.text("11 NOV 2026  /  NIGHTGLASS 1.0", end, 2265, 16, p["muted"], anchor="right")

    inputs = [TOKENS_PATH, Path(__file__), ASSETS / "kohbor-mark.svg", ASSETS / "kohbor-companion.svg", *sorted(FONTS.glob("*.ttf"))]
    manifest = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in inputs}
    source_hash = hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    metadata = PngImagePlugin.PngInfo()
    metadata.add_text("Title", "Kohbor / เกาะเบาะ — NIGHTGLASS technical design board")
    metadata.add_text("EvidenceClass", "static design study; design candidate 1.0; native NOT_RUN; not a screenshot")
    metadata.add_text("SourceSHA256", source_hash)
    metadata.add_text("SourceManifest", json.dumps(manifest, sort_keys=True, separators=(",", ":")))
    metadata.add_text("Renderer", f"Pillow {Image.__version__}; Raqm; deterministic vector/text drawing; {AA}x supersampling")
    rendered = image.convert("RGB").resize((WIDTH, HEIGHT), Image.Resampling.LANCZOS)
    buffer = io.BytesIO()
    rendered.save(buffer, format="PNG", pnginfo=metadata, compress_level=9, optimize=False, dpi=(144, 144))
    return buffer.getvalue(), {"dimensions": [WIDTH, HEIGHT], "sourceSha256": source_hash, "inputCount": len(manifest), "evidenceClass": "static", "native": "NOT_RUN", "map": {"width": dimension(document, "semantic.layout.live.map.width"), "height": dimension(document, "semantic.layout.live.map.height")}, "right": {"width": dimension(document, "semantic.layout.live.right.width"), "height": dimension(document, "semantic.layout.live.right.height")}}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="render and compare exact bytes without changing the poster")
    args = parser.parse_args()
    try:
        output, report = make_poster()
        if args.check:
            if not OUTPUT.is_file() or OUTPUT.read_bytes() != output:
                raise ValueError("Poster is stale/missing; run python tools/render_design_poster.py")
        else:
            OUTPUT.parent.mkdir(parents=True, exist_ok=True)
            OUTPUT.write_bytes(output)
        report["status"] = "PASS"
        report["action"] = "checked" if args.check else "rendered"
        report["output"] = str(OUTPUT.relative_to(ROOT))
        report["pngSha256"] = hashlib.sha256(output).hexdigest()
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0
    except (ValueError, OSError, RuntimeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
