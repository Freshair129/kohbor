// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatDuration, heroName, type HistoryEntry } from "../overview/overviewModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatGameClock } from "./scoreboardModel.ts";

type Locale = "th" | "en";
type Obj = Record<string, unknown>;

export interface PostGameReplayInput {
  /** Install-keyed history hash. Raw Dota match IDs are never accepted or displayed here. */
  matchKey: string;
  ready: boolean;
  headline: { deaths: number | null; netWorth: number | null };
}

export interface PostGameSummaryInput {
  entry: HistoryEntry | null;
  snapshot: unknown;
  scoreboard: { radiant: number | null; dire: number | null; clockSeconds: number | null };
  recap: { events: readonly unknown[]; droppedCount: number } | null;
  derivedStats: readonly { label: string; value: string | null }[];
  replay: PostGameReplayInput | null;
}

export interface PostGameSummaryView {
  result: "win" | "loss" | "—";
  duration: string;
  heroName: string;
  kda: string;
  lastHitsDenies: string;
  gpmXpm: string;
  finalScore: string;
  recapAvailable: boolean;
  recapEvents: { event: string; time: string; kind: string; outcome: string }[];
  recapDroppedCount: number;
  derivedStats: { label: string; value: string }[];
  replay: null | { headline: { label: string; value: string }[]; openMatchKey: string };
}

const HASH_RE = /^[0-9a-f]{64}$/;

function isObj(value: unknown): value is Obj {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function snapshotField(snapshot: unknown, key: string): unknown {
  if (!isObj(snapshot) || snapshot.availability !== "fresh" || !isObj(snapshot.own) || !isObj(snapshot.own.fields)) return null;
  const field = snapshot.own.fields[key];
  return isObj(field) && field.availability === "fresh" ? field.value : null;
}

function numberField(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : null;
}

function ownNumber(snapshot: unknown, key: string, fallback: number | null): number | null {
  return numberField(snapshotField(snapshot, key)) ?? fallback;
}

function resultFromGsi(snapshot: unknown, radiantScore: number | null, direScore: number | null): "win" | "loss" | null {
  if (!isObj(snapshot) || snapshot.availability !== "fresh" || !isObj(snapshot.own)
    || (snapshot.own.team !== "radiant" && snapshot.own.team !== "dire")
    || radiantScore === null || direScore === null || radiantScore === direScore) return null;
  const ownScore = snapshot.own.team === "radiant" ? radiantScore : direScore;
  const otherScore = snapshot.own.team === "radiant" ? direScore : radiantScore;
  return ownScore > otherScore ? "win" : "loss";
}

function fmt(value: number | null, locale: Locale): string {
  if (value === null) return "—";
  return new Intl.NumberFormat(locale === "th" ? "th-TH" : "en-US", { maximumFractionDigits: 0 }).format(value);
}

function eventRows(recap: PostGameSummaryInput["recap"]): PostGameSummaryView["recapEvents"] {
  if (!recap) return [];
  return recap.events.flatMap((raw): PostGameSummaryView["recapEvents"] => {
    if (!isObj(raw) || typeof raw.event_type !== "string" || typeof raw.audio_class !== "string" || typeof raw.outcome !== "string") return [];
    return [{
      event: raw.event_type,
      time: formatGameClock(numberField(raw.game_clock_seconds)),
      kind: raw.audio_class,
      outcome: raw.outcome,
    }];
  });
}

function replayPanel(entry: HistoryEntry | null, replay: PostGameReplayInput | null, locale: Locale): PostGameSummaryView["replay"] {
  if (!entry || !replay?.ready || !HASH_RE.test(entry.matchKey) || replay.matchKey !== entry.matchKey) return null;
  return {
    headline: [
      { label: locale === "th" ? "จำนวนครั้งที่ตาย" : "Deaths", value: fmt(numberField(replay.headline.deaths), locale) },
      { label: locale === "th" ? "มูลค่าสุทธิ" : "Net worth", value: fmt(numberField(replay.headline.netWorth), locale) },
    ],
    openMatchKey: entry.matchKey,
  };
}

export function postGameSummaryModel(input: PostGameSummaryInput, locale: Locale): PostGameSummaryView {
  const clockSeconds = numberField(input.scoreboard.clockSeconds);
  const entry = input.entry;
  const hero = snapshotField(input.snapshot, "hero_id");
  const heroId = typeof hero === "string" && hero.length > 0 ? hero : entry?.hero ?? null;
  const kills = ownNumber(input.snapshot, "kills", entry?.kills ?? null);
  const deaths = ownNumber(input.snapshot, "deaths", entry?.deaths ?? null);
  const assists = ownNumber(input.snapshot, "assists", entry?.assists ?? null);
  const lastHits = ownNumber(input.snapshot, "last_hits", null);
  const denies = ownNumber(input.snapshot, "denies", null);
  const gpm = ownNumber(input.snapshot, "gpm", null);
  const xpm = ownNumber(input.snapshot, "xpm", null);
  const duration = numberField(entry?.durationS) ?? (clockSeconds !== null ? clockSeconds : null);
  const replay = replayPanel(entry, input.replay, locale);
  const result = entry?.result ?? resultFromGsi(input.snapshot, input.scoreboard.radiant, input.scoreboard.dire) ?? "—";

  return {
    result,
    duration: formatDuration(duration),
    heroName: heroId ? heroName(heroId, locale) : "—",
    kda: `${fmt(kills, locale)} / ${fmt(deaths, locale)} / ${fmt(assists, locale)}`,
    lastHitsDenies: `${fmt(lastHits, locale)} / ${fmt(denies, locale)}`,
    gpmXpm: `${fmt(gpm, locale)} / ${fmt(xpm, locale)}`,
    finalScore: `${fmt(numberField(input.scoreboard.radiant), locale)} — ${fmt(numberField(input.scoreboard.dire), locale)}`,
    recapAvailable: input.recap !== null,
    recapEvents: eventRows(input.recap),
    recapDroppedCount: numberField(input.recap?.droppedCount) ?? 0,
    derivedStats: input.derivedStats.flatMap(stat => typeof stat.label === "string" && stat.label.length > 0
      ? [{ label: stat.label, value: stat.value ?? "—" }]
      : []),
    replay,
  };
}
