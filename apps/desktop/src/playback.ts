export type PlaybackClass = "critical" | "correction" | "notice" | "ordinary" | "voice";

export type PlaybackEndReason =
  | "finished"
  | "cancelled_preempted"
  | "cancelled_session"
  | "cancelled_revision"
  | "cancelled_muted"
  | "cancelled_device_lost";

export interface PlaybackStartedEvent {
  kind: "started";
  playback_id: string;
  class: PlaybackClass;
  event_type: string | null;
  tier: number | null;
  visual_url: string | null;
  audio_duration_ms: number;
  visual_tail_ms: number;
  at_ms: number;
}

export interface PlaybackEndedEvent {
  kind: "ended";
  playback_id: string;
  reason: PlaybackEndReason;
  at_ms: number;
}

export type PlaybackEvent = PlaybackStartedEvent | PlaybackEndedEvent;

export interface PlaybackState {
  /** The most recently admitted playback id, retained to fence duplicate starts. */
  currentPlaybackId: string | null;
  /** At most one verified native-provided video can be visible. */
  visual: (PlaybackStartedEvent & { visual_url: string }) | null;
  /** A C1 cue can yield a non-critical banner without replacing its video. */
  visualYielding: boolean;
  /** Original video deadline; yielding banners retain it through critical preemption. */
  visualEndAtMs: number | null;
  /** The time a yielded banner's 180 ms opacity transition finishes. */
  visualFadeOutAtMs: number | null;
  /** Preemption may publish the old end before the replacement start. */
  preemptedBannerGraceUntilMs: number | null;
  /** Compact C1 warning shown immediately over a yielded banner. */
  criticalStrip: CriticalStripState | null;
  /** Audio lifecycle is independent of whether this clip has a visual. */
  audio: PlaybackStartedEvent | null;
  audioEndAtMs: number | null;
  /** Earliest presentation deadline in the event clock domain. */
  failSafeAtMs: number | null;
  malformedEventCount: number;
}

export type CriticalStripEventType = "health_danger" | "risk_alert";

export interface CriticalStripState {
  playback_id: string;
  event_type: CriticalStripEventType;
  visual_tail_ms: number;
  until_ms: number;
}

const PLAYBACK_ID = /^[A-Za-z0-9_-]{8,64}$/;
const EVENT_TYPE = /^[a-z][a-z0-9_]{0,79}$/;
const MEDIA_SCHEME = "kohbor-media://";
const PLAYBACK_CLASSES: readonly PlaybackClass[] = ["critical", "correction", "notice", "ordinary", "voice"];
export const PREEMPTED_BANNER_GRACE_MS = 250;
export const YIELDED_BANNER_FADE_MS = 180;
const VISUAL_FAILSAFE_GRACE_MS = 500;
const END_REASONS: readonly PlaybackEndReason[] = [
  "finished",
  "cancelled_preempted",
  "cancelled_session",
  "cancelled_revision",
  "cancelled_muted",
  "cancelled_device_lost",
];

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function validId(value: unknown): value is string {
  return typeof value === "string" && PLAYBACK_ID.test(value);
}

function nonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0;
}

/** Parse the closed playback lifecycle payloads without trusting their source shape. */
export function parsePlaybackEvent(value: unknown): PlaybackEvent | null {
  const event = record(value);
  if (!event || !validId(event.playback_id) || !nonNegativeInteger(event.at_ms)) return null;

  if (event.kind === "ended") {
    if (typeof event.reason !== "string" || !END_REASONS.includes(event.reason as PlaybackEndReason)) return null;
    return {
      kind: "ended",
      playback_id: event.playback_id,
      reason: event.reason as PlaybackEndReason,
      at_ms: event.at_ms,
    };
  }

  if (event.kind !== "started") return null;
  if (typeof event.class !== "string" || !PLAYBACK_CLASSES.includes(event.class as PlaybackClass)) return null;
  if (event.event_type !== null && (
    typeof event.event_type !== "string"
    || !EVENT_TYPE.test(event.event_type)
    || EVENT_TYPE.exec(event.event_type)?.[0] !== event.event_type
  )) return null;
  if (event.tier !== null && !nonNegativeInteger(event.tier)) return null;
  if (event.visual_url !== null && (typeof event.visual_url !== "string" || !event.visual_url.startsWith(MEDIA_SCHEME))) {
    return null;
  }
  if (!nonNegativeInteger(event.audio_duration_ms) || !nonNegativeInteger(event.visual_tail_ms)) return null;

  return {
    kind: "started",
    playback_id: event.playback_id,
    class: event.class as PlaybackClass,
    event_type: event.event_type,
    tier: event.tier,
    visual_url: event.visual_url,
    audio_duration_ms: event.audio_duration_ms,
    visual_tail_ms: event.visual_tail_ms,
    at_ms: event.at_ms,
  };
}

export function createPlaybackState(): PlaybackState {
  return {
    currentPlaybackId: null,
    visual: null,
    visualYielding: false,
    visualEndAtMs: null,
    visualFadeOutAtMs: null,
    preemptedBannerGraceUntilMs: null,
    criticalStrip: null,
    audio: null,
    audioEndAtMs: null,
    failSafeAtMs: null,
    malformedEventCount: 0,
  };
}

function nextPresentationDeadline(state: PlaybackState): number | null {
  const deadlines: number[] = [];
  if (state.visual) {
    const visualDeadline = state.visualFadeOutAtMs ?? state.visualEndAtMs;
    if (visualDeadline !== null) deadlines.push(visualDeadline);
  }
  if (state.preemptedBannerGraceUntilMs !== null) deadlines.push(state.preemptedBannerGraceUntilMs + 1);
  if (state.criticalStrip) deadlines.push(state.criticalStrip.until_ms);
  return deadlines.length === 0 ? null : Math.min(...deadlines);
}

function withPresentationDeadline(state: PlaybackState): PlaybackState {
  const failSafeAtMs = nextPresentationDeadline(state);
  return failSafeAtMs === state.failSafeAtMs ? state : { ...state, failSafeAtMs };
}

function clearVisual(state: PlaybackState): PlaybackState {
  return {
    ...state,
    visual: null,
    visualYielding: false,
    visualEndAtMs: null,
    visualFadeOutAtMs: null,
    preemptedBannerGraceUntilMs: null,
  };
}

/** Apply event-clock deadlines before handling a new event or timer callback. */
function advancePlayback(state: PlaybackState, now_ms: number): PlaybackState {
  let next = state;

  if (next.audio !== null && next.audioEndAtMs !== null && now_ms >= next.audioEndAtMs) {
    next = { ...next, audio: null, audioEndAtMs: null };
  }

  if (next.criticalStrip !== null && now_ms >= next.criticalStrip.until_ms) {
    next = { ...next, criticalStrip: null };
  }

  if (next.preemptedBannerGraceUntilMs !== null && now_ms > next.preemptedBannerGraceUntilMs) {
    next = clearVisual(next);
  } else if (next.visual !== null) {
    if (next.visualFadeOutAtMs !== null && now_ms >= next.visualFadeOutAtMs) {
      next = clearVisual(next);
    } else if (next.visualFadeOutAtMs === null && next.visualEndAtMs !== null && now_ms >= next.visualEndAtMs) {
      if (next.visualYielding) {
        next = { ...next, visualFadeOutAtMs: now_ms + YIELDED_BANNER_FADE_MS };
      } else {
        next = clearVisual(next);
      }
    }
  }

  return withPresentationDeadline(next);
}

function criticalStackEventType(event: PlaybackStartedEvent): CriticalStripEventType | null {
  if (event.class !== "critical") return null;
  return event.event_type === "health_danger" || event.event_type === "risk_alert"
    ? event.event_type
    : null;
}

function canYieldBanner(state: PlaybackState, at_ms: number): state is PlaybackState & {
  visual: PlaybackStartedEvent & { visual_url: string };
} {
  return state.visual !== null
    && state.visual.class !== "critical"
    && state.visualFadeOutAtMs === null
    && (state.visualEndAtMs === null || at_ms < state.visualEndAtMs);
}

/** Reduce lifecycle events in arrival order. The renderer never queues or selects media. */
export function reducePlayback(state: PlaybackState, input: unknown): PlaybackState {
  const event = parsePlaybackEvent(input);
  if (!event) return { ...state, malformedEventCount: state.malformedEventCount + 1 };

  if (event.kind === "ended") {
    const isCurrent = event.playback_id === state.currentPlaybackId;
    const matchingStrip = state.criticalStrip?.playback_id === event.playback_id;
    if (!isCurrent && !matchingStrip) return state;

    let next = state;
    if (matchingStrip && next.criticalStrip) {
      next = {
        ...next,
        criticalStrip: {
          ...next.criticalStrip,
          until_ms: event.at_ms + next.criticalStrip.visual_tail_ms + VISUAL_FAILSAFE_GRACE_MS,
        },
      };
    }
    if (!isCurrent) return withPresentationDeadline(next);

    next = advancePlayback(next, event.at_ms);
    if (next.visual?.playback_id === event.playback_id) {
      if (event.reason === "cancelled_preempted" && next.visual.class !== "critical") {
        const graceUntilMs = event.at_ms + PREEMPTED_BANNER_GRACE_MS;
        next = {
          ...next,
          preemptedBannerGraceUntilMs: next.visualEndAtMs === null
            ? graceUntilMs
            : Math.min(graceUntilMs, next.visualEndAtMs),
        };
      } else {
        next = clearVisual(next);
      }
    }
    next = { ...next, audio: null, audioEndAtMs: null };
    return withPresentationDeadline(next);
  }

  if (event.playback_id === state.currentPlaybackId) return state;

  const advanced = advancePlayback(state, event.at_ms);
  const stripEventType = criticalStackEventType(event);
  const yieldBanner = stripEventType !== null && canYieldBanner(advanced, event.at_ms);
  const visual = yieldBanner
    ? advanced.visual
    : event.visual_url !== null && event.class !== "voice"
      ? event as PlaybackStartedEvent & { visual_url: string }
      : null;
  const visualEndAtMs = yieldBanner
    ? advanced.visualEndAtMs
    : visual
      ? event.at_ms + event.audio_duration_ms + event.visual_tail_ms + VISUAL_FAILSAFE_GRACE_MS
      : null;
  const criticalStrip = yieldBanner && stripEventType !== null
    ? {
        playback_id: event.playback_id,
        event_type: stripEventType,
        visual_tail_ms: event.visual_tail_ms,
        until_ms: event.at_ms + event.audio_duration_ms + event.visual_tail_ms + VISUAL_FAILSAFE_GRACE_MS,
      }
    : advanced.criticalStrip;

  return withPresentationDeadline({
    ...advanced,
    currentPlaybackId: event.playback_id,
    visual,
    visualYielding: yieldBanner,
    visualEndAtMs,
    visualFadeOutAtMs: null,
    preemptedBannerGraceUntilMs: null,
    criticalStrip,
    audio: event.audio_duration_ms > 0 ? event : null,
    audioEndAtMs: event.audio_duration_ms > 0 ? event.at_ms + event.audio_duration_ms : null,
  });
}

/** Advance audio, video, preemption-grace and critical-strip lifetimes in the host event clock. */
export function tickPlayback(state: PlaybackState, now_ms: number): PlaybackState {
  return advancePlayback(state, now_ms);
}
