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
  /** At most one verified native-provided visual can be visible. */
  visual: (PlaybackStartedEvent & { visual_url: string }) | null;
  /** Audio lifecycle is independent of whether this clip has a visual. */
  audio: PlaybackStartedEvent | null;
  audioEndAtMs: number | null;
  /** Deadline in the event clock domain, including the 500 ms fail-safe margin. */
  failSafeAtMs: number | null;
  malformedEventCount: number;
}

const PLAYBACK_ID = /^[A-Za-z0-9_-]{8,64}$/;
const EVENT_TYPE = /^[a-z][a-z0-9_]{0,79}$/;
const MEDIA_SCHEME = "kohbor-media://";
const PLAYBACK_CLASSES: readonly PlaybackClass[] = ["critical", "correction", "notice", "ordinary", "voice"];
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
    audio: null,
    audioEndAtMs: null,
    failSafeAtMs: null,
    malformedEventCount: 0,
  };
}

/** Reduce lifecycle events in arrival order. The renderer never queues or selects media. */
export function reducePlayback(state: PlaybackState, input: unknown): PlaybackState {
  const event = parsePlaybackEvent(input);
  if (!event) return { ...state, malformedEventCount: state.malformedEventCount + 1 };

  if (event.kind === "ended") {
    if (event.playback_id !== state.currentPlaybackId || (!state.visual && !state.audio)) return state;
    return { ...state, visual: null, failSafeAtMs: null, audio: null, audioEndAtMs: null };
  }

  if (event.playback_id === state.currentPlaybackId) return state;

  const visual = event.visual_url !== null && event.class !== "voice" ? event as PlaybackStartedEvent & { visual_url: string } : null;
  return {
    ...state,
    currentPlaybackId: event.playback_id,
    visual,
    audio: event.audio_duration_ms > 0 ? event : null,
    audioEndAtMs: event.audio_duration_ms > 0 ? event.at_ms + event.audio_duration_ms : null,
    failSafeAtMs: visual ? event.at_ms + event.audio_duration_ms + event.visual_tail_ms + 500 : null,
  };
}

/** Hide a stuck visual at the native event timestamp plus audio, tail and grace period. */
export function tickPlayback(state: PlaybackState, now_ms: number): PlaybackState {
  const audioExpired = state.audio !== null && state.audioEndAtMs !== null && now_ms >= state.audioEndAtMs;
  const visualExpired = state.visual !== null && state.failSafeAtMs !== null && now_ms >= state.failSafeAtMs;
  if (!audioExpired && !visualExpired) return state;
  return { ...state,
    ...(audioExpired ? { audio: null, audioEndAtMs: null } : {}),
    ...(visualExpired ? { visual: null, failSafeAtMs: null } : {}),
  };
}
