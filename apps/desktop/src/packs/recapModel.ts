// @ts-ignore Explicit source extensions support the Node test runner.
import { isAuditionOutcomeRecord, isDemoOutcomeRecord, parseOutcomeRecord, type AudioOutcomeRecordV11 } from "../history.ts";

export type MatchRecapEvent = AudioOutcomeRecordV11 & { event_type: string };

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
const LOCAL_MATCH_ID = /^local_match_[1-9][0-9]{0,15}$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseEvent(value: unknown, matchId: string): MatchRecapEvent | null {
  const event = parseOutcomeRecord(value);
  if (!event || !("schema_version" in event) || event.match_id !== matchId || event.event_type === null
    || isAuditionOutcomeRecord(event) || isDemoOutcomeRecord(event)) return null;
  return { ...event, event_type: event.event_type };
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
