// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { allModesScopeLabel, formatDuration, heroName, type HistoryEntry, type MatchHistory } from "../overview/overviewModel.ts";

export interface PlayDaySummary {
  matches: number;
  wins: number;
  losses: number;
  noResult: number;
  winRate: number | null;
}

export interface RecentMatchCard {
  hero: string | null;
  heroName: string;
  result: "win" | "loss" | null;
  kda: string;
  duration: string;
}

export interface BestHeroThisWeek {
  hero: string;
  heroName: string;
  games: number;
  wins: number;
  losses: number;
  winRate: number;
}

export interface PregameDashboardView {
  state: "empty" | "ready" | "unavailable";
  emptyMessage: string | null;
  today: PlayDaySummary;
  yesterday: PlayDaySummary;
  /** Matches the Overview all-modes label when local history has unknown match types. */
  scopeLabel: string | null;
  lastMatch: RecentMatchCard | null;
  bestHeroThisWeek: BestHeroThisWeek | null;
}

const EMPTY_DAY: PlayDaySummary = { matches: 0, wins: 0, losses: 0, noResult: 0, winRate: null };
const COUNTED_TYPES = new Set(["ranked", "unranked", "unknown"]);
function rangeForLocalDay(now: Date, offset: number): readonly [number, number] {
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset).getTime() / 1000;
  const end = new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset + 1).getTime() / 1000;
  return [start, end];
}

function summarizeDay(entries: readonly HistoryEntry[], start: number, end: number): PlayDaySummary {
  const inDay = entries.filter(entry => entry.endedAt >= start && entry.endedAt < end);
  const wins = inDay.filter(entry => entry.result === "win").length;
  const losses = inDay.filter(entry => entry.result === "loss").length;
  return {
    matches: inDay.length,
    wins,
    losses,
    noResult: inDay.length - wins - losses,
    winRate: wins + losses === 0 ? null : Math.round((wins / (wins + losses)) * 1_000) / 10,
  };
}

function localEntries(history: MatchHistory): HistoryEntry[] {
  return history.entries.filter(entry => entry.source === "kohbor").slice().sort((a, b) => b.endedAt - a.endedAt);
}

function recentCard(entry: HistoryEntry, locale: "th" | "en"): RecentMatchCard {
  const stat = (value: number | null) => value === null ? "—" : String(value);
  return {
    hero: entry.hero,
    heroName: heroName(entry.hero, locale),
    result: entry.result,
    kda: `${stat(entry.kills)} / ${stat(entry.deaths)} / ${stat(entry.assists)}`,
    duration: formatDuration(entry.durationS),
  };
}

function weekStart(now: Date): number {
  const mondayOffset = (now.getDay() + 6) % 7;
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() - mondayOffset).getTime() / 1000;
}

function bestHero(entries: readonly HistoryEntry[], now: Date, locale: "th" | "en"): BestHeroThisWeek | null {
  const start = weekStart(now);
  const end = now.getTime() / 1000;
  const grouped = new Map<string, { wins: number; losses: number }>();
  for (const entry of entries) {
    if (entry.endedAt < start || entry.endedAt >= end || entry.hero === null || entry.result === null || !COUNTED_TYPES.has(entry.matchType)) continue;
    const current = grouped.get(entry.hero) ?? { wins: 0, losses: 0 };
    if (entry.result === "win") current.wins += 1;
    else current.losses += 1;
    grouped.set(entry.hero, current);
  }
  const candidates = [...grouped.entries()]
    .map(([hero, value]) => ({ hero, ...value, games: value.wins + value.losses }))
    .filter(candidate => candidate.games >= 3)
    .sort((left, right) => right.wins / right.games - left.wins / left.games || right.games - left.games || left.hero.localeCompare(right.hero));
  const first = candidates[0];
  if (!first) return null;
  return {
    hero: first.hero,
    heroName: heroName(first.hero, locale),
    games: first.games,
    wins: first.wins,
    losses: first.losses,
    winRate: Math.round((first.wins / first.games) * 1_000) / 10,
  };
}

export function pregameDashboardModel(history: MatchHistory | null, now: Date, locale: "th" | "en"): PregameDashboardView {
  if (!history) {
    return { state: "unavailable", emptyMessage: null, today: EMPTY_DAY, yesterday: EMPTY_DAY,
      scopeLabel: null, lastMatch: null, bestHeroThisWeek: null };
  }

  const entries = localEntries(history);
  const [todayStart, todayEnd] = rangeForLocalDay(now, 0);
  const [yesterdayStart, yesterdayEnd] = rangeForLocalDay(now, -1);
  const counted = entries.filter(entry => entry.result !== null && COUNTED_TYPES.has(entry.matchType));
  const unknownCount = counted.filter(entry => entry.matchType === "unknown").length;
  const hasAnyCounted = counted.length > 0;
  const empty = entries.length === 0;
  return {
    state: empty ? "empty" : "ready",
    emptyMessage: empty ? locale === "th" ? "ยังไม่มีแมตช์ที่บันทึก" : "No matches recorded yet" : null,
    today: summarizeDay(entries, todayStart, todayEnd),
    yesterday: summarizeDay(entries, yesterdayStart, yesterdayEnd),
    scopeLabel: !hasAnyCounted || unknownCount > 0 ? allModesScopeLabel(locale) : null,
    lastMatch: entries[0] ? recentCard(entries[0], locale) : null,
    bestHeroThisWeek: bestHero(entries, now, locale),
  };
}
