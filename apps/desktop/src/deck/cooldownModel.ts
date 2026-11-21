export interface CooldownReport {
  remaining: number | null;
  stale?: boolean;
  ready?: boolean;
}

export interface CooldownState {
  /** Estimated seconds at reportedAtMs; this is presentation state, never an observed readiness claim. */
  remaining: number;
  /** Largest fresh remaining value seen in this cooldown episode. The true full length is unavailable and
   * this observed maximum can be up to one GSI update interval shorter than the true cooldown length. */
  denominator: number;
  fraction: number;
  display: string;
  precision: 0 | 1;
  reportedAtMs: number;
  active: boolean;
  stale: boolean;
}

export interface CooldownView {
  remaining: number;
  fraction: number;
  display: string;
  active: boolean;
  stale: boolean;
}

const bounded = (value: number, upper = 1): number => Math.min(upper, Math.max(0, value));
const validPositive = (value: number | null): value is number => typeof value === "number" && Number.isFinite(value) && value > 0;

function precisionOf(value: number): 0 | 1 {
  return Number.isInteger(value) ? 0 : 1;
}

function formatRemaining(value: number, precision: 0 | 1): string {
  const scale = 10 ** precision;
  const rounded = Math.ceil(Math.max(0, value) * scale - 1e-10) / scale;
  return precision === 0 ? String(rounded) : rounded.toFixed(1);
}

/** Format one source value without exposing more precision than the numeric source can represent. */
export function formatCooldownValue(value: number): string {
  const safe = Number.isFinite(value) ? Math.max(0, value) : 0;
  return formatRemaining(safe, precisionOf(safe));
}

function stateAt(remaining: number, denominator: number, precision: 0 | 1, reportedAtMs: number,
  stale: boolean, active: boolean): CooldownState {
  const safeRemaining = Math.max(0, remaining);
  const fraction = denominator > 0 ? bounded(safeRemaining / denominator) : 0;
  return { remaining: safeRemaining, denominator, fraction, display: formatRemaining(safeRemaining, precision),
    precision, reportedAtMs, active, stale };
}

function inactive(nowMs: number, stale: boolean): CooldownState {
  return stateAt(0, 0, 0, nowMs, stale, false);
}

/**
 * Track one observed cooldown report. Between fresh reports the renderer may estimate elapsed time from the
 * monotonic browser clock, but stale data freezes that estimate and only a fresh source report can reset it.
 */
export function trackCooldown(prev: CooldownState | null, report: CooldownReport, nowMs: number): CooldownState {
  const now = Number.isFinite(nowMs) ? nowMs : prev?.reportedAtMs ?? 0;
  if (report.stale) {
    if (!prev?.active) return inactive(now, true);
    const frozen = prev.stale ? prev.remaining : estimateCooldown(prev, now).remaining;
    return stateAt(frozen, prev.denominator, prev.precision, now, true, true);
  }

  const reported = report.remaining;
  if (report.ready || !validPositive(reported)) return inactive(now, false);
  // GSI repeats the same whole-second value many times per second. Re-anchoring on every repeat would pull the
  // estimate back up after each update (a sawtooth), so keep the anchor until the reported value changes.
  if (prev?.active && !prev.stale && reported === prev.remaining) return prev;

  const denominator = prev?.active ? Math.max(prev.denominator, reported) : reported;
  return stateAt(reported, denominator, precisionOf(reported), now, false, true);
}

/** Distinguish both source identity and slot occupant so a new cooldown never inherits an old denominator. */
export function cooldownTrackKey(kind: "item" | "ability", contextKey: string, slot: string | number,
  rawId: string | null): string {
  return `${kind}:${JSON.stringify([contextKey, slot, rawId])}`;
}

/** Smoothly estimate between fresh reports without letting the estimate become negative. */
export function estimateCooldown(state: CooldownState, nowMs: number): CooldownView {
  const elapsedSeconds = state.stale || !Number.isFinite(nowMs) ? 0 : Math.max(0, nowMs - state.reportedAtMs) / 1000;
  const remaining = state.active ? Math.max(0, state.remaining - elapsedSeconds) : 0;
  const fraction = state.denominator > 0 ? bounded(remaining / state.denominator) : 0;
  return { remaining, fraction, display: formatRemaining(remaining, state.precision), active: state.active, stale: state.stale };
}

/** Select exactly one clock strategy for the group: no loop when idle/hidden, 1 Hz when motion is reduced. */
export function cooldownTickMode(states: readonly CooldownState[], nowMs: number, visible: boolean,
  reducedMotion: boolean): "raf" | "interval" | null {
  if (!visible || !states.some(state => state.active && !state.stale && estimateCooldown(state, nowMs).remaining > 0)) return null;
  return reducedMotion ? "interval" : "raf";
}
