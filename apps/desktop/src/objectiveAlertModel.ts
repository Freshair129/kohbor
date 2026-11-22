import type { Locale } from "./copy";

export type ObjectiveId = "water_rune" | "power_rune" | "bounty_rune" | "wisdom_rune" | "tormentor";
export type ObjectiveAlertPhase = "pre_warn" | "spawn";

export interface ObjectiveAlertItem {
  objective: ObjectiveId;
  phase: ObjectiveAlertPhase;
  lead_s: number | null;
  occurrence_seconds: number;
  due_seconds: number;
}

export interface ObjectiveAlertBatch {
  schema_version: 1;
  observed_clock_seconds: number;
  alerts: ObjectiveAlertItem[];
}

export interface ObjectiveAlertState {
  batch: ObjectiveAlertBatch | null;
  shownUntilMs: number | null;
  serial: number;
  malformedEventCount: number;
}

export interface ObjectiveCardContent {
  objective: ObjectiveId;
  sentence: string;
  additionalCount: number;
}

export const OBJECTIVE_ALERT_AUTO_HIDE_MS = 3_000;
const MAX_ALERT_ITEMS = 16;
const OBJECTIVES: readonly ObjectiveId[] = ["water_rune", "power_rune", "bounty_rune", "wisdom_rune", "tormentor"];
const PHASES: readonly ObjectiveAlertPhase[] = ["pre_warn", "spawn"];

const OBJECTIVE_ENGLISH_NAMES: Readonly<Record<ObjectiveId, string>> = {
  water_rune: "Water rune",
  power_rune: "Power rune",
  bounty_rune: "Bounty rune",
  wisdom_rune: "Wisdom rune",
  tormentor: "Tormentor",
};

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function safeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value);
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  return Object.keys(value).every((key) => allowed.includes(key));
}

export function parseObjectiveAlertBatch(value: unknown): ObjectiveAlertBatch | null {
  const batch = record(value);
  if (!batch || !hasOnlyKeys(batch, ["schema_version", "observed_clock_seconds", "alerts"])
    || batch.schema_version !== 1 || !safeInteger(batch.observed_clock_seconds)
    || batch.observed_clock_seconds < -600
    || !Array.isArray(batch.alerts) || batch.alerts.length === 0 || batch.alerts.length > MAX_ALERT_ITEMS) return null;

  const seen = new Set<string>();
  const alerts: ObjectiveAlertItem[] = [];
  for (const value of batch.alerts) {
    const item = record(value);
    if (!item || !hasOnlyKeys(item, ["objective", "phase", "lead_s", "occurrence_seconds", "due_seconds"])
      || typeof item.objective !== "string" || !OBJECTIVES.includes(item.objective as ObjectiveId)
      || typeof item.phase !== "string" || !PHASES.includes(item.phase as ObjectiveAlertPhase)
      || !safeInteger(item.occurrence_seconds) || item.occurrence_seconds < 0
      || !safeInteger(item.due_seconds) || item.due_seconds < -600) return null;

    const objective = item.objective as ObjectiveId;
    const phase = item.phase as ObjectiveAlertPhase;
    const occurrence = item.occurrence_seconds;
    const due = item.due_seconds;
    const leadS = item.lead_s;
    if (phase === "spawn") {
      if (leadS !== null || due !== occurrence) return null;
    } else if (typeof leadS !== "number" || !Number.isInteger(leadS) || leadS < 5 || leadS > 60
      || due !== occurrence - leadS) {
      return null;
    }

    const key = `${objective}:${occurrence}:${phase}:${leadS ?? "spawn"}`;
    if (seen.has(key)) return null;
    seen.add(key);
    alerts.push({
      objective,
      phase,
      lead_s: leadS as number | null,
      occurrence_seconds: occurrence,
      due_seconds: due,
    });
  }

  return {
    schema_version: 1,
    observed_clock_seconds: batch.observed_clock_seconds,
    alerts,
  };
}

export function createObjectiveAlertState(): ObjectiveAlertState {
  return { batch: null, shownUntilMs: null, serial: 0, malformedEventCount: 0 };
}

export function receiveObjectiveAlert(state: ObjectiveAlertState, value: unknown, nowMs: number): ObjectiveAlertState {
  const batch = parseObjectiveAlertBatch(value);
  if (!batch) return { ...state, malformedEventCount: state.malformedEventCount + 1 };
  return {
    ...state,
    batch,
    shownUntilMs: nowMs + OBJECTIVE_ALERT_AUTO_HIDE_MS,
    serial: state.serial + 1,
  };
}

export function objectiveAlertVisible(state: ObjectiveAlertState, nowMs: number): boolean {
  return state.batch !== null && state.shownUntilMs !== null && nowMs < state.shownUntilMs;
}

export function objectiveEnglishName(objective: ObjectiveId): string {
  return OBJECTIVE_ENGLISH_NAMES[objective];
}

export function objectiveAlertSentence(alert: ObjectiveAlertItem, locale: Locale): string {
  const name = objectiveEnglishName(alert.objective);
  if (alert.phase === "spawn") return locale === "th" ? `${name} เกิดแล้ว` : `${name} spawned`;
  return locale === "th"
    ? `${name} จะเกิดใน ${alert.lead_s} วินาที`
    : `${name} spawns in ${alert.lead_s} s`;
}

export function objectiveCardContent(batch: ObjectiveAlertBatch | null, locale: Locale): ObjectiveCardContent | null {
  if (!batch || batch.alerts.length === 0) return null;
  const first = batch.alerts[0];
  if (!first) return null;
  return {
    objective: first.objective,
    sentence: objectiveAlertSentence(first, locale),
    additionalCount: Math.max(0, batch.alerts.length - 1),
  };
}
