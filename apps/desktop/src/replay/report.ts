export const REPORT_FORMAT = "kohbor.post-match-replay.v2";
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
interface ReplayMatch {
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
  format: typeof REPORT_FORMAT;
  source: "DEM";
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

function match(value: unknown, legacy: boolean): boolean {
  const keysForMatch = ["id", "hero", "team", "duration", "points", "deaths", "roster"];
  if (!legacy) keysForMatch.push("patch", "start");
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
  return true;
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
  if (!legacy && (!fields(value, ["format", "source", "maps", "matches"]) || value.format !== REPORT_FORMAT)) return null;
  if (value.source !== "DEM" || !Array.isArray(value.matches) || value.matches.length === 0
    || value.matches.length > 12 || !value.matches.every(game => match(game, legacy))) return null;
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
