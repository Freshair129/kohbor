// @ts-ignore Node's built-in TypeScript loader requires explicit source extensions for pure-model tests.
import { pinnedMapContents, type PlacedLayout, type Rect } from "./secondMonitorLayout.ts";
// @ts-ignore Node's built-in TypeScript loader requires explicit source extensions for pure-model tests.
import { deriveRoshanPitStatus, formatRuneCountdown, READY_WINDOW_SECONDS, ROSHAN_PIT_RULE, type CountdownObjective, type ObjectiveAvailability, type ObjectiveMapFields, type ObjectiveSchedule, type RoshanPitCycleRule as RoshanPitCycleRuleType, type ScheduleObjective } from "./objectiveCountdownModel.ts";

export interface LiveMatchSample {
  sequence: number;
  atMs: number;
  gameSeconds: number | null;
  hp: number | null;
  gpm: number | null;
  xpm: number | null;
  lastHits: number | null;
}

export interface TimelineScoreSample {
  atMs: number;
  gameSeconds?: number;
  radiant: number;
  dire: number;
}

export interface DamageRow {
  hpLost: number;
  hpAfter: number;
  gameSeconds: number | null;
}

export interface HpPoint { gameSeconds: number; hp: number }

/** Build a newest-first, in-memory list. Healing is omitted from rows but remains in the HP series. */
export function buildDamageLog(samples: readonly LiveMatchSample[]): DamageRow[] {
  const ordered = [...samples].sort((left, right) => left.atMs - right.atMs);
  const rows: (DamageRow & { lastDropAtMs: number })[] = [];
  let previous: LiveMatchSample | null = null;
  for (const sample of ordered) {
    if (!Number.isSafeInteger(sample.hp) || sample.hp === null || !Number.isFinite(sample.atMs)) {
      previous = null;
      continue;
    }
    if (previous && Number.isSafeInteger(previous.hp) && previous.hp !== null
      && sample.atMs - previous.atMs <= 2_500 && sample.hp < previous.hp) {
      const hpLost = previous.hp - sample.hp;
      const latest = rows.at(-1);
      if (latest && sample.atMs - latest.lastDropAtMs <= 500) {
        latest.hpLost += hpLost;
        latest.hpAfter = sample.hp;
        latest.lastDropAtMs = sample.atMs;
      } else {
        rows.push({ hpLost, hpAfter: sample.hp, gameSeconds: sample.gameSeconds, lastDropAtMs: sample.atMs });
      }
    }
    previous = sample;
  }
  return rows.reverse().map(({ hpLost, hpAfter, gameSeconds }) => ({ hpLost, hpAfter, gameSeconds }));
}

/** Keep only the last 60 observed game seconds; wall time is not used to extend a GSI value. */
export function buildHpSparkline(samples: readonly LiveMatchSample[], gameSeconds: number | null): HpPoint[] {
  if (!Number.isSafeInteger(gameSeconds) || gameSeconds === null) return [];
  return samples.flatMap(sample => Number.isSafeInteger(sample.gameSeconds) && sample.gameSeconds !== null
    && sample.gameSeconds >= gameSeconds - 60 && sample.gameSeconds <= gameSeconds
    && Number.isSafeInteger(sample.hp) && sample.hp !== null
    ? [{ gameSeconds: sample.gameSeconds, hp: sample.hp }] : []);
}

export interface FarmPoint {
  gameSeconds: number;
  gpm: number | null;
  xpm: number | null;
  lastHits: number | null;
}

export interface FarmSeries {
  points: FarmPoint[];
  lhAt10: { gameSeconds: number; lastHits: number } | null;
  latest: { gpm: number | null; xpm: number | null; lastHits: number | null };
}

export function buildFarmSeries(samples: readonly LiveMatchSample[]): FarmSeries | null {
  const points = samples
    .filter(sample => Number.isSafeInteger(sample.gameSeconds) && sample.gameSeconds !== null)
    .map(sample => ({
      gameSeconds: sample.gameSeconds as number,
      gpm: Number.isSafeInteger(sample.gpm) ? sample.gpm : null,
      xpm: Number.isSafeInteger(sample.xpm) ? sample.xpm : null,
      lastHits: Number.isSafeInteger(sample.lastHits) ? sample.lastHits : null,
    }))
    .sort((left, right) => left.gameSeconds - right.gameSeconds);
  if (points.length === 0) return null;
  const latestValue = (key: "gpm" | "xpm" | "lastHits") => [...points].reverse().find(point => point[key] !== null)?.[key] ?? null;
  const lastGameClock = points.at(-1)?.gameSeconds ?? -1;
  const atTen = lastGameClock >= 600
    ? [...points].reverse().find(point => point.gameSeconds <= 600 && point.lastHits !== null)
    : null;
  return {
    points,
    lhAt10: atTen ? { gameSeconds: 600, lastHits: atTen.lastHits as number } : null,
    latest: { gpm: latestValue("gpm"), xpm: latestValue("xpm"), lastHits: latestValue("lastHits") },
  };
}

export function buildKillTimeline(samples: readonly TimelineScoreSample[]): { gameSeconds: number; radiant: number; dire: number }[] {
  return samples.flatMap(sample => Number.isSafeInteger(sample.gameSeconds)
    && Number.isSafeInteger(sample.radiant) && sample.radiant >= 0
    && Number.isSafeInteger(sample.dire) && sample.dire >= 0
    ? [{ gameSeconds: sample.gameSeconds as number, radiant: sample.radiant, dire: sample.dire }] : []);
}

const TIMER_IDS = ["power_rune", "bounty_rune", "wisdom_rune", "lotus", "tormentor", "day_night", "roshan", "roshan_switch"] as const;
export type TimerId = (typeof TIMER_IDS)[number];
export interface ObjectiveTimerRow { id: TimerId; state: "countdown" | "moving" | "ready" | "spawned" | "observed" | "unknown"; text: string; patchVersion: string | null }
const UNKNOWN_TIMER = "— ไม่ทราบ";
const TIMER_NAMES: Record<TimerId, ScheduleObjective | null> = {
  power_rune: "power_rune", bounty_rune: "bounty_rune", wisdom_rune: "wisdom_rune",
  lotus: "lotus", tormentor: "tormentor", day_night: "day_night", roshan: null, roshan_switch: null,
};

export type { RoshanPitCycleRule } from "./objectiveCountdownModel.ts";

export function deriveObjectiveTimerRows(
  schedule: ObjectiveSchedule | null,
  receivedAtMs: number | null,
  nowMs: number,
  gameClockSeconds: number | null = null,
  mapFields: ObjectiveMapFields | null = null,
  sourceAvailability: ObjectiveAvailability = "unknown",
  locale: "th" | "en" = "th",
  roshanPitRule: RoshanPitCycleRuleType | null = ROSHAN_PIT_RULE,
): ObjectiveTimerRow[] {
  const unknownText = locale === "th" ? UNKNOWN_TIMER : "— Unknown";
  const fresh = sourceAvailability === "fresh" && schedule !== null && schedule.patch_version !== null && receivedAtMs !== null
    && nowMs >= receivedAtMs && nowMs - receivedAtMs <= 5_000;
  const rulePatch = (id: TimerId): string | null =>
    ["power_rune", "bounty_rune", "wisdom_rune", "tormentor"].includes(id) && schedule?.rule_source === "ALERT-008"
      ? schedule.patch_version : null;
  return TIMER_IDS.map(id => {
    if (id === "day_night") {
      if (sourceAvailability !== "fresh" || !mapFields) return { id, state: "unknown", text: unknownText, patchVersion: null };
      const nightStalkerNight = mapFields.nightstalker_night.availability === "fresh" && mapFields.nightstalker_night.value === true;
      const daytime = mapFields.daytime.availability === "fresh" ? mapFields.daytime.value : null;
      const phase = nightStalkerNight
        ? (locale === "th" ? "กลางคืน (Night Stalker)" : "Night Stalker night")
        : daytime === true ? (locale === "th" ? "กลางวัน" : "Daytime")
          : daytime === false ? (locale === "th" ? "กลางคืน" : "Nighttime") : null;
      return phase
        ? { id, state: "observed", text: `${phase} · ${locale === "th" ? "สลับ" : "switch"} ${unknownText}`, patchVersion: null }
        : { id, state: "unknown", text: unknownText, patchVersion: null };
    }
    if (id === "roshan") {
      const field = mapFields?.roshan_respawn_seconds;
      if (sourceAvailability !== "fresh" || !field || field.availability !== "fresh" || !Number.isSafeInteger(field.value) || field.value === null || field.value < 0) {
        return { id, state: "unknown", text: unknownText, patchVersion: null };
      }
      if (field.value === 0) return { id, state: "ready", text: "พร้อม", patchVersion: null };
      const minutes = Math.floor(field.value / 60);
      const seconds = field.value % 60;
      return { id, state: "countdown", text: `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`, patchVersion: null };
    }
    if (id === "roshan_switch") {
      const clock = mapFields?.clock_time;
      const daytime = mapFields?.daytime;
      if (sourceAvailability !== "fresh" || !clock || clock.availability !== "fresh"
        || !daytime || daytime.availability !== "fresh" || receivedAtMs === null
        || nowMs < receivedAtMs || nowMs - receivedAtMs > 5_000) {
        return { id, state: "unknown", text: unknownText, patchVersion: null };
      }
      const pitStatus = deriveRoshanPitStatus(clock.value, daytime.value, roshanPitRule);
      if (pitStatus.state === "unknown") return { id, state: "unknown", text: unknownText, patchVersion: null };
      if (pitStatus.state === "moving") {
        const seconds = Math.ceil(pitStatus.secondsRemaining);
        return { id, state: "moving", text: locale === "th" ? `กำลังย้าย ${seconds} วิ` : `Moving ${seconds}s`, patchVersion: pitStatus.patchVersion };
      }
      const compact = formatRuneCountdown(pitStatus.secondsUntilSwitch);
      const minutes = Math.floor(Math.ceil(pitStatus.secondsUntilSwitch) / 60);
      const seconds = Math.ceil(pitStatus.secondsUntilSwitch) % 60;
      const time = compact ?? `${minutes}:${String(seconds).padStart(2, "0")}`;
      return { id, state: "countdown", text: locale === "th" ? `ย้ายใน ${time}` : `Switch in ${time}`, patchVersion: pitStatus.patchVersion };
    }
    if (!fresh || !schedule) return { id, state: "unknown", text: unknownText, patchVersion: rulePatch(id) };
    const patchVersion = schedule.patch_version;
    if (patchVersion === null) return { id, state: "unknown", text: unknownText, patchVersion: null };
    const objective = TIMER_NAMES[id];
    if (!objective) return { id, state: "unknown", text: unknownText, patchVersion: null };
    const scheduled = schedule.upcoming.find(item => item.objective === objective);
    if (!scheduled) return { id, state: "unknown", text: unknownText, patchVersion: null };
    const clockSeconds = Number.isSafeInteger(gameClockSeconds) && gameClockSeconds !== null
      ? gameClockSeconds : schedule.observed_clock_seconds;
    if (clockSeconds === null) return { id, state: "unknown", text: unknownText, patchVersion: null };
    const sinceLastOccurrence = scheduled.last_occurrence_seconds === undefined || scheduled.last_occurrence_seconds === null
      ? null : clockSeconds - scheduled.last_occurrence_seconds;
    if (sinceLastOccurrence !== null && sinceLastOccurrence >= 0 && sinceLastOccurrence <= READY_WINDOW_SECONDS) {
      return { id, state: "spawned", text: "", patchVersion };
    }
    const remaining = scheduled.occurrence_seconds - clockSeconds;
    if (remaining <= 0) return { id, state: "unknown", text: unknownText, patchVersion: null };
    const minutes = Math.floor(remaining / 60);
    const seconds = remaining % 60;
    return { id, state: "countdown", text: `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`, patchVersion };
  });
}

export interface RoshanObservation {
  source: "gsi" | "patch-rule";
  patchVerified: boolean;
  pit: "radiant" | "dire";
  changed?: boolean;
}

export function deriveRoshanOverlay(observation: RoshanObservation | null): {
  state: "unknown" | "known"; text: string; changed: boolean;
} {
  if (!observation || !observation.patchVerified) return { state: "unknown", text: UNKNOWN_TIMER, changed: false };
  return {
    state: "known",
    text: observation.pit === "radiant" ? "Radiant" : "Dire",
    changed: observation.changed === true,
  };
}

export interface RoshanRespawnRule { minSeconds: number; maxSeconds: number; patchVerified: boolean }
export function canShowRoshanDeathControl(rule: RoshanRespawnRule | null): boolean {
  return rule !== null && rule.patchVerified
    && Number.isSafeInteger(rule.minSeconds) && Number.isSafeInteger(rule.maxSeconds)
    && rule.minSeconds >= 0 && rule.maxSeconds >= rule.minSeconds;
}

export function deriveAgentOrbMotion(input: {
  outputLevel: number | null;
  playbackActive: boolean;
  reducedMotion: boolean;
  visible: boolean;
}): { source: "level" | "playback" | "none"; level: number | null; motion: "idle" | "playing" | "static"; animate: boolean } {
  const hasLevel = typeof input.outputLevel === "number" && Number.isFinite(input.outputLevel)
    && input.outputLevel >= 0 && input.outputLevel <= 1;
  const source = hasLevel ? "level" : input.playbackActive ? "playback" : "none";
  const allowed = input.visible && !input.reducedMotion;
  return {
    source,
    level: hasLevel ? input.outputLevel : null,
    motion: allowed ? (input.playbackActive ? "playing" : "idle") : "static",
    animate: allowed,
  };
}

export const PANEL_PRIORITY = ["objectives", "damage", "orb", "farm", "kills"] as const;
export type PanelId = (typeof PANEL_PRIORITY)[number];
const PANEL_HEIGHT: Record<PanelId, number> = { objectives: 172, damage: 190, orb: 140, farm: 206, kills: 140 };
const PANEL_GAP = 12;
const TWO_COLUMN_MIN_WIDTH = 1_080;

export function outerPanelBox(layout: PlacedLayout): Rect {
  const contents = pinnedMapContents({ width: layout.map.w, height: layout.map.h, mapEdge: layout.mapEdge, phase: "live" });
  const map = contents?.map;
  if (!map) return { x: 0, y: layout.map.y, w: 0, h: 0 };
  const onLeft = map.x > 0;
  return {
    x: onLeft ? 0 : map.w,
    y: layout.map.y,
    w: onLeft ? map.x : layout.map.w - map.w,
    h: layout.map.h,
  };
}

export interface PanelPlacement { id: PanelId; rect: Rect }
export interface PinnedPanelLayout {
  panels: PanelPlacement[];
  dropped: PanelId[];
  minimumTextPx: 12;
  scroll: false;
}

/** Priority-ordered panels are packed into two columns only when both columns remain legible. */
export function pinnedPanelLayout(box: Rect): PinnedPanelLayout {
  const columns = box.w >= TWO_COLUMN_MIN_WIDTH ? 2 : 1;
  const columnWidth = columns === 2 ? (box.w - PANEL_GAP) / 2 : box.w;
  const kept: PanelPlacement[] = [];
  const rowHeights: number[] = [];
  for (const [index, id] of PANEL_PRIORITY.entries()) {
    const row = Math.floor(index / columns);
    const column = index % columns;
    const nextHeight = Math.max(rowHeights[row] ?? 0, PANEL_HEIGHT[id]);
    const projectedHeight = rowHeights.reduce((sum, height, rowIndex) => sum + height + (rowIndex > 0 ? PANEL_GAP : 0), 0)
      + (rowHeights[row] === undefined ? nextHeight + (row > 0 ? PANEL_GAP : 0) : nextHeight - rowHeights[row]);
    if (columnWidth <= 0 || projectedHeight > box.h) break;
    rowHeights[row] = nextHeight;
    const y = rowHeights.slice(0, row).reduce((sum, height) => sum + height + PANEL_GAP, 0);
    kept.push({ id, rect: { x: column * (columnWidth + PANEL_GAP), y, w: columnWidth, h: PANEL_HEIGHT[id] } });
  }
  return { panels: kept, dropped: PANEL_PRIORITY.slice(kept.length), minimumTextPx: 12, scroll: false };
}
