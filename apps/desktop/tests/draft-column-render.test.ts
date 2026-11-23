import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { runInThisContext } from "node:vm";
import { fileURLToPath } from "node:url";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { transformWithOxc } from "vite";
import { draftColumnPlacement, formatDraftTime, parseDraftColumnView } from "../src/draft/draftColumnModel.ts";

const PUBLIC_FETCH_MS = 1790000000000;
const DURATION_FETCH_MS = 1790003600000;
const requireFromApp = createRequire(import.meta.url);

function evaluateCommonJs(source: string, filename: string, moduleRequire: NodeRequire) {
  const wrapped = runInThisContext(`(function(exports, require, module, __filename, __dirname) { ${source}\n })`, { filename });
  const commonJsModule = { exports: {} as Record<string, unknown> };
  wrapped(commonJsModule.exports, moduleRequire, commonJsModule, filename, filename.slice(0, filename.lastIndexOf("\\")));
  return commonJsModule.exports;
}

async function loadTsxModule(file: string, exportName: string, resolve: (specifier: string) => unknown) {
  const path = fileURLToPath(new URL(`../src/draft/${file}`, import.meta.url));
  const source = readFileSync(path, "utf8").replace(/^import "\.\/[\w-]+\.css";\s*$/gm, "");
  const transformed = await transformWithOxc(source, path, {
    lang: "tsx", sourceType: "module", jsx: { runtime: "automatic", development: false },
  });
  const commonJsSource = transformed.code
    .replace(/^import\s+\{([\s\S]*?)\}\s+from\s+["']([^"']+)["'];?\s*/gm, (_match, bindings: string, specifier: string) => {
      const properties = bindings.split(",").map((binding) => {
        const [imported, local] = binding.trim().split(/\s+as\s+/);
        return local ? `${imported}: ${local}` : imported;
      }).join(", ");
      return `const { ${properties} } = require(${JSON.stringify(specifier)});\n`;
    })
    .replace(new RegExp(String.raw`\bexport\s+function\s+${exportName}\b`), `function ${exportName}`)
    + `\nmodule.exports = { ${exportName} };`;
  return evaluateCommonJs(commonJsSource, path, resolve)[exportName];
}

async function loadDraftColumnView() {
  const model = await import("../src/draft/draftColumnModel.ts");
  const resolve = (overlay: unknown) => (specifier: string) => {
    if (specifier === "./draftColumnModel") return model;
    if (specifier === "./OverlayColumn") return { OverlayColumn: overlay };
    // The portrait calls the host; static renders show the neutral placeholder.
    if (specifier === "./OverlayPortrait") return {
      OverlayPortrait: ({ heroKey }: { heroKey: string | null }) =>
        React.createElement("span", { className: "draft-ov-pt is-missing", "data-hero-key": heroKey ?? "" }, "?"),
    };
    return requireFromApp(specifier);
  };
  const overlay = await loadTsxModule("OverlayColumn.tsx", "OverlayColumn", resolve(null));
  return await loadTsxModule("DraftColumnView.tsx", "DraftColumnView", resolve(overlay)) as React.ComponentType<Record<string, unknown>>;
}

function suggestionPayload() {
  return {
    schema: 1,
    mode: "suggestions",
    overlay: { visible: true, hidden_reason: null },
    phase: "hero_selection",
    placement_verified: true,
    data_state: "live",
    context: { position: 1, bracket: 3 },
    source: {
      provider: "OpenDota", window: null, fetched_at_ms: PUBLIC_FETCH_MS, failed_at_ms: null,
      failure: null, http_status: null, retry_after_ms: null, table_requests_24h: 2, table_request_budget: 4,
    },
    suggestions: {
      rows: [{ hero_id: 1, name: "Spectre", rank: 1, games: 44465, win_rate_tenths: 531, low_data: false, taken: false }],
      ranked_count: 1,
      draft_bar: { available: false, identified: 0, slots: 10 },
    },
    pick_card: null,
  };
}

function pickPayload(durationState: "live" | "stale") {
  const base = suggestionPayload();
  return {
    ...base,
    mode: "pick_card",
    data_state: "live",
    source: {
      ...base.source,
      failed_at_ms: durationState === "stale" ? 1790007200000 : null,
      failure: durationState === "stale" ? "timeout" : null,
    },
    suggestions: null,
    pick_card: {
      hero_id: 54,
      name: "Lifestealer",
      state: "live",
      games: 48210,
      win_rate_tenths: 505,
      form_tenths: 5,
      durations: {
        state: durationState,
        fetched_at_ms: DURATION_FETCH_MS,
        cells: [
          { bin: "short", games: 300, win_rate_tenths: 540 },
          { bin: "mid", games: 400, win_rate_tenths: 510 },
          { bin: "long", games: 500, win_rate_tenths: 470 },
        ],
      },
    },
  };
}

test("rendered overlay is passive and includes provider footer provenance", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const view = parseDraftColumnView(suggestionPayload());
  const markup = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
  assert.doesNotMatch(markup, /<(?:button|input|select|a)(?:\s|>)/i);
  assert.doesNotMatch(markup, /\btabindex\s*=/i);
  assert.match(markup, /ที่มา: OpenDota · ไม่ระบุช่วง/);
  assert.ok(markup.includes(formatDraftTime(PUBLIC_FETCH_MS)));
  assert.match(markup, /กลุ่มตำแหน่งเป็นค่าประมาณ/);
});

test("rendered pick card shows each duration cell's own time and sample for live and stale data", async () => {
  const DraftColumnView = await loadDraftColumnView();
  for (const durationState of ["live", "stale"] as const) {
    const view = parseDraftColumnView(pickPayload(durationState));
    const markup = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.ok(markup.includes(formatDraftTime(PUBLIC_FETCH_MS)));
    assert.ok(markup.includes(formatDraftTime(DURATION_FETCH_MS)));
    assert.match(markup, /300 เกม/);
    assert.match(markup, /400 เกม/);
    assert.match(markup, /500 เกม/);
    if (durationState === "stale") assert.match(markup, /ข้อมูลเก่า/);
  }
});

test("invalid host payload renders the exact deck error while overlay stays empty", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const invalid = parseDraftColumnView({ ...suggestionPayload(), source: { ...suggestionPayload().source, provider: 42 } });
  assert.equal(renderToStaticMarkup(React.createElement(DraftColumnView, { view: invalid, surface: "overlay" })), "");
  const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view: invalid, surface: "deck" }));
  assert.match(deck, /ข้อมูลจากโฮสต์ไม่ถูกต้อง/);
});

test("rendered data-state and pick-card labels match every approved state exactly", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const stateLabel = (markup: string) => {
    const match = markup.match(/<span class="[^"]*draft-state-flag[^"]*"[^>]*>([\s\S]*?)<\/span>/);
    assert.ok(match, "render includes a state label");
    return match[1]!.replace(/<[^>]+>/g, "");
  };
  const dataStates: Array<[string, string]> = [
    ["consent_off", "คำแนะนำฮีโร่ปิดอยู่ · เปิดได้ที่หน้าต่าง Kohbor"],
    ["loading", "กำลังขอสถิติจาก OpenDota…"],
    ["live", "สด"],
    ["stale", "ข้อมูลเก่า"],
    ["fetch_failed", "ดึงข้อมูลไม่ได้"],
    ["rate_limited", "ถึงโควตาของ OpenDota แล้ว"],
    ["provider_error", "ผู้ให้บริการสถิติตอบกลับผิดปกติ"],
    ["bracket_empty", "OpenDota ไม่มีสถิติของแรงก์ Immortal ตอนนี้"],
  ];
  for (const [data_state, expected] of dataStates) {
    const base = suggestionPayload();
    const view = parseDraftColumnView({
      ...base,
      data_state,
      context: { position: 1, bracket: 8 },
      source: {
        ...base.source,
        fetched_at_ms: data_state === "stale" ? PUBLIC_FETCH_MS : base.source.fetched_at_ms,
        failed_at_ms: data_state === "stale" ? DURATION_FETCH_MS : null,
        failure: data_state === "stale" ? "timeout" : null,
      },
    });
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    assert.equal(stateLabel(deck), expected, `rendered deck data_state=${data_state}`);
    const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.ok(overlay.includes(expected), `rendered overlay data_state=${data_state}`);
  }

  const pickStates: Array<["live" | "stale" | "loading" | "unknown_hero" | "not_enough_matches" | "fetch_failed" | "rate_limited" | "provider_error" | "bracket_empty", string]> = [
    ["live", "สด"],
    ["stale", "ข้อมูลเก่า"],
    ["loading", "กำลังขอสถิติจาก OpenDota…"],
    ["unknown_hero", "ยังไม่มีสถิติของฮีโร่ตัวนี้"],
    ["not_enough_matches", "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม"],
    ["fetch_failed", "ดึงข้อมูลไม่ได้"],
    ["rate_limited", "ถึงโควตาของ OpenDota แล้ว"],
    ["provider_error", "ผู้ให้บริการสถิติตอบกลับผิดปกติ"],
    ["bracket_empty", "OpenDota ไม่มีสถิติของแรงก์ที่เลือกตอนนี้"],
  ];
  for (const [state, expected] of pickStates) {
    const base = pickPayload(state === "stale" ? "stale" : "live");
    const pick_card = {
      ...base.pick_card!,
      state,
      games: state === "not_enough_matches" ? 999 : base.pick_card!.games,
    };
    const view = parseDraftColumnView({ ...base, pick_card });
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    assert.equal(stateLabel(deck), expected, `rendered deck pick_card.state=${state}`);
    const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.ok(overlay.includes(expected), `rendered overlay pick_card.state=${state}`);
  }
});

test("the shared deck and overlay view render each host mode", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const deckPanel = readFileSync(new URL("../src/draft/DraftDeckPanel.tsx", import.meta.url), "utf8");
  const overlayStage = readFileSync(new URL("../src/draft/DraftStage.tsx", import.meta.url), "utf8");
  assert.match(deckPanel, /<DraftColumnView view=\{view\} surface="deck"/);
  assert.match(overlayStage, /<DraftColumnView view=\{view\} surface="overlay"/);
  const base = suggestionPayload();
  const cases = [
    { mode: "hidden", overlay: { visible: false, hidden_reason: "unsupported_output" }, expected: "แถบในเกมรองรับ 1920×1080 ที่ 100% เท่านั้นในตอนนี้" },
    { mode: "consent_chip", data_state: "consent_off", expected: "คำแนะนำฮีโร่ปิดอยู่ · เปิดได้ที่หน้าต่าง Kohbor" },
    { mode: "suggestions", expected: "ฮีโร่เมต้า" },
    { mode: "pick_card", ...pickPayload("live"), expected: "ตัวเลือกของคุณ", overlayExpected: "การ์ดฮีโร่" },
  ] as const;
  for (const item of cases) {
    let fixture: Record<string, unknown>;
    if (item.mode === "pick_card") fixture = item;
    else fixture = { ...base, ...item, suggestions: item.mode === "suggestions" ? base.suggestions : null, pick_card: null };
    const view = parseDraftColumnView(fixture);
    assert.equal(view.valid, true, `${item.mode} fixture must satisfy the host contract`);
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.ok(deck.includes(item.expected), `deck renders ${item.mode}`);
    if (item.mode === "hidden") assert.equal(overlay, "", "hidden overlay draws nothing");
    else assert.ok(overlay.includes("overlayExpected" in item ? item.overlayExpected : item.expected), `overlay renders ${item.mode}`);
  }
});

test("deck says why the column is empty when statistics consent is off", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const base = suggestionPayload();
  const consentText = "ยังไม่ได้เปิดการยินยอมสถิติ";
  for (const overlay of [
    { visible: false, hidden_reason: null },
    { visible: false, hidden_reason: "no_draft" },
  ]) {
    const view = parseDraftColumnView({
      ...base, mode: "hidden", data_state: "consent_off", overlay, suggestions: null, pick_card: null,
    });
    assert.equal(view.valid, true);
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    assert.ok(deck.includes(consentText), `deck names consent for ${overlay.hidden_reason}`);
    assert.ok(deck.includes("ดึงสถิติฮีโร่จากอินเทอร์เน็ต"), "deck names the switch");
    assert.equal(renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" })), "");
  }
  const consentOn = parseDraftColumnView({
    ...base, mode: "hidden", data_state: "loading", overlay: { visible: false, hidden_reason: null },
    suggestions: null, pick_card: null,
  });
  const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view: consentOn, surface: "deck" }));
  assert.ok(!deck.includes(consentText), "no consent text once consent is on");
});

test("Option A blocks have the fixed heights the host sizes the window from", async () => {
  const css = readFileSync(new URL("../src/draft/draft-overlay.css", import.meta.url), "utf8");
  const { OVERLAY_LAYOUT: L } = await import("../src/draft/draftColumnModel.ts");
  const height = (selector: string) => {
    const match = css.match(new RegExp(String.raw`(?:^|\n)(?:[^{}\n]*,\s*)?${selector}\s*(?:,[^{]*)?\{[^}]*?\bheight:\s*(\d+)px`, "s"));
    assert.ok(match, `CSS gives ${selector} a fixed height`);
    return Number(match[1]);
  };
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-brand`), L.brand);
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-head`), L.head);
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-head\.is-enemy`), L.counterHead);
  assert.equal(height(String.raw`\.draft-ov \.draft-row`), L.metaRow);
  assert.equal(height(String.raw`\.draft-ov \.draft-counter-row`), L.counterRow);
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-message`), L.message);
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-pick`), L.pickBody);
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-durations`), L.durationBody);
  assert.equal(height(String.raw`\.draft-ov \.draft-ov-footer`), L.footer);
  assert.equal(height(String.raw`\.draft-ov \.draft-consent-chip\.is-overlay`), L.chip);
  assert.match(css, /\.draft-ov\s*\{[^}]*gap:\s*6px/s);
  assert.match(css, /\.draft-ov \.draft-ov-card\s*\{[^}]*border:\s*1px solid/s);
  assert.match(css, /\.draft-ov\s*\{[^}]*width:\s*180px/s);
  // Inner parts add up to their fixed block.
  assert.equal(30 + 3 * 34 + 34 + 8 + 4, L.pickBody, "hero line, three stats, two-line detail, padding");
  assert.equal(3 * 44 + 8, L.durationBody);
  assert.equal(5 * 17 + 14 + 2, L.footer, "five 17 px lines, padding and border");
  assert.equal(L.metaRow, 70, "card v3 (Variant B) rows are 70 px");
  assert.equal(L.counterRow, L.metaRow, "counter cards share the meta card layout");
});

test("Option A overlay text is never smaller than 12 px", () => {
  const css = readFileSync(new URL("../src/draft/draft-overlay.css", import.meta.url), "utf8");
  const sizes = [...css.matchAll(/font-size:\s*([\d.]+)px/g)].map((match) => Number(match[1]));
  assert.ok(sizes.length > 10);
  assert.ok(sizes.every((size) => size >= 12), `overlay font sizes: ${sizes.join(", ")}`);
  assert.match(css, /\.draft-ov \.draft-ov-head h2\s*\{[^}]*font-size:\s*15px/s, "section headings are larger than body text");
  assert.match(css, /\.draft-ov \.draft-row-name strong\s*\{[^}]*font-size:\s*15px/s);
  assert.match(css, /\.draft-ov \.draft-ov-footer span\s*\{[^}]*font-size:\s*12px/s, "the source footer stays readable");
});

test("each Option A state renders its sections at 180 px and fits above y 860", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const model = await import("../src/draft/draftColumnModel.ts");
  const L = model.OVERLAY_LAYOUT;
  const base = suggestionPayload();
  const metaRows = Array.from({ length: 10 }, (_, index) => ({
    hero_id: 200 + index, name: `Meta ${index + 1}`, rank: index + 1, games: 90000 - index * 1000,
    win_rate_tenths: 505, low_data: false, taken: false,
  }));
  const lowRows = metaRows.map((row, index) => ({ ...row, games: index < 3 ? 1500 : 700, rank: index < 3 ? index + 1 : null, low_data: index >= 3 }));
  const failed = { ...base.source, fetched_at_ms: null, failed_at_ms: DURATION_FETCH_MS, failure: "network" };
  const states: Array<[string, Record<string, unknown>, number, string[]]> = [
    ["meta", { ...base, suggestions: { ...base.suggestions, rows: metaRows, ranked_count: 10 } }, 685, ["ฮีโร่เมต้า"]],
    ["counters", counterPayload("ready", PL_COUNTERS), 693, ["แก้ทาง Phantom Lancer", "ฮีโร่เมต้า"]],
    ["pick", pickPayload("live"), 571, ["การ์ดฮีโร่", "ตามความยาวเกม"]],
    ["low-data", { ...base, suggestions: { ...base.suggestions, rows: lowRows, ranked_count: 3 } }, 685, ["ฮีโร่เมต้า", "ข้อมูลน้อย"]],
    ["error", { ...base, data_state: "fetch_failed", source: failed }, 283, ["ฮีโร่เมต้า", "ดึงข้อมูลไม่ได้"]],
    ["error with counters", { ...counterPayload("ready", PL_COUNTERS), data_state: "fetch_failed", source: failed }, 571, ["แก้ทาง Phantom Lancer", "ดึงข้อมูลไม่ได้"]],
    ["consent chip", { ...base, mode: "consent_chip", data_state: "consent_off", suggestions: null }, 56, ["คำแนะนำฮีโร่ปิดอยู่"]],
  ];
  for (const [name, payload, expectedHeight, texts] of states) {
    const view = model.parseDraftColumnView(payload);
    assert.equal(view.valid, true, `${name} is a valid host view`);
    const height = model.overlayColumnHeight(view);
    assert.equal(height, expectedHeight, `${name} height`);
    assert.ok(L.top + height <= L.maxBottom, `${name} ends at y ${L.top + height}`);
    assert.deepEqual(model.draftStagePlacement(180, height), { x: 0, y: 0, width: 180, height, scale: 1 }, `${name} placement`);
    const markup = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.match(markup, /class="draft-column-surface draft-column-surface-overlay draft-ov"/);
    for (const text of texts) assert.ok(markup.includes(text), `${name} shows ${text}`);
    assert.doesNotMatch(markup, /style="[^"]*font-size/, `${name} has no inline font size`);
    assert.doesNotMatch(markup, /<(?:button|input|select|a)(?:\s|>)/i, `${name} has no controls`);
    assert.doesNotMatch(markup.replace(/\d+(?:\.\d)?%/g, ""), /%/, `${name} shows no percentage except historical win rates`);
  }
});

test("overlay follows the untaken-first row rule while deck retains every host row", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const base = suggestionPayload();
  const baseRow = base.suggestions!.rows[0]!;
  const cases = [
    {
      name: "3 taken + 8 untaken",
      rows: Array.from({ length: 11 }, (_, index) => ({
        ...baseRow, hero_id: index + 1, name: index < 3 ? `Taken ${index + 1}` : `Untaken ${index - 2}`,
        rank: index + 1, taken: index < 3,
      })),
      ranked_count: 8,
      expected: ["Untaken 1", "Untaken 2", "Untaken 3", "Untaken 4", "Untaken 5", "Untaken 6", "Untaken 7"],
      expectedTaken: 0,
    },
    {
      name: "2 taken ranked above + 5 untaken",
      rows: [
        { ...baseRow, hero_id: 1, name: "Taken 1", rank: 1, taken: true },
        { ...baseRow, hero_id: 2, name: "Taken 2", rank: 2, taken: true },
        ...Array.from({ length: 5 }, (_, index) => ({
          ...baseRow, hero_id: index + 3, name: `Untaken ${index + 1}`, rank: index + 3, taken: false,
        })),
      ],
      ranked_count: 7,
      expected: ["Taken 1", "Taken 2", "Untaken 1", "Untaken 2", "Untaken 3", "Untaken 4", "Untaken 5"],
      expectedTaken: 2,
    },
    {
      name: "0 taken + 12 untaken",
      rows: Array.from({ length: 12 }, (_, index) => ({
        ...baseRow, hero_id: index + 1, name: `Pickable ${index + 1}`, rank: index + 1, taken: false,
      })),
      ranked_count: 12,
      expected: ["Pickable 1", "Pickable 2", "Pickable 3", "Pickable 4", "Pickable 5", "Pickable 6", "Pickable 7"],
      expectedTaken: 0,
    },
  ];
  const rowNames = (markup: string) => [...markup.matchAll(/<li class="draft-row[^>]*>[\s\S]*?<span class="draft-row-name"><strong>(.*?)<\/strong>/g)]
    .map((match) => match[1]);
  for (const item of cases) {
    const view = parseDraftColumnView({
      ...base,
      suggestions: { ...base.suggestions, rows: item.rows, ranked_count: item.ranked_count },
    });
    assert.equal(view.valid, true, `${item.name} is a valid host view`);
    const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    assert.deepEqual(rowNames(overlay), item.expected, `${item.name}: overlay rows`);
    assert.equal([...overlay.matchAll(/<li class="draft-row is-taken/g)].length, item.expectedTaken, `${item.name}: struck rows`);
    assert.deepEqual(rowNames(deck), item.rows.map((row) => row.name), `${item.name}: deck rows`);
  }
});

test("synthetic badge appears only for mock fixtures, not a real host view", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const stage = readFileSync(new URL("../src/draft/DraftStage.tsx", import.meta.url), "utf8");
  const panel = readFileSync(new URL("../src/draft/DraftDeckPanel.tsx", import.meta.url), "utf8");
  for (const source of [stage, panel]) {
    assert.match(source, /import\.meta\.env\.DEV && new URLSearchParams\(window\.location\.search\)\.get\("mock"\) === "1"/);
    assert.match(source, /synthetic=\{synthetic\}/);
  }
  const view = parseDraftColumnView(suggestionPayload());
  const real = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay", synthetic: false }));
  const mock = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay", synthetic: true }));
  assert.doesNotMatch(real, /ตัวอย่างสังเคราะห์/);
  assert.match(mock, /ตัวอย่างสังเคราะห์/);
});

test("rendering an unknown game count says so and withholds rank and rate", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const parsed = parseDraftColumnView(suggestionPayload());
  const sourceRow = parsed.suggestions!.rows[0]!;
  const view = {
    ...parsed,
    suggestions: {
      ...parsed.suggestions!,
      rows: [{ ...sourceRow, rank: 1, games: null, win_rate_tenths: 540, low_data: false }],
    },
  };
  const markup = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
  const row = markup.match(/<li class="draft-row[\s\S]*?<\/li>/)?.[0];
  assert.ok(row);
  assert.match(row, /ไม่ทราบจำนวนเกม/);
  assert.doesNotMatch(row, /ข้อมูลน้อย · ต่ำกว่า 1,000 เกม/);
  assert.doesNotMatch(row, /draft-row-rank|draft-row-rate/);
});

function counterPayload(state: string, rows: Array<{ hero_id: number; name: string; against: string[]; reason: string }>) {
  const base = suggestionPayload();
  const metaRows = Array.from({ length: 8 }, (_, index) => ({
    hero_id: 100 + index, name: `Meta ${index + 1}`, rank: index + 1, games: 90000 - index * 1000,
    win_rate_tenths: 500, low_data: false, taken: false,
  }));
  return {
    ...base,
    suggestions: {
      ...base.suggestions,
      rows: metaRows,
      ranked_count: 8,
      draft_bar: { available: true, identified: 1, slots: 10 },
      counters: {
        state, source: "kohbor_reviewed_table", enemies: state === "ready" ? ["Phantom Lancer"] : [],
        enemies_identified: state === "ready" ? 1 : 0, position_filtered: true, rows,
      },
    },
  };
}

const PL_COUNTERS = [
  { hero_id: 7, name: "Earthshaker", against: ["Phantom Lancer"], reason: "Echo Slam" },
  { hero_id: 18, name: "Sven", against: ["Phantom Lancer"], reason: "Great Cleave" },
  { hero_id: 106, name: "Ember Spirit", against: ["Phantom Lancer"], reason: "Sleight of Fist" },
  { hero_id: 94, name: "Medusa", against: ["Phantom Lancer"], reason: "Split Shot" },
];

test("overlay draws at most three counter rows on top and gives up the same meta rows", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const view = parseDraftColumnView(counterPayload("ready", PL_COUNTERS));
  const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
  assert.match(overlay, /แก้ทาง Phantom Lancer/);
  assert.equal((overlay.match(/class="draft-counter-row[" ]/g) ?? []).length, 3);
  assert.doesNotMatch(overlay, /Medusa/);
  assert.equal((overlay.match(/class="draft-row[" ]/g) ?? []).length, 3, "three counter cards leave three meta cards (6 - k)");
  assert.ok(overlay.indexOf("draft-counter-block") < overlay.indexOf("draft-row-list"), "counters sit above the meta list");
  assert.match(overlay, /ไม่ใช่สถิติ/);
  const counterText = overlay.slice(overlay.indexOf("draft-counter-block"), overlay.indexOf("draft-row-list"));
  assert.doesNotMatch(counterText, /%/, "counters never show a percentage");
  assert.doesNotMatch(overlay, /<(?:button|input|select|a)(?:\s|>)/i);

  const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
  assert.equal((deck.match(/class="draft-counter-row[" ]/g) ?? []).length, 4, "the deck keeps every host counter row");
  assert.match(deck, /Echo Slam/);
});

test("without a ready counter block the overlay shows no counter section and keeps eight meta rows", async () => {
  const DraftColumnView = await loadDraftColumnView();
  for (const state of ["build_disabled", "draft_bar_unavailable", "sides_unknown", "no_enemy"]) {
    const view = parseDraftColumnView(counterPayload(state, []));
    assert.equal(view.suggestions?.counters?.state, state);
    const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.doesNotMatch(overlay, /draft-counter-block/, state);
    assert.equal((overlay.match(/class="draft-row[" ]/g) ?? []).length, 7, state);
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    assert.match(deck, /draft-counter-status/, state);
  }
  const sides = renderToStaticMarkup(React.createElement(DraftColumnView, {
    view: parseDraftColumnView(counterPayload("sides_unknown", [])), surface: "deck",
  }));
  assert.match(sides, /ยังไม่รู้ว่าฮีโร่ไหนอยู่ฝั่งตรงข้าม/);
});

test("card v3 shows the portrait slot, win-rate tone, delta and the selected lane with its caveat", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const model = await import("../src/draft/draftColumnModel.ts");
  const base = suggestionPayload();
  const lane = (share: number, rate: number | null) => ({ position: 3, lane: "off", shared_with: 4, games: 4200, share_tenths: share, win_rate_tenths: rate });
  const rows = [
    { hero_id: 33, name: "Enigma", rank: 1, games: 38511, win_rate_tenths: 527, low_data: false, taken: false, hero_key: "npc_dota_hero_enigma", lane: lane(712, 519) },
    { hero_id: 29, name: "Tidehunter", rank: 2, games: 31066, win_rate_tenths: 488, low_data: false, taken: false, hero_key: "npc_dota_hero_tidehunter", lane: lane(820, null) },
    { hero_id: 77, name: "Lycan", rank: 3, games: 22904, win_rate_tenths: 505, low_data: false, taken: false, hero_key: "bad key", lane: { lane: "jungle" } },
  ];
  const view = model.parseDraftColumnView({ ...base, context: { position: 3, bracket: 3 }, suggestions: { ...base.suggestions, rows, ranked_count: 3 } });
  assert.equal(view.valid, true);
  assert.equal(view.suggestions!.rows[0]!.lane!.share_tenths, 712);
  assert.equal(view.suggestions!.rows[2]!.hero_key, null, "an invalid key becomes null, the row stays");
  assert.equal(view.suggestions!.rows[2]!.lane, null, "an invalid lane becomes null, the row stays");
  const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
  assert.match(overlay, /data-hero-key="npc_dota_hero_enigma"/);
  assert.match(overlay, /draft-ov-edge is-up/);
  assert.match(overlay, /draft-ov-edge is-down/);
  assert.match(overlay, /draft-ov-edge is-flat/);
  assert.match(overlay, /\+2\.7/);
  assert.match(overlay, /\u22121\.2/);
  assert.match(overlay, /Off 71% · ชนะ <span class="is-up">51\.9%<\/span>/);
  assert.match(overlay, /Off 82% · ชนะ <span class="is-flat">—<\/span>/, "lane win rate withheld below 100 games");
  assert.match(overlay, /Pos 3\+4 = เลน Off ทุกแรงก์/);
  assert.match(overlay, /Pos 3 · Offlane/);
  const css = readFileSync(new URL("../src/draft/draft-overlay.css", import.meta.url), "utf8");
  assert.match(css, /--ov-ground:\s*#141414/, "card backgrounds are solid");
  assert.match(css, /\.draft-ov-edge\.is-up\s*\{[^}]*var\(--ov-ally\)/s);
  assert.match(css, /\.draft-ov-edge\.is-down\s*\{[^}]*var\(--ov-critical\)/s);
  const source = readFileSync(new URL("../src/draft/OverlayPortrait.tsx", import.meta.url), "utf8");
  assert.match(source, /useHeroPortrait\(heroKey\)/, "portraits come from the existing hero_portrait path");
  assert.doesNotMatch(source, /https?:|\.png"|\.webp"/, "no bundled or remote art");
});
