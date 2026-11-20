export type MatchRecapOutcome =
  | "admitted"
  | "preempted"
  | "unmapped"
  | "invalid_media"
  | "inert"
  | "not_voiced"
  | "detector_unavailable"
  | "stale_session"
  | "superseded_revision"
  | "critical_active"
  | "channel_busy"
  | "lower_rank_same_tick"
  | "cooldown"
  | "muted";

export type MatchRecapPlaybackEnd = "finished" | "cut" | "failed" | null;
export type MatchRecapSelection = "random" | "shuffle" | "sequence" | null;

export interface MatchRecapEvent {
  instance_id: string;
  playback_id: string | null;
  event_type: string;
  outcome: MatchRecapOutcome;
  tier: number | null;
  variant_id: string | null;
  clip_id: string | null;
  selection: MatchRecapSelection;
  game_clock_seconds: number | null;
  playback_end: MatchRecapPlaybackEnd;
}

export interface MatchRecapMatch {
  /** Internal local-only key; the component does not display it. */
  match_id: string;
  playedEvents: MatchRecapEvent[];
  droppedCount: number;
}

export interface MatchRecapSnapshot {
  matches: MatchRecapMatch[];
  skipped: number;
  readable: boolean;
}

const FILE_SCHEMA = "1.0.0";
const OUTCOME_SCHEMA = "1.1.0";
const OUTCOMES: readonly MatchRecapOutcome[] = [
  "admitted",
  "preempted",
  "unmapped",
  "invalid_media",
  "inert",
  "not_voiced",
  "detector_unavailable",
  "stale_session",
  "superseded_revision",
  "critical_active",
  "channel_busy",
  "lower_rank_same_tick",
  "cooldown",
  "muted",
];
const RECORD_KEYS = [
  "schema_version",
  "instance_id",
  "event_type",
  "session_epoch",
  "audio_class",
  "outcome",
  "playback_id",
  "package_revision",
  "decided_monotonic_ms",
  "tier",
  "variant_id",
  "clip_id",
  "selection",
  "game_clock_seconds",
  "match_id",
  "playback_end",
] as const;

const EVENT_ID = /^[a-z][a-z0-9_]{0,79}$/;
const IDENTIFIER = /^[A-Za-z0-9_-]{8,96}$/;
const PLAYBACK_ID = /^[A-Za-z0-9_-]{8,64}$/;
const VARIANT_ID = /^[a-z0-9][a-z0-9_-]{0,31}$/;
const LOCAL_MATCH_ID = /^local_match_[1-9][0-9]{0,15}$/;
const SELECTIONS: readonly MatchRecapSelection[] = ["random", "shuffle", "sequence", null];
const PLAYBACK_ENDS: readonly MatchRecapPlaybackEnd[] = ["finished", "cut", "failed", null];
const AUDIO_CLASSES = ["critical", "correction", "notice", "ordinary"] as const;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isSafeInteger(value: unknown, minimum: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= minimum;
}

function isLocalMatchForEpoch(value: unknown, epoch: unknown): value is string {
  if (typeof value !== "string" || !LOCAL_MATCH_ID.test(value) || !isSafeInteger(epoch, 1)) return false;
  return value === `local_match_${epoch}`;
}

function parseEvent(value: unknown, matchId: string): MatchRecapEvent | null {
  if (!isRecord(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== RECORD_KEYS.length || !RECORD_KEYS.every((key) => Object.hasOwn(value, key))) return null;
  if (value.schema_version !== OUTCOME_SCHEMA || typeof value.instance_id !== "string" || !IDENTIFIER.test(value.instance_id)) return null;
  if (typeof value.event_type !== "string" || !EVENT_ID.test(value.event_type) || value.event_type === "audition") return null;
  if (typeof value.audio_class !== "string" || !AUDIO_CLASSES.includes(value.audio_class as (typeof AUDIO_CLASSES)[number])) return null;
  if (typeof value.outcome !== "string" || !OUTCOMES.includes(value.outcome as MatchRecapOutcome)) return null;
  if (!isLocalMatchForEpoch(value.match_id, value.session_epoch) || value.match_id !== matchId) return null;
  if (!isSafeInteger(value.session_epoch, 1) || !isSafeInteger(value.decided_monotonic_ms, 0)) return null;

  const playing = value.outcome === "admitted" || value.outcome === "preempted";
  if (playing) {
    if (typeof value.playback_id !== "string" || !PLAYBACK_ID.test(value.playback_id)) return null;
  } else if (value.playback_id !== null) {
    return null;
  }

  if (value.package_revision !== null && !isSafeInteger(value.package_revision, 1)) return null;
  if (value.outcome === "admitted" && value.package_revision === null) return null;
  if (value.tier !== null && (!isSafeInteger(value.tier, 1) || value.tier > 10)) return null;
  if (value.variant_id !== null && (typeof value.variant_id !== "string" || !VARIANT_ID.test(value.variant_id))) return null;
  if (value.clip_id !== null && (typeof value.clip_id !== "string" || value.clip_id.length < 1 || value.clip_id.length > 48)) return null;
  if (!SELECTIONS.includes(value.selection as MatchRecapSelection)) return null;
  if (value.game_clock_seconds !== null && !isSafeInteger(value.game_clock_seconds, 0)) return null;
  if (!PLAYBACK_ENDS.includes(value.playback_end as MatchRecapPlaybackEnd)) return null;
  if (value.outcome === "preempted" && value.playback_end !== "cut") return null;
  if (!playing && value.playback_end !== null) return null;

  return {
    instance_id: value.instance_id,
    playback_id: value.playback_id as string | null,
    event_type: value.event_type,
    outcome: value.outcome as MatchRecapOutcome,
    tier: value.tier as number | null,
    variant_id: value.variant_id as string | null,
    clip_id: value.clip_id as string | null,
    selection: value.selection as MatchRecapSelection,
    game_clock_seconds: value.game_clock_seconds as number | null,
    playback_end: value.playback_end as MatchRecapPlaybackEnd,
  };
}

/** Parse and group the host-owned local recap without displaying dropped events as played. */
export function parseMatchRecap(value: unknown): MatchRecapSnapshot {
  if (!isRecord(value) || value.schema_version !== FILE_SCHEMA || !Array.isArray(value.matches) || value.matches.length > 5) {
    return { matches: [], skipped: 1, readable: false };
  }

  const matches: MatchRecapMatch[] = [];
  const seenMatches = new Set<string>();
  let skipped = 0;
  for (const rawMatch of value.matches) {
    if (!isRecord(rawMatch) || typeof rawMatch.match_id !== "string" || !LOCAL_MATCH_ID.test(rawMatch.match_id) || !Array.isArray(rawMatch.events) || rawMatch.events.length > 128) {
      skipped += 1;
      continue;
    }
    if (seenMatches.has(rawMatch.match_id)) {
      skipped += 1;
      continue;
    }
    seenMatches.add(rawMatch.match_id);
    const playedEvents: MatchRecapEvent[] = [];
    let droppedCount = 0;
    const seenEvents = new Set<string>();
    for (const rawEvent of rawMatch.events) {
      const event = parseEvent(rawEvent, rawMatch.match_id);
      if (!event) {
        skipped += 1;
        continue;
      }
      const eventKey = event.playback_id ?? event.instance_id;
      if (seenEvents.has(eventKey)) {
        skipped += 1;
        continue;
      }
      seenEvents.add(eventKey);
      if (event.outcome === "admitted" || event.outcome === "preempted") playedEvents.push(event);
      else droppedCount += 1;
    }
    matches.push({ match_id: rawMatch.match_id, playedEvents, droppedCount });
  }
  return { matches, skipped, readable: true };
}

export function emptyMatchRecap(): MatchRecapSnapshot {
  return { matches: [], skipped: 0, readable: true };
}

export function formatGameClock(seconds: number | null): string {
  if (seconds === null) return "—";
  const minutes = Math.floor(seconds / 60);
  const remainder = String(seconds % 60).padStart(2, "0");
  return `${minutes}:${remainder}`;
}

export type MatchRecapHostCommand = (command: "match_recap" | "clear_match_recap") => Promise<unknown>;

export async function loadMatchRecap(command: MatchRecapHostCommand): Promise<MatchRecapSnapshot> {
  try {
    return parseMatchRecap(await command("match_recap"));
  } catch {
    return { matches: [], skipped: 0, readable: false };
  }
}

export async function clearMatchRecap(command: MatchRecapHostCommand): Promise<void> {
  const result = await command("clear_match_recap");
  if (!isRecord(result) || result.cleared !== true) throw new Error("RECAP_CLEAR_FAILED");
}
