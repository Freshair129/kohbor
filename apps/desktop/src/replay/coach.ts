export type DeathType = "solo" | "gank" | "teamfight" | "other";
export interface CoachDeath {
  tick: number;
  t: number;
  type: DeathType;
  enemiesInvolved: number;
  alliesNear: number;
  nearbyDeaths: number;
  sampleAge: number;
  enemyVisibleSeconds: number | null;
  ownWard: boolean;
  enemyWard: boolean;
  enemySmoke: number | null;
}
export type LaneName = "top" | "mid" | "bot";
export interface LaneRow {
  lane: LaneName;
  heroes: string[];
  opponents: number;
  nwDiff: number | null;
  deathsBefore: number;
  outcome: "ahead" | "even" | "behind" | null;
}
export interface CoachNote { kind: string; evidence: number[]; th: string; en: string }
export interface MatchCoach {
  deaths: CoachDeath[];
  lane10: { lanes: LaneRow[]; roaming: string[] } | null;
  notes: CoachNote[];
}
export interface CoachRules {
  version: string;
  provisional: true;
  enemyPositions: "full-map";
  deathType: { radius: number; windowSeconds: number; allyRadius: number; teamfightDeaths: number; teamfightSeconds: number; teamfightRadius: number };
  vision: { observerRadius: number; visibleWindowSeconds: number };
  smoke: { windowSeconds: number };
  lane: { minute: number; threshold: number };
  notes: { max: number; seenSeconds: number };
}
export interface PostMatchReplay {
  rules?: CoachRules;
  matches: Array<{ id: string; hero: string; patch: string | null; coach?: MatchCoach }>;
}

/** Sample-size rule (DEC-COACH-REVIEW): below this many matches, patterns are shown as k/n only, never as rates. */
export const RATE_MIN_MATCHES = 10;
export const DEATH_TYPES: DeathType[] = ["solo", "gank", "teamfight", "other"];
export const PHASES = [[0, 600], [600, 1200], [1200, 1800], [1800, Infinity]] as const;

export const ANALYTICS_TEMPLATE_QUESTIONS = [
  { id: "summary", th: "สรุปแมตช์นี้", en: "Summarize this match" },
  { id: "kills", th: "ใครทำ hero kills มากสุด", en: "Who had the most hero kills?" },
  { id: "deaths", th: "แสดงจังหวะตายของฮีโร่ฉัน", en: "Show my hero's deaths" },
] as const;

export function answerAnalyticsTemplate(input: {
  question: string;
  locale: "th" | "en";
  durationS: number | null;
  heroes: readonly { id: string; name: string; team: number }[];
  deaths: readonly { t: number; hero: string; killer: string }[];
  ownHero: string | null;
  localResult: "win" | "loss" | null;
}): string {
  const th = input.locale === "th";
  const unknown = th ? "— ไม่ทราบ" : "— unknown";
  const question = input.question.toLocaleLowerCase();
  const heroNames = new Map(input.heroes.map(hero => [hero.id, hero.name]));
  const clockText = input.durationS == null ? unknown : clock(input.durationS);
  const knownKills = input.deaths.filter(death => heroNames.has(death.killer));
  if (/kill|ฆ่า|คิล/.test(question)) {
    if (!knownKills.length) return th ? "ไม่มีข้อมูล hero-kill ที่ยืนยันได้ในรีเพลย์นี้" : "No confirmed hero-kill events are available in this replay.";
    const counts = new Map<string, number>();
    for (const death of knownKills) counts.set(death.killer, (counts.get(death.killer) ?? 0) + 1);
    const highest = Math.max(...counts.values());
    const leaders = [...counts].filter(([, count]) => count === highest).map(([id]) => heroNames.get(id) ?? unknown);
    return th ? `hero kills มากสุด ${highest} ครั้ง: ${leaders.join(", ")}` : `Most hero kills: ${highest} — ${leaders.join(", ")}`;
  }
  if (/death|ตาย/.test(question)) {
    if (!input.ownHero) return th ? `ฮีโร่ของคุณยังไม่ทราบ จึงจับคู่จังหวะตายให้ไม่ได้ · ${unknown}` : `Your hero is unknown, so deaths cannot be attributed · ${unknown}`;
    const rows = input.deaths.filter(death => death.hero === input.ownHero);
    if (!rows.length) return th ? "ไม่พบจังหวะตายของฮีโร่ที่ระบุในข้อมูล" : "No death events were recorded for the selected hero.";
    return rows.map(death => `${clock(death.t)} · ${heroNames.get(death.killer) ?? unknown}`).join("\n");
  }
  const radiant = input.heroes.filter(hero => hero.team === 2).map(hero => hero.name);
  const dire = input.heroes.filter(hero => hero.team === 3).map(hero => hero.name);
  const result = input.localResult === "win" ? (th ? "ชนะ" : "win") : input.localResult === "loss" ? (th ? "แพ้" : "loss") : unknown;
  const killerTotal = knownKills.length ? String(knownKills.length) : unknown;
  const deathTotal = input.deaths.length ? String(input.deaths.length) : "0";
  return th
    ? `ระยะเวลา ${clockText} · ผล ${result} · hero kills ${killerTotal} · deaths ${deathTotal}\nRadiant ${radiant.length ? radiant.join(", ") : unknown}\nDire ${dire.length ? dire.join(", ") : unknown}\nคำตอบนี้ใช้ข้อมูลในเครื่องเท่านั้น; รูปแบบ k/n แสดงเป็นจำนวนก่อนมีตัวอย่างแมตช์เพียงพอ.`
    : `Duration ${clockText} · result ${result} · hero kills ${killerTotal} · deaths ${deathTotal}\nRadiant: ${radiant.length ? radiant.join(", ") : unknown}\nDire: ${dire.length ? dire.join(", ") : unknown}\nThis answer uses local data only; k/n counts remain counts until enough matches are available.`;
}

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
