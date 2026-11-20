// Audio outcome history: parsing and display helpers for the `audio_history`
// host command. The renderer also uses this parser for saved match history.

export type HistoryClass = "critical" | "correction" | "notice" | "ordinary" | "voice";

export type HistoryOutcome =
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

export interface AudioOutcomeRecord {
  instance_id: string;
  event_type: string | null;
  session_epoch: number;
  audio_class: HistoryClass;
  outcome: HistoryOutcome;
  playback_id: string | null;
  package_revision: number | null;
  /** Milliseconds on the host's monotonic clock, counted from when intake started. */
  decided_monotonic_ms: number;
}

export interface AudioOutcomeRecordV11 extends AudioOutcomeRecord {
  schema_version: "1.1.0";
  tier: number | null;
  variant_id: string | null;
  clip_id: string | null;
  selection: "random" | "shuffle" | "sequence" | null;
  game_clock_seconds: number | null;
  match_id: string | null;
  playback_end: "finished" | "cut" | "failed" | null;
}

const DEMO_INSTANCE_ID = /^ev_g18446744073709551615_s\d+_\d+$/;

/** Demo rows use the reserved native generation; the audio-outcome 1.0.0 shape stays closed. */
export function isDemoOutcomeRecord(record: Pick<AudioOutcomeRecord, "instance_id">): boolean {
  return DEMO_INSTANCE_ID.test(record.instance_id);
}

/** Auditions stay in audio history, but they are not events in the match feed. */
export function isAuditionOutcomeRecord(record: Pick<AudioOutcomeRecord, "event_type">): boolean {
  return record.event_type === "audition";
}

export interface AudioHistory {
  /** Newest first. */
  records: (AudioOutcomeRecord | AudioOutcomeRecordV11)[];
  /** Records the host sent that do not satisfy the contract; they are skipped, never guessed. */
  skipped: number;
}

/** How many records the panel asks for and shows. */
export const HISTORY_LIMIT = 100;
/** Host buffer cap. Its limited snapshot returns the oldest prefix, so fetch it all. */
export const HISTORY_FETCH_LIMIT = 256;

export async function readAudioHistory(command: (limit: number) => Promise<unknown>): Promise<AudioHistory | null> {
  return parseAudioHistory(await command(HISTORY_FETCH_LIMIT));
}

export type HistoryLoadState = "loading" | "ok" | "unreadable" | "failed";

export const HISTORY_CLASSES: readonly HistoryClass[] = ["critical", "correction", "notice", "ordinary", "voice"];
export const HISTORY_OUTCOMES: readonly HistoryOutcome[] = [
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

const INSTANCE_ID = /^[A-Za-z0-9_-]{8,96}$/;
const PLAYBACK_ID = /^[A-Za-z0-9_-]{8,64}$/;
const EVENT_TYPE = /^[a-z][a-z0-9_]{0,79}$/;
const KEYS = [
  "schema_version", "instance_id", "event_type", "session_epoch", "audio_class",
  "outcome", "playback_id", "package_revision", "decided_monotonic_ms",
] as const;
const KEYS_V11 = [...KEYS, "tier", "variant_id", "clip_id", "selection", "game_clock_seconds", "match_id", "playback_end"] as const;
const VARIANT_ID = /^[a-z0-9][a-z0-9_-]{0,31}$/;

function matches(pattern: RegExp, value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = pattern.exec(value);
  return match !== null && match[0] === value;
}

function safeInteger(value: unknown, minimum: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= minimum;
}

/** Parse either closed audio-outcome shape, including its conditional rules. */
export function parseOutcomeRecord(value: unknown): AudioOutcomeRecord | AudioOutcomeRecordV11 | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const version = record.schema_version;
  if (version !== "1.0.0" && version !== "1.1.0") return null;
  const keys = version === "1.1.0" ? KEYS_V11 : KEYS;
  if (Object.keys(record).length !== keys.length || !keys.every(key => Object.hasOwn(record, key))) return null;

  if (!matches(INSTANCE_ID, record.instance_id)) return null;
  if (!safeInteger(record.session_epoch, 1) || !safeInteger(record.decided_monotonic_ms, 0)) return null;
  if (typeof record.audio_class !== "string" || !HISTORY_CLASSES.includes(record.audio_class as HistoryClass)) {
    return null;
  }
  if (typeof record.outcome !== "string" || !HISTORY_OUTCOMES.includes(record.outcome as HistoryOutcome)) return null;
  if (record.event_type !== null && !matches(EVENT_TYPE, record.event_type)) return null;
  if (record.playback_id !== null && !matches(PLAYBACK_ID, record.playback_id)) return null;
  if (record.package_revision !== null && !safeInteger(record.package_revision, 1)) return null;

  const audioClass = record.audio_class as HistoryClass;
  const outcome = record.outcome as HistoryOutcome;
  const playing = outcome === "admitted" || outcome === "preempted";
  if (playing !== (record.playback_id !== null)) return null;
  if (outcome === "admitted" && audioClass !== "voice" && record.package_revision === null) return null;
  if (audioClass === "voice") {
    if (record.event_type !== null || record.package_revision !== null) return null;
  } else if (record.event_type === null) {
    return null;
  }

  const common: AudioOutcomeRecord = {
    instance_id: record.instance_id,
    event_type: record.event_type,
    session_epoch: record.session_epoch,
    audio_class: audioClass,
    outcome,
    playback_id: record.playback_id,
    package_revision: record.package_revision,
    decided_monotonic_ms: record.decided_monotonic_ms,
  };
  if (version === "1.0.0") return common;

  const { tier, variant_id, clip_id, selection, game_clock_seconds, match_id, playback_end } = record;
  if (tier !== null && (!safeInteger(tier, 1) || tier > 10)) return null;
  if (variant_id !== null && !matches(VARIANT_ID, variant_id)) return null;
  if (clip_id !== null && (typeof clip_id !== "string" || clip_id.length === 0 || new TextEncoder().encode(clip_id).length > 48)) return null;
  if (selection !== null && selection !== "random" && selection !== "shuffle" && selection !== "sequence") return null;
  if (game_clock_seconds !== null && !safeInteger(game_clock_seconds, 0)) return null;
  if (playback_end !== null && playback_end !== "finished" && playback_end !== "cut" && playback_end !== "failed") return null;
  if ((outcome === "preempted" && playback_end !== "cut") || (!playing && playback_end !== null)) return null;

  if (audioClass === "voice") {
    if ([tier, variant_id, clip_id, selection, game_clock_seconds, match_id].some(field => field !== null)) return null;
  } else if (match_id !== `local_match_${common.session_epoch}`) {
    return null;
  }
  return { ...common, schema_version: version, tier, variant_id, clip_id, selection, game_clock_seconds,
    match_id: match_id as string | null, playback_end };
}

/**
 * Parse the `audio_history` result (oldest first from the host). A non-array
 * answer is unreadable (null); invalid records are counted and skipped.
 */
export function parseAudioHistory(value: unknown): AudioHistory | null {
  if (!Array.isArray(value)) return null;
  const records: AudioHistory["records"] = [];
  let skipped = 0;
  for (const raw of value) {
    const record = parseOutcomeRecord(raw);
    if (record) records.push(record);
    else skipped += 1;
  }
  records.reverse();
  return { records, skipped };
}

export type OutcomeTone = "played" | "blocked" | "silent";

/**
 * `played` means the request was admitted to the audio hand-off, not that sound
 * was heard. `blocked` means a rule stopped or cut audio that was wanted.
 * `silent` means nothing was due to play for that event.
 */
export function outcomeTone(outcome: HistoryOutcome): OutcomeTone {
  switch (outcome) {
    case "admitted":
      return "played";
    case "preempted":
    case "critical_active":
    case "channel_busy":
    case "muted":
    case "invalid_media":
      return "blocked";
    default:
      return "silent";
  }
}

/** m:ss, or h:mm:ss once past an hour. */
export function formatElapsed(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const ss = String(seconds).padStart(2, "0");
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${ss}` : `${minutes}:${ss}`;
}
