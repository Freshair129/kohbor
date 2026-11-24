import type { ReplayLibraryEntry, ReplayStatus } from "./replayLibraryClient";
import type { MatchBundle } from "./matchBundle";
import { replayPackForPatch, type ReplayPatchData } from "./replayPack.mjs";

export interface ReplayMetadata {
  matchDate: number | null;
  ownHero: string | null;
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
    let status = statusOverrides[entry.match_id] ?? entry.status;
    const packLookup = replayPackForPatch(entry.patch);
    if (status.state === "ready" && packLookup.state === "needs_pack") status = { state: "needs_pack" };
    const added = new Date(entry.created_at_unix_ms);
    const addedLabel = Number.isFinite(added.getTime()) ? formatter.format(added) : copy.unknown;
    const entryDate = entry.match_date_utc ? Date.parse(entry.match_date_utc) : NaN;
    const matchDateMs = Number.isFinite(entryDate) ? entryDate : detail?.matchDate;
    const matchDate = matchDateMs === null || matchDateMs === undefined || !Number.isFinite(matchDateMs)
      ? copy.unknown : formatter.format(new Date(matchDateMs));
    const durationS = entry.duration_s ?? detail?.durationS;
    const duration = durationS === null || durationS === undefined
      ? copy.unknown : copy.duration(formatDuration(durationS));
    const ownHero = entry.own_hero ?? detail?.ownHero ?? null;
    const ownHeroSlug = ownHero?.replace(/^npc_dota_hero_/, "") ?? null;
    const ownHeroName = ownHeroSlug
      ? (packLookup.state === "ready" ? packLookup.pack.hero_names[ownHeroSlug] : undefined)
        ?? ownHeroSlug.replaceAll("_", " ")
      : null;
    const heroNames = ownHeroName ? [ownHeroName] : [];
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
  const markedOwn = heroes.find(value => isRecord(value) && (value.own === true || value.is_own === true));
  const ownHeroValue = isRecord(meta) && typeof meta.own_hero === "string"
    ? meta.own_hero
    : isRecord(markedOwn) && typeof markedOwn.id === "string" ? markedOwn.id : null;
  const ownHero = typeof ownHeroValue === "string" && ownHeroValue.startsWith("npc_dota_hero_")
    ? ownHeroValue : null;
  const heroNames = [...new Set(heroes.flatMap(value => {
    if (!isRecord(value) || typeof value.id !== "string") return [];
    const slug = value.id.replace(/^npc_dota_hero_/, "");
    return [pack?.hero_names[slug] ?? slug.replaceAll("_", " ")];
  }))];
  return {
    matchDate: Number.isFinite(endTime) ? endTime : null,
    ownHero,
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
