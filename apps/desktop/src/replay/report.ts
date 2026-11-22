export const REPORT_FORMAT = "kohbor.post-match-replay.v2";
export const REPORT_FORMAT_V3 = "kohbor.post-match-replay.v3";
const LEGACY_FORMAT = "kohbor.post-match-replay.v1";
export const MAX_REPORT_BYTES = 16 * 1024 * 1024;
const MAX_DURATION = 7200;
const MIN_START = -600;

type Team = "Radiant" | "Dire";
type Point = [number, number, number];
interface LifeEnd { t: number; x: number; y: number }
interface CombatAction { actor: string; ability: string | null; illusion: boolean }
interface CombatMoment extends CombatAction { t: number; kind: "damage" | "control" | "cast" }
interface ReplayText { th: string; en: string }
interface DeathReview {
  finish: CombatAction;
  moments: CombatMoment[];
  summary: ReplayText | null;
  advice: ReplayText | null;
}
interface Death extends LifeEnd { tick: number; killer: string | null; review?: DeathReview }
interface Actor {
  key: string;
  hero: string;
  team: Team;
  lifeEnds: LifeEnd[];
  points?: Point[];
}
interface MapLayout {
  patch: string | null;
  width: number;
  height: number;
  calibration: { sx: number; sy: number; ox: number; oy: number };
  map: string | null;
}
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
interface ReplayMatch {
  coach?: MatchCoach;
  patch: string | null;
  id: string;
  team: Team;
  hero: string;
  start: number;
  duration: number;
  points: Point[];
  deaths: Death[];
  roster: Actor[];
}
export interface PostMatchReplay {
  format: typeof REPORT_FORMAT | typeof REPORT_FORMAT_V3;
  source: "DEM";
  rules?: CoachRules;
  maps: MapLayout[];
  matches: ReplayMatch[];
}

const own = ["self", "ally-1", "ally-2", "ally-3", "ally-4"];
const enemies = ["enemy-1", "enemy-2", "enemy-3", "enemy-4", "enemy-5"];
const actorKeys = new Set([...own, ...enemies]);
const fields = (x: unknown, keys: string[]): x is Record<string, unknown> => {
  if (!x || typeof x !== "object" || Array.isArray(x)) return false;
  const present = Object.keys(x);
  return present.length === keys.length && present.every(key => keys.includes(key));
};
const number = (x: unknown, min: number, max: number): x is number =>
  typeof x === "number" && Number.isFinite(x) && x >= min && x <= max;
const coordinate = (x: unknown) => number(x, -32768, 32768);
const name = (x: unknown): x is string =>
  typeof x === "string" && x.trim().length > 0 && x.length <= 80 && !/[\u0000-\u001f\u007f]/.test(x);
const team = (x: unknown): x is Team => x === "Radiant" || x === "Dire";
const actionKeys = ["actor", "ability", "illusion"];
const action = (x: Record<string, unknown>) =>
  name(x.actor) && (x.ability === null || name(x.ability)) && typeof x.illusion === "boolean";
const sentence = (x: unknown) =>
  typeof x === "string" && x.trim().length > 0 && x.length <= 480 && !/[\u0000-\u001f\u007f]/.test(x);
const replayText = (x: unknown) => x === null || (fields(x, ["th", "en"]) && sentence(x.th) && sentence(x.en));

function deathReview(review: unknown, start: number, end: number): boolean {
  if (!fields(review, ["finish", "moments", "summary", "advice"])
    || !fields(review.finish, actionKeys) || !action(review.finish)
    || !Array.isArray(review.moments) || review.moments.length > 8
    || !replayText(review.summary) || !replayText(review.advice)) return false;
  let previous = Math.max(start, end - 15) - .001;
  for (const moment of review.moments) {
    if (!fields(moment, [...actionKeys, "t", "kind"]) || !action(moment)
      || !number(moment.t, previous, end)
      || !["damage", "control", "cast"].some(kind => kind === moment.kind)
      || (moment.kind !== "damage" && moment.ability === null)) return false;
    previous = moment.t;
  }
  return true;
}

function points(value: unknown, start: number, duration: number): boolean {
  if (!Array.isArray(value) || value.length === 0 || value.length > duration - start + 1) return false;
  let prev = start - 1;
  for (const point of value) {
    if (!Array.isArray(point) || point.length !== 3 || !number(point[0], start, duration)
      || !Number.isInteger(point[0]) || point[0] <= prev || !coordinate(point[1]) || !coordinate(point[2])) return false;
    prev = point[0];
  }
  return true;
}

function events(value: unknown, start: number, duration: number, deaths: boolean): boolean {
  if (!Array.isArray(value) || value.length > 1000) return false;
  const keys = deaths ? ["t", "x", "y", "tick", "killer"] : ["t", "x", "y"];
  const reviewKeys = [...keys, "review"];
  let prev = start;
  for (const entry of value) {
    if (!(fields(entry, keys) || (deaths && fields(entry, reviewKeys)))
      || !number(entry.t, start, duration) || entry.t < prev
      || !coordinate(entry.x) || !coordinate(entry.y)) return false;
    if (deaths && (!(number(entry.tick, 0, 0x7fffffff) && Number.isInteger(entry.tick))
      || !(entry.killer === null || name(entry.killer)))) return false;
    if ("review" in entry && !deathReview(entry.review, start, entry.t)) return false;
    prev = entry.t;
  }
  return true;
}

function match(value: unknown, legacy: boolean, v3 = false): boolean {
  const keysForMatch = ["id", "hero", "team", "duration", "points", "deaths", "roster"];
  if (!legacy) keysForMatch.push("patch", "start");
  if (v3) keysForMatch.push("coach");
  if (!fields(value, keysForMatch)
    || typeof value.id !== "string" || !/^[1-9][0-9]{0,19}$/.test(value.id)
    || !team(value.team) || !name(value.hero) || !number(value.duration, 1, MAX_DURATION)
    || !Number.isInteger(value.duration) || !Array.isArray(value.roster)
    || value.roster.length !== actorKeys.size) return false;
  const start = legacy ? 0 : value.start;
  if (!number(start, MIN_START, 0) || !Number.isInteger(start) || !points(value.points, start, value.duration)
    || !events(value.deaths, start, value.duration, true)) return false;
  const keys = new Set<string>();
  for (const actor of value.roster) {
    const keysForActor = actor?.key === "self" ? ["key", "hero", "team", "lifeEnds"] : ["key", "hero", "team", "lifeEnds", "points"];
    if (!fields(actor, keysForActor) || typeof actor.key !== "string" || !actorKeys.has(actor.key) || keys.has(actor.key)
      || !name(actor.hero) || !team(actor.team) || (own.includes(actor.key) !== (actor.team === value.team))
      || !events(actor.lifeEnds, start, value.duration, false)) return false;
    if (actor.key === "self" ? actor.hero !== value.hero : !points(actor.points, start, value.duration)) return false;
    keys.add(actor.key);
  }
  return !v3 || coach(value.coach, value.deaths as { tick: number }[], value.duration);
}

const integer = (x: unknown, min: number, max: number): x is number => number(x, min, max) && Number.isInteger(x);
const deathTypes = ["solo", "gank", "teamfight", "other"];
const laneNames = ["top", "mid", "bot"];
const outcomes = [null, "ahead", "even", "behind"];
const coachDeathKeys = ["tick", "t", "type", "enemiesInvolved", "alliesNear", "nearbyDeaths", "sampleAge",
  "enemyVisibleSeconds", "ownWard", "enemyWard", "enemySmoke"];

function coach(value: unknown, deaths: { tick: number }[], duration: number): boolean {
  if (!fields(value, ["deaths", "lane10", "notes"]) || !Array.isArray(value.deaths)
    || value.deaths.length !== deaths.length || !Array.isArray(value.notes) || value.notes.length > 3) return false;
  const ticks = new Set(deaths.map(d => d.tick));
  for (const [i, d] of value.deaths.entries()) {
    if (!fields(d, coachDeathKeys) || d.tick !== deaths[i]!.tick || !number(d.t, MIN_START, duration)
      || !deathTypes.includes(d.type as string) || !integer(d.enemiesInvolved, 0, 5) || !integer(d.alliesNear, 0, 4)
      || !integer(d.nearbyDeaths, 0, 10) || !number(d.sampleAge, 0, 60)
      || !(d.enemyVisibleSeconds === null || integer(d.enemyVisibleSeconds, 0, 60))
      || typeof d.ownWard !== "boolean" || typeof d.enemyWard !== "boolean"
      || !(d.enemySmoke === null || number(d.enemySmoke, 0, 120))) return false;
  }
  if (value.lane10 !== null) {
    const lane10 = value.lane10;
    if (!fields(lane10, ["lanes", "roaming"]) || !Array.isArray(lane10.lanes) || lane10.lanes.length > 3
      || !Array.isArray(lane10.roaming) || !lane10.roaming.every(key => own.includes(key as string))) return false;
    for (const row of lane10.lanes) {
      if (!fields(row, ["lane", "heroes", "opponents", "nwDiff", "deathsBefore", "outcome"])
        || !laneNames.includes(row.lane as string) || !Array.isArray(row.heroes) || row.heroes.length === 0
        || !row.heroes.every(key => own.includes(key as string)) || !integer(row.opponents, 0, 5)
        || !(row.nwDiff === null || integer(row.nwDiff, -200000, 200000)) || !integer(row.deathsBefore, 0, 100)
        || !outcomes.includes(row.outcome as string | null)) return false;
    }
  }
  return value.notes.every(note => fields(note, ["kind", "evidence", "th", "en"])
    && typeof note.kind === "string" && /^[a-z][a-z0-9-]{0,39}$/.test(note.kind)
    && sentence(note.th) && sentence(note.en) && Array.isArray(note.evidence) && note.evidence.length <= 100
    && note.evidence.every(tick => ticks.has(tick as number)));
}

const positive = (x: unknown) => number(x, 0, 100000);
function rules(value: unknown): value is CoachRules {
  return fields(value, ["version", "provisional", "enemyPositions", "deathType", "vision", "smoke", "lane", "notes"])
    && typeof value.version === "string" && /^[a-z0-9.-]{1,40}$/.test(value.version)
    && value.provisional === true && value.enemyPositions === "full-map"
    && fields(value.deathType, ["radius", "windowSeconds", "allyRadius", "teamfightDeaths", "teamfightSeconds", "teamfightRadius"])
    && Object.values(value.deathType).every(positive)
    && fields(value.vision, ["observerRadius", "visibleWindowSeconds"]) && Object.values(value.vision).every(positive)
    && fields(value.smoke, ["windowSeconds"]) && positive(value.smoke.windowSeconds)
    && fields(value.lane, ["minute", "threshold"]) && Object.values(value.lane).every(positive)
    && fields(value.notes, ["max", "seenSeconds"]) && integer(value.notes.max, 0, 3) && positive(value.notes.seenSeconds);
}

const patchName = (value: unknown): value is string => typeof value === "string" && /^\d{1,2}\.\d{2}[a-z]?$/.test(value);
const dimension = (value: unknown): value is number => number(value, 256, 2048) && Number.isInteger(value);
const image = (value: unknown): value is string =>
  typeof value === "string" && value.length <= 1_500_000
  && /^data:image\/(?:webp|png|jpeg);base64,[A-Za-z0-9+/]+={0,2}$/.test(value);

function calibration(value: unknown, width: number, height: number): value is MapLayout["calibration"] {
  return fields(value, ["sx", "sy", "ox", "oy"])
    && number(value.sx, .001, 1) && number(value.sy, -1, -.001)
    && number(value.ox, 0, width) && number(value.oy, 0, height);
}

function mapLayout(value: unknown): value is MapLayout {
  return fields(value, ["patch", "map", "calibration", "width", "height"])
    && patchName(value.patch) && dimension(value.width) && dimension(value.height)
    && calibration(value.calibration, value.width, value.height) && image(value.map);
}

export function readPostMatchReplay(value: unknown): PostMatchReplay | null {
  const legacy = fields(value, ["format", "source", "calibration", "map", "matches"])
    && value.format === LEGACY_FORMAT;
  const v3 = !legacy && fields(value, ["format", "source", "rules", "maps", "matches"]) && value.format === REPORT_FORMAT_V3;
  if (!legacy && !v3 && (!fields(value, ["format", "source", "maps", "matches"]) || value.format !== REPORT_FORMAT)) return null;
  if (v3 && !rules(value.rules)) return null;
  if (value.source !== "DEM" || !Array.isArray(value.matches) || value.matches.length === 0
    || value.matches.length > 12 || !value.matches.every(game => match(game, legacy, v3))) return null;
  if (new Set(value.matches.map(m => m.id)).size !== value.matches.length) return null;
  if (legacy) {
    if (!calibration(value.calibration, 1000, 959) || !(value.map === null || image(value.map))) return null;
    return {
      format: REPORT_FORMAT,
      source: "DEM",
      maps: [{ patch: null, width: 1000, height: 959, calibration: value.calibration, map: value.map }],
      matches: value.matches.map(game => ({ ...game, patch: null, start: 0 })),
    };
  }
  if (!Array.isArray(value.maps) || value.maps.length === 0 || value.maps.length > 12
    || !value.maps.every(mapLayout)) return null;
  const mapPatches = new Set(value.maps.map(layout => layout.patch));
  if (mapPatches.size !== value.maps.length || !value.matches.every(game => mapPatches.has(game.patch))) return null;
  return value as unknown as PostMatchReplay;
}
