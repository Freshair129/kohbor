import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  COUNTDOWN_OBJECTIVES,
  RUNE_SPOTS,
  SCHEDULE_STALE_MS,
  countdownLabel,
  countdownLabels,
  parseObjectiveSchedule,
  spotLabels,
  // @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
} from "../src/live/objectiveCountdownModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { draftBoardCopy, draftBoardRows, formatWinRate } from "../src/live/draftBoardModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { createDraftRosterMemory, rememberDraftFrame } from "../src/live/secondMonitorModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftBarFrame } from "../src/deck/draftBarModel.ts";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

// ---- Rune countdowns (UI-018) ----
const power = { objective: "power_rune" as const, occurrence_seconds: 480, lead_s: [30, 15] };

test("countdown shows coarse minutes while far, then seconds from the largest configured lead time", () => {
  const at = (clock: number) => countdownLabel(power, clock);
  assert.deepEqual([at(300)?.text, at(300)?.mode], ["3m", "minutes"]);
  assert.equal(at(359)?.text, "2m");
  assert.equal(at(361)?.text, "1m");
  assert.equal(at(449)?.text, "1m", "31 s left with a 30 s lead time is still minutes");
  assert.deepEqual([at(450)?.text, at(450)?.mode], ["30", "seconds"]);
  assert.equal(at(479)?.text, "1");
  assert.deepEqual([at(480)?.text, at(480)?.mode], ["0", "now"]);
  assert.equal(at(481), null, "a past occurrence has no label");
  const short = { ...power, lead_s: [15] };
  assert.equal(countdownLabel(short, 460)?.text, "1m");
  assert.equal(countdownLabel(short, 465)?.text, "15");
  const none = { objective: "bounty_rune" as const, occurrence_seconds: 480, lead_s: [] };
  assert.equal(countdownLabel(none, 419)?.text, "1m");
  assert.equal(countdownLabel(none, 420)?.text, "60", "no lead time set: seconds in the last minute");
});

test("schedule parser is strict and admits only labelled rune objectives", () => {
  const valid = { schema_version: 1, observed_clock_seconds: 300, upcoming: [power, { objective: "wisdom_rune", occurrence_seconds: 420, lead_s: [] }] };
  assert.ok(parseObjectiveSchedule(valid));
  for (const bad of [
    null, { ...valid, schema_version: 2 }, { ...valid, extra: 1 }, { ...valid, observed_clock_seconds: 1.5 },
    { ...valid, upcoming: [{ ...power, objective: "tormentor" }] },
    { ...valid, upcoming: [{ ...power, objective: "roshan" }] },
    { ...valid, upcoming: [{ ...power, lead_s: [4] }] },
    { ...valid, upcoming: [{ ...power, lead_s: [30, 30] }] },
    { ...valid, upcoming: [{ ...power, lead_s: [10, 20, 30] }] },
    { ...valid, upcoming: [power, power] },
    { ...valid, upcoming: [{ ...power, occurrence_seconds: -1 }] },
  ]) assert.equal(parseObjectiveSchedule(bad), null, JSON.stringify(bad));
  assert.deepEqual([...COUNTDOWN_OBJECTIVES].sort(), ["bounty_rune", "power_rune", "water_rune", "wisdom_rune"]);
});

test("labels use only the schedule's game clock; a stale or missing schedule shows nothing", () => {
  const schedule = parseObjectiveSchedule({ schema_version: 1, observed_clock_seconds: 450, upcoming: [power] });
  assert.deepEqual(countdownLabels(schedule, 1_000, 1_000).map(label => label.text), ["30"]);
  assert.deepEqual(countdownLabels(schedule, 1_000, 1_000 + 4_000).map(label => label.text), ["30"], "no wall-clock extrapolation");
  assert.deepEqual(countdownLabels(schedule, 1_000, 1_000 + SCHEDULE_STALE_MS + 1), []);
  assert.deepEqual(countdownLabels(null, 1_000, 1_000), []);
  assert.deepEqual(countdownLabels(schedule, null, 1_000), []);
});

test("one label per map spot; water and power share river spots and the sooner one wins", () => {
  const spots = spotLabels([
    { objective: "power_rune", text: "1m", mode: "minutes", remaining: 90 },
    { objective: "water_rune", text: "20", mode: "seconds", remaining: 20 },
    { objective: "bounty_rune", text: "3m", mode: "minutes", remaining: 200 },
  ]);
  assert.equal(spots.length, RUNE_SPOTS.power_rune.length + RUNE_SPOTS.bounty_rune.length);
  assert.ok(spots.filter(spot => RUNE_SPOTS.power_rune.some(p => p.x === spot.x && p.y === spot.y)).every(spot => spot.label.objective === "water_rune"));
  for (const list of Object.values(RUNE_SPOTS)) for (const spot of list) assert.ok(spot.x > 0 && spot.x < 1 && spot.y > 0 && spot.y < 1);
});

test("countdown sources: no timing table, no tormentor or Roshan, no wall-clock game time, original markers", () => {
  const model = read("../src/live/objectiveCountdownModel.ts");
  assert.doesNotMatch(model, /spawn_times_seconds|repeat_every_seconds|\b(?:120|240|360|420)\b(?!\s*s)/, "the ALERT-008 timing table is not duplicated");
  const view = read("../src/live/MapCountdowns.tsx");
  assert.doesNotMatch(view + model, /<img|\.png|\.svg"|valve/i);
  assert.match(read("../src/live/secondMonitor.css"), /\.sm-map-overlay\s*\{[^}]*pointer-events:\s*none/);
  // styles.css paints `.live-map-overlay` with an opaque surface; reusing that name hid the minimap in the first mock.
  assert.doesNotMatch(read("../src/live/MinimapPanel.tsx"), /className="live-map-overlay"/);
});

// ---- Draft board (UI-019) ----
function draftFrame(ids: (string | null)[]): DraftBarFrame {
  return {
    active: true, detail: null, bgra: null,
    slots: ids.map((heroId, slot) => ({ slot, team: slot < 5 ? "radiant" : "dire", hero_id: heroId, confidence: heroId ? 0.9 : 0, reason: heroId ? "identified" : "low_confidence" })),
  } as DraftBarFrame;
}
const memory = rememberDraftFrame(createDraftRosterMemory(), 3, draftFrame([
  "npc_dota_hero_axe", null, null, null, null, "npc_dota_hero_sniper", null, null, null, null,
]));

test("draft board: two team rows of five; identity from same-session draft only; unknown is null", () => {
  const rows = draftBoardRows({ memory, epoch: 3, ownTeam: "dire", showNames: false });
  assert.deepEqual(rows.map(row => [row.team, row.ours, row.slots.length]), [["radiant", false, 5], ["dire", true, 5]]);
  assert.equal(rows[0].slots[0].heroName, "Axe");
  assert.equal(rows[1].slots[0].heroName, "Sniper");
  assert.ok(rows.flatMap(row => row.slots).every(slot => slot.playerName === null && slot.winRate === null && slot.position === null));
  assert.ok(draftBoardRows({ memory, epoch: 4, ownTeam: "dire", showNames: false }).every(row => row.slots.every(slot => slot.heroName === null)));
  assert.ok(draftBoardRows({ memory, epoch: 3, ownTeam: null, showNames: false }).every(row => row.ours === null), "unknown team: no side marked");
});

test("draft board: player names only while the visible setting is on; stats only for picked heroes", () => {
  const names = { 0: "  Player\u0007One  ", 5: "x".repeat(80), 1: "Hidden" };
  const off = draftBoardRows({ memory, epoch: 3, ownTeam: "radiant", names, showNames: false });
  assert.ok(off.flatMap(row => row.slots).every(slot => slot.playerName === null), "setting off: no names at all");
  const on = draftBoardRows({ memory, epoch: 3, ownTeam: "radiant", names, showNames: true });
  assert.equal(on[0].slots[0].playerName, "PlayerOne");
  assert.equal(on[1].slots[0].playerName?.length, 32);
  const stats = { 0: { win_rate_tenths: 523, position: 3 as const, low_data: false }, 1: { win_rate_tenths: 600, position: 1 as const, low_data: false },
    5: { win_rate_tenths: 1200, position: 9 as never, low_data: true } };
  const rows = draftBoardRows({ memory, epoch: 3, ownTeam: "radiant", stats, showNames: false });
  assert.deepEqual([rows[0].slots[0].winRate, rows[0].slots[0].position], [52.3, 3]);
  assert.deepEqual([rows[0].slots[1].winRate, rows[0].slots[1].position], [null, null], "no pick: stats not shown");
  assert.deepEqual([rows[1].slots[0].winRate, rows[1].slots[0].position, rows[1].slots[0].lowData], [null, null, true], "out-of-range values are unknown");
  assert.equal(formatWinRate(null), "—");
  assert.equal(formatWinRate(52.3), "52.3%");
});

test("draft board never shows a win chance and labels estimates and guesses; names never leave memory", () => {
  for (const locale of ["th", "en"] as const) {
    const all = JSON.stringify(draftBoardCopy(locale)).toLowerCase();
    for (const banned of ["win chance", "โอกาสชนะ", "chance"]) assert.ok(!all.includes(banned), banned);
  }
  assert.equal(draftBoardCopy("th").estimate, "ค่าประมาณ");
  assert.equal(draftBoardCopy("th").guess, "เดา");
  const sources = read("../src/live/draftBoardModel.ts") + read("../src/live/DraftBoard.tsx");
  assert.doesNotMatch(sources, /localStorage|sessionStorage|invoke\(|fetch\(|console\.|logHost/, "names are display-only, in memory");
});

// ---- Chip row overlap (owner report, build 8d9f8c3) ----
test("status chip row wraps instead of letting the start button paint over the chips", () => {
  const css = read("../src/steam/steam.css").replace(/\/\*[\s\S]*?\*\//g, "");
  const bar = css.match(/\.steam-start-bar\s*\{([^}]+)\}/)?.[1] ?? "";
  const chips = css.match(/\.steam-service-chips\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(bar, /flex-wrap:\s*wrap/);
  assert.match(chips, /flex-wrap:\s*wrap/);
  assert.doesNotMatch(css, /\.steam-[a-z-]+\s*\{[^}]*position:\s*absolute/);
});
