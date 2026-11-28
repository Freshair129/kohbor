import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  admitLibraryBundle, admitMatchBundle, bundledIcons, countFound, iconKey, iconResolver, MAX_BUNDLE_BYTES, neededIconKeys, type PickedFile,
} from "../src/replay/matchBundle.ts";

// Synthetic Match Bundle (schema 3.0 shape, two heroes, five seconds). No recorded match data.
const L = 5;
const series = (v: unknown) => Array.from({ length: L }, () => v);
function hero(id: string, team: number, pid: number) {
  return {
    id, name: id.replace("npc_dota_hero_", ""), team, pid, color: pid ? "#ff5a4f" : "#57d16a",
    pos: series([0, 0]), hp: series(600), mhp: series(600), mp: series(300), mmp: series(300), lvl: series(1),
    alive: series(1), eco: series([600, 600, 0, 0]),
    abil: [{ n: `${id.replace("npc_dota_hero_", "")}_one`, lv: series(1), cd: series(0), cl: series(10), mc: series(90) }],
    inv: [], icd: {}, mods: [], casts: [], buys: [], finds: [],
  };
}
const match = {
  meta: { schema: "3.0", patch: "7.41f", game_mode: 2, duration_s: 4, match_id: 1 },
  t0: -2, t1: 2,
  heroes: [hero("npc_dota_hero_alpha", 2, 0), hero("npc_dota_hero_beta", 3, 1)],
  deaths: [{ t: 1, hero: "npc_dota_hero_alpha", killer: "npc_dota_hero_beta", last: [], by: [], win: [0, 1], hits: [], heals: [], seen: {}, out: 0 }],
  names: { npc_dota_hero_alpha: "Alpha" },
  statdb: { items: {}, enh: {}, buffs: {}, heroes: {}, src: {}, sim: {}, pierce: {}, abpierce: {} },
};
const world = { v: 2, qh: 8, qu: 8, t0: -2, heroes: {}, tracks: [], runes: [] };

const enc = (v: unknown) => new TextEncoder().encode(JSON.stringify(v));
const sha = (b: Uint8Array) => createHash("sha256").update(b).digest("hex");
const file = (name: string, bytes: Uint8Array, size = bytes.byteLength): PickedFile => ({
  name, size, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer,
});

function bundle(overrides: { manifest?: object; match?: object } = {}) {
  const m = enc(overrides.match ?? match), w = enc(world);
  const manifest = {
    schema: "3.0", match_id: 1, patch: "7.41f",
    files: { "match.json": { bytes: m.byteLength, sha256: sha(m) }, "world.json": { bytes: w.byteLength, sha256: sha(w) } },
    map: { image: "map.png", calibration: { px: [0.049583, 506.7], py: [-0.049108, 506.3] } },
    icons: { needed: { heroes: ["alpha", "beta"], items: [], abilities: ["alpha_one"] } },
    ...overrides.manifest,
  };
  return [file("manifest.json", enc(manifest)), file("match.json", m), file("world.json", w)];
}

function schema31Match(overrides: { winner?: unknown; draft?: unknown } = {}) {
  return {
    ...match,
    meta: {
      ...match.meta,
      schema: "3.1",
      winner: overrides.winner === undefined ? "radiant" : overrides.winner,
      ...(overrides.winner === null ? { winner_reason: "not in replay" } : {}),
      draft: overrides.draft === undefined ? [{ order: 0, team: "radiant", kind: "pick", hero: "npc_dota_hero_alpha" }] : overrides.draft,
      ...(overrides.draft === null ? { draft_reason: "not in replay" } : {}),
    },
    heroes: match.heroes.map(h => ({
      ...h,
      gold_earned: [null, 100, 200, 300, 400],
      gold_earned_final: 400,
      dmg_out: { total: 75, p: 25, m: 40, u: 10 },
    })),
  };
}

function schema31Bundle(matchData = schema31Match()) {
  return bundle({ manifest: { schema: "3.1" }, match: matchData });
}

function schema32Match(overrides: { daynight?: unknown; modsLong?: unknown; modStacks?: unknown } = {}) {
  const base = schema31Match();
  return {
    ...base,
    meta: {
      ...base.meta,
      schema: "3.2",
      daynight: overrides.daynight === undefined ? [[-90, "night"], [0, "day"], [300, "night"]] : overrides.daynight,
      ...(overrides.daynight === null ? { daynight_reason: "not in replay" } : {}),
    },
    heroes: base.heroes.map((h, index) => ({
      ...h,
      mods_long: index === 0
        ? (overrides.modsLong === undefined ? [
          [1.2, null, "modifier_alpha_permanent", "npc_dota_hero_alpha", ["permanent", "stacking"]],
          [2, 3, "modifier_tower_aura", { unit: "dota_goodguys_tower3_mid", team: "radiant" }, ["aura"]],
          [2.5, 4, "modifier_unknown_source", null, []],
        ] : overrides.modsLong)
        : [],
      mod_stacks: index === 0
        ? (overrides.modStacks === undefined ? [[1.2, "modifier_alpha_permanent", 1], [2, "modifier_alpha_permanent", 0]] : overrides.modStacks)
        : [],
    })),
  };
}

function schema32Bundle(matchData = schema32Match()) {
  return bundle({ manifest: { schema: "3.2" }, match: matchData });
}

test("a complete schema 3.0 bundle is admitted with its recorded metadata", async () => {
  const result = await admitMatchBundle(bundle());
  assert.equal(result.ok, true);
  if (!result.ok) return;
  assert.equal(result.bundle.patch, "7.41f");
  assert.equal(result.bundle.gameMode, 2);
  assert.equal(result.bundle.durationS, 4);
  assert.equal(result.bundle.mapImage, "map.png");
});

test("a schema 3.1 bundle with the new replay fields is admitted by both renderer paths", async () => {
  const result = await admitMatchBundle(schema31Bundle());
  assert.equal(result.ok, true);
  if (!result.ok) return;
  const meta = result.bundle.match.meta as Record<string, unknown>;
  const heroes = result.bundle.match.heroes as Array<Record<string, unknown>>;
  assert.equal(meta.schema, "3.1");
  assert.equal(meta.winner, "radiant");
  assert.deepEqual(meta.draft, [{ order: 0, team: "radiant", kind: "pick", hero: "npc_dota_hero_alpha" }]);
  assert.deepEqual(heroes[0].gold_earned, [null, 100, 200, 300, 400]);
  assert.equal(heroes[0].gold_earned_final, 400);
  assert.deepEqual(heroes[0].dmg_out, { total: 75, p: 25, m: 40, u: 10 });

  const opened = admitLibraryBundle({ manifest: result.bundle.manifest, match_data: result.bundle.match, world: result.bundle.world });
  assert.equal(opened.ok, true);
});

test("a schema 3.2 bundle admits day/night and all modifier source shapes", async () => {
  const result = await admitMatchBundle(schema32Bundle());
  assert.equal(result.ok, true);
  if (!result.ok) return;
  const meta = result.bundle.match.meta as Record<string, unknown>;
  const heroes = result.bundle.match.heroes as Array<Record<string, unknown>>;
  assert.equal(meta.schema, "3.2");
  assert.deepEqual(meta.daynight, [[-90, "night"], [0, "day"], [300, "night"]]);
  assert.deepEqual(heroes[0].mods_long, [
    [1.2, null, "modifier_alpha_permanent", "npc_dota_hero_alpha", ["permanent", "stacking"]],
    [2, 3, "modifier_tower_aura", { unit: "dota_goodguys_tower3_mid", team: "radiant" }, ["aura"]],
    [2.5, 4, "modifier_unknown_source", null, []],
  ]);
  assert.deepEqual(heroes[0].mod_stacks, [[1.2, "modifier_alpha_permanent", 1], [2, "modifier_alpha_permanent", 0]]);
  assert.equal(admitLibraryBundle({ manifest: result.bundle.manifest, match_data: result.bundle.match, world: result.bundle.world }).ok, true);
});

test("schema 3.1 accepts null winner, draft and gold values with reasons where defined", async () => {
  const data = schema31Match({ winner: null, draft: null });
  data.heroes = data.heroes.map(h => ({ ...h, gold_earned: [null, null, null, null, null], gold_earned_final: null }));
  const result = await admitMatchBundle(schema31Bundle(data));
  assert.equal(result.ok, true);
  if (!result.ok) return;
  const meta = result.bundle.match.meta as Record<string, unknown>;
  const heroes = result.bundle.match.heroes as Array<Record<string, unknown>>;
  assert.equal(meta.winner, null);
  assert.equal(meta.winner_reason, "not in replay");
  assert.equal(meta.draft, null);
  assert.equal(meta.draft_reason, "not in replay");
  assert.deepEqual(heroes[0].gold_earned, [null, null, null, null, null]);
  assert.equal(heroes[0].gold_earned_final, null);
});

test("a wrong type on a schema 3.1 field is rejected", async () => {
  const data = schema31Match();
  data.heroes = data.heroes.map((h, index) => index === 0
    ? { ...h, dmg_out: { total: "75", p: 25, m: 40, u: 10 } }
    : h);
  const result = await admitMatchBundle(schema31Bundle(data));
  assert.deepEqual(!result.ok && result.error, { code: "shape", file: "match.json", path: "heroes[0].dmg_out.total" });
});

test("schema 3.3 and mismatched manifest/match schemas are rejected", async () => {
  const badManifest = await admitMatchBundle(bundle({ manifest: { schema: "3.3" } }));
  assert.deepEqual(!badManifest.ok && badManifest.error, { code: "schema", file: "manifest.json", found: "3.3" });

  const wrongMatchVersion = { ...schema32Match(), meta: { ...schema32Match().meta, schema: "3.3" } };
  const badMatch = await admitMatchBundle(schema32Bundle(wrongMatchVersion));
  assert.deepEqual(!badMatch.ok && badMatch.error, { code: "schema", file: "match.json", found: "3.3" });

  const mismatch = await admitMatchBundle(bundle({ manifest: { schema: "3.1" }, match: schema32Match() }));
  assert.equal(mismatch.ok, false);
});

test("wrong types on schema 3.2 day/night and modifier additions are rejected", async () => {
  const badDaynight = await admitMatchBundle(schema32Bundle(schema32Match({ daynight: "night" })));
  assert.deepEqual(!badDaynight.ok && badDaynight.error, { code: "shape", file: "match.json", path: "meta.daynight" });

  const badModsLong = await admitMatchBundle(schema32Bundle(schema32Match({ modsLong: [[1, 2, "name", 42, []]] })));
  assert.deepEqual(!badModsLong.ok && badModsLong.error, { code: "shape", file: "match.json", path: "heroes[0].mods_long[0][3]" });

  const badModStacks = await admitMatchBundle(schema32Bundle(schema32Match({ modStacks: [[1, "modifier_stack", "two"]] })));
  assert.deepEqual(!badModStacks.ok && badModStacks.error, { code: "shape", file: "match.json", path: "heroes[0].mod_stacks[0][2]" });
});

test("a missing bundle file is named", async () => {
  const result = await admitMatchBundle(bundle().filter(f => f.name !== "world.json"));
  assert.deepEqual(result, { ok: false, error: { code: "missing", files: ["world.json"] } });
});

test("an oversized file is refused before it is read", async () => {
  let read = false;
  const files = bundle().map(f => f.name === "match.json"
    ? { ...f, size: MAX_BUNDLE_BYTES["match.json"] + 1, arrayBuffer: async () => { read = true; return new ArrayBuffer(0); } }
    : f);
  const result = await admitMatchBundle(files);
  assert.equal(result.ok, false);
  assert.equal(!result.ok && result.error.code, "too-large");
  assert.equal(read, false);
});

test("a different schema version is refused", async () => {
  const result = await admitMatchBundle(bundle({ manifest: { schema: "2.0" } }));
  assert.deepEqual(result, { ok: false, error: { code: "schema", file: "manifest.json", found: "2.0" } });
});

test("a file that does not match its manifest hash is refused", async () => {
  const files = bundle();
  const tampered = enc({ ...match, t1: 3 });
  files[1] = file("match.json", tampered);
  const result = await admitMatchBundle(files);
  assert.deepEqual(result, { ok: false, error: { code: "integrity", file: "match.json" } });
});

test("unsafe or inconsistent fields are shape errors, not defaults", async () => {
  const badColor = { ...match, heroes: [{ ...match.heroes[0], color: "red;x" }, match.heroes[1]] };
  const r1 = await admitMatchBundle(bundle({ match: badColor }));
  assert.deepEqual(!r1.ok && r1.error, { code: "shape", file: "match.json", path: "heroes[0].color" });
  const shortSeries = { ...match, heroes: [{ ...match.heroes[0], hp: [1, 2] }, match.heroes[1]] };
  const r2 = await admitMatchBundle(bundle({ match: shortSeries }));
  assert.deepEqual(!r2.ok && r2.error, { code: "shape", file: "match.json", path: "heroes[0].hp" });
  const strangerDeath = { ...match, deaths: [{ ...match.deaths[0], hero: "npc_dota_hero_gamma" }] };
  const r3 = await admitMatchBundle(bundle({ match: strangerDeath }));
  assert.deepEqual(!r3.ok && r3.error, { code: "shape", file: "match.json", path: "deaths[0]" });
});

test("bundled icons resolve by exact name from the newest build, else to a named placeholder", async () => {
  assert.equal(iconKey("../assets/dota/6989/heroes/alpha.png"), "hero:alpha");
  assert.equal(iconKey("../assets/dota/6989/items/blink.png"), "item:blink");
  assert.equal(iconKey("../assets/dota/6989/abilities/alpha_one.png"), "ability:alpha_one");
  assert.equal(iconKey("../assets/dota/6989/map.png"), "map:");
  assert.equal(iconKey("../assets/dota/6989/provenance.json"), null);
  const { icons, build } = bundledIcons({
    "../assets/dota/6988/heroes/alpha.png": "/assets/old-alpha.png",
    "../assets/dota/6988/heroes/zeta.png": "/assets/old-zeta.png",
    "../assets/dota/6989/heroes/alpha.png": "/assets/alpha.png",
    "../assets/dota/6989/map.png": "/assets/map.png",
    "../assets/other/heroes/beta.png": "/assets/beta.png",
    "../assets/dota/6989/heroes/Bad-Name.png": "/assets/bad.png",
  });
  assert.equal(build, "6989");
  assert.deepEqual([...icons.entries()].sort(), [["hero:alpha", "/assets/alpha.png"], ["map:", "/assets/map.png"]]);
  assert.deepEqual(bundledIcons({}), { icons: new Map(), build: null });
  const admitted = await admitMatchBundle(bundle());
  assert.ok(admitted.ok);
  if (!admitted.ok) return;
  const keys = neededIconKeys(admitted.bundle);
  assert.ok(keys.has("hero:alpha") && keys.has("hero:beta") && keys.has("map:"));
  assert.equal(countFound(icons, keys), 2);
  const icon = iconResolver(icons);
  assert.equal(icon("hero", "alpha"), "/assets/alpha.png");
  assert.equal(icon("map", "map"), "/assets/map.png");
  const missing = decodeURIComponent(icon("hero", "beta"));
  assert.match(missing, /^data:image\/svg\+xml/);
  assert.match(missing, />beta</);
  assert.equal(icon("hero", "beta").startsWith("/assets/"), false, "never another hero's icon");
});

test("the Match view has no icon folder picker and selects the patch's bundled set", () => {
  const panel = readFileSync(new URL("../src/replay/MatchViewPanel.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(panel, /webkitdirectory|readIcons/);
  assert.match(panel, /replayIconsForPatch\(initialBundle\.patch\)/);
  const glob = readFileSync(new URL("../src/replay/bundledIcons.ts", import.meta.url), "utf8");
  assert.match(glob, /import\.meta\.glob/);
});

test("replay stat rows use the shared card model and keep their existing value cells", () => {
  const panel = readFileSync(new URL("../src/replay/MatchViewPanel.tsx", import.meta.url), "utf8");
  const viewer = readFileSync(new URL("../src/replay/matchViewer.mjs", import.meta.url), "utf8");
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8");
  assert.match(panel, /STAT_ICON_CATALOG/);
  assert.match(panel, /heroCard\.css\?raw/);
  assert.match(panel, /statIcon: \(id: StatIconId\) => statIconUrl\(id\)/);
  assert.match(viewer, /statDefinition\(key\)\.accessibleLabel/);
  assert.match(viewer, /const sharedView = heroCardView\(\{/);
  assert.match(viewer, /reg: statValue\("hpreg"\)/);
  assert.match(viewer, /const hpreg = known\("hpreg"\), mpreg = known\("mpreg"\)/);
  assert.match(viewer, /data-card-row="\$\{id\}"/);
  assert.match(viewer, /const dmin = known\("dmin", null\), dmax = known\("dmax", null\), dbonus = known\("dbonus", null\)/);
  assert.match(viewer, /d\.armor\.toFixed\(1\)/);
  assert.match(css, /\.rc \.stat-glyph img\s*\{[^}]*width:\s*10px;\s*height:\s*10px/);
  assert.match(css, /\.rc \.row\.st\s*\{[^}]*grid-area:\s*st/);
  assert.match(css, /\.rc \.row\.dvs\s*\{[^}]*grid-area:\s*dv/);
});

test("the Match view sources make no network request and use patch-scoped data", () => {
  const dir = new URL("../src/replay/", import.meta.url);
  for (const name of ["matchViewer.mjs", "matchViewer.html", "matchViewer.css", "MatchViewPanel.tsx", "bundledIcons.ts", "replayPack.mjs"]) {
    const source = readFileSync(new URL(name, dir), "utf8");
    assert.doesNotMatch(source, /https?:\/\/|fetch\(|XMLHttpRequest|@import|fonts\.googleapis/, name);
  }
  const viewer = readFileSync(new URL("matchViewer.mjs", dir), "utf8");
  assert.doesNotMatch(viewer, /const (MAP|ULTS|ULT_ABIL|CC|MOBILITY) =/);
  assert.match(viewer, /matchViewerCopy\[locale\]\.viewerStrings/);
});
