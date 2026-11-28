// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatDuration, heroName, type HistoryEntry } from "../overview/overviewModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { eventLabel } from "../eventLabels.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatGameClock } from "./scoreboardModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { postGameCopy } from "./copy.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { buildMatchSummaryViewModel, type MatchSummaryViewModel } from "../analytics/knowledge/matchSummaryModel.mjs";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { buildHeroBenchmarksViewModel } from "../analytics/heroBenchmarksModel.mjs";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { gameModeName } from "../replay/matchBundle.ts";

type Locale = "th" | "en";
type Obj = Record<string, unknown>;

export type ReplayParseState = "not_found" | "available" | "queued" | "parsing" | "ready" | "failed" | "needs_pack" | "auto_parse_off" | "unknown"
  | "off" | "waiting_for_match_end" | "looking_up" | "requested" | "downloading" | "not_available_yet" | "expired" | "rate_limited" | "offline";
export type ReplayStateContext = "match" | "unknown";

export interface PostGameReplayStateInput {
  state: ReplayParseState;
  context: ReplayStateContext;
}

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
    level?: number | null;
    netWorth?: number | null;
    items?: (string | null)[] | null;
    backpack?: (string | null)[] | null;
    neutralItem?: string | null;
    gameMode?: number | string | null;
    patch?: string | null;
  };
}

export interface PostGameReplayInput {
  /** Install-keyed history hash. Raw Dota match IDs are never accepted or displayed here. */
  matchKey: string | null;
  /** Raw ID is accepted only from a hash-linked ready bundle. */
  bundleMatchId?: string | null;
  /** Raw ID retained only in native memory for the current GSI session. */
  sessionMatchId?: string | null;
  ready: boolean;
  parseState?: ReplayParseState;
  parseContext?: ReplayStateContext;
  failureReason?: string | null;
  headline: { deaths: number | null; netWorth: number | null };
  digest?: unknown;
  benchmarks?: PostGameBenchmarkInput | null;
  lastFinishedMatch?: LastFinishedMatchView | null;
  watcher?: ReplayWatcherView;
}

export interface PostGameBenchmarkInput {
  heroId: string | null;
  enabled: boolean;
  cached: {
    gold_per_min?: Array<{ percentile: number; value: number }> | null;
    xp_per_min?: Array<{ percentile: number; value: number }> | null;
    kills_per_min?: Array<{ percentile: number; value: number }> | null;
    last_hits_per_min?: Array<{ percentile: number; value: number }> | null;
    hero_damage_per_min?: Array<{ percentile: number; value: number }> | null;
  } | null;
}

export interface DraftRosterSlotView {
  team: "radiant" | "dire";
  slot: number;
  heroId: string | null;
}

export interface ReplayWatcherView {
  state: "unknown";
  folder: "known" | "not_found" | "unknown";
  /** The legacy replay-library list has no scan timestamp. Keep it explicitly unknown. */
  lastScanAtMs: number | null;
}

export type PostGameSummaryLayoutMode = "compact" | "full";

/** Compact the summary when the actual centre-pane box cannot reserve full-height chrome. */
export function postGameSummaryLayoutMode(box: { width: number; height: number }): PostGameSummaryLayoutMode {
  const width = Number.isFinite(box.width) ? Math.max(0, box.width) : 0;
  const height = Number.isFinite(box.height) ? Math.max(0, box.height) : 0;
  return width < 900 || height < 820 ? "compact" : "full";
}

export interface PostGameSummaryInput {
  entry: HistoryEntry | null;
  snapshot: unknown;
  scoreboard: { radiant: number | null; dire: number | null; clockSeconds: number | null };
  recap: { events: readonly unknown[]; droppedCount: number; skippedCount?: number } | null;
  derivedStats: readonly { label: string; value: string | null; sources?: readonly string[]; basisLabel?: string; basisTitle?: string }[];
  replay: PostGameReplayInput | null;
  watcher?: ReplayWatcherView;
  sessionMatchId?: string | null;
  benchmarks?: PostGameBenchmarkInput | null;
  roster?: readonly DraftRosterSlotView[];
  rosterEpoch?: number | null;
  ownSlot?: { team: "radiant" | "dire"; slot: number } | null;
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
  recapSkippedCount: number;
  derivedStats: { label: string; value: string; sources?: string[]; basisLabel?: string; basisTitle?: string }[];
  replayParseState: ReplayParseState;
  replayState: { label: string; hint: string | null; settingsSection: "replay-ai" | null; settingsLinkLabel: string | null };
  replayFailureReason: string | null;
  watcher: ReplayWatcherView;
  lastFinishedMatchAvailable: boolean;
  replay: null | { headline: { label: string; value: string }[]; openMatchKey: string };
  resultBanner: {
    resultLabel: string;
    duration: string;
    gameMode: string;
    patch: string | null;
    finalScore: string;
    matchId: string | null;
  };
  heroHeader: {
    heroId: string | null;
    heroName: string;
    levelValue: number | null;
    level: string;
    kda: string;
    lastHitsDenies: string;
    gpm: string;
    xpm: string;
    netWorth: string;
    netWorthReason: string | null;
    benchmarkComparisons: { key: string; label: string; value: string; percentile: number }[];
  };
  inventory: { slots: (string | null)[]; backpack: (string | null)[]; neutral: string | null };
  stage2: { ready: boolean; digest: unknown | null; openMatchKey: string | null };
  frozenCards: {
    team: "radiant" | "dire";
    slot: number;
    heroId: string | null;
    own: boolean;
    final: true;
    level: number | null;
    kills: number | null;
    deaths: number | null;
    assists: number | null;
    netWorth: number | null;
    lastHits: number | null;
    denies: number | null;
    gpm: number | null;
    xpm: number | null;
    items: string[] | null;
  }[];
}

const HASH_RE = /^[0-9a-f]{64}$/;
const MATCH_ID_RE = /^[0-9]{1,24}$/;
const ITEM_ID_RE = /^item_[a-z0-9_]{1,64}$/;
const PARSE_STATES: readonly ReplayParseState[] = ["not_found", "available", "queued", "parsing", "ready", "failed", "needs_pack", "auto_parse_off", "unknown", "off", "waiting_for_match_end", "looking_up", "requested", "downloading", "not_available_yet", "expired", "rate_limited", "offline"];

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

function safeMatchId(value: unknown): string | null {
  return typeof value === "string" && MATCH_ID_RE.test(value) ? value : null;
}

function itemId(value: unknown): string | null {
  return typeof value === "string" && ITEM_ID_RE.test(value) ? value : null;
}

function itemRow(value: unknown, length: number): (string | null)[] {
  if (!Array.isArray(value)) return Array.from({ length }, () => null);
  return Array.from({ length }, (_, index) => itemId(value[index]));
}

function internalRosterSlot(team: "radiant" | "dire", value: unknown): number | null {
  if (!Number.isSafeInteger(value)) return null;
  const slot = value as number;
  if (team === "dire" && slot >= 128 && slot <= 132) return slot - 123;
  // UI-023 digests and a number of replay parsers store each side as local slots 0..4.
  if (team === "dire" && slot >= 0 && slot <= 4) return slot + 5;
  if (slot >= (team === "radiant" ? 0 : 5) && slot <= (team === "radiant" ? 4 : 9)) return slot;
  return null;
}

function displayGameMode(value: unknown): string {
  if (typeof value === "number" && Number.isSafeInteger(value)) return gameModeName(value) ?? "";
  if (typeof value !== "string") return "";
  const trimmed = value.trim();
  if (!trimmed) return "";
  const numeric = Number(trimmed);
  if (Number.isSafeInteger(numeric)) return gameModeName(numeric) ?? "";
  return trimmed;
}

export function postGameReplayStatePresentation(
  state: ReplayParseState,
  context: ReplayStateContext,
  locale: Locale,
): PostGameSummaryView["replayState"] {
  const text = postGameCopy(locale);
  const label = state === "not_found" ? text.stateNotFound
    : state === "available" ? text.stateAvailable
      : state === "queued" ? text.stateQueued
        : state === "parsing" ? text.stateParsing
          : state === "ready" ? text.stateReady
            : state === "failed" ? text.stateFailed
              : state === "needs_pack" ? text.stateNeedsPack
                : state === "auto_parse_off" ? text.stateAutoParseOff
                  : state === "off" ? text.stateFetchOff
                    : state === "waiting_for_match_end" ? text.stateWaitingForMatchEnd
                      : state === "looking_up" ? text.stateLookingUp
                        : state === "requested" ? text.stateRequested
                          : state === "downloading" ? text.stateDownloading
                            : state === "not_available_yet" ? text.stateNotAvailableYet
                              : state === "expired" ? text.stateExpired
                                : state === "rate_limited" ? text.stateRateLimited
                                  : state === "offline" ? text.stateOffline
                                    : text.stateUnknown;
  const settingsSection = state === "auto_parse_off" || state === "off" ? "replay-ai" as const : null;
  return {
    label,
    hint: state === "not_found" || state === "off" ? text.stateNotFoundHint : null,
    settingsSection,
    settingsLinkLabel: settingsSection ? text.replayAutoParseSettings : null,
  };
}

/** Keep derivation metadata available to the tooltip while the visible cell label stays plain. */
export function postGameDerivedStatPresentation(stat: {
  label: string;
  value: string | null;
  sources?: readonly string[];
  basisLabel?: string;
  basisTitle?: string;
}, withoutTemporaryEffects: string): { label: string; value: string; title: string } {
  const basisLabel = stat.basisLabel?.trim() ?? "";
  const label = basisLabel && stat.label.endsWith(` ${basisLabel}`)
    ? stat.label.slice(0, -basisLabel.length - 1).trimEnd()
    : stat.label;
  return {
    label,
    value: stat.value ?? "—",
    title: [stat.sources?.join(" · "), withoutTemporaryEffects, stat.basisTitle, basisLabel].filter(Boolean).join("\n"),
  };
}

/** Preserve draft identities only for the ended match epoch; the fixed ten slots stay visible when identity is unknown. */
export function frozenRosterSlots(memory: { epoch: number | null; slots: readonly (string | null)[] } | null, endedEpoch: number): DraftRosterSlotView[] {
  const sameMatch = memory?.epoch === endedEpoch;
  return Array.from({ length: 10 }, (_, slot) => ({
    team: slot < 5 ? "radiant" as const : "dire" as const,
    slot,
    heroId: sameMatch ? memory?.slots[slot] ?? null : null,
  }));
}

/** Return the number of event rows that fit while reserving a visible +N more line. */
export function fitTimelineRows(rowHeights: readonly number[], availableHeight: number, moreHeight: number, gap: number): number {
  let used = 0;
  for (let index = 0; index < rowHeights.length; index += 1) {
    const height = Number.isFinite(rowHeights[index]) ? Math.max(0, rowHeights[index]!) : 0;
    const next = used + (index === 0 ? 0 : gap) + height;
    const needsMoreLine = index < rowHeights.length - 1;
    if (next + (needsMoreLine ? gap + moreHeight : 0) > Math.max(0, availableHeight)) return index;
    used = next;
  }
  return rowHeights.length;
}

function benchmarkComparisons(input: PostGameSummaryInput, heroId: string | null, values: {
  kills: number | null; lastHits: number | null; gpm: number | null; xpm: number | null; duration: number | null;
}): PostGameSummaryView["heroHeader"]["benchmarkComparisons"] {
  const source = input.benchmarks;
  if (!source?.enabled || !source.cached || !heroId || source.heroId !== heroId || values.duration === null || values.duration <= 0) return [];
  const durationMinutes = values.duration / 60;
  const view = buildHeroBenchmarksViewModel({
    enabled: true,
    requestState: "ready",
    benchmarks: source.cached,
    observed: {
      gpm: values.gpm,
      xpm: values.xpm,
      lhpm: values.lastHits === null ? null : values.lastHits / durationMinutes,
      kpm: values.kills === null ? null : values.kills / durationMinutes,
    },
  });
  return view.metrics.flatMap(metric => metric.value === null || metric.percentile === null
    ? []
    : [{
      key: metric.key,
      label: metric.label,
      value: new Intl.NumberFormat("en-US", { maximumFractionDigits: 1 }).format(metric.value),
      percentile: metric.percentile,
    }]);
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

function rawHeroRows(digest: unknown): Obj[] {
  if (!isObj(digest) || !Array.isArray(digest.heroes)) return [];
  return digest.heroes.filter(isObj);
}

function frozenCards(input: PostGameSummaryInput, lastMatch: LastFinishedMatchView | null, digest: unknown | null): PostGameSummaryView["frozenCards"] {
  const endedEpoch = lastMatch?.epoch ?? input.rosterEpoch ?? -1;
  const roster = frozenRosterSlots(
    input.roster ? { epoch: input.rosterEpoch ?? null, slots: Array.from({ length: 10 }, (_, index) => input.roster?.find(slot => internalRosterSlot(slot.team, slot.slot) === index)?.heroId ?? null) } : null,
    endedEpoch,
  );
  const rows = rawHeroRows(digest);
  const model: MatchSummaryViewModel | null = digest === null ? null : buildMatchSummaryViewModel(digest);
  const ownSlot = input.ownSlot ? internalRosterSlot(input.ownSlot.team, input.ownSlot.slot) : null;
  return roster.map(slot => {
      const own = input.ownSlot?.team === slot.team && ownSlot === slot.slot;
      const expectedHeroId = own ? lastMatch?.endValues.heroId ?? slot.heroId : slot.heroId;
      const heroRow = rows.find(hero => hero.side === slot.team && internalRosterSlot(slot.team, hero.slot) === slot.slot)
        ?? rows.find(hero => hero.id === expectedHeroId && hero.side === slot.team);
      const digestHeroId = typeof heroRow?.id === "string" ? heroRow.id : expectedHeroId;
      const scoreboardRow = model?.scoreboard[slot.team].rows.find(row => internalRosterSlot(slot.team, row.slot) === slot.slot)
        ?? model?.scoreboard[slot.team].rows.find(row => row.heroId === digestHeroId);
      const end = lastMatch?.endValues;
      const value = (key: keyof LastFinishedMatchView["endValues"], modelKey?: "k" | "d" | "a" | "nw" | "lh" | "dn" | "gpm" | "xpm"): number | null => {
        if (model && digest !== null) return scoreboardRow && modelKey ? numberField(scoreboardRow.metrics[modelKey].value) : null;
        return own && end ? numberField(end[key]) : null;
      };
      const sourceItems = model && digest !== null
        ? scoreboardRow?.itemsKnown ? scoreboardRow.items.map(item => item.id) : null
      : own && end ? end.items?.flatMap(item => itemId(item) ?? []) ?? null : null;
      return {
        team: slot.team,
        slot: slot.slot,
        heroId: model && digest !== null ? scoreboardRow?.heroId ?? digestHeroId : digestHeroId,
        own,
        final: true as const,
        level: model && digest !== null
          ? numberField(heroRow?.level) ?? (own ? numberField(end?.level) : null)
          : own && end ? numberField(end.level) : null,
        kills: value("kills", "k"),
        deaths: value("deaths", "d"),
        assists: value("assists", "a"),
        netWorth: value("netWorth", "nw"),
        lastHits: value("lastHits", "lh"),
        denies: value("denies", "dn"),
        gpm: value("gpm", "gpm"),
        xpm: value("xpm", "xpm"),
        items: sourceItems,
      };
    });
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
  const stage2Digest = replay && isObj(input.replay?.digest) ? input.replay.digest : null;
  const stage2Ready = replay !== null && stage2Digest !== null;
  const result = matchResult(lastMatch?.endValues.result) ?? matchResult(entry?.result);
  const radiantScore = lastMatch ? numberField(lastMatch.endValues.radiantScore) : numberField(input.scoreboard.radiant);
  const direScore = lastMatch ? numberField(lastMatch.endValues.direScore) : numberField(input.scoreboard.dire);
  const mode = stage2Ready && isObj(stage2Digest) && isObj(stage2Digest.meta)
    ? stage2Digest.meta.game_mode
    : lastMatch?.endValues.gameMode;
  const patch = stage2Ready && isObj(stage2Digest) && isObj(stage2Digest.meta)
    ? stage2Digest.meta.patch
    : lastMatch?.endValues.patch;
  const safeMode = displayGameMode(mode);
  const safePatch = typeof patch === "string" && patch.trim().length > 0 ? patch : null;
  const matchId = (stage2Ready ? safeMatchId(input.replay?.bundleMatchId) : null)
    ?? safeMatchId(input.sessionMatchId ?? input.replay?.sessionMatchId)
    ?? "";
  const safeMatch = matchId.length > 0 ? matchId : null;
  const level = endNumber(input, "level", "level");
  const replayNetWorth = stage2Ready ? numberField(input.replay?.headline.netWorth) : null;
  const netWorth = lastMatch
    ? numberField(lastMatch.endValues.netWorth) ?? replayNetWorth
    : ownNumber(input.snapshot, "net_worth", replayNetWorth);
  const netWorthReason = netWorth === null
    ? stage2Ready ? postGameCopy(locale).netWorthReplayUnavailable : postGameCopy(locale).netWorthAfterReplay
    : null;
  const replayParseState = input.replay?.parseState && PARSE_STATES.includes(input.replay.parseState) ? input.replay.parseState : "unknown";
  const replayState = postGameReplayStatePresentation(replayParseState, input.replay?.parseContext ?? "unknown", locale);
  const finalItems = lastMatch?.endValues.items ?? null;
  const backpack = lastMatch?.endValues.backpack ?? null;

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
    recapSkippedCount: numberField(input.recap?.skippedCount) ?? 0,
    derivedStats: input.derivedStats.flatMap(stat => typeof stat.label === "string" && stat.label.length > 0
      ? [{ label: stat.label, value: stat.value ?? "—",
          ...(Array.isArray(stat.sources) ? { sources: stat.sources.filter((source): source is string => typeof source === "string") } : {}),
          ...(typeof stat.basisLabel === "string" ? { basisLabel: stat.basisLabel } : {}),
          ...(typeof stat.basisTitle === "string" ? { basisTitle: stat.basisTitle } : {}) }]
      : []),
    replayParseState,
    replayState,
    replayFailureReason: input.replay?.failureReason ?? null,
    watcher: input.watcher ?? input.replay?.watcher ?? { state: "unknown", folder: "unknown", lastScanAtMs: null },
    lastFinishedMatchAvailable: lastMatch !== null,
    replay,
    resultBanner: {
      resultLabel: result === "win" ? postGameCopy(locale).win : result === "loss" ? postGameCopy(locale).loss : "— ไม่ทราบ",
      duration: formatDuration(duration),
      gameMode: safeMode,
      patch: safePatch,
      finalScore: `Radiant ${fmt(radiantScore, locale)} : ${fmt(direScore, locale)} Dire`,
      matchId: safeMatch,
    },
    heroHeader: {
      heroId,
      heroName: heroId ? heroName(heroId, locale) : "—",
      levelValue: level,
      level: fmt(level, locale),
      kda: `${fmt(kills, locale)} / ${fmt(deaths, locale)} / ${fmt(assists, locale)}`,
      lastHitsDenies: `${fmt(lastHits, locale)} / ${fmt(denies, locale)}`,
      gpm: fmt(gpm, locale),
      xpm: fmt(xpm, locale),
      netWorth: fmt(netWorth, locale),
      netWorthReason,
      benchmarkComparisons: benchmarkComparisons({ ...input, benchmarks: input.benchmarks ?? input.replay?.benchmarks }, heroId, { kills, lastHits, gpm, xpm, duration: numberField(duration) }),
    },
    inventory: {
      slots: itemRow(finalItems, 6),
      backpack: itemRow(backpack, 3),
      neutral: itemId(lastMatch?.endValues.neutralItem),
    },
    stage2: { ready: stage2Ready, digest: stage2Digest, openMatchKey: replay?.openMatchKey ?? null },
    frozenCards: frozenCards(input, lastMatch, stage2Ready ? stage2Digest : null),
  };
}
