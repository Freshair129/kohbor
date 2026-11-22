export type ObjectiveId = "water_rune" | "power_rune" | "bounty_rune" | "wisdom_rune" | "tormentor";
export type ObjectiveAlertPhase = "pre_warn_30" | "pre_warn_15" | "spawn";

export interface ObjectiveAlertItem {
  objective: ObjectiveId;
  phase: ObjectiveAlertPhase;
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

export type ObjectiveAlertLabels = Readonly<
  Record<ObjectiveId, Readonly<Partial<Record<ObjectiveAlertPhase, string>>>>
>;

export const OBJECTIVE_ALERT_AUTO_HIDE_MS = 3_000;
const MAX_ALERT_ITEMS = 16;
const OBJECTIVES: readonly ObjectiveId[] = ["water_rune", "power_rune", "bounty_rune", "wisdom_rune", "tormentor"];
const PHASES: readonly ObjectiveAlertPhase[] = ["pre_warn_30", "pre_warn_15", "spawn"];

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function nonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  return Object.keys(value).every((key) => allowed.includes(key));
}

export function parseObjectiveAlertBatch(value: unknown): ObjectiveAlertBatch | null {
  const batch = record(value);
  if (!batch || !hasOnlyKeys(batch, ["schema_version", "observed_clock_seconds", "alerts"])
    || batch.schema_version !== 1 || !nonNegativeInteger(batch.observed_clock_seconds)
    || !Array.isArray(batch.alerts) || batch.alerts.length === 0 || batch.alerts.length > MAX_ALERT_ITEMS) return null;

  const seen = new Set<string>();
  const alerts: ObjectiveAlertItem[] = [];
  for (const value of batch.alerts) {
    const item = record(value);
    if (!item || !hasOnlyKeys(item, ["objective", "phase", "occurrence_seconds", "due_seconds"])
      || typeof item.objective !== "string" || !OBJECTIVES.includes(item.objective as ObjectiveId)
      || typeof item.phase !== "string" || !PHASES.includes(item.phase as ObjectiveAlertPhase)
      || !nonNegativeInteger(item.occurrence_seconds) || !nonNegativeInteger(item.due_seconds)) return null;
    const objective = item.objective as ObjectiveId;
    const phase = item.phase as ObjectiveAlertPhase;
    const occurrence = item.occurrence_seconds;
    const due = item.due_seconds;
    if ((phase !== "spawn" && objective !== "water_rune" && objective !== "power_rune")
      || (phase === "spawn" ? due !== occurrence
        : phase === "pre_warn_30" ? due !== occurrence - 30 : due !== occurrence - 15)) return null;
    const key = `${objective}:${occurrence}:${phase}`;
    if (seen.has(key)) return null;
    seen.add(key);
    alerts.push({ objective, phase, occurrence_seconds: occurrence, due_seconds: due });
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

export function objectiveAlertText(
  state: ObjectiveAlertState,
  labels: ObjectiveAlertLabels,
): string | null {
  if (!state.batch) return null;
  return state.batch.alerts
    .map((alert) => labels[alert.objective][alert.phase] ?? "")
    .join(" · ");
}

/** The alert occupies its own layer and leaves a playing video banner present. */
export function objectiveOverlayLayers(
  state: ObjectiveAlertState,
  nowMs: number,
  hasVideoBanner: boolean,
): { textAlert: boolean; videoBanner: boolean; visible: boolean } {
  const textAlert = objectiveAlertVisible(state, nowMs);
  return { textAlert, videoBanner: hasVideoBanner, visible: textAlert || hasVideoBanner };
}
