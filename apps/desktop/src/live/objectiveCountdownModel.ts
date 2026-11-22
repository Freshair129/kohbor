// Rune countdowns on the pinned minimap (UI-018; owner request 2026-11-23).
// The timing table and scheduler belong to ALERT-008 on the host. This module only reads a schedule snapshot the host
// publishes (next occurrence per objective, its configured lead times and the GSI game clock it was computed at) and
// formats it. It holds no spawn times and never uses the wall clock as the game clock.

/** Objectives the minimap labels. Tormentor and Roshan are not labelled: their respawn timing is unknown. */
export const COUNTDOWN_OBJECTIVES = ["water_rune", "power_rune", "bounty_rune", "wisdom_rune"] as const;
export type CountdownObjective = (typeof COUNTDOWN_OBJECTIVES)[number];

export interface ScheduledObjective {
  objective: CountdownObjective;
  /** GSI game-clock second of the next known occurrence. */
  occurrence_seconds: number;
  /** Lead times from the objective Settings (ALERT-008 prefs), 5..60 s, at most two. Empty means none set. */
  lead_s: number[];
}

/**
 * Proposed host read-model `objective://schedule` (schema 1). Not produced on this branch; ALERT-008 owns the producer.
 * Objectives whose next occurrence is not known are simply absent.
 */
export interface ObjectiveSchedule {
  schema_version: 1;
  observed_clock_seconds: number;
  upcoming: ScheduledObjective[];
}

const MAX_ITEMS = 8;

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}
const int = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value);
const onlyKeys = (value: Record<string, unknown>, keys: readonly string[]) => Object.keys(value).every(key => keys.includes(key));

/** Strict parser: anything unexpected yields null (no countdowns), never a partial guess. */
export function parseObjectiveSchedule(value: unknown): ObjectiveSchedule | null {
  const root = record(value);
  if (!root || !onlyKeys(root, ["schema_version", "observed_clock_seconds", "upcoming"]) || root.schema_version !== 1
    || !int(root.observed_clock_seconds) || root.observed_clock_seconds < -600
    || !Array.isArray(root.upcoming) || root.upcoming.length > MAX_ITEMS) return null;
  const seen = new Set<string>();
  const upcoming: ScheduledObjective[] = [];
  for (const raw of root.upcoming) {
    const item = record(raw);
    if (!item || !onlyKeys(item, ["objective", "occurrence_seconds", "lead_s"])
      || typeof item.objective !== "string" || !(COUNTDOWN_OBJECTIVES as readonly string[]).includes(item.objective)
      || !int(item.occurrence_seconds) || item.occurrence_seconds < 0
      || !Array.isArray(item.lead_s) || item.lead_s.length > 2
      || !item.lead_s.every(lead => int(lead) && lead >= 5 && lead <= 60)
      || new Set(item.lead_s).size !== item.lead_s.length
      || seen.has(item.objective)) return null;
    seen.add(item.objective);
    upcoming.push({ objective: item.objective as CountdownObjective, occurrence_seconds: item.occurrence_seconds, lead_s: [...item.lead_s] as number[] });
  }
  return { schema_version: 1, observed_clock_seconds: root.observed_clock_seconds, upcoming };
}

/**
 * When no lead time is set for an objective, the label switches to seconds in the last minute.
 * Assumption recorded in UI-018; the owner can change it.
 */
export const SECONDS_WINDOW_WITHOUT_LEAD = 60;

export interface CountdownLabel { objective: CountdownObjective; text: string; mode: "minutes" | "seconds" | "now"; remaining: number }

/**
 * Coarse minutes while far ("3m", "2m", "1m"), then seconds from the largest configured lead time ("30", "29" …),
 * "0" at the spawn second. Past occurrences produce no label (the host moves to the next occurrence).
 */
export function countdownLabel(item: ScheduledObjective, clockSeconds: number): CountdownLabel | null {
  const remaining = item.occurrence_seconds - clockSeconds;
  if (!Number.isFinite(remaining) || remaining < 0) return null;
  const window = item.lead_s.length > 0 ? Math.max(...item.lead_s) : SECONDS_WINDOW_WITHOUT_LEAD;
  if (remaining === 0) return { objective: item.objective, text: "0", mode: "now", remaining };
  if (remaining <= window) return { objective: item.objective, text: String(remaining), mode: "seconds", remaining };
  return { objective: item.objective, text: `${Math.max(1, Math.floor(remaining / 60))}m`, mode: "minutes", remaining };
}

/** A schedule older than this (wall time since it arrived) is treated as unknown: the labels disappear. */
export const SCHEDULE_STALE_MS = 5_000;

export function countdownLabels(schedule: ObjectiveSchedule | null, receivedAtMs: number | null, nowMs: number): CountdownLabel[] {
  if (!schedule || receivedAtMs === null || nowMs - receivedAtMs > SCHEDULE_STALE_MS || nowMs < receivedAtMs) return [];
  return schedule.upcoming.map(item => countdownLabel(item, schedule.observed_clock_seconds)).filter((label): label is CountdownLabel => label !== null);
}

/**
 * Original marker spots in minimap coordinates (0..1, x to the right, y down, Radiant base bottom-left).
 * APPROXIMATE, patch 7.42, not confirmed by the owner: placement only, never a measurement.
 * Water runes spawn at the river rune spots, so they share the power-rune spots.
 */
export const RUNE_SPOTS: Readonly<Record<CountdownObjective, readonly { x: number; y: number }[]>> = {
  power_rune: [{ x: 0.335, y: 0.355 }, { x: 0.665, y: 0.655 }],
  water_rune: [{ x: 0.335, y: 0.355 }, { x: 0.665, y: 0.655 }],
  bounty_rune: [{ x: 0.255, y: 0.555 }, { x: 0.575, y: 0.835 }, { x: 0.425, y: 0.165 }, { x: 0.745, y: 0.445 }],
  wisdom_rune: [{ x: 0.075, y: 0.505 }, { x: 0.925, y: 0.495 }],
};
export const RUNE_SPOTS_CONFIRMED = false;

export interface SpotLabel { key: string; x: number; y: number; label: CountdownLabel }

/** One label per spot: when two objectives share a spot (water and power), the sooner one is shown. */
export function spotLabels(labels: CountdownLabel[]): SpotLabel[] {
  const bySpot = new Map<string, SpotLabel>();
  for (const label of labels) {
    for (const spot of RUNE_SPOTS[label.objective]) {
      const key = `${spot.x}:${spot.y}`;
      const current = bySpot.get(key);
      if (!current || label.remaining < current.label.remaining) bySpot.set(key, { key, x: spot.x, y: spot.y, label });
    }
  }
  return [...bySpot.values()];
}

export function objectiveShortName(objective: CountdownObjective, locale: "th" | "en"): string {
  const th = locale === "th";
  switch (objective) {
    case "water_rune": return th ? "รูนน้ำ" : "Water";
    case "power_rune": return th ? "รูนพลัง" : "Power";
    case "bounty_rune": return th ? "รูนทอง" : "Bounty";
    case "wisdom_rune": return th ? "รูนปัญญา" : "Wisdom";
  }
}
