// Pure view models for the Overview page (OVERVIEW-001..003): parse the host replies of
// `match_history_get` and `news_feed_get` and shape what the page renders. No I/O.
// Unknown stays unknown: a missing value renders "—", never a default number.

// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { overviewCopy, type OverviewLocale } from "./overviewCopy.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { displayHeroName } from "../deck/heroName.ts";

export type MatchResult = "win" | "loss";
export type MatchType = "ranked" | "unranked" | "bot" | "custom" | "lobby" | "unknown";

export interface HistoryEntry {
  matchKey: string;
  hero: string | null;
  result: MatchResult | null;
  end: "completed" | "abandoned";
  kills: number | null;
  deaths: number | null;
  assists: number | null;
  durationS: number | null;
  endedAt: number;
  matchType: MatchType;
}

export interface HeroSummary {
  hero: string;
  games: number;
  countedGames: number;
  wins: number;
  losses: number;
  winRate: number | null;
}

export interface MatchHistory {
  recorded: number;
  counted: number;
  /** Counted matches whose type is unknown; non-zero means the rate covers all modes. */
  countedUnknownType: number;
  excludedMatchType: number;
  excludedNoResult: number;
  overall: { wins: number; losses: number; winRate: number | null };
  last: { results: (MatchResult | null)[]; wins: number; losses: number; winRate: number | null };
  trend: number[];
  topHeroes: HeroSummary[];
  best: { minGames: number; allTime: BestHero[]; thisMonth: BestHero[] };
  entries: HistoryEntry[];
}

/** One host best-hero row (win-rate ranked from `minGames` counted games, else most played). */
export interface BestHero {
  hero: string;
  basis: "win_rate" | "most_played";
  games: number;
  countedGames: number;
  wins: number;
  losses: number;
  winRate: number | null;
  avgKills: number | null;
  avgDeaths: number | null;
  avgAssists: number | null;
}

type Obj = Record<string, unknown>;

const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const count = (v: unknown): number | null => (typeof v === "number" && Number.isInteger(v) && v >= 0 ? v : null);
const rate = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) && v >= 0 && v <= 100 ? v : null);
const result = (v: unknown): MatchResult | null => (v === "win" || v === "loss" ? v : null);
const average = (v: unknown): number | null => (typeof v === "number" && Number.isFinite(v) && v >= 0 ? v : null);

function parseBest(raw: unknown): BestHero[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((h): BestHero[] => {
    if (!isObj(h) || typeof h.hero !== "string") return [];
    return [{
      hero: h.hero,
      basis: h.basis === "win_rate" ? "win_rate" : "most_played",
      games: count(h.games) ?? 0,
      countedGames: count(h.counted_games) ?? 0,
      wins: count(h.wins) ?? 0,
      losses: count(h.losses) ?? 0,
      winRate: rate(h.win_rate),
      avgKills: average(h.avg_kills),
      avgDeaths: average(h.avg_deaths),
      avgAssists: average(h.avg_assists),
    }];
  }).slice(0, 3);
}
const MATCH_TYPES: readonly MatchType[] = ["ranked", "unranked", "bot", "custom", "lobby", "unknown"];

function parseEntry(raw: unknown): HistoryEntry | null {
  if (!isObj(raw) || typeof raw.match_key !== "string" || raw.source !== "kohbor") return null;
  const endedAt = count(raw.ended_at);
  if (endedAt === null || (raw.end !== "completed" && raw.end !== "abandoned")) return null;
  return {
    matchKey: raw.match_key,
    hero: typeof raw.hero === "string" ? raw.hero : null,
    result: result(raw.result),
    end: raw.end,
    kills: count(raw.kills),
    deaths: count(raw.deaths),
    assists: count(raw.assists),
    durationS: count(raw.duration_s),
    endedAt,
    matchType: MATCH_TYPES.includes(raw.match_type as MatchType) ? (raw.match_type as MatchType) : "unknown",
  };
}

/** Parse the `match_history_get` reply. An unrecognised shape is `null` (shown as unavailable). */
export function parseMatchHistory(raw: unknown): MatchHistory | null {
  if (!isObj(raw) || raw.schema_version !== "1.0.0" || !isObj(raw.summary) || !Array.isArray(raw.entries)) return null;
  const s = raw.summary;
  if (!isObj(s.overall) || !isObj(s.last) || !isObj(s.trend) || !Array.isArray(s.top_heroes)) return null;
  const last = s.last;
  const entries = raw.entries.map(parseEntry).filter((e): e is HistoryEntry => e !== null);
  return {
    recorded: count(s.recorded) ?? entries.length,
    counted: count(s.counted) ?? 0,
    countedUnknownType: count(s.counted_unknown_type) ?? 0,
    excludedMatchType: count(s.excluded_match_type) ?? 0,
    excludedNoResult: count(s.excluded_no_result) ?? 0,
    overall: { wins: count(s.overall.wins) ?? 0, losses: count(s.overall.losses) ?? 0, winRate: rate(s.overall.win_rate) },
    last: {
      results: Array.isArray(last.results) ? last.results.map(result) : [],
      wins: count(last.wins) ?? 0,
      losses: count(last.losses) ?? 0,
      winRate: rate(last.win_rate),
    },
    trend: Array.isArray(s.trend.points) ? s.trend.points.map(rate).filter((p): p is number => p !== null) : [],
    topHeroes: s.top_heroes.flatMap((h): HeroSummary[] => {
      if (!isObj(h) || typeof h.hero !== "string") return [];
      return [{
        hero: h.hero,
        games: count(h.games) ?? 0,
        countedGames: count(h.counted_games) ?? 0,
        wins: count(h.wins) ?? 0,
        losses: count(h.losses) ?? 0,
        winRate: rate(h.win_rate),
      }];
    }),
    best: {
      minGames: isObj(s.best_heroes) ? count(s.best_heroes.min_games) ?? 3 : 3,
      allTime: isObj(s.best_heroes) ? parseBest(s.best_heroes.all_time) : [],
      thisMonth: isObj(s.best_heroes) ? parseBest(s.best_heroes.this_month) : [],
    },
    entries,
  };
}

/** What the page lays out before the host answers or when it cannot: every count is zero. */
export const EMPTY_HISTORY: MatchHistory = {
  recorded: 0, counted: 0, countedUnknownType: 0, excludedMatchType: 0, excludedNoResult: 0,
  overall: { wins: 0, losses: 0, winRate: null },
  last: { results: [], wins: 0, losses: 0, winRate: null },
  trend: [], topHeroes: [], best: { minGames: 3, allTime: [], thisMonth: [] }, entries: [],
};

/** Unix seconds of the start of the local calendar month containing `now` (sent to the host). */
export function localMonthStart(now: Date): number {
  return Math.floor(new Date(now.getFullYear(), now.getMonth(), 1, 0, 0, 0, 0).getTime() / 1000);
}

export function percent(value: number | null): string {
  return value === null ? "—" : `${value.toFixed(1)}%`;
}

export function formatDuration(seconds: number | null): string {
  if (seconds === null) return "—";
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function dateLocale(locale: OverviewLocale): string {
  return locale === "th" ? "th-TH-u-ca-gregory" : "en-GB";
}

export function formatDay(unixSeconds: number, locale: OverviewLocale): string {
  return new Intl.DateTimeFormat(dateLocale(locale), { day: "numeric", month: "short" }).format(new Date(unixSeconds * 1000));
}

export function formatDate(unixSeconds: number, locale: OverviewLocale): string {
  return new Intl.DateTimeFormat(dateLocale(locale), { day: "numeric", month: "short", year: "numeric" }).format(new Date(unixSeconds * 1000));
}

export function formatDateTime(unixSeconds: number, locale: OverviewLocale): string {
  return new Intl.DateTimeFormat(dateLocale(locale), { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit", hour12: false })
    .format(new Date(unixSeconds * 1000));
}

/** SVG polyline points (viewBox 0 0 300 h) for rolling rates; one point draws a flat line. */
export function trendPolyline(points: readonly number[], height = 64, width = 300): string | null {
  if (points.length === 0) return null;
  const y = (v: number) => (height - 4 - (v / 100) * (height - 8)).toFixed(1);
  if (points.length === 1) return `0,${y(points[0]!)} ${width},${y(points[0]!)}`;
  return points.map((v, i) => `${((i / (points.length - 1)) * width).toFixed(1)},${y(v)}`).join(" ");
}

export function heroName(hero: string | null, locale: OverviewLocale): string {
  return displayHeroName(hero)?.name ?? overviewCopy(locale).stats.unknownHero;
}

export const LAST_SLOTS = 20;

export interface DonutSegment { kind: "win" | "loss" | "none"; value: number; length: number; offset: number }

export interface TileView {
  overall: { value: string; sub: string; known: boolean; scope: string | null };
  /** Always 20 slots, oldest to newest from the left; null slots are drawn as empty outlines. */
  last: { title: string; value: string; sub: string; slots: (MatchResult | null)[] };
  /** Wins, losses and no-result over recorded matches; segment lengths are on a 100-unit ring. */
  donut: { total: number; wins: number; losses: number; noResult: number; segments: DonutSegment[] };
  trend: { points: string | null; label: string | null; need: string | null };
}

/** Owner 2026-11-23 (DEC-OVERVIEW rev 3): the whole layout always renders; with no counted match
 * the rate reads "0.0%" over a base of 0 matches, still labelled all modes. */
export function statTiles(history: MatchHistory, locale: OverviewLocale): TileView {
  const t = overviewCopy(locale).stats;
  const filled = history.last.results.filter((r): r is MatchResult => r !== null).slice(0, LAST_SLOTS).reverse();
  const slots: (MatchResult | null)[] = [...filled, ...Array<null>(LAST_SLOTS - filled.length).fill(null)];
  const points = history.trend.length > 0 && history.counted >= 10 ? trendPolyline(history.trend) : null;
  const latest = history.trend.at(-1);
  const zero = (v: number | null) => (v === null ? "0.0%" : percent(v));
  const noResult = history.excludedNoResult;
  const total = history.overall.wins + history.overall.losses + noResult;
  const segments: DonutSegment[] = [];
  let offset = 0;
  for (const [kind, value] of [["win", history.overall.wins], ["loss", history.overall.losses], ["none", noResult]] as const) {
    if (value <= 0 || total === 0) continue;
    const length = (value / total) * 100;
    segments.push({ kind, value, length, offset });
    offset += length;
  }
  return {
    overall: {
      value: zero(history.overall.winRate),
      sub: t.overallSub(history.overall.wins, history.overall.losses, history.counted),
      known: history.overall.winRate !== null,
      // Owner 2026-11-23 "Count all, labelled": unknown-type matches count, so say it covers all modes.
      scope: history.counted === 0 || history.countedUnknownType > 0 ? t.allModes : null,
    },
    last: {
      title: t.lastTitle(filled.length),
      value: zero(history.last.winRate),
      sub: t.lastSub(history.last.wins, history.last.losses),
      slots,
    },
    donut: { total, wins: history.overall.wins, losses: history.overall.losses, noResult, segments },
    trend: {
      points,
      label: points === null || latest === undefined ? null : t.trendLabel(percent(latest)),
      need: points === null ? t.trendNeed(history.counted) : null,
    },
  };
}

/** Deterministic placeholder heroes for empty best-hero slots (never counted, always labelled). */
export const PLACEHOLDER_HEROES: readonly string[] = ["npc_dota_hero_pudge", "npc_dota_hero_juggernaut", "npc_dota_hero_crystal_maiden"];

export interface BestHeroCard {
  hero: string;
  name: string;
  placeholder: boolean;
  /** Short label on the card; the full ranking rule is in `basisTitle`. */
  basis: string | null;
  basisTitle: string | null;
  games: string;
  record: string;
  rate: string;
  kda: string;
  title: string | null;
}

/** Always three cards: the host's ranked heroes, then labelled placeholders with every stat at zero. */
export function bestHeroPanel(heroes: readonly BestHero[], minGames: number, locale: OverviewLocale): BestHeroCard[] {
  const t = overviewCopy(locale).stats;
  const one = (v: number | null) => (v === null ? "—" : v.toFixed(1));
  const cards: BestHeroCard[] = heroes.slice(0, 3).map((h) => ({
    hero: h.hero,
    name: heroName(h.hero, locale),
    placeholder: false,
    basis: h.basis === "win_rate" ? t.basisRateShort : t.basisMostPlayedShort,
    basisTitle: h.basis === "win_rate" ? t.basisRate(minGames) : t.basisMostPlayed(minGames),
    games: t.games(h.games),
    record: `${h.wins}–${h.losses}`,
    rate: percent(h.winRate),
    kda: `${one(h.avgKills)} / ${one(h.avgDeaths)} / ${one(h.avgAssists)}`,
    title: null,
  }));
  for (const hero of PLACEHOLDER_HEROES) {
    if (cards.length >= 3) break;
    if (cards.some((c) => c.hero === hero)) continue;
    cards.push({
      hero, name: heroName(hero, locale), placeholder: true, basis: null, basisTitle: null,
      games: t.games(0), record: "0–0", rate: "0.0%", kda: "0.0 / 0.0 / 0.0", title: t.placeholderTip,
    });
  }
  return cards;
}

/** Rows to draw so the table keeps its shape: real rows, then faint empty rows up to five. */
export const MIN_TABLE_ROWS = 5;
export function placeholderRowCount(realRows: number): number {
  return Math.max(0, MIN_TABLE_ROWS - realRows);
}

/** The honest counting note under the tiles: the rule and what was left out. */
export function countingNotes(history: MatchHistory, locale: OverviewLocale): string[] {
  const t = overviewCopy(locale).stats;
  const notes = [t.countRule];
  if (history.excludedMatchType > 0) notes.push(t.excludedType(history.excludedMatchType));
  if (history.excludedNoResult > 0) notes.push(t.excludedNoResult(history.excludedNoResult));
  return notes;
}

export interface HeroCardView {
  hero: string;
  name: string;
  games: string;
  /** "73.3%" from five counted games, otherwise wins–losses ("2–1"), or "—" when none counted. */
  record: string;
  tone: "up" | "down" | "neutral";
  usesRate: boolean;
}

export function heroCards(history: MatchHistory, locale: OverviewLocale): HeroCardView[] {
  const t = overviewCopy(locale).stats;
  return history.topHeroes.map((h) => {
    const usesRate = h.winRate !== null;
    return {
      hero: h.hero,
      name: heroName(h.hero, locale),
      games: t.games(h.games),
      record: usesRate ? percent(h.winRate) : h.countedGames > 0 ? `${h.wins}–${h.losses}` : "—",
      tone: usesRate ? (h.winRate! >= 50 ? "up" : "down") : "neutral",
      usesRate,
    };
  });
}

export interface MatchRowView {
  key: string;
  hero: string | null;
  heroName: string;
  result: "W" | "L" | "—";
  resultTone: "w" | "l" | "unknown";
  resultTitle: string;
  kda: string;
  duration: string;
  date: string;
  counted: boolean;
  typeTitle: string | null;
}

export function matchRows(history: MatchHistory, locale: OverviewLocale, limit: number | null): MatchRowView[] {
  const t = overviewCopy(locale).stats;
  const entries = limit === null ? history.entries : history.entries.slice(0, limit);
  const n = (v: number | null) => (v === null ? "—" : String(v));
  return entries.map((e) => {
    // Same rule as the host: ranked, unranked and unknown type count; bot, custom and lobby do not.
    const counted = (e.matchType === "ranked" || e.matchType === "unranked" || e.matchType === "unknown") && e.result !== null;
    return {
      key: e.matchKey,
      hero: e.hero,
      heroName: heroName(e.hero, locale),
      result: e.result === "win" ? "W" : e.result === "loss" ? "L" : "—",
      resultTone: e.result === "win" ? "w" : e.result === "loss" ? "l" : "unknown",
      resultTitle: e.result === "win" ? t.win : e.result === "loss" ? t.loss : `${t.unknownResult} · ${t.abandoned}`,
      kda: `${n(e.kills)} / ${n(e.deaths)} / ${n(e.assists)}`,
      duration: formatDuration(e.durationS),
      date: formatDay(e.endedAt, locale),
      counted,
      typeTitle: e.matchType === "unknown" ? t.typeUnknown : null,
    };
  });
}

// ---------------------------------------------------------------------------------------------
// News

export type NewsType = "announcement" | "app_update" | "patch_notes";
export type NewsFilter = "all" | NewsType;
export type NewsState = "loading" | "ok" | "offline" | "unavailable" | "disabled";
export interface Localized { th: string; en: string }

export interface NewsPost {
  id: string;
  type: NewsType;
  publishedAt: number;
  pinned: boolean;
  title: Localized;
  summary: Localized;
  body: Localized | null;
  image: { dataUrl: string | null; width: number; height: number; alt: Localized } | null;
  /** `index` is the link's position in the host feed; the host resolves the URL from it. */
  links: { label: Localized; index: number }[];
  appVersion: string | null;
  gamePatch: string | null;
}

export interface NewsView {
  state: NewsState;
  enabled: boolean;
  disclosureSeen: boolean;
  feedUrl: string;
  fetchedAt: number | null;
  posts: NewsPost[];
}

const NEWS_TYPES: readonly NewsType[] = ["announcement", "app_update", "patch_notes"];
const NEWS_STATES: readonly NewsState[] = ["loading", "ok", "offline", "unavailable", "disabled"];

function localized(v: unknown): Localized | null {
  return isObj(v) && typeof v.th === "string" && typeof v.en === "string" ? { th: v.th, en: v.en } : null;
}

function parsePost(raw: unknown): NewsPost | null {
  if (!isObj(raw) || typeof raw.id !== "string" || !NEWS_TYPES.includes(raw.type as NewsType)) return null;
  const title = localized(raw.title);
  const summary = localized(raw.summary);
  const publishedAt = count(raw.published_at);
  if (!title || !summary || publishedAt === null) return null;
  let image: NewsPost["image"] = null;
  if (isObj(raw.image)) {
    const alt = localized(raw.image.alt);
    const dataUrl = typeof raw.image.data_url === "string" && /^data:image\/(png|jpeg|webp);base64,/.test(raw.image.data_url)
      ? raw.image.data_url : null;
    if (alt) image = { dataUrl, width: count(raw.image.width) ?? 0, height: count(raw.image.height) ?? 0, alt };
  }
  return {
    id: raw.id,
    type: raw.type as NewsType,
    publishedAt,
    pinned: raw.pinned === true,
    title,
    summary,
    body: localized(raw.body),
    image,
    links: Array.isArray(raw.links) ? raw.links.flatMap((l, index) => {
      const label = isObj(l) ? localized(l.label) : null;
      return label ? [{ label, index }] : [];
    }) : [],
    appVersion: typeof raw.app_version === "string" ? raw.app_version : null,
    gamePatch: typeof raw.game_patch === "string" ? raw.game_patch : null,
  };
}

/** Parse the `news_feed_get` reply; an unrecognised shape is `null`. */
export function parseNewsView(raw: unknown): NewsView | null {
  if (!isObj(raw) || !NEWS_STATES.includes(raw.state as NewsState) || !Array.isArray(raw.posts)) return null;
  return {
    state: raw.state as NewsState,
    enabled: raw.enabled === true,
    disclosureSeen: raw.disclosure_seen === true,
    feedUrl: typeof raw.feed_url === "string" ? raw.feed_url : "",
    fetchedAt: count(raw.fetched_at),
    posts: raw.posts.map(parsePost).filter((p): p is NewsPost => p !== null),
  };
}

export const NEWS_FILTERS: readonly NewsFilter[] = ["all", "announcement", "app_update", "patch_notes"];
/** Posts shown in the side column before "All news" expands the list in place. */
export const NEWS_COLUMN_POSTS = 4;

export interface NewsColumn {
  counts: Record<NewsFilter, number>;
  pinned: NewsPost | null;
  posts: NewsPost[];
  hidden: number;
}

/** Pinned post first (when it matches the filter), then newest first; collapsed to four unless expanded. */
export function newsColumn(posts: readonly NewsPost[], filter: NewsFilter, expanded: boolean): NewsColumn {
  const counts: Record<NewsFilter, number> = { all: posts.length, announcement: 0, app_update: 0, patch_notes: 0 };
  for (const p of posts) counts[p.type] += 1;
  const matching = posts.filter((p) => filter === "all" || p.type === filter).slice().sort((a, b) => b.publishedAt - a.publishedAt);
  const pinned = matching.find((p) => p.pinned) ?? null;
  const rest = matching.filter((p) => p !== pinned);
  const room = expanded ? rest.length : Math.max(0, NEWS_COLUMN_POSTS - (pinned ? 1 : 0));
  return { counts, pinned, posts: rest.slice(0, room), hidden: Math.max(0, rest.length - room) };
}

export function pick(text: Localized, locale: OverviewLocale): string {
  return locale === "en" ? text.en : text.th;
}
