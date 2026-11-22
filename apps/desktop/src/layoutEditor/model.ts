export type LayoutWidgetId = "objective_card";

export interface WidgetLayout {
  x_percent: number;
  y_percent: number;
  scale: number;
}

export interface LayoutDocument {
  schema_version: 1;
  widgets: Record<LayoutWidgetId, WidgetLayout>;
}

export interface LayoutViewport {
  width: number;
  height: number;
}

interface ProtectedRegion {
  id: string;
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export const OBJECTIVE_CARD_LAYOUT_DEFAULT: Readonly<WidgetLayout> = {
  x_percent: 62.5,
  y_percent: 6.5,
  scale: 1,
};

export const OBJECTIVE_CARD_BASE_WIDTH_PX = 272;
export const OBJECTIVE_CARD_BASE_HEIGHT_PX = 32;
export const LAYOUT_SAFE_EDGE_PX = 8;
export const LAYOUT_SCALE_BOUNDS = [0.75, 2] as const;
export const LAYOUT_PRESETS = [0.75, 1, 1.25, 1.5] as const;
export const LAYOUT_WIDGET_REGISTRY: readonly { id: LayoutWidgetId; default: Readonly<WidgetLayout> }[] = [
  { id: "objective_card", default: OBJECTIVE_CARD_LAYOUT_DEFAULT },
];

const PROTECTED_REGIONS: readonly ProtectedRegion[] = [
  { id: "minimap", left: 0, top: 70, right: 20, bottom: 100 },
  { id: "bottom_hud", left: 0, top: 78, right: 100, bottom: 100 },
  { id: "top_bar", left: 0, top: 0, right: 100, bottom: 5 },
  { id: "announcement", left: 30, top: 8, right: 70, bottom: 19 },
];

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function boundsFor(scale: number, viewport: LayoutViewport) {
  const widthScale = viewport.width / 1920;
  const widthPercent = (OBJECTIVE_CARD_BASE_WIDTH_PX * widthScale * scale / viewport.width) * 100;
  const heightPercent = (OBJECTIVE_CARD_BASE_HEIGHT_PX * widthScale * scale / viewport.height) * 100;
  const marginX = LAYOUT_SAFE_EDGE_PX / viewport.width * 100;
  const marginY = LAYOUT_SAFE_EDGE_PX / viewport.height * 100;
  return {
    halfWidth: widthPercent / 2,
    halfHeight: heightPercent / 2,
    minX: marginX + widthPercent / 2,
    maxX: 100 - marginX - widthPercent / 2,
    minY: marginY + heightPercent / 2,
    maxY: 100 - marginY - heightPercent / 2,
  };
}

function intersects(layout: WidgetLayout, region: ProtectedRegion, halfWidth: number, halfHeight: number): boolean {
  return layout.x_percent - halfWidth < region.right
    && layout.x_percent + halfWidth > region.left
    && layout.y_percent - halfHeight < region.bottom
    && layout.y_percent + halfHeight > region.top;
}

function collides(layout: WidgetLayout, halfWidth: number, halfHeight: number): boolean {
  return PROTECTED_REGIONS.some((region) => intersects(layout, region, halfWidth, halfHeight));
}

/**
 * Clamp the card's screen-centre coordinates to the 8 px edge and safe regions.
 * When it overlaps a protected region, select the closest collision-free point.
 */
export function clampObjectiveWidget(
  requested: WidgetLayout,
  viewport: LayoutViewport = { width: 1920, height: 1080 },
): WidgetLayout {
  const width = Math.max(1, viewport.width);
  const height = Math.max(1, viewport.height);
  const safeViewport = { width, height };
  const scale = clamp(
    Number.isFinite(requested.scale) ? requested.scale : OBJECTIVE_CARD_LAYOUT_DEFAULT.scale,
    LAYOUT_SCALE_BOUNDS[0],
    LAYOUT_SCALE_BOUNDS[1],
  );
  const bounds = boundsFor(scale, safeViewport);
  const start = {
    x_percent: clamp(Number.isFinite(requested.x_percent) ? requested.x_percent : OBJECTIVE_CARD_LAYOUT_DEFAULT.x_percent, bounds.minX, bounds.maxX),
    y_percent: clamp(Number.isFinite(requested.y_percent) ? requested.y_percent : OBJECTIVE_CARD_LAYOUT_DEFAULT.y_percent, bounds.minY, bounds.maxY),
    scale,
  };

  const xCandidates = [bounds.minX, bounds.maxX, start.x_percent];
  const yCandidates = [bounds.minY, bounds.maxY, start.y_percent];
  for (const region of PROTECTED_REGIONS) {
    xCandidates.push(
      clamp(region.left - bounds.halfWidth, bounds.minX, bounds.maxX),
      clamp(region.right + bounds.halfWidth, bounds.minX, bounds.maxX),
    );
    yCandidates.push(
      clamp(region.top - bounds.halfHeight, bounds.minY, bounds.maxY),
      clamp(region.bottom + bounds.halfHeight, bounds.minY, bounds.maxY),
    );
  }

  let closest: WidgetLayout | null = null;
  let closestDistance = Number.POSITIVE_INFINITY;
  for (const x of xCandidates) {
    for (const y of yCandidates) {
      const candidate = { x_percent: x, y_percent: y, scale };
      if (collides(candidate, bounds.halfWidth, bounds.halfHeight)) continue;
      const dx = (x - start.x_percent) * width / 100;
      const dy = (y - start.y_percent) * height / 100;
      const distance = dx * dx + dy * dy;
      if (distance < closestDistance) {
        closest = candidate;
        closestDistance = distance;
      }
    }
  }

  return closest ?? { ...OBJECTIVE_CARD_LAYOUT_DEFAULT, scale };
}

export function parseLayoutDocument(value: unknown, viewport?: LayoutViewport): LayoutDocument | null {
  if (!record(value) || Object.keys(value).length !== 2 || value.schema_version !== 1 || !record(value.widgets)) return null;
  const widgets = value.widgets;
  if (Object.keys(widgets).length !== 1 || !Object.hasOwn(widgets, "objective_card")) return null;
  const entry = widgets.objective_card;
  if (!record(entry) || Object.keys(entry).length !== 3
    || typeof entry.x_percent !== "number" || !Number.isFinite(entry.x_percent)
    || typeof entry.y_percent !== "number" || !Number.isFinite(entry.y_percent)
    || typeof entry.scale !== "number" || !Number.isFinite(entry.scale)) return null;
  return {
    schema_version: 1,
    widgets: {
      objective_card: clampObjectiveWidget({
        x_percent: entry.x_percent,
        y_percent: entry.y_percent,
        scale: entry.scale,
      }, viewport),
    },
  };
}

export function defaultLayoutDocument(): LayoutDocument {
  return { schema_version: 1, widgets: { objective_card: { ...OBJECTIVE_CARD_LAYOUT_DEFAULT } } };
}
