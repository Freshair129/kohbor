import type { ReplayLibraryEntry, ReplayStatus } from "./replayLibraryClient";
import type { MatchBundle } from "./matchBundle";
import type { ReplayPatchData } from "./replayPack.mjs";

export interface ReplayMetadata {
  matchDate: number | null;
  heroNames: string[];
  durationS: number | null;
}

export interface ReplayListCopy {
  states: {
    available: string;
    queued: string;
    parsing: string;
    ready: string;
    needs_pack: string;
    failed: (reason: string) => string;
  };
  source: { dota: string; manual: string };
  unknown: string;
  noDate: string;
  dateAdded: (date: string) => string;
  duration: (duration: string) => string;
  heroes: (names: string[]) => string;
}

export interface ReplayListItem {
  matchId: string;
  status: ReplayStatus;
  statusLabel: string;
  canOpen: boolean;
  canParse: boolean;
  sourceLabel: string;
  addedDateLabel: string;
  matchDateLabel: string;
  heroesLabel: string;
  durationLabel: string;
}

const dateLocale = (locale: "th" | "en") => locale === "th" ? "th-TH-u-ca-gregory" : "en-US";

export function replayStatusLabel(status: ReplayStatus, copy: ReplayListCopy["states"]): string {
  switch (status.state) {
    case "available": return copy.available;
    case "queued": return copy.queued;
    case "parsing": return copy.parsing;
    case "ready": return copy.ready;
    case "needs_pack": return copy.needs_pack;
    case "failed": return copy.failed(status.reason);
  }
}

export function replayListViewModel(
  entries: readonly ReplayLibraryEntry[],
  metadata: Readonly<Record<string, ReplayMetadata>>,
  copy: ReplayListCopy,
  locale: "th" | "en",
  statusOverrides: Readonly<Record<string, ReplayStatus>> = {},
): ReplayListItem[] {
  const formatter = new Intl.DateTimeFormat(dateLocale(locale), { year: "numeric", month: "short", day: "numeric" });
  return entries.map(entry => {
    const detail = metadata[entry.match_id];
    const status = statusOverrides[entry.match_id] ?? entry.status;
    const added = new Date(entry.created_at_unix_ms);
    const addedLabel = Number.isFinite(added.getTime()) ? formatter.format(added) : copy.unknown;
    const matchDate = detail?.matchDate === null || detail?.matchDate === undefined
      ? copy.unknown : formatter.format(new Date(detail.matchDate));
    const duration = detail?.durationS === null || detail?.durationS === undefined
      ? copy.unknown : copy.duration(formatDuration(detail.durationS));
    const heroNames = detail?.heroNames ?? [];
    return {
      matchId: entry.match_id,
      status,
      statusLabel: replayStatusLabel(status, copy.states),
      canOpen: status.state === "ready",
      canParse: status.state === "available" || status.state === "failed",
      sourceLabel: entry.source === "dota_replays_folder" ? copy.source.dota : copy.source.manual,
      addedDateLabel: copy.dateAdded(addedLabel),
      matchDateLabel: matchDate,
      heroesLabel: heroNames.length ? copy.heroes(heroNames) : copy.unknown,
      durationLabel: duration,
    };
  });
}

export function replayMetadataFromBundle(bundle: MatchBundle, pack: ReplayPatchData | null): ReplayMetadata {
  const meta = bundle.match.meta;
  const endTime = isRecord(meta) && typeof meta.end_time_utc === "string" ? Date.parse(meta.end_time_utc) : NaN;
  const heroes = Array.isArray(bundle.match.heroes) ? bundle.match.heroes : [];
  const heroNames = [...new Set(heroes.flatMap(value => {
    if (!isRecord(value) || typeof value.id !== "string") return [];
    const slug = value.id.replace(/^npc_dota_hero_/, "");
    return [pack?.hero_names[slug] ?? slug.replaceAll("_", " ")];
  }))];
  return {
    matchDate: Number.isFinite(endTime) ? endTime : null,
    heroNames,
    durationS: bundle.durationS,
  };
}

function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "";
  const whole = Math.floor(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const remainder = whole % 60;
  return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${String(remainder).padStart(2, "0")}`
    : `${minutes}:${String(remainder).padStart(2, "0")}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
