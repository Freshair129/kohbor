// Admission of a local Match Bundle (replay engine schema 3.0 or 3.1) for the Lab "Match view".
// New parses produce 3.1; existing 3.0 library entries remain readable.
// Manual file admission remains only for the legacy report route; the Match view uses the host library.

export const BUNDLE_SCHEMA = "3.1" as const;
export const BUNDLE_FILES = ["manifest.json", "match.json", "world.json"] as const;
export type BundleFileName = (typeof BUNDLE_FILES)[number];
export type BundleSchema = "3.0" | "3.1";

/** Per-file read limits. A 37-minute match is about 5.2 MB (match) and 1.5 MB (world). */
export const MAX_BUNDLE_BYTES: Record<BundleFileName, number> = {
  "manifest.json": 1024 * 1024,
  "match.json": 48 * 1024 * 1024,
  "world.json": 24 * 1024 * 1024,
};

/** The part of a browser File this module needs; tests pass plain objects. */
export interface PickedFile {
  name: string;
  size: number;
  webkitRelativePath?: string;
  arrayBuffer(): Promise<ArrayBuffer>;
}

export type BundleError =
  | { code: "missing"; files: string[] }
  | { code: "duplicate"; file: string }
  | { code: "too-large"; file: string; limit: number }
  | { code: "unreadable"; file: string }
  | { code: "schema"; file: string; found: string }
  | { code: "integrity"; file: string }
  | { code: "shape"; file: string; path: string };

type Json = unknown;
type Obj = Record<string, Json>;

export type DraftTeam = "radiant" | "dire";
export type DraftKind = "pick" | "ban";

export interface DraftEntry extends Obj {
  order: number;
  team: DraftTeam;
  kind: DraftKind;
  hero: string | null;
}

export interface DamageOut extends Obj {
  total: number;
  p: number;
  m: number;
  u: number;
}

export interface BundleMeta extends Obj {
  schema: BundleSchema;
  winner?: DraftTeam | null;
  winner_reason?: string;
  draft?: DraftEntry[] | null;
  draft_reason?: string;
}

export interface BundleHero extends Obj {
  id: string;
  gold_earned?: Array<number | null>;
  gold_earned_final?: number | null;
  dmg_out?: DamageOut;
}

export interface BundleMatch extends Obj {
  meta: BundleMeta;
  heroes: BundleHero[];
}

const SUPPORTED_BUNDLE_SCHEMAS: ReadonlySet<unknown> = new Set(["3.0", "3.1"]);
const isBundleSchema = (value: Json): value is BundleSchema => SUPPORTED_BUNDLE_SCHEMAS.has(value);

export interface MatchBundle {
  manifest: Obj;
  match: BundleMatch;
  world: Obj;
  patch: string | null;
  gameMode: number | null;
  durationS: number | null;
  calibration: Obj | null;
  mapSize: number | null;
  mapImage: string | null;
}

export type Admission = { ok: true; bundle: MatchBundle } | { ok: false; error: BundleError };

class ShapeError extends Error {
  file: string;
  path: string;
  constructor(file: string, path: string) {
    super(`${file}: ${path}`);
    this.file = file;
    this.path = path;
  }
}

const isObj = (v: Json): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const isNum = (v: Json): v is number => typeof v === "number" && Number.isFinite(v);
const HERO_ID = /^npc_dota_hero_[a-z0-9_]+$/;
const COLOR = /^#[0-9a-fA-F]{6}$/;
const SAFE_TEXT = /^[^<>]*$/;

function baseName(file: PickedFile): string {
  const path = file.webkitRelativePath || file.name;
  return path.split(/[\\/]/).pop()!.toLowerCase();
}

async function sha256Hex(bytes: ArrayBuffer): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), b => b.toString(16).padStart(2, "0")).join("");
}

function parse(file: BundleFileName, bytes: ArrayBuffer): Json {
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw { code: "unreadable", file } satisfies BundleError;
  }
}

function need(cond: boolean, file: string, path: string): void {
  if (!cond) throw new ShapeError(file, path);
}

function checkSeries(file: string, path: string, v: Json, length: number, item: (x: Json) => boolean): void {
  need(Array.isArray(v) && v.length === length, file, path);
  const list = v as Json[];
  for (let i = 0; i < list.length; i++) if (!item(list[i])) throw new ShapeError(file, `${path}[${i}]`);
}

const num = (x: Json) => isNum(x);
const flag = (x: Json) => x === 0 || x === 1 || x === true || x === false;
const pair = (x: Json) => Array.isArray(x) && x.length >= 2 && isNum(x[0]) && isNum(x[1]);
const eco = (x: Json) => Array.isArray(x) && x.length === 4 && x.every(isNum);
const text = (x: Json) => typeof x === "string" && SAFE_TEXT.test(x);

function checkReasonedNullableField(
  meta: Obj,
  field: "winner" | "draft",
  reason: "winner_reason" | "draft_reason",
  validKnown: (value: Json) => boolean,
): void {
  const valuePresent = Object.prototype.hasOwnProperty.call(meta, field);
  const reasonPresent = Object.prototype.hasOwnProperty.call(meta, reason);
  if (!valuePresent) {
    need(!reasonPresent, "match.json", `meta.${reason}`);
    return;
  }

  const value = meta[field];
  if (value === null) {
    need(typeof meta[reason] === "string" && (meta[reason] as string).length > 0, "match.json", `meta.${reason}`);
    return;
  }
  need(validKnown(value), "match.json", `meta.${field}`);
  need(!reasonPresent, "match.json", `meta.${reason}`);
}

function checkDraftEntry(value: Json, path: string): void {
  need(isObj(value), "match.json", path);
  const entry = value as Obj;
  const keys = ["order", "team", "kind", "hero"];
  need(Object.keys(entry).length === keys.length && keys.every(key => Object.prototype.hasOwnProperty.call(entry, key)), "match.json", path);
  need(Number.isInteger(entry.order) && (entry.order as number) >= 0, "match.json", `${path}.order`);
  need(entry.team === "radiant" || entry.team === "dire", "match.json", `${path}.team`);
  need(entry.kind === "pick" || entry.kind === "ban", "match.json", `${path}.kind`);
  need(entry.hero === null || (typeof entry.hero === "string" && HERO_ID.test(entry.hero)), "match.json", `${path}.hero`);
}

function checkNewMetaFields(meta: Obj): void {
  checkReasonedNullableField(meta, "winner", "winner_reason", value => value === "radiant" || value === "dire");
  checkReasonedNullableField(meta, "draft", "draft_reason", value => Array.isArray(value));
  if (Array.isArray(meta.draft)) {
    meta.draft.forEach((entry, index) => checkDraftEntry(entry, `meta.draft[${index}]`));
  }
}

function checkNewHeroFields(hero: Obj, path: string): void {
  if (hero.gold_earned !== undefined) {
    need(Array.isArray(hero.gold_earned)
      && hero.gold_earned.every(value => value === null || (Number.isInteger(value) && (value as number) >= 0)),
    "match.json", `${path}.gold_earned`);
  }
  if (hero.gold_earned_final !== undefined) {
    need(hero.gold_earned_final === null
      || (Number.isInteger(hero.gold_earned_final) && (hero.gold_earned_final as number) >= 0),
    "match.json", `${path}.gold_earned_final`);
  }
  if (hero.dmg_out !== undefined) {
    need(isObj(hero.dmg_out), "match.json", `${path}.dmg_out`);
    const damage = hero.dmg_out as Obj;
    const keys = ["total", "p", "m", "u"];
    need(Object.keys(damage).length === keys.length && keys.every(key => Object.prototype.hasOwnProperty.call(damage, key)),
      "match.json", `${path}.dmg_out`);
    for (const key of keys) {
      need(Number.isInteger(damage[key]) && (damage[key] as number) >= 0, "match.json", `${path}.dmg_out.${key}`);
    }
  }
}

/** Checks the fields the viewer reads. Fields it guards itself (st, din, hin, finds, sampleT) stay optional. */
export function checkMatch(match: Json): asserts match is BundleMatch {
  const F = "match.json";
  need(isObj(match), F, "$");
  const m = match as Obj;
  need(isObj(m.meta), F, "meta");
  need(isBundleSchema((m.meta as Obj).schema), F, "meta.schema");
  checkNewMetaFields(m.meta as Obj);
  need(isNum(m.t0) && isNum(m.t1) && Number.isInteger(m.t0) && Number.isInteger(m.t1) && (m.t1 as number) > (m.t0 as number), F, "t0/t1");
  const length = (m.t1 as number) - (m.t0 as number) + 1;
  need(Array.isArray(m.heroes) && m.heroes.length >= 1 && m.heroes.length <= 10, F, "heroes");
  const ids = new Set<string>(), pids = new Set<number>();
  (m.heroes as Json[]).forEach((raw, k) => {
    const p = `heroes[${k}]`;
    need(isObj(raw), F, p);
    const h = raw as Obj;
    checkNewHeroFields(h, p);
    need(typeof h.id === "string" && HERO_ID.test(h.id) && !ids.has(h.id), F, `${p}.id`);
    ids.add(h.id as string);
    need(text(h.name), F, `${p}.name`);
    need(h.team === 2 || h.team === 3, F, `${p}.team`);
    need(Number.isInteger(h.pid) && (h.pid as number) >= 0 && (h.pid as number) < 64 && !pids.has(h.pid as number), F, `${p}.pid`);
    pids.add(h.pid as number);
    need(typeof h.color === "string" && COLOR.test(h.color), F, `${p}.color`);
    checkSeries(F, `${p}.pos`, h.pos, length, pair);
    for (const key of ["hp", "mhp", "mp", "mmp", "lvl"]) checkSeries(F, `${p}.${key}`, h[key], length, num);
    checkSeries(F, `${p}.alive`, h.alive, length, flag);
    checkSeries(F, `${p}.eco`, h.eco, length, eco);
    need(Array.isArray(h.abil), F, `${p}.abil`);
    (h.abil as Json[]).forEach((a, j) => {
      need(isObj(a) && typeof (a as Obj).n === "string", F, `${p}.abil[${j}]`);
      for (const key of ["lv", "cd", "cl", "mc"]) checkSeries(F, `${p}.abil[${j}].${key}`, (a as Obj)[key], length, num);
    });
    for (const key of ["inv", "mods", "casts", "buys"]) need(Array.isArray(h[key]), F, `${p}.${key}`);
    need(isObj(h.icd), F, `${p}.icd`);
    if (h.finds !== undefined) need(Array.isArray(h.finds), F, `${p}.finds`);
    if (h.st !== undefined && h.st !== null) need(isObj(h.st), F, `${p}.st`);
  });
  need(Array.isArray(m.deaths), F, "deaths");
  (m.deaths as Json[]).forEach((d, k) => {
    const r = d as Obj;
    need(isObj(d) && isNum(r.t) && ids.has(r.hero as string) && typeof r.killer === "string"
      && ["last", "by", "win", "hits", "heals"].every(key => Array.isArray(r[key])) && isObj(r.seen), F, `deaths[${k}]`);
  });
  need(isObj(m.names) && Object.values(m.names as Obj).every(text), F, "names");
  need(isObj(m.statdb), F, "statdb");
  for (const key of ["heroes", "sim", "pierce", "abpierce"]) need(isObj((m.statdb as Obj)[key]), F, `statdb.${key}`);
  if (m.sampleT !== undefined) checkSeries(F, "sampleT", m.sampleT, length, num);
  if (m.dtick !== undefined) need(isNum(m.dtick) && (m.dtick as number) > 0, F, "dtick");
}

export function checkWorld(world: Json): void {
  const F = "world.json";
  need(isObj(world), F, "$");
  const w = world as Obj;
  for (const key of ["t0", "qh", "qu"]) need(isNum(w[key]), F, key);
  need(isObj(w.heroes), F, "heroes");
  for (const [id, h] of Object.entries(w.heroes as Obj)) {
    need(isObj(h) && ["t", "x", "y", "brk"].every(k => Array.isArray((h as Obj)[k])), F, `heroes.${id}`);
  }
  need(Array.isArray(w.tracks), F, "tracks");
  (w.tracks as Json[]).forEach((tr, k) => {
    const t = tr as Obj;
    need(isObj(tr) && typeof t.k === "string" && isNum(t.t0) && isNum(t.t1) && (t.t1 as number) >= (t.t0 as number)
      && (t.t1 as number) - (t.t0 as number) < 100_000
      && Array.isArray(t.p) && t.p.length === 3 && t.p.every(Array.isArray), F, `tracks[${k}]`);
  });
  need(Array.isArray(w.runes), F, "runes");
}

function checkManifest(manifest: Json): Obj {
  const F = "manifest.json";
  need(isObj(manifest), F, "$");
  const m = manifest as Obj;
  if (!isBundleSchema(m.schema)) throw { code: "schema", file: F, found: String(m.schema ?? "missing") } satisfies BundleError;
  need(isObj(m.files), F, "files");
  for (const name of ["match.json", "world.json"]) {
    const entry = (m.files as Obj)[name];
    need(isObj(entry) && isNum((entry as Obj).bytes) && typeof (entry as Obj).sha256 === "string", F, `files.${name}`);
  }
  if (m.map !== undefined) need(isObj(m.map), F, "map");
  if (isObj(m.map) && m.map.size !== undefined) need(isNum(m.map.size) && m.map.size > 0, F, "map.size");
  return m;
}

function buildBundle(manifest: Obj, match: Obj, world: Obj): MatchBundle {
  const meta = isObj(match.meta) ? match.meta : null;
  if (meta && !isBundleSchema(meta.schema)) {
    throw { code: "schema", file: "match.json", found: String(meta.schema ?? "missing") } satisfies BundleError;
  }
  checkMatch(match);
  const matchMeta = match.meta;
  need(manifest.schema === matchMeta.schema, "match.json", "meta.schema");
  need(String(matchMeta.match_id) === String(manifest.match_id), "match.json", "meta.match_id");
  checkWorld(world);
  const map = isObj(manifest.map) ? manifest.map : null;
  const patch = matchMeta.patch, mode = matchMeta.game_mode, duration = matchMeta.duration_s;
  const calibration = map ? map.calibration : null;
  const image = map ? map.image : null;
  return {
    manifest,
    match,
    world,
    patch: typeof patch === "string" && SAFE_TEXT.test(patch) ? patch : null,
    gameMode: typeof mode === "number" && Number.isInteger(mode) ? mode : null,
    durationS: isNum(duration) ? duration : null,
    calibration: isObj(calibration) ? calibration : null,
    mapSize: map && isNum(map.size) ? map.size : null,
    mapImage: typeof image === "string" ? image.toLowerCase() : null,
  };
}

/** Structural admission for host-opened bundles; the RP1 host already verified on-disk hashes. */
export function admitLibraryBundle(value: unknown): Admission {
  try {
    need(isObj(value), "manifest.json", "$");
    const raw = value as Obj;
    const manifest = checkManifest(raw.manifest);
    need(isObj(raw.match_data), "match.json", "$");
    need(isObj(raw.world), "world.json", "$");
    return { ok: true, bundle: buildBundle(manifest, raw.match_data as Obj, raw.world as Obj) };
  } catch (error) {
    if (error instanceof ShapeError) return { ok: false, error: { code: "shape", file: error.file, path: error.path } };
    if (isObj(error) && typeof error.code === "string") return { ok: false, error: error as BundleError };
    throw error;
  }
}

/** Validates a multi-file pick holding one Match Bundle. Other picked files are ignored. */
export async function admitMatchBundle(files: readonly PickedFile[]): Promise<Admission> {
  try {
    const found = new Map<BundleFileName, PickedFile>();
    for (const file of files) {
      const name = baseName(file) as BundleFileName;
      if (!BUNDLE_FILES.includes(name)) continue;
      if (found.has(name)) return { ok: false, error: { code: "duplicate", file: name } };
      found.set(name, file);
    }
    const missing = BUNDLE_FILES.filter(name => !found.has(name));
    if (missing.length) return { ok: false, error: { code: "missing", files: missing } };
    for (const name of BUNDLE_FILES) {
      if (found.get(name)!.size > MAX_BUNDLE_BYTES[name]) {
        return { ok: false, error: { code: "too-large", file: name, limit: MAX_BUNDLE_BYTES[name] } };
      }
    }
    const bytes = {} as Record<BundleFileName, ArrayBuffer>;
    for (const name of BUNDLE_FILES) {
      try {
        bytes[name] = await found.get(name)!.arrayBuffer();
      } catch {
        return { ok: false, error: { code: "unreadable", file: name } };
      }
      // The reported size can differ from what was read; the read bytes govern.
      if (bytes[name].byteLength > MAX_BUNDLE_BYTES[name]) {
        return { ok: false, error: { code: "too-large", file: name, limit: MAX_BUNDLE_BYTES[name] } };
      }
    }
    const manifest = checkManifest(parse("manifest.json", bytes["manifest.json"]));
    for (const name of ["match.json", "world.json"] as const) {
      const entry = (manifest.files as Obj)[name] as Obj;
      if (entry.bytes !== bytes[name].byteLength || (entry.sha256 as string).toLowerCase() !== await sha256Hex(bytes[name])) {
        return { ok: false, error: { code: "integrity", file: name } };
      }
    }
    const match = parse("match.json", bytes["match.json"]);
    const world = parse("world.json", bytes["world.json"]);
    need(isObj(match), "match.json", "$");
    need(isObj(world), "world.json", "$");
    return { ok: true, bundle: buildBundle(manifest, match as Obj, world as Obj) };
  } catch (error) {
    if (error instanceof ShapeError) return { ok: false, error: { code: "shape", file: error.file, path: error.path } };
    if (isObj(error) && typeof error.code === "string") return { ok: false, error: error as BundleError };
    throw error;
  }
}

/* ---------- icons: bundled with Kohbor, built from the installed game ---------- */
// DEC-REPLAY-INTEGRATION item 3: hero, ability and item images and the minimap ship as app assets
// under src/assets/dota/<game build>/, exported from the owner's installed game by the dev tool
// kohbor-asset-export (crates/kohbor-game-assets). Rights review is required before public release.

export type IconKind = "hero" | "ability" | "item" | "map";

const ICON_DIR: Record<string, IconKind> = { heroes: "hero", abilities: "ability", items: "item" };
const IMAGE_EXT = new Set(["png", "webp", "jpg", "jpeg"]);

/**
 * Maps an asset path to its icon key. Hero, ability and item icons are `<dir>/<name>.<ext>` with
 * dir heroes, abilities or items. A map image is any image whose base name is in `mapNames`.
 */
export function iconKey(path: string, mapNames: readonly string[] = ["map.png"]): string | null {
  const parts = path.toLowerCase().split(/[\\/]/).filter(Boolean);
  const file = parts.pop();
  if (!file) return null;
  const dot = file.lastIndexOf(".");
  if (dot <= 0 || !IMAGE_EXT.has(file.slice(dot + 1))) return null;
  if (mapNames.includes(file)) return "map:";
  const kind = ICON_DIR[parts[parts.length - 1] ?? ""];
  const name = file.slice(0, dot);
  return kind && /^[a-z0-9_]+$/.test(name) ? `${kind}:${name}` : null;
}

/** Neutral placeholder: a grey tile with the name. Never another hero's icon. */
export function placeholderIcon(kind: IconKind, name: string): string {
  const label = name.replace(/^npc_dota_hero_|^item_/, "").replace(/_/g, " ").slice(0, kind === "hero" ? 12 : 18);
  const safe = label.replace(/[&<>"']/g, c => `&#${c.charCodeAt(0)};`);
  // Hero tiles keep the 256 x 144 portrait shape: the map view crops its centre 144 x 144 square.
  const [w, h] = kind === "hero" ? [256, 144] : kind === "item" ? [88, 64] : kind === "map" ? [256, 256] : [64, 64];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">`
    + `<rect width="${w}" height="${h}" fill="#3b3936"/><rect x="1" y="1" width="${w - 2}" height="${h - 2}" fill="none" stroke="#8f897e" stroke-dasharray="4 3"/>`
    + `<text x="50%" y="50%" fill="#ede6d8" font-family="sans-serif" font-size="${kind === "hero" ? 22 : Math.max(9, Math.round(h / 6))}" text-anchor="middle" dominant-baseline="middle">${safe}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

/** Icon names a bundle needs: the manifest's `icons.needed` list plus every hero. */
export function neededIconKeys(bundle: MatchBundle): Set<string> {
  const keys = new Set<string>(["map:"]);
  const needed = isObj(bundle.manifest.icons) && isObj(bundle.manifest.icons.needed) ? bundle.manifest.icons.needed : {};
  const kinds: Record<string, IconKind> = { heroes: "hero", abilities: "ability", items: "item" };
  for (const [dir, kind] of Object.entries(kinds)) {
    const list = needed[dir];
    if (Array.isArray(list)) for (const name of list) if (typeof name === "string") keys.add(`${kind}:${name.toLowerCase()}`);
  }
  for (const h of bundle.match.heroes as Obj[]) keys.add(`hero:${(h.id as string).replace("npc_dota_hero_", "")}`);
  return keys;
}

export interface BundledIcons {
  /** icon key ("hero:slark", "item:blink", "map:") to the app asset URL */
  icons: Map<string, string>;
  /** game build the icons were exported from (the newest when several are bundled), or null */
  build: string | null;
}

/**
 * Builds the icon table from bundled asset URLs keyed by path, as `import.meta.glob` returns them
 * (`.../assets/dota/<build>/heroes/<name>.png`). Only the newest game build is used, so a lookup
 * never mixes two builds; paths outside `dota/<digits>/` are ignored.
 */
export function bundledIcons(urls: Readonly<Record<string, unknown>>): BundledIcons {
  const byBuild = new Map<number, Map<string, string>>();
  for (const [path, url] of Object.entries(urls)) {
    const build = /(?:^|\/)dota\/(\d{1,12})\//.exec(path.replace(/\\/g, "/"));
    const key = iconKey(path);
    if (!build || !key || typeof url !== "string" || !url) continue;
    const n = Number(build[1]);
    if (!byBuild.has(n)) byBuild.set(n, new Map());
    byBuild.get(n)!.set(key, url);
  }
  if (byBuild.size === 0) return { icons: new Map(), build: null };
  const newest = Math.max(...byBuild.keys());
  return { icons: byBuild.get(newest)!, build: String(newest) };
}

/** How many of the needed keys the bundled set has. */
export function countFound(icons: ReadonlyMap<string, string>, keys: ReadonlySet<string>): number {
  let found = 0;
  for (const key of keys) if (icons.has(key)) found++;
  return found;
}

/** The one icon function the viewer uses: bundled icon, else a neutral named placeholder. */
export function iconResolver(icons: ReadonlyMap<string, string>): (kind: IconKind, name: string) => string {
  const cache = new Map<string, string>();
  return (kind, name) => {
    const key = `${kind}:${kind === "map" ? "" : name.toLowerCase()}`;
    const hit = icons.get(key) ?? cache.get(key);
    if (hit) return hit;
    const url = placeholderIcon(kind, name);
    cache.set(key, url);
    return url;
  };
}

const GAME_MODES: Record<number, string> = {
  1: "All Pick", 2: "Captains Mode", 3: "Random Draft", 4: "Single Draft", 5: "All Random",
  16: "Captains Draft", 18: "Ability Draft", 22: "All Pick", 23: "Turbo",
};

/** Display name of a recorded game mode; null when the bundle does not name a known one. */
export function gameModeName(mode: number | null): string | null {
  return mode === null ? null : GAME_MODES[mode] ?? null;
}
