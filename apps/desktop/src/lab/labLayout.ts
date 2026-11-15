// Pure model of the overlay editor (docs/design/KOHBOR-LAB.md, stage L3). Geometry of the
// banner window comes from the host (`lab_layout_preview` runs the real placement function);
// this file holds only the editing math: clamping, dragging, nudging, overlap and parsing.

export interface BannerLayout {
  x_percent: number;
  y_percent: number;
  scale: number;
  enabled: boolean;
}

export interface Bounds {
  x_percent: [number, number];
  y_percent: [number, number];
  scale: [number, number];
}

export interface HudZone {
  name: string;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface LayoutView {
  banner: BannerLayout;
  default: BannerLayout;
  bounds: Bounds;
  hudZones: HudZone[];
  source: "embedded" | "override";
  overridePath: string | null;
  applies: string;
}

export interface Preview {
  disabled: boolean;
  window: Rect | null;
  stage: Rect | null;
}

const EPSILON = 1e-6;
const NUDGE_STEP = 0.1;
const NUDGE_STEP_SHIFT = 1;

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

function clampTo(value: number, range: [number, number], fallback: number): number {
  const base = Number.isFinite(value) ? value : fallback;
  return round2(Math.min(range[1], Math.max(range[0], base)));
}

export function clampLayout(layout: BannerLayout, bounds: Bounds): BannerLayout {
  const middle = (range: [number, number]) => (range[0] + range[1]) / 2;
  return {
    x_percent: clampTo(layout.x_percent, bounds.x_percent, middle(bounds.x_percent)),
    y_percent: clampTo(layout.y_percent, bounds.y_percent, middle(bounds.y_percent)),
    scale: clampTo(layout.scale, bounds.scale, 1),
    enabled: layout.enabled,
  };
}

/** Pointer drag on a canvas of `canvasW` x `canvasH` pixels: pixel deltas become percent of the screen. */
export function dragLayout(
  start: BannerLayout,
  dxPx: number,
  dyPx: number,
  canvasW: number,
  canvasH: number,
  bounds: Bounds,
): BannerLayout {
  if (!(canvasW > 0) || !(canvasH > 0)) return start;
  return clampLayout(
    {
      ...start,
      x_percent: start.x_percent + (dxPx / canvasW) * 100,
      y_percent: start.y_percent + (dyPx / canvasH) * 100,
    },
    bounds,
  );
}

/** Arrow-key nudge: 0.1 percent, 1 percent with shift. `null` for any other key. */
export function nudgeLayout(
  layout: BannerLayout,
  key: string,
  shift: boolean,
  bounds: Bounds,
): BannerLayout | null {
  const step = shift ? NUDGE_STEP_SHIFT : NUDGE_STEP;
  switch (key) {
    case "ArrowLeft":
      return clampLayout({ ...layout, x_percent: layout.x_percent - step }, bounds);
    case "ArrowRight":
      return clampLayout({ ...layout, x_percent: layout.x_percent + step }, bounds);
    case "ArrowUp":
      return clampLayout({ ...layout, y_percent: layout.y_percent - step }, bounds);
    case "ArrowDown":
      return clampLayout({ ...layout, y_percent: layout.y_percent + step }, bounds);
    default:
      return null;
  }
}

/** Names of the protected HUD zones the stage rectangle overlaps (touching edges do not count). */
export function overlappingZones(
  stage: Rect,
  monitor: { w: number; h: number },
  zones: HudZone[],
): string[] {
  return zones
    .filter((zone) => {
      const x0 = zone.x0 * monitor.w;
      const x1 = zone.x1 * monitor.w;
      const y0 = zone.y0 * monitor.h;
      const y1 = zone.y1 * monitor.h;
      const width = Math.min(stage.x + stage.w, x1) - Math.max(stage.x, x0);
      const height = Math.min(stage.y + stage.h, y1) - Math.max(stage.y, y0);
      return width > EPSILON && height > EPSILON;
    })
    .map((zone) => zone.name);
}

/** A monitor-space rectangle in canvas pixels. */
export function rectOnCanvas(
  rect: Rect,
  monitorW: number,
  canvasW: number,
): { left: number; top: number; width: number; height: number } {
  if (!(monitorW > 0)) return { left: 0, top: 0, width: 0, height: 0 };
  const k = canvasW / monitorW;
  return { left: rect.x * k, top: rect.y * k, width: rect.w * k, height: rect.h * k };
}

export function sameLayout(a: BannerLayout, b: BannerLayout): boolean {
  return (
    a.x_percent === b.x_percent &&
    a.y_percent === b.y_percent &&
    a.scale === b.scale &&
    a.enabled === b.enabled
  );
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseBanner(value: unknown): BannerLayout | null {
  if (!isObject(value)) return null;
  const { x_percent, y_percent, scale, enabled } = value;
  if (
    typeof x_percent !== "number" ||
    typeof y_percent !== "number" ||
    typeof scale !== "number" ||
    typeof enabled !== "boolean"
  ) {
    return null;
  }
  return { x_percent, y_percent, scale, enabled };
}

function parseRange(value: unknown): [number, number] | null {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const [low, high] = value;
  return typeof low === "number" && typeof high === "number" ? [low, high] : null;
}

function parseRect(value: unknown): Rect | null {
  if (!isObject(value)) return null;
  const { x, y, w, h } = value;
  return [x, y, w, h].every((n) => typeof n === "number") ? { x: x as number, y: y as number, w: w as number, h: h as number } : null;
}

export function parseLayoutView(value: unknown): LayoutView | null {
  if (!isObject(value) || !isObject(value.bounds) || !Array.isArray(value.hud_zones)) return null;
  const banner = parseBanner(value.banner);
  const base = parseBanner(value.default);
  const x = parseRange(value.bounds.x_percent);
  const y = parseRange(value.bounds.y_percent);
  const scale = parseRange(value.bounds.scale);
  if (!banner || !base || !x || !y || !scale) return null;
  const zones: HudZone[] = [];
  for (const zone of value.hud_zones) {
    if (!isObject(zone) || typeof zone.name !== "string") return null;
    const { x0, y0, x1, y1 } = zone;
    if (![x0, y0, x1, y1].every((n) => typeof n === "number")) return null;
    zones.push({ name: zone.name, x0: x0 as number, y0: y0 as number, x1: x1 as number, y1: y1 as number });
  }
  if (value.source !== "embedded" && value.source !== "override") return null;
  return {
    banner,
    default: base,
    bounds: { x_percent: x, y_percent: y, scale },
    hudZones: zones,
    source: value.source,
    overridePath: typeof value.override_path === "string" ? value.override_path : null,
    applies: typeof value.applies === "string" ? value.applies : "",
  };
}

export function parsePreview(value: unknown): Preview | null {
  if (!isObject(value) || typeof value.disabled !== "boolean") return null;
  if (value.disabled) return { disabled: true, window: null, stage: null };
  const window = parseRect(value.window);
  const stage = parseRect(value.stage);
  return window && stage ? { disabled: false, window, stage } : null;
}
