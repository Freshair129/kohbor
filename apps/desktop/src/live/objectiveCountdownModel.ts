// Pinned map objective countdowns (UI-018; owner amendment 2026-11-24).
// Timing belongs to ALERT-008. This renderer holds no spawn table and uses only the admitted GSI clock.

// @ts-ignore Node's built-in TypeScript loader requires explicit source extensions for pure-model tests.
import { RUNE_SPOTS as REPLAY_RUNE_SPOTS, RUNE_SPOT_PROVENANCE } from "./runeSpotCoordinates.ts";

/** Objectives the minimap labels. Tormentor and Roshan are not labelled here. */
export const COUNTDOWN_OBJECTIVES = ["power_rune", "bounty_rune", "wisdom_rune", "lotus"] as const;
export type CountdownObjective = (typeof COUNTDOWN_OBJECTIVES)[number];
export type ScheduleObjective = CountdownObjective | "water_rune" | "tormentor" | "day_night";

export interface ScheduledObjective {
  objective: ScheduleObjective;
  /** GSI game-clock second of the next known occurrence. */
  occurrence_seconds: number;
  /** Most recent occurrence, used only for a brief game-clock spawned state. */
  last_occurrence_seconds?: number | null;
  /** Lead times from the objective Settings (ALERT-008 prefs), 5..60 s, at most two. Empty means none set. */
  lead_s: number[];
}

/**
 * Host read-model from the same ALERT-008 table used by objective alerts.
 */
export interface ObjectiveSchedule {
  schema_version: 1;
  rule_source: "ALERT-008";
  patch_version: string | null;
  observed_clock_seconds: number | null;
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
  if (!root || !onlyKeys(root, ["schema_version", "rule_source", "patch_version", "observed_clock_seconds", "upcoming"]) || root.schema_version !== 1
    || Object.keys(root).length !== 5
    || root.rule_source !== "ALERT-008"
    || (root.patch_version !== undefined && root.patch_version !== null
      && (typeof root.patch_version !== "string" || !/^\d+\.\d+[a-z]?$/i.test(root.patch_version)))
    || !(root.observed_clock_seconds === null || (int(root.observed_clock_seconds) && root.observed_clock_seconds >= -600 && root.observed_clock_seconds <= 86_400))
    || !Array.isArray(root.upcoming) || root.upcoming.length > MAX_ITEMS) return null;
  const seen = new Set<string>();
  const upcoming: ScheduledObjective[] = [];
  for (const raw of root.upcoming) {
    const item = record(raw);
    if (!item || !onlyKeys(item, ["objective", "occurrence_seconds", "last_occurrence_seconds", "lead_s"])
      || typeof item.objective !== "string" || !(["power_rune", "bounty_rune", "wisdom_rune", "lotus", "water_rune", "tormentor", "day_night"] as string[]).includes(item.objective)
      || !int(item.occurrence_seconds) || item.occurrence_seconds < 0
      || (item.last_occurrence_seconds !== undefined && item.last_occurrence_seconds !== null && (!int(item.last_occurrence_seconds) || item.last_occurrence_seconds < 0 || item.last_occurrence_seconds > item.occurrence_seconds))
      || !Array.isArray(item.lead_s) || item.lead_s.length > 2
      || !item.lead_s.every(lead => int(lead) && lead >= 5 && lead <= 60)
      || new Set(item.lead_s).size !== item.lead_s.length
      || seen.has(item.objective)) return null;
    if (int(root.observed_clock_seconds) && item.occurrence_seconds < root.observed_clock_seconds) return null;
    seen.add(item.objective);
    upcoming.push({ objective: item.objective as ScheduleObjective, occurrence_seconds: item.occurrence_seconds, last_occurrence_seconds: item.last_occurrence_seconds as number | null | undefined, lead_s: [...item.lead_s] as number[] });
  }
  if ((root.patch_version === null || root.observed_clock_seconds === null) && upcoming.length > 0) return null;
  return {
    schema_version: 1,
    rule_source: "ALERT-008",
    patch_version: typeof root.patch_version === "string" ? root.patch_version : null,
    observed_clock_seconds: int(root.observed_clock_seconds) ? root.observed_clock_seconds : null,
    upcoming,
  };
}

export type ObjectiveAvailability = "fresh" | "stale" | "unknown" | "invalid" | "unsupported";
export interface ObjectiveField<T> { value: T | null; availability: ObjectiveAvailability }
export interface ObjectiveMapFields {
  clock_time: ObjectiveField<number>;
  daytime: ObjectiveField<boolean>;
  nightstalker_night: ObjectiveField<boolean>;
  roshan_state: ObjectiveField<string>;
  roshan_respawn_seconds: ObjectiveField<number>;
}
export interface ObjectiveReadModel {
  schema_version: 1;
  availability: ObjectiveAvailability;
  source: { generation: number; sequence: number | null; age_ms: number | null };
  schedule: ObjectiveSchedule;
  map: ObjectiveMapFields;
}

function objectiveField<T>(value: unknown, accepts: (value: unknown) => value is T): ObjectiveField<T> | null {
  const field = record(value);
  const states: ObjectiveAvailability[] = ["fresh", "stale", "unknown", "invalid", "unsupported"];
  if (!field || !onlyKeys(field, ["value", "availability"]) || !states.includes(field.availability as ObjectiveAvailability)) return null;
  if (field.value !== null && !accepts(field.value)) return null;
  if ((field.availability === "unknown" || field.availability === "invalid" || field.availability === "unsupported") && field.value !== null) return null;
  return { value: field.value as T | null, availability: field.availability as ObjectiveAvailability };
}

const boundedClock = (value: unknown): value is number => int(value) && value >= -86_400 && value <= 86_400;
const objectiveState = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 32 && /^[A-Za-z0-9_-]+$/.test(value);

/** Strict parsing keeps missing, stale and malformed GSI values explicit. */
export function parseObjectiveReadModel(value: unknown): ObjectiveReadModel | null {
  const root = record(value);
  if (!root || !onlyKeys(root, ["schema_version", "availability", "source", "schedule", "map"]) || root.schema_version !== 1) return null;
  const states: ObjectiveAvailability[] = ["fresh", "stale", "unknown", "invalid", "unsupported"];
  if (!states.includes(root.availability as ObjectiveAvailability)) return null;
  const source = record(root.source), map = record(root.map);
  if (!source || !onlyKeys(source, ["generation", "sequence", "age_ms"])
    || !int(source.generation) || source.generation < 1
    || !(source.sequence === null || (int(source.sequence) && source.sequence >= 1))
    || !(source.age_ms === null || (int(source.age_ms) && source.age_ms >= 0))
    || !map || !onlyKeys(map, ["clock_time", "daytime", "nightstalker_night", "roshan_state", "roshan_respawn_seconds"])) return null;
  const schedule = parseObjectiveSchedule(root.schedule);
  const clock = objectiveField(map.clock_time, boundedClock);
  const daytime = objectiveField(map.daytime, (input): input is boolean => typeof input === "boolean");
  const nightstalkerNight = objectiveField(map.nightstalker_night, (input): input is boolean => typeof input === "boolean");
  const roshanState = objectiveField(map.roshan_state, objectiveState);
  const respawn = objectiveField(map.roshan_respawn_seconds, (input): input is number => int(input) && input >= 0 && input <= 7_200);
  if (!schedule || !clock || !daytime || !nightstalkerNight || !roshanState || !respawn) return null;
  return {
    schema_version: 1,
    availability: root.availability as ObjectiveAvailability,
    source: { generation: source.generation, sequence: source.sequence as number | null, age_ms: source.age_ms as number | null },
    schedule,
    map: {
      clock_time: clock,
      daytime,
      nightstalker_night: nightstalkerNight,
      roshan_state: roshanState,
      roshan_respawn_seconds: respawn,
    },
  };
}

export const READY_WINDOW_SECONDS = 3;

export interface CountdownLabel { objective: CountdownObjective; text: string | null; mode: "countdown" | "spawned"; remaining: number; patch_version: string | null }

/** The map counter is limited to two characters; long timers stay off-map until they fit. */
export function formatRuneCountdown(seconds: number): string | null {
  if (!Number.isFinite(seconds) || seconds < 1) return null;
  const remaining = Math.ceil(seconds);
  if (remaining < 60) return String(remaining).padStart(2, "0");
  const minutes = Math.ceil(remaining / 60);
  return minutes <= 9 ? `${minutes}m` : null;
}

export function runeAppearance(spawned: boolean): { filter: "grayscale" | "color"; showCountdown: boolean } {
  return spawned ? { filter: "color", showCountdown: false } : { filter: "grayscale", showCountdown: true };
}

const RUNE_ICON_KEYS = new Set<string>(COUNTDOWN_OBJECTIVES);
export function runeIconKey(objective: string, available: Readonly<Record<string, boolean>>): string {
  return RUNE_ICON_KEYS.has(objective) && available[objective] === true ? objective : "neutral";
}

/** Uses only scheduled game-clock observations. A brief full-colour state appears at a recorded spawn. */
export function countdownLabel(item: ScheduledObjective, clockSeconds: number, patchVersion: string | null = null): CountdownLabel | null {
  const remaining = item.occurrence_seconds - clockSeconds;
  if (!Number.isFinite(remaining) || !Number.isFinite(clockSeconds)) return null;
  const sinceLastOccurrence = item.last_occurrence_seconds === undefined || item.last_occurrence_seconds === null
    ? null : clockSeconds - item.last_occurrence_seconds;
  if (remaining === 0 || (sinceLastOccurrence !== null && sinceLastOccurrence >= 0 && sinceLastOccurrence <= READY_WINDOW_SECONDS)) {
    return { objective: item.objective as CountdownObjective, text: null, mode: "spawned", remaining: 0, patch_version: patchVersion };
  }
  if (remaining < 1) return null;
  const text = formatRuneCountdown(remaining);
  if (text === null) return null;
  return { objective: item.objective as CountdownObjective, text, mode: "countdown", remaining, patch_version: patchVersion };
}

/** A schedule older than this (wall time since it arrived) is treated as unknown: the labels disappear. */
export const SCHEDULE_STALE_MS = 5_000;

export function countdownLabels(schedule: ObjectiveSchedule | null, receivedAtMs: number | null, nowMs: number): CountdownLabel[] {
  if (!schedule || schedule.rule_source !== "ALERT-008" || !schedule.patch_version || schedule.observed_clock_seconds === null
    || receivedAtMs === null || nowMs - receivedAtMs > SCHEDULE_STALE_MS || nowMs < receivedAtMs) return [];
  return schedule.upcoming
    .filter(item => (COUNTDOWN_OBJECTIVES as readonly string[]).includes(item.objective))
    .map(item => countdownLabel(item as ScheduledObjective, schedule.observed_clock_seconds as number, schedule.patch_version))
    .filter((label): label is CountdownLabel => label !== null);
}

/**
 * Replay-derived coordinates retain their 7.41f source patch and unknown Dota build in the data file.
 */
export const RUNE_SPOTS: Readonly<Record<CountdownObjective, readonly { x: number; y: number }[]>> = REPLAY_RUNE_SPOTS;
export { RUNE_SPOT_PROVENANCE };

export interface SpotLabel { key: string; x: number; y: number; labels: CountdownLabel[]; positionPatchDiffers: boolean }

/** Group labels that share a map coordinate without dropping any objective timer. */
export function spotLabels(
  labels: CountdownLabel[],
  positions: Readonly<Partial<Record<CountdownObjective, readonly { x: number; y: number }[]>>> = RUNE_SPOTS,
  positionsConfirmed = true,
): SpotLabel[] {
  if (!positionsConfirmed) return [];
  const bySpot = new Map<string, SpotLabel>();
  for (const label of labels) {
    for (const spot of positions[label.objective] ?? []) {
      if (!Number.isFinite(spot.x) || !Number.isFinite(spot.y) || spot.x < 0 || spot.x > 1 || spot.y < 0 || spot.y > 1) continue;
      const key = `${spot.x}:${spot.y}`;
      const current = bySpot.get(key);
      if (current) current.labels.push(label);
      else bySpot.set(key, {
        key,
        x: spot.x,
        y: spot.y,
        labels: [label],
        positionPatchDiffers: label.patch_version !== null && label.patch_version !== RUNE_SPOT_PROVENANCE.patch,
      });
    }
  }
  return [...bySpot.values()].map(spot => ({
    ...spot,
    labels: spot.labels.sort((left, right) => left.objective.localeCompare(right.objective)),
    positionPatchDiffers: spot.labels.some(label => label.patch_version !== null && label.patch_version !== RUNE_SPOT_PROVENANCE.patch),
  }));
}

export function objectiveShortName(objective: CountdownObjective, locale: "th" | "en"): string {
  const th = locale === "th";
  switch (objective) {
    case "power_rune": return th ? "รูนพลัง" : "Power";
    case "bounty_rune": return th ? "รูนทอง" : "Bounty";
    case "wisdom_rune": return th ? "รูนปัญญา" : "Wisdom";
    case "lotus": return th ? "บัว" : "Lotus";
  }
}
