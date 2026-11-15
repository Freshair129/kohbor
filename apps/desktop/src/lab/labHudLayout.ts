// Pure HUD layout editor model. Positions are module centres in screen percent; reference boxes
// are measured at 1920x1080 and scale uniformly with each module's scale value.
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { HUD_DEFAULT_STRIP_HEIGHT_PERCENT, HUD_LAYOUT_BOUNDS, parseHudLayoutModules, type HudLayoutBounds, type HudLayoutItem, type HudModuleId } from "../hudModel.ts";

export type LabHudModule = HudLayoutItem;
export type LabHudModules = readonly LabHudModule[];

export interface LabHudLayoutReply {
  modules: LabHudModule[];
  default: LabHudModule[];
  bounds: HudLayoutBounds;
  strip_height_percent: number;
  source: "embedded" | "override";
}

export interface LabHudPreviewReply {
  preview_until_ms: number;
}

export interface HudModuleBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export const HUD_REFERENCE_SCREEN = { width: 1920, height: 1080 } as const;
export const HUD_MODULE_REFERENCE_SIZES: Readonly<Record<HudModuleId, { width: number; height: number }>> = {
  gmeter: { width: 300, height: 36 },
  kda: { width: 90, height: 43 },
  gold: { width: 90, height: 43 },
  gpm: { width: 78, height: 43 },
  nw: { width: 96, height: 43 },
  xpm: { width: 78, height: 43 },
};

const EPSILON = 1e-6;

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const ownKeys = Object.keys(value);
  return ownKeys.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function finiteRange(value: unknown, min: number, max: number): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= min && value <= max;
}

function parseBounds(input: unknown): HudLayoutBounds | null {
  if (!isRecord(input) || !exactKeys(input, ["x_percent", "y_percent", "scale"])) return null;
  const expected = HUD_LAYOUT_BOUNDS;
  for (const key of ["x_percent", "y_percent", "scale"] as const) {
    const actual = input[key];
    const wanted = expected[key];
    if (!Array.isArray(actual) || actual.length !== 2 || actual[0] !== wanted[0] || actual[1] !== wanted[1]) return null;
  }
  return {
    x_percent: [0, 100],
    y_percent: [0, 100],
    scale: [0.2, 3.0],
  };
}

export function parseLabHudLayoutReply(input: unknown): LabHudLayoutReply | null {
  if (!isRecord(input) || !exactKeys(input, ["modules", "default", "bounds", "strip_height_percent", "source"])) return null;
  if (!finiteRange(input.strip_height_percent, HUD_DEFAULT_STRIP_HEIGHT_PERCENT, 100)) return null;
  if (input.source !== "embedded" && input.source !== "override") return null;
  const modules = parseHudLayoutModules(input.modules);
  const defaults = parseHudLayoutModules(input.default);
  const bounds = parseBounds(input.bounds);
  if (modules === null || defaults === null || bounds === null) return null;
  return { modules, default: defaults, bounds, strip_height_percent: input.strip_height_percent, source: input.source };
}

export function parseLabHudPreviewReply(input: unknown): LabHudPreviewReply | null {
  if (!isRecord(input) || !exactKeys(input, ["preview_until_ms"]) || !finiteRange(input.preview_until_ms, 0, Number.MAX_VALUE)) return null;
  return { preview_until_ms: input.preview_until_ms };
}

function clamp(value: number, min: number, max: number, fallback: number): number {
  const safe = Number.isFinite(value) ? value : fallback;
  return Math.round(Math.min(max, Math.max(min, safe)) * 100) / 100;
}

export function clampHudModule(module: LabHudModule, bounds: HudLayoutBounds = HUD_LAYOUT_BOUNDS): LabHudModule {
  return {
    ...module,
    x_percent: clamp(module.x_percent, bounds.x_percent[0], bounds.x_percent[1], 50),
    y_percent: clamp(module.y_percent, bounds.y_percent[0], bounds.y_percent[1], 50),
    scale: clamp(module.scale, bounds.scale[0], bounds.scale[1], 1),
  };
}

export function updateHudModule(modules: LabHudModules, id: HudModuleId, update: Partial<LabHudModule>, bounds: HudLayoutBounds = HUD_LAYOUT_BOUNDS): LabHudModule[] {
  return modules.map((module) => module.id === id ? clampHudModule({ ...module, ...update }, bounds) : { ...module });
}

export function dragHudModule(
  modules: LabHudModules,
  id: HudModuleId,
  deltaX: number,
  deltaY: number,
  canvasWidth: number,
  canvasHeight: number,
  bounds: HudLayoutBounds = HUD_LAYOUT_BOUNDS,
): LabHudModule[] {
  if (!(canvasWidth > 0) || !(canvasHeight > 0) || !Number.isFinite(deltaX) || !Number.isFinite(deltaY)) {
    return modules.map((module) => ({ ...module }));
  }
  const module = modules.find((row) => row.id === id);
  if (!module) return modules.map((row) => ({ ...row }));
  return updateHudModule(modules, id, {
    x_percent: module.x_percent + (deltaX / canvasWidth) * 100,
    y_percent: module.y_percent + (deltaY / canvasHeight) * 100,
  }, bounds);
}

export function nudgeHudModule(
  modules: LabHudModules,
  id: HudModuleId,
  key: string,
  shift: boolean,
  bounds: HudLayoutBounds = HUD_LAYOUT_BOUNDS,
): LabHudModule[] | null {
  const step = shift ? 1 : 0.1;
  const module = modules.find((row) => row.id === id);
  if (!module) return null;
  const delta = key === "ArrowLeft" ? { x_percent: module.x_percent - step }
    : key === "ArrowRight" ? { x_percent: module.x_percent + step }
      : key === "ArrowUp" ? { y_percent: module.y_percent - step }
        : key === "ArrowDown" ? { y_percent: module.y_percent + step }
          : null;
  return delta === null ? null : updateHudModule(modules, id, delta, bounds);
}

export function hudModuleBox(module: LabHudModule): HudModuleBox {
  const size = HUD_MODULE_REFERENCE_SIZES[module.id];
  const width = size.width * module.scale;
  const height = size.height * module.scale;
  return {
    x: (module.x_percent / 100) * HUD_REFERENCE_SCREEN.width - width / 2,
    y: (module.y_percent / 100) * HUD_REFERENCE_SCREEN.height - height / 2,
    width,
    height,
  };
}

/** Matches the host's strip rule: max(9, enabled module centre Y + 3.5), capped at 100. */
export function computeHudStripHeightPercent(modules: LabHudModules): number {
  const bottomRule = modules.reduce(
    (height, module) => module.enabled ? Math.max(height, module.y_percent + 3.5) : height,
    HUD_DEFAULT_STRIP_HEIGHT_PERCENT,
  );
  return Math.min(100, bottomRule);
}

export function modulesOutsideHudStrip(modules: LabHudModules, stripHeightPercent = computeHudStripHeightPercent(modules)): HudModuleId[] {
  const stripBottomPixels = (stripHeightPercent / 100) * HUD_REFERENCE_SCREEN.height;
  return modules
    .filter((module) => module.enabled && hudModuleBox(module).y + hudModuleBox(module).height > stripBottomPixels + EPSILON)
    .map((module) => module.id);
}

export interface HudModuleOverlap {
  first: HudModuleId;
  second: HudModuleId;
}

export function overlappingHudModules(modules: LabHudModules): HudModuleOverlap[] {
  const enabled = modules.filter((module) => module.enabled);
  const overlaps: HudModuleOverlap[] = [];
  for (let first = 0; first < enabled.length; first += 1) {
    const a = enabled[first]!;
    const aBox = hudModuleBox(a);
    for (let second = first + 1; second < enabled.length; second += 1) {
      const b = enabled[second]!;
      const bBox = hudModuleBox(b);
      const width = Math.min(aBox.x + aBox.width, bBox.x + bBox.width) - Math.max(aBox.x, bBox.x);
      const height = Math.min(aBox.y + aBox.height, bBox.y + bBox.height) - Math.max(aBox.y, bBox.y);
      if (width > EPSILON && height > EPSILON) overlaps.push({ first: a.id, second: b.id });
    }
  }
  return overlaps;
}

export function sameHudLayout(a: LabHudModules, b: LabHudModules): boolean {
  if (a.length !== b.length) return false;
  const byId = new Map(b.map((module) => [module.id, module]));
  return a.every((module) => {
    const other = byId.get(module.id);
    return other !== undefined
      && module.x_percent === other.x_percent
      && module.y_percent === other.y_percent
      && module.scale === other.scale
      && module.enabled === other.enabled;
  });
}

export interface HudLayoutEditorState {
  confirmed: LabHudLayoutReply;
  draft: LabHudModule[];
  error: string | null;
}

export function createHudLayoutEditorState(confirmed: LabHudLayoutReply): HudLayoutEditorState {
  return { confirmed, draft: confirmed.modules.map((module) => ({ ...module })), error: null };
}

export function changeHudLayoutDraft(state: HudLayoutEditorState, draft: LabHudModules): HudLayoutEditorState {
  return { ...state, draft: draft.map((module) => ({ ...module })), error: null };
}

export function confirmHudLayout(state: HudLayoutEditorState, confirmed: LabHudLayoutReply): HudLayoutEditorState {
  return createHudLayoutEditorState(confirmed);
}

export function failHudLayoutAction(state: HudLayoutEditorState, error: string): HudLayoutEditorState {
  return { ...state, error };
}

export function hudLayoutHasUnsavedChanges(state: HudLayoutEditorState): boolean {
  return !sameHudLayout(state.draft, state.confirmed.modules);
}
