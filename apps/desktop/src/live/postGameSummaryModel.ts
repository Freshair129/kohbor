// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatDuration, heroName, type HistoryEntry } from "../overview/overviewModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { eventLabel } from "../eventLabels.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatGameClock } from "./scoreboardModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { postGameCopy } from "./copy.ts";

type Locale = "th" | "en";
type Obj = Record<string, unknown>;

export type ReplayParseState = "not_found" | "available" | "queued" | "parsing" | "ready" | "failed" | "needs_pack" | "unknown";

export interface LastFinishedMatchView {
  epoch: number;
  matchKey: string;
  endedAt: number;
  endValues: {
    heroId: string | null;
    result: "win" | "loss" | null;
    kills: number | null;
    deaths: number | null;
    assists: number | null;
    lastHits: number | null;
    denies: number | null;
    gpm: number | null;
    xpm: number | null;
    durationSeconds: number | null;
    radiantScore: number | null;
    direScore: number | null;
  };
}

export interface PostGameReplayInput {
  /** Install-keyed history hash. Raw Dota match IDs are never accepted or displayed here. */
  matchKey: string | null;
  ready: boolean;
  parseState?: ReplayParseState;
  failureReason?: string | null;
  headline: { deaths: number | null; netWorth: number | null };
  lastFinishedMatch?: LastFinishedMatchView | null;
  watcher?: ReplayWatcherView;
}

export interface ReplayWatcherView {
  state: "unknown";
  folder: "known" | "not_found" | "unknown";
  /** The legacy replay-library list has no scan timestamp. Keep it explicitly unknown. */
  lastScanAtMs: number | null;
}

export interface PostGameSummaryInput {
  entry: HistoryEntry | null;
  snapshot: unknown;
  scoreboard: { radiant: number | null; dire: number | null; clockSeconds: number | null };
  recap: { events: readonly unknown[]; droppedCount: number } | null;
  derivedStats: readonly { label: string; value: string | null }[];
  replay: PostGameReplayInput | null;
  watcher?: ReplayWatcherView;
}

export interface PostGameSummaryView {
  result: "win" | "loss" | null;
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
  replayParseState: ReplayParseState;
  replayFailureReason: string | null;
  watcher: ReplayWatcherView;
  lastFinishedMatchAvailable: boolean;
  replay: null | { headline: { label: string; value: string }[]; openMatchKey: string };
}

const HASH_RE = /^[0-9a-f]{64}$/;
const PARSE_STATES: readonly ReplayParseState[] = ["not_found", "available", "queued", "parsing", "ready", "failed", "needs_pack", "unknown"];

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

function endNumber(input: PostGameSummaryInput, key: keyof LastFinishedMatchView["endValues"], snapshotKey: string, fallback: number | null = null): number | null {
  const endValues = input.replay?.lastFinishedMatch?.endValues;
  if (endValues) return numberField(endValues[key]);
  return ownNumber(input.snapshot, snapshotKey, fallback);
}

function fmt(value: number | null, locale: Locale): string {
  if (value === null) return "—";
  return new Intl.NumberFormat(locale === "th" ? "th-TH" : "en-US", { maximumFractionDigits: 0 }).format(value);
}

function localizedEvent(id: string, locale: Locale): string {
  const label = eventLabel(id, locale);
  return label === id ? postGameCopy(locale).unknownEvent : label;
}

function localizedOutcome(value: string, locale: Locale): string {
  const text = postGameCopy(locale);
  switch (value) {
    case "played":
    case "admitted": return text.outcomePlayed;
    case "preempted": return text.outcomePreempted;
    case "blocked":
    case "suppressed":
    case "skipped": return text.outcomeNotPlayed;
    case "cut": return text.outcomeCut;
    case "silent": return text.outcomeSilent;
    default: return text.outcomeUnknown;
  }
}

function localizedEventKind(value: string, locale: Locale): string {
  const text = postGameCopy(locale);
  switch (value) {
    case "critical": return text.eventKindCritical;
    case "ordinary": return text.eventKindOrdinary;
    case "voice": return text.eventKindVoice;
    default: return text.eventKind;
  }
}

function matchResult(value: unknown): "win" | "loss" | null {
  return value === "win" || value === "loss" ? value : null;
}

function eventRows(recap: PostGameSummaryInput["recap"], locale: Locale): PostGameSummaryView["recapEvents"] {
  if (!recap) return [];
  return recap.events.flatMap((raw): PostGameSummaryView["recapEvents"] => {
    if (!isObj(raw) || typeof raw.event_type !== "string" || typeof raw.audio_class !== "string" || typeof raw.outcome !== "string") return [];
    return [{
      event: localizedEvent(raw.event_type, locale),
      time: formatGameClock(numberField(raw.game_clock_seconds)),
      kind: localizedEventKind(raw.audio_class, locale),
      outcome: localizedOutcome(raw.outcome, locale),
    }];
  });
}

function replayPanel(entry: HistoryEntry | null, replay: PostGameReplayInput | null, locale: Locale): PostGameSummaryView["replay"] {
  const expectedMatchKey = replay?.lastFinishedMatch?.matchKey ?? entry?.matchKey ?? null;
  if (!replay?.ready || !expectedMatchKey || !HASH_RE.test(expectedMatchKey) || replay.matchKey !== expectedMatchKey) return null;
  return {
    headline: [
      { label: locale === "th" ? "จำนวนครั้งที่ตาย" : "Deaths", value: fmt(numberField(replay.headline.deaths), locale) },
      { label: locale === "th" ? "มูลค่าสุทธิ" : "Net worth", value: fmt(numberField(replay.headline.netWorth), locale) },
    ],
    openMatchKey: expectedMatchKey,
  };
}

export function postGameSummaryModel(input: PostGameSummaryInput, locale: Locale): PostGameSummaryView {
  const entry = input.entry;
  const lastMatch = input.replay?.lastFinishedMatch ?? null;
  const hero = lastMatch ? lastMatch.endValues.heroId : snapshotField(input.snapshot, "hero_id");
  const heroId = typeof hero === "string" && hero.length > 0 ? hero : entry?.hero ?? null;
  const kills = endNumber(input, "kills", "kills", entry?.kills ?? null);
  const deaths = endNumber(input, "deaths", "deaths", entry?.deaths ?? null);
  const assists = endNumber(input, "assists", "assists", entry?.assists ?? null);
  const lastHits = endNumber(input, "lastHits", "last_hits");
  const denies = endNumber(input, "denies", "denies");
  const gpm = endNumber(input, "gpm", "gpm");
  const xpm = endNumber(input, "xpm", "xpm");
  const duration = lastMatch
    ? numberField(lastMatch.endValues.durationSeconds)
    : numberField(entry?.durationS) ?? numberField(input.scoreboard.clockSeconds);
  const replay = replayPanel(entry, input.replay, locale);
  const result = matchResult(lastMatch?.endValues.result) ?? matchResult(entry?.result);
  const radiantScore = lastMatch ? numberField(lastMatch.endValues.radiantScore) : numberField(input.scoreboard.radiant);
  const direScore = lastMatch ? numberField(lastMatch.endValues.direScore) : numberField(input.scoreboard.dire);

  return {
    result,
    duration: formatDuration(duration),
    heroName: heroId ? heroName(heroId, locale) : "—",
    kda: `${fmt(kills, locale)} / ${fmt(deaths, locale)} / ${fmt(assists, locale)}`,
    lastHitsDenies: `${fmt(lastHits, locale)} / ${fmt(denies, locale)}`,
    gpmXpm: `${fmt(gpm, locale)} / ${fmt(xpm, locale)}`,
    finalScore: `Radiant ${fmt(radiantScore, locale)} : ${fmt(direScore, locale)} Dire`,
    recapAvailable: input.recap !== null,
    recapEvents: eventRows(input.recap, locale),
    recapDroppedCount: numberField(input.recap?.droppedCount) ?? 0,
    derivedStats: input.derivedStats.flatMap(stat => typeof stat.label === "string" && stat.label.length > 0
      ? [{ label: stat.label, value: stat.value ?? "—" }]
      : []),
    replayParseState: input.replay?.parseState && PARSE_STATES.includes(input.replay.parseState) ? input.replay.parseState : "unknown",
    replayFailureReason: input.replay?.failureReason ?? null,
    watcher: input.watcher ?? input.replay?.watcher ?? { state: "unknown", folder: "unknown", lastScanAtMs: null },
    lastFinishedMatchAvailable: lastMatch !== null,
    replay,
  };
}
