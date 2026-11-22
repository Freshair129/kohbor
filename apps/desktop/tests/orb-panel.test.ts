// Server-rendered orb panel at one to four channels (owner request 2026-11-22: at most four rows). Checks the columns,
// the panel width per count, the honest states and that every control stays out of the keyboard focus order.
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createServer } from "vite";
import react from "@vitejs/plugin-react";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseMixer, parseOrbState, type MixerView } from "../src/orb/orbModel.ts";

const desktopRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let Panel: (props: Record<string, unknown>) => unknown;
let closeVite: (() => Promise<void>) | undefined;

before(async () => {
  const server = await createServer({
    configFile: false,
    plugins: [react()],
    root: desktopRoot,
    appType: "custom",
    resolve: { preserveSymlinks: true },
    optimizeDeps: { noDiscovery: true, include: [] },
    server: { middlewareMode: true },
  });
  closeVite = () => server.close();
  ({ Panel } = await server.ssrLoadModule("/src/orb/OrbStage.tsx"));
});

after(async () => {
  await closeVite?.();
});

const layout = { disc: 52, dead: 8, cy: 324, out_cx: 34, hot: { x: 8, y: 290, w: 28, h: 68 }, panel_x: 72, panel_w: 231, sp: 1, monitor_h: 1080 };
const orb = parseOrbState({ visible: true, phase: "open", reason: null, dwell_ms: 1000, charge_ms: null, locked: false,
  panel: { x: 72, y: 293, w: 231, h: 403 }, layout });

const game = { id: "game", kind: "game", state: "present", exe: "dota2.exe", percent: 80, muted: false, range: null };
const kohbor = { id: "kohbor", kind: "kohbor", state: "present", exe: null, percent: 60, muted: false, range: null };
const music = { id: "music", kind: "music", state: "present", exe: "spotify.exe", percent: 65, muted: false, range: null };
const discord = { id: "app:discord.exe", kind: "app", state: "mixed", exe: "discord.exe", percent: null, muted: false, range: [30, 100] };

function view(channels: unknown[]): MixerView {
  const parsed = parseMixer({ available: true, reason: null, channels, slots: [], app_choices: ["spotify.exe", "discord.exe"],
    music_selected: null, focus: { active: false, percent: 20, channels: [] }, restore_pending: 0 });
  assert.ok(parsed);
  return parsed;
}

function render(channels: unknown[]): string {
  return renderToStaticMarkup(createElement(Panel as never, { state: orb, view: view(channels), error: null, onReply: () => {}, onError: () => {} }));
}

const cases: Array<[number, unknown[], number, string[]]> = [
  [1, [game], 154, ["เกม"]],
  [2, [game, music], 154, ["เกม", "เพลง"]],
  [3, [game, kohbor, music], 220, ["เกม", "เกาะเบาะ", "เพลง"]],
  [4, [game, kohbor, music, discord], 288, ["เกม", "เกาะเบาะ", "เพลง", "Discord"]],
];

for (const [count, channels, width, names] of cases) {
  test(`panel renders ${count} channel(s) in order at ${width} CSS px`, () => {
    const html = render(channels);
    assert.equal(html.match(/class="orb-col[ "]/g)?.length, count);
    assert.match(html, new RegExp(`--cols:${count}`));
    assert.match(html, new RegExp(`width:${width}px`));
    const found = [...html.matchAll(/<div class="orb-name">([^<]*)<\/div>/g)].map((match) => match[1]);
    assert.deepEqual(found, names);
    // Every slider and button stays out of the keyboard order (Dota keeps every key).
    const controls = (html.match(/<button /g)?.length ?? 0) + (html.match(/role="slider"/g)?.length ?? 0);
    assert.equal(html.match(/tabindex="-1"/g)?.length, controls);
    assert.equal(html.match(/role="slider"/g)?.length, count);
  });
}

test("a mixed app is drawn as a range band with no thumb and no single value", () => {
  const html = render([game, kohbor, music, discord]);
  const columns = html.split(/<div class="orb-col(?=[ "])/).slice(1);
  const last = columns[3] ?? "";
  assert.match(last, /orb-range/);
  assert.doesNotMatch(last, /orb-thumb/);
  assert.match(last, /30–100%/);
  assert.doesNotMatch(last, /aria-valuenow/);
  assert.match(columns[0] ?? "", /aria-valuenow="80"/);
});

test("an added app without a session is a disabled hatched column with the honest note", () => {
  const gone = { ...discord, state: "missing", range: null, muted: null };
  const html = render([game, kohbor, music, gone]);
  const last = html.split(/<div class="orb-col(?=[ "])/).slice(1)[3] ?? "";
  assert.match(last, /^ is-disabled/);
  assert.match(last, /aria-disabled="true"/);
  assert.match(last, />—</);
  assert.match(html, /ยังไม่พบเสียงของ Discord/);
});

test("only the Music column offers the app picker and Focus names the other sound", () => {
  const html = render([game, kohbor, music, discord]);
  assert.equal(html.match(/orb-change/g)?.length, 1);
  assert.match(html, /ลดเสียงอื่นเหลือ 20%/);
  const gameOnly = render([game]);
  assert.match(gameOnly, /class="orb-focus"[^>]*disabled=""/, "nothing for Focus to lower with Game alone");
});
