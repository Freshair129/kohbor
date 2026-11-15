// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { dangerTone } from "./live/meter.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { RiskView } from "./live/types.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { hudCopy } from "./hudCopy.ts";

// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
export { hudCopy } from "./hudCopy.ts";

export type HudAvailability = "fresh" | "stale" | "unknown" | "invalid" | "unsupported";
export interface HudField {
  value: number | null;
  availability: HudAvailability;
}

/** Net worth is derived by the host (gold + item costs); `approximate` = some item had no known price. */
export interface HudNetWorth extends HudField {
  approximate: boolean;
}

export interface HudState {
  schema: 1;
  visible: boolean;
  fields: {
    kills: HudField;
    deaths: HudField;
    assists: HudField;
    gold: HudField;
    gpm: HudField;
    xpm: HudField;
  };
  net_worth: HudNetWorth;
}

export type HudModuleId = "gmeter" | "kda" | "gold" | "gpm" | "nw" | "xpm";
export interface HudLayoutItem {
  id: HudModuleId;
  x_percent: number;
  y_percent: number;
  scale: number;
  enabled: boolean;
}

export const HUD_DEFAULT_LAYOUT = [
  { id: "gmeter", x_percent: 50, y_percent: 5.5, scale: 0.65, enabled: true },
  { id: "kda", x_percent: 9.64, y_percent: 2.2, scale: 0.78, enabled: true },
  { id: "gold", x_percent: 13.56, y_percent: 2.2, scale: 0.78, enabled: true },
  { id: "gpm", x_percent: 17.23, y_percent: 2.2, scale: 0.78, enabled: true },
  { id: "nw", x_percent: 21.02, y_percent: 2.2, scale: 0.78, enabled: true },
  { id: "xpm", x_percent: 24.82, y_percent: 2.2, scale: 0.78, enabled: true },
] as const satisfies readonly HudLayoutItem[];

const FIELD_NAMES = ["kills", "deaths", "assists", "gold", "gpm", "xpm"] as const;
const AVAILABILITIES = new Set<HudAvailability>(["fresh", "stale", "unknown", "invalid", "unsupported"]);

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const ownKeys = Object.keys(value);
  return ownKeys.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function unknownField(): HudField {
  return { value: null, availability: "unknown" };
}

export function unknownHudState(visible = false): HudState {
  return {
    schema: 1,
    visible,
    fields: {
      kills: unknownField(),
      deaths: unknownField(),
      assists: unknownField(),
      gold: unknownField(),
      gpm: unknownField(),
      xpm: unknownField(),
    },
    net_worth: { ...unknownField(), approximate: false },
  };
}

function parseField(input: unknown): HudField | null {
  if (!record(input) || !exactKeys(input, ["value", "availability"])) return null;
  if (typeof input.availability !== "string" || !AVAILABILITIES.has(input.availability as HudAvailability)) return null;
  if (input.value !== null && typeof input.value !== "number") return null;
  if (typeof input.value === "number" && (!Number.isInteger(input.value) || input.value < 0)) return unknownField();
  return { value: input.value, availability: input.availability as HudAvailability };
}

function parseNetWorth(input: unknown): HudNetWorth | null {
  if (!record(input) || !exactKeys(input, ["value", "availability", "approximate"]) || typeof input.approximate !== "boolean") return null;
  const field = parseField({ value: input.value, availability: input.availability });
  if (field === null) return null;
  return { ...field, approximate: field.value === null ? false : input.approximate };
}

/** Parse the closed host DTO. Bad shape invalidates every value; bad numeric range invalidates one field. */
export function parseHudState(input: unknown): HudState {
  if (!record(input)) return unknownHudState(true);
  const visible = typeof input.visible === "boolean" ? input.visible : true;
  if (!exactKeys(input, ["schema", "visible", "fields", "net_worth"]) || input.schema !== 1 || typeof input.visible !== "boolean") {
    return unknownHudState(visible);
  }
  if (!record(input.fields) || !exactKeys(input.fields, FIELD_NAMES)) return unknownHudState(visible);

  const fields = {} as HudState["fields"];
  for (const name of FIELD_NAMES) {
    const field = parseField(input.fields[name]);
    if (field === null) return unknownHudState(visible);
    fields[name] = field;
  }
  const netWorth = parseNetWorth(input.net_worth);
  if (netWorth === null) return unknownHudState(visible);
  return { schema: 1, visible, fields, net_worth: netWorth };
}

export interface MeterModel {
  level: 0 | 1 | 2 | 3 | null;
  ticksLit: number;
  tone: "unknown" | "none" | "ally" | "warn" | "critical";
  label: string;
  blind: boolean;
}

function blindMeter(): MeterModel {
  return { level: null, ticksLit: 0, tone: "unknown", label: hudCopy.meterLabels.unavailable, blind: true };
}

function validLevel(value: unknown): MeterModel["level"] {
  return Number.isInteger(value) && typeof value === "number" && value >= 0 && value <= 3
    ? value as 0 | 1 | 2 | 3
    : null;
}

function levelTone(level: 0 | 1 | 2 | 3): MeterModel["tone"] {
  const danger = dangerTone(level);
  if (danger === "none") return "none";
  if (danger === "critical") return "critical";
  return level === 1 ? "ally" : "warn";
}

/** Read existing admitted risk state; this maps presentation only and never recalculates risk. */
export function meterModel(riskView: RiskView | null): MeterModel {
  const state = riskView?.state;
  const level = validLevel(state?.meter);
  const events = state?.events;
  const activeAlert = (Array.isArray(events) && events.includes("risk_alert"))
    || riskView?.lastVisionEvent === "risk_alert"
    || level === 3;

  // A previously admitted alert or danger remains salient while sources report blind.
  if (activeAlert) {
    return { level: 3, ticksLit: 4, tone: "critical", label: hudCopy.meterLabels.danger, blind: false };
  }
  if (!state || state.gsi_source !== true || state.vision_source !== true || level === null) return blindMeter();

  const label = level === 0
    ? hudCopy.meterLabels.none
    : level === 1
      ? hudCopy.meterLabels.unseen
      : level === 2
        ? hudCopy.meterLabels.risk
        : hudCopy.meterLabels.danger;
  return {
    level,
    // Like the reference meter: segments 0..level are lit, so level 0 lights the first (grey) one.
    ticksLit: level + 1,
    tone: levelTone(level),
    label,
    blind: false,
  };
}

export type ChipKind = "kda" | "gold" | "gpm" | "nw" | "xpm";
export interface ChipDisplay {
  text: string;
  stale: boolean;
}
export type KdaFields = Pick<HudState["fields"], "kills" | "deaths" | "assists">;

function knownValue(input: unknown): { value: number; stale: boolean } | null {
  if (!record(input) || (input.availability !== "fresh" && input.availability !== "stale")) return null;
  if (typeof input.value !== "number" || !Number.isInteger(input.value) || input.value < 0) return null;
  return { value: input.value, stale: input.availability === "stale" };
}

function numberText(value: number): string {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 }).format(value);
}

export function formatChip(kind: ChipKind, input: HudField | KdaFields | unknown): ChipDisplay {
  if (kind === "kda") {
    if (!record(input)) return { text: "—/—/—", stale: false };
    const parts = [input.kills, input.deaths, input.assists].map(knownValue);
    return {
      text: parts.map((part) => part === null ? "—" : String(part.value)).join("/"),
      stale: parts.some((part) => part?.stale === true),
    };
  }

  const parsed = knownValue(input);
  if (parsed === null) return { text: "—", stale: false };
  // Net worth is derived by the host; `approximate` (an item without a known price) shows a leading "~".
  const approximate = kind === "nw" && record(input) && input.approximate === true;
  return { text: `${approximate ? "~" : ""}${numberText(parsed.value)}`, stale: parsed.stale };
}

/** Shared render gate lets the mounted HUD keep its listeners while drawing nothing when hidden. */
export function renderWhenHudVisible<T>(visible: boolean, render: () => T): T | null {
  return visible ? render() : null;
}
