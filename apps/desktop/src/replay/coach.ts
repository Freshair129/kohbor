import type { CoachDeath, CoachNote, CoachRules, DeathType, LaneName, PostMatchReplay } from "./report";

/** Sample-size rule (DEC-COACH-REVIEW): below this many matches, patterns are shown as k/n only, never as rates. */
export const RATE_MIN_MATCHES = 10;
export const DEATH_TYPES: DeathType[] = ["solo", "gank", "teamfight", "other"];
export const PHASES = [[0, 600], [600, 1200], [1200, 1800], [1800, Infinity]] as const;

export interface DeathRow extends CoachDeath { matchId: string; hero: string; patch: string | null }
export interface LaneSummary { lane: LaneName; matches: number; behind: number; even: number; ahead: number; unknown: number; medianDiff: number | null }
export interface CoachSummary {
  rules: CoachRules;
  matches: number;
  deaths: DeathRow[];
  byType: Record<DeathType, number>;
  matchesWithType: Record<DeathType, number>;
  byPhase: Record<DeathType, number>[];
  vision: { ownWard: number; enemyWard: number; seen: number; seenUnknown: number; smoke: number };
  lanes: LaneSummary[];
  notes: { matchId: string; hero: string; note: CoachNote }[];
}

const zero = (): Record<DeathType, number> => ({ solo: 0, gank: 0, teamfight: 0, other: 0 });

export function median(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid]! : Math.round((sorted[mid - 1]! + sorted[mid]!) / 2);
}

/** "k/n", plus a percentage only once the sample reaches RATE_MIN_MATCHES matches. */
export function ratio(k: number, n: number, matches: number): string {
  if (n === 0) return "0/0";
  return matches >= RATE_MIN_MATCHES ? `${k}/${n} (${Math.round((100 * k) / n)}%)` : `${k}/${n}`;
}

export function summarize(report: PostMatchReplay, matchIds?: ReadonlySet<string>): CoachSummary | null {
  const rules = report.rules;
  if (!rules) return null;
  const games = report.matches.filter(m => m.coach && (!matchIds || matchIds.has(m.id)));
  if (games.length === 0) return null;
  const byType = zero();
  const matchesWithType = zero();
  const byPhase = PHASES.map(zero);
  const vision = { ownWard: 0, enemyWard: 0, seen: 0, seenUnknown: 0, smoke: 0 };
  const deaths: DeathRow[] = [];
  const notes: CoachSummary["notes"] = [];
  const laneDiffs = new Map<LaneName, { rows: number; behind: number; even: number; ahead: number; unknown: number; diffs: number[] }>();
  for (const game of games) {
    const coach = game.coach!;
    const seenTypes = new Set<DeathType>();
    for (const d of coach.deaths) {
      deaths.push({ ...d, matchId: game.id, hero: game.hero, patch: game.patch });
      byType[d.type]++;
      seenTypes.add(d.type);
      const phase = PHASES.findIndex(([lo, hi]) => d.t >= lo && d.t < hi);
      byPhase[phase < 0 ? 0 : phase]![d.type]++;
      if (d.ownWard) vision.ownWard++;
      if (d.enemyWard) vision.enemyWard++;
      if (d.enemyVisibleSeconds === null) vision.seenUnknown++;
      else if (d.enemyVisibleSeconds >= rules.notes.seenSeconds) vision.seen++;
      if (d.enemySmoke !== null) vision.smoke++;
    }
    for (const type of seenTypes) matchesWithType[type]++;
    const mine = coach.lane10?.lanes.find(row => row.heroes.includes("self"));
    if (mine) {
      const entry = laneDiffs.get(mine.lane) ?? { rows: 0, behind: 0, even: 0, ahead: 0, unknown: 0, diffs: [] };
      entry.rows++;
      entry[mine.outcome ?? "unknown"]++;
      if (mine.nwDiff !== null) entry.diffs.push(mine.nwDiff);
      laneDiffs.set(mine.lane, entry);
    }
    for (const note of coach.notes) notes.push({ matchId: game.id, hero: game.hero, note });
  }
  const lanes = [...laneDiffs].map(([lane, e]) => ({ lane, matches: e.rows, behind: e.behind, even: e.even,
    ahead: e.ahead, unknown: e.unknown, medianDiff: median(e.diffs) }));
  return { rules, matches: games.length, deaths, byType, matchesWithType, byPhase, vision, lanes, notes };
}

export function clock(seconds: number): string {
  const s = Math.floor(Math.abs(seconds));
  return `${seconds < 0 ? "−" : ""}${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
