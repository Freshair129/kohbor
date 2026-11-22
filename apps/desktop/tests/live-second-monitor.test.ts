import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  SECOND_MONITOR_STORAGE_KEY,
  createDraftRosterMemory,
  enemyRows,
  formatCountdown,
  mapEdgeFor,
  missingFromMap,
  persistSecondMonitorSide,
  readSecondMonitorSide,
  rememberDraftFrame,
  secondMonitorCopy,
  // @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
} from "../src/live/secondMonitorModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftBarFrame } from "../src/deck/draftBarModel.ts";


function storage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => { data.set(key, value); }, data };
}

function draftFrame(ids: (string | null)[]): DraftBarFrame {
  return {
    active: true, detail: null, bgra: null,
    slots: ids.map((heroId, slot) => ({
      slot, team: slot < 5 ? "radiant" : "dire", hero_id: heroId,
      confidence: heroId ? 0.9 : 0, reason: heroId ? "identified" : "low_confidence",
    })),
  } as DraftBarFrame;
}

const ROSTER = [
  "npc_dota_hero_silencer", "npc_dota_hero_axe", "npc_dota_hero_lion", "npc_dota_hero_juggernaut", "npc_dota_hero_earthshaker",
  "npc_dota_hero_sniper", "npc_dota_hero_pudge", "npc_dota_hero_phantom_assassin", "npc_dota_hero_crystal_maiden", null,
];

test("the side setting defaults to off and survives a broken or unknown stored value", () => {
  assert.equal(readSecondMonitorSide(storage()), "off");
  assert.equal(readSecondMonitorSide(storage({ [SECOND_MONITOR_STORAGE_KEY]: "top" })), "off");
  assert.equal(readSecondMonitorSide({ getItem: () => { throw new Error("blocked"); } }), "off");
  const store = storage();
  assert.equal(persistSecondMonitorSide(store, "right"), "right");
  assert.equal(readSecondMonitorSide(store), "right");
  assert.equal(persistSecondMonitorSide({ setItem: () => { throw new Error("full"); } }, "left"), "left");
});

test("monitor 2 on the right pins the map to the left edge, and the reverse", () => {
  assert.equal(mapEdgeFor("right"), "left");
  assert.equal(mapEdgeFor("left"), "right");
});

test("missing-from-map is derived only when both counts are known; unknown is never zero", () => {
  assert.equal(missingFromMap(null), null);
  const base = { meter: 1, gsi_source: true, vision_source: true, events: [] };
  assert.equal(missingFromMap({ ...base, visible: null, baseline: 4 }), null);
  assert.equal(missingFromMap({ ...base, visible: 2, baseline: null }), null);
  assert.equal(missingFromMap({ ...base, visible: 2, baseline: 5 }), 3);
  assert.equal(missingFromMap({ ...base, visible: 0, baseline: 0 }), 0);
  assert.equal(missingFromMap({ ...base, visible: 5, baseline: 3 }), 0);
});

test("enemy rows come only from draft identities in the same session; alive/dead stays unknown", () => {
  const empty = enemyRows(createDraftRosterMemory(), 7, "radiant");
  assert.deepEqual(empty.map(row => row.heroName), [null, null, null, null, null]);
  assert.ok(empty.every(row => row.life === null));

  const memory = rememberDraftFrame(createDraftRosterMemory(), 7, draftFrame(ROSTER));
  assert.deepEqual(enemyRows(memory, 7, "radiant").map(row => row.heroName),
    ["Sniper", "Pudge", "Phantom Assassin", "Crystal Maiden", null]);
  assert.deepEqual(enemyRows(memory, 7, "dire").map(row => row.heroName),
    ["Silencer", "Axe", "Lion", "Juggernaut", "Earthshaker"]);
  assert.ok(enemyRows(memory, 7, "radiant").every(row => row.life === null));
  assert.deepEqual(enemyRows(memory, 7, null).map(row => row.heroName), [null, null, null, null, null], "unknown team: no side guessed");
  assert.deepEqual(enemyRows(memory, 8, "radiant").map(row => row.heroName), [null, null, null, null, null], "other session: nothing carried over");
});

test("draft memory clears on a session change and keeps earlier identifications within a session", () => {
  let memory = rememberDraftFrame(createDraftRosterMemory(), 7, draftFrame(ROSTER));
  memory = rememberDraftFrame(memory, 7, draftFrame(Array(10).fill(null)));
  assert.equal(memory.slots[5], "npc_dota_hero_sniper");
  memory = rememberDraftFrame(memory, 7, null);
  assert.equal(memory.slots[5], "npc_dota_hero_sniper", "no frame after the draft keeps the match memory");
  memory = rememberDraftFrame(memory, 9, null);
  assert.deepEqual(memory.slots, Array(10).fill(null));
  memory = rememberDraftFrame(memory, null, draftFrame(ROSTER));
  assert.deepEqual(memory.slots, Array(10).fill(null), "an unknown session admits nothing");
});

test("the next-objective countdown renders an em dash without a source", () => {
  assert.equal(formatCountdown(null), "—");
  assert.equal(formatCountdown(Number.NaN), "—");
  assert.equal(formatCountdown(-1), "—");
  assert.equal(formatCountdown(24), "0:24");
  assert.equal(formatCountdown(84.9), "1:24");
});

test("copy labels every pending source and never claims ally vitals or abnormal states", () => {
  for (const locale of ["th", "en"] as const) {
    const text = secondMonitorCopy(locale);
    assert.equal(text.lifeUnknown, "—");
    assert.ok(text.source.lifePending.length > 0);
    assert.ok(text.source.peerPending.length > 0);
    const all = JSON.stringify(text).toLowerCase();
    for (const banned of ["stun", "สตั้น", "โดนเล็ง", "targeted", "ready", "พร้อม"]) assert.ok(!all.includes(banned), banned);
  }
  assert.equal(secondMonitorCopy("th").sideOptions.right, "ขวาจอหลัก");
});

test("component sources render no invented values", () => {
  const deck = readFileSync(new URL("../src/live/SecondMonitorDeck.tsx", import.meta.url), "utf8");
  assert.match(deck, /row\.heroName \?\? "—"/);
  assert.match(deck, /card\.heroName \?\? "—"/);
  assert.match(deck, /missing === null \? "—"/);
  assert.match(deck, /formatCountdown\(next\?\.secondsUntil \?\? null\)/);
  assert.doesNotMatch(deck, /Math\.random|placeholder hero|npc_dota_hero_/);
});

test("pinned CSS: the map size comes from the layout variable; nothing scrolls; text does not shrink", () => {
  const css = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(css, /\.live-map-pane\s*\{[^}]*width:\s*var\(--sm-map\);\s*height:\s*var\(--sm-map\)/);
  assert.match(css, /\.sm-host\s*\{[^}]*overflow:\s*hidden/);
  assert.doesNotMatch(css, /overflow(?:-[xy])?:\s*(?:auto|scroll)/, "no scrolling region in second-monitor mode");
  assert.doesNotMatch(css, /font-size:\s*(?:[0-9]|1[01])px/, "no text below 12 px");
  assert.match(css, /\.app\[data-live-monitor\]\[data-match="true"\] \.shell-rail-right\s*\{\s*display:\s*none/);
});
