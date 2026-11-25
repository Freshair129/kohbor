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

test("pinned countdown stays small until the final ten seconds or configured alert lead", () => {
  const at = (clock: number) => countdownLabel({ ...power, last_occurrence_seconds: 480 }, clock, "7.42");
  assert.equal(at(300), null);
  assert.equal(at(419), null);
  assert.deepEqual([at(420)?.text, at(420)?.mode], ["60", "seconds"]);
  assert.deepEqual([at(450)?.text, at(450)?.mode], ["30", "seconds"]);
  assert.equal(at(450)?.fullChip, true, "the configured 30-second alert lead expands the chip");
  assert.equal(at(479)?.text, "1");
  assert.deepEqual([at(480)?.text, at(480)?.mode], ["พร้อม", "ready"]);
  assert.deepEqual([at(482)?.text, at(482)?.mode], ["พร้อม", "ready"]);
  assert.equal(at(484), null, "ready is brief");
  const short = { ...power, lead_s: [15] };
  assert.equal(countdownLabel(short, 420, "7.42")?.fullChip, false);
  assert.equal(countdownLabel(short, 465, "7.42")?.fullChip, true);
  assert.equal(countdownLabel(short, 471, "7.42")?.fullChip, true, "the final ten seconds expand even with a shorter lead");
  const none = { objective: "bounty_rune" as const, occurrence_seconds: 480, lead_s: [] };
  assert.equal(countdownLabel(none, 420)?.text, "60");
  assert.equal(countdownLabel(none, 470)?.fullChip, true);
});

test("schedule parser is strict and admits only objective rows used by this map", () => {
  const valid = { schema_version: 1, rule_source: "ALERT-008", patch_version: "7.42", observed_clock_seconds: 300, upcoming: [power, { objective: "wisdom_rune", occurrence_seconds: 420, lead_s: [] }] };
  assert.ok(parseObjectiveSchedule(valid));
  for (const bad of [
    null, { ...valid, schema_version: 2 }, { ...valid, extra: 1 }, { ...valid, observed_clock_seconds: 1.5 },
    { ...valid, upcoming: [{ ...power, objective: "unknown" }] },
    { ...valid, patch_version: null },
    { ...valid, upcoming: [{ ...power, lead_s: [4] }] },
    { ...valid, upcoming: [{ ...power, lead_s: [30, 30] }] },
    { ...valid, upcoming: [{ ...power, lead_s: [10, 20, 30] }] },
    { ...valid, upcoming: [power, power] },
    { ...valid, upcoming: [{ ...power, occurrence_seconds: -1 }] },
  ]) assert.equal(parseObjectiveSchedule(bad), null, JSON.stringify(bad));
  assert.deepEqual([...COUNTDOWN_OBJECTIVES].sort(), ["bounty_rune", "lotus", "power_rune", "wisdom_rune"]);
});

test("labels use only the schedule's game clock; a stale or missing schedule shows nothing", () => {
  const schedule = parseObjectiveSchedule({ schema_version: 1, rule_source: "ALERT-008", patch_version: "7.42", observed_clock_seconds: 450, upcoming: [power] });
  assert.deepEqual(countdownLabels(schedule, 1_000, 1_000).map(label => label.text), ["30"]);
  assert.deepEqual(countdownLabels(schedule, 1_000, 1_000 + 4_000).map(label => label.text), ["30"], "no wall-clock extrapolation");
  assert.deepEqual(countdownLabels({ ...schedule!, observed_clock_seconds: 460 }, 1_000, 1_000).map(label => label.text), ["20"], "only a fresh GSI game clock advances the label");
  assert.deepEqual(countdownLabels(schedule, 1_000, 1_000 + SCHEDULE_STALE_MS + 1), []);
  assert.deepEqual(countdownLabels(null, 1_000, 1_000), []);
  assert.deepEqual(countdownLabels(schedule, null, 1_000), []);
  assert.deepEqual(countdownLabels(parseObjectiveSchedule({ schema_version: 1, rule_source: "ALERT-008", patch_version: null, observed_clock_seconds: 450, upcoming: [power] }), 1_000, 1_000), [], "a schedule without its recorded patch stays unknown");
});

test("unknown positions are omitted; confirmed normalised positions accept seconds and ready labels", () => {
  const spots = spotLabels([
    { objective: "power_rune", text: "20", mode: "seconds", remaining: 20, patch_version: null, fullChip: false },
    { objective: "bounty_rune", text: "พร้อม", mode: "ready", remaining: 0, patch_version: null, fullChip: true },
  ]);
  assert.ok(spots.length > 0, "confirmed replay positions render labels");
  assert.deepEqual(spotLabels([
    { objective: "power_rune", text: "20", mode: "seconds", remaining: 20, patch_version: null, fullChip: false },
    { objective: "bounty_rune", text: "พร้อม", mode: "ready", remaining: 0, patch_version: null, fullChip: true },
  ], {}), []);
  const confirmed = spotLabels([
    { objective: "power_rune", text: "20", mode: "seconds", remaining: 20, patch_version: "7.42", fullChip: false },
    { objective: "bounty_rune", text: "พร้อม", mode: "ready", remaining: 0, patch_version: "7.42", fullChip: true },
  ], { power_rune: [{ x: 0.25, y: 0.75 }], bounty_rune: [{ x: 0.75, y: 0.25 }] }, true);
  assert.deepEqual(confirmed.map(({ x, y }) => [x, y]), [[0.25, 0.75], [0.75, 0.25]]);
  assert.ok(confirmed.every(spot => spot.positionPatchDiffers), "7.42 timing rules differ from 7.41f replay positions");
  assert.ok(RUNE_SPOTS.power_rune.length > 0);
  for (const list of Object.values(RUNE_SPOTS)) for (const spot of list) assert.ok(spot.x > 0 && spot.x < 1 && spot.y > 0 && spot.y < 1);
});

test("countdown sources: no timing table, no tormentor or Roshan, no wall-clock game time, original markers", () => {
  const model = read("../src/live/objectiveCountdownModel.ts");
  assert.doesNotMatch(model, /spawn_times_seconds|repeat_every_seconds|\b(?:120|240|360|420)\b(?!\s*s)/, "the ALERT-008 timing table is not duplicated");
  const view = read("../src/live/MapCountdowns.tsx");
  assert.doesNotMatch(view + model, /<img|\.png|\.svg"|valve/i);
  assert.match(read("../src/live/secondMonitor.css"), /\.sm-map-overlay\s*\{[^}]*pointer-events:\s*none/);
  assert.match(view, /sm-countdown-legend/);
  assert.match(view, /data-position-patch-differs/);
  assert.match(read("../src/live/secondMonitor.css"), /\.sm-countdown-spot\[data-position-patch-differs="true"\] \.sm-countdown-anchor\s*\{[^}]*border-style:\s*dashed/);
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

// ---- Captured minimap on monitor 2 (owner decision 2026-11-23, "3 ก") ----
test("the pinned map shows the captured minimap frame through the existing canvas path, smoothly scaled", () => {
  const panel = read("../src/live/MinimapPanel.tsx");
  assert.match(panel, /ref=\{minimap\.canvasRef\}[\s\S]*className=\{`live-map-canvas/, "one canvas fed by useMinimap for every layout");
  assert.match(read("../src/live/useMinimap.ts"), /minimap_subscribe/);
  const css = (read("../src/live/live.css") + read("../src/live/secondMonitor.css")).replace(/\/\*[\s\S]*?\*\//g, "");
  assert.doesNotMatch(css, /live-map-canvas[^}]*image-rendering:\s*(?:pixelated|crisp-edges)/);
  assert.match(css, /\.sm-pinned \.live-map-canvas\s*\{[^}]*width:\s*100%[^}]*height:\s*100%[^}]*image-rendering:\s*auto/);
});

test("a stale or missing frame is never shown as live", async () => {
  // @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
  const { deriveMinimapPresentation } = await import("../src/live/minimapPresentation.ts");
  const stale = deriveMinimapPresentation("stale", null, true, "th");
  assert.equal(stale.dimmed, true);
  assert.equal(stale.staleOverlay, "age");
  assert.equal(deriveMinimapPresentation("stale", null, false, "th").staleOverlay, "age");
  assert.equal(deriveMinimapPresentation("live", null, true, "th").dimmed, false);
});

test("no hero identification or motion arrows on the second-monitor map", () => {
  const sources = ["../src/live/SecondMonitorDeck.tsx", "../src/live/MapCountdowns.tsx", "../src/live/objectiveCountdownModel.ts", "../src/live/secondMonitorCards.ts"].map(read).join("\n");
  assert.doesNotMatch(sources, /templateMatch|matchTemplate|motion[-_ ]?arrow|pathTrace|hero(?:Direction|Facing)/i);
});
