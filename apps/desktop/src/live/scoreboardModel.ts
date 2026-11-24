export interface ScoreSample {
  atMs: number;
  radiant: number;
  dire: number;
}

export interface KillMomentum {
  radiantKills: number;
  direKills: number;
  /** Radiant share of the observed kills; null means an empty neutral bar. */
  radiantPercent: number | null;
  leaning: "radiant" | "dire" | "neutral";
}

export const KILL_MOMENTUM_WINDOW_MS = 3 * 60 * 1000;
export const MAX_SCORE_SAMPLE_AGE_MS = 2_500;

export function formatScore(value: number | null | undefined): string {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? String(value) : "—";
}

export function formatGameClock(seconds: number | null | undefined): string {
  if (typeof seconds !== "number" || !Number.isSafeInteger(seconds)) return "—";
  const magnitude = Math.abs(seconds);
  const clock = `${String(Math.floor(magnitude / 60)).padStart(2, "0")}:${String(magnitude % 60).padStart(2, "0")}`;
  return seconds < 0 ? `-${clock}` : clock;
}

/**
 * A neutral-only view of observed kills in the last three minutes. It deliberately
 * requires a full-window baseline and a fresh current sample; it never estimates win chance.
 */
export function killMomentum(samples: readonly ScoreSample[], nowMs: number): KillMomentum | null {
  if (!Number.isFinite(nowMs) || samples.length < 2) return null;
  const valid = samples
    .filter(sample => Number.isFinite(sample.atMs)
      && Number.isSafeInteger(sample.radiant) && sample.radiant >= 0
      && Number.isSafeInteger(sample.dire) && sample.dire >= 0
      && sample.atMs <= nowMs)
    .slice()
    .sort((left, right) => left.atMs - right.atMs);
  const current = valid.at(-1);
  const cutoff = nowMs - KILL_MOMENTUM_WINDOW_MS;
  const baseline = [...valid].reverse().find(sample => sample.atMs <= cutoff);
  if (!current || !baseline || current.atMs <= baseline.atMs || nowMs - current.atMs > MAX_SCORE_SAMPLE_AGE_MS
    || cutoff - baseline.atMs > MAX_SCORE_SAMPLE_AGE_MS) return null;

  const radiantKills = current.radiant - baseline.radiant;
  const direKills = current.dire - baseline.dire;
  if (radiantKills < 0 || direKills < 0) return null;
  const total = radiantKills + direKills;
  if (total === 0) return { radiantKills, direKills, radiantPercent: null, leaning: "neutral" };

  const radiantPercent = Math.round((radiantKills / total) * 1_000) / 10;
  return {
    radiantKills,
    direKills,
    radiantPercent,
    leaning: radiantKills === direKills ? "neutral" : radiantKills > direKills ? "radiant" : "dire",
  };
}
