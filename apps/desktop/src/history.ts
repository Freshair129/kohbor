// Audio outcome history: parsing and display helpers for the `audio_history`
// host command (contracts/audio-outcome 1.0.0, proposed). The host keeps the
// records in memory only; the renderer never stores or sends them anywhere.

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

export interface AudioHistory {
  /** Newest first. */
  records: AudioOutcomeRecord[];
  /** Records the host sent that do not satisfy the contract; they are skipped, never guessed. */
  skipped: number;
}

/** How many records the panel asks for and shows. */
export const HISTORY_LIMIT = 100;

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

function matches(pattern: RegExp, value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = pattern.exec(value);
  return match !== null && match[0] === value;
}

function safeInteger(value: unknown, minimum: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= minimum;
}

/** Parse one audio-outcome 1.0.0 record, including its conditional rules. */
export function parseOutcomeRecord(value: unknown): AudioOutcomeRecord | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record);
  if (keys.length !== KEYS.length || !KEYS.every((key) => Object.hasOwn(record, key))) return null;

  if (record.schema_version !== "1.0.0" || !matches(INSTANCE_ID, record.instance_id)) return null;
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

  return {
    instance_id: record.instance_id,
    event_type: record.event_type,
    session_epoch: record.session_epoch,
    audio_class: audioClass,
    outcome,
    playback_id: record.playback_id,
    package_revision: record.package_revision,
    decided_monotonic_ms: record.decided_monotonic_ms,
  };
}

/**
 * Parse the `audio_history` result (oldest first from the host). A non-array
 * answer is unreadable (null); invalid records are counted and skipped.
 */
export function parseAudioHistory(value: unknown): AudioHistory | null {
  if (!Array.isArray(value)) return null;
  const records: AudioOutcomeRecord[] = [];
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
