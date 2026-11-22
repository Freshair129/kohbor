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

async function loadDraftColumnView() {
  const viewPath = fileURLToPath(new URL("../src/draft/DraftColumnView.tsx", import.meta.url));
  const viewSource = readFileSync(viewPath, "utf8").replace(/^import "\.\/draft-column\.css";\s*$/m, "");
  const transformed = await transformWithOxc(viewSource, viewPath, {
    lang: "tsx", sourceType: "module", jsx: { runtime: "automatic", development: false },
  });
  const model = await import("../src/draft/draftColumnModel.ts");
  const commonJsSource = transformed.code
    .replace(/^import\s+\{([\s\S]*?)\}\s+from\s+["']([^"']+)["'];?\s*/gm, (_match, bindings: string, specifier: string) => {
      const properties = bindings.split(",").map((binding) => {
        const [imported, local] = binding.trim().split(/\s+as\s+/);
        return local ? `${imported}: ${local}` : imported;
      }).join(", ");
      return `const { ${properties} } = require(${JSON.stringify(specifier)});\n`;
    })
    .replace(/\bexport\s+function\s+DraftColumnView\b/, "function DraftColumnView")
    + "\nmodule.exports = { DraftColumnView };";
  const view = evaluateCommonJs(commonJsSource, viewPath, (specifier: string) => {
    if (specifier === "./draftColumnModel") return model;
    return requireFromApp(specifier);
  });
  return view.DraftColumnView as React.ComponentType<Record<string, unknown>>;
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
    for (const surface of ["overlay", "deck"] as const) {
      const markup = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface }));
      assert.equal(stateLabel(markup), expected, `rendered ${surface} data_state=${data_state}`);
    }
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
    for (const surface of ["overlay", "deck"] as const) {
      const markup = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface }));
      assert.equal(stateLabel(markup), expected, `rendered ${surface} pick_card.state=${state}`);
    }
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
    { mode: "pick_card", ...pickPayload("live"), expected: "ตัวเลือกของคุณ" },
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
    else assert.ok(overlay.includes(item.expected), `overlay renders ${item.mode}`);
  }
});

test("overlay suggestion rows fit the 180 by 488 budget without clipping", async () => {
  const css = readFileSync(new URL("../src/draft/draft-column.css", import.meta.url), "utf8");
  const dimensions = [
    "header-height", "state-height", "subtitle-height", "context-height", "row-heading-height", "row-height",
    "notes-height", "footer-height", "stale-height", "synthetic-height", "gap", "padding-block", "border",
  ];
  const declared: Record<string, number> = {};
  for (const dimension of dimensions) {
    const match = css.match(new RegExp(`--draft-overlay-${dimension}:\\s*(\\d+)px`));
    assert.ok(match, `CSS declares --draft-overlay-${dimension}`);
    declared[dimension] = Number(match[1]);
  }
  const appliedHeightRules: Array<[string, RegExp]> = [
    ["header-height", /\.draft-column-surface-overlay\s+\.draft-suggestions-card\s+\.draft-column-header\s*\{[^}]*height:\s*var\(--draft-overlay-header-height\)/s],
    ["state-height", /\.draft-column-surface-overlay\s+\.draft-overlay-state\s*\{[^}]*min-height:\s*var\(--draft-overlay-state-height\)/s],
    ["subtitle-height", /\.draft-column-surface-overlay\s+\.draft-column-subtitle\s*\{[^}]*height:\s*var\(--draft-overlay-subtitle-height\)/s],
    ["context-height", /\.draft-column-surface-overlay\s+\.draft-context-block\s*\{[^}]*height:\s*var\(--draft-overlay-context-height\)/s],
    ["row-heading-height", /\.draft-column-surface-overlay\s+\.draft-row-heading\s*\{[^}]*height:\s*var\(--draft-overlay-row-heading-height\)/s],
    ["row-height", /\.draft-column-surface-overlay\s+\.draft-row\s*\{[^}]*height:\s*var\(--draft-overlay-row-height\)/s],
    ["notes-height", /\.draft-column-surface-overlay\s+\.draft-column-notes\s*\{[^}]*height:\s*var\(--draft-overlay-notes-height\)/s],
    ["footer-height", /\.draft-column-surface-overlay\s+\.draft-column-footer\s*\{[^}]*height:\s*var\(--draft-overlay-footer-height\)/s],
    ["stale-height", /\.draft-column-surface-overlay\s+\.draft-stale-times\s*\{[^}]*height:\s*var\(--draft-overlay-stale-height\)/s],
    ["synthetic-height", /\.draft-column-surface-overlay\s+\.draft-synthetic-flag\s*\{[^}]*height:\s*var\(--draft-overlay-synthetic-height\)/s],
  ];
  for (const [name, rule] of appliedHeightRules) assert.match(css, rule, `${name} is applied to the rendered part`);
  const worstCase = declared["header-height"]! + declared["subtitle-height"]! + declared["context-height"]! +
    declared["state-height"]! + declared["row-heading-height"]! + 8 * declared["row-height"]! + declared["notes-height"]! +
    declared["footer-height"]! + declared["stale-height"]! + declared["synthetic-height"]! +
    9 * declared.gap! + 2 * declared["padding-block"]! + 2 * declared.border!;
  assert.ok(worstCase <= 488, `8-row overlay worst case is ${worstCase}px, within 488px`);
  assert.match(css, /\.draft-column-card\s*\{[^}]*width:\s*180px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-suggestions-card\s*\{[^}]*height:\s*488px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-suggestions-card\s*\{[^}]*gap:\s*var\(--draft-overlay-gap\)/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-suggestions-card\s*\{[^}]*padding:\s*var\(--draft-overlay-padding-block\)/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-row\s*\{[^}]*height:\s*var\(--draft-overlay-row-height\)/s);
  assert.doesNotMatch(css, /\.draft-(?:column-card|column-header|state-flag|row|column-notes|column-footer)[^{]*\{[^}]*overflow:\s*(?:hidden|clip)/s);

  const overlayRules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].filter((match) => match[1]!.includes(".draft-column-surface-overlay"));
  const overlayFontSizes = overlayRules.flatMap((match) => [...match[2]!.matchAll(/font-size:\s*([\d.]+)px/g)].map((font) => Number(font[1])));
  assert.ok(overlayFontSizes.length > 0);
  assert.ok(overlayFontSizes.every((size) => size >= 8.5), `overlay font sizes are ${overlayFontSizes.join(", ")}`);
  assert.match(css, /\.draft-column-surface-overlay,\s*\.draft-column-surface-overlay \*\s*\{[^}]*font-size:\s*8\.5px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-column-header > strong\s*\{[^}]*font-size:\s*12px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-row-name strong\s*\{[^}]*font-size:\s*11px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-row-rank\s*\{[^}]*font-size:\s*10px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-row-games\s*\{[^}]*font-size:\s*8\.5px/s);
  assert.match(css, /\.draft-column-surface-overlay\s+\.draft-row-rate\s*\{[^}]*font-size:\s*11px/s);
});

test("rendered 1920x1080 scale-1 overlay applies no text style below 8.5px", async () => {
  const DraftColumnView = await loadDraftColumnView();
  const css = readFileSync(new URL("../src/draft/draft-column.css", import.meta.url), "utf8");
  const placement = draftColumnPlacement(1920, 1080);
  assert.deepEqual(placement, { x: 1728, y: 132, width: 180, height: 488, scale: 1 });
  const rendered = renderToStaticMarkup(React.createElement("main", {
    className: "draft-stage",
    lang: "th",
    "aria-live": "off",
    style: {
      "--draft-overlay-scale": placement.scale,
      "--draft-overlay-x": `${placement.x}px`,
      "--draft-overlay-y": `${placement.y}px`,
    } as React.CSSProperties,
  }, React.createElement(DraftColumnView, {
    view: parseDraftColumnView(suggestionPayload()),
    surface: "overlay",
  })));
  assert.match(rendered, /class="draft-column-surface draft-column-surface-overlay"/);
  assert.match(rendered, /--draft-overlay-scale:1/);
  const overlayRules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter((match) => match[1]!.includes(".draft-column-surface-overlay"));
  const appliedTextSizes = overlayRules.flatMap((match) => [...match[2]!.matchAll(/font-size:\s*([\d.]+)px/g)]
    .map((font) => Number(font[1])));
  assert.ok(appliedTextSizes.length > 0);
  assert.ok(appliedTextSizes.every((size) => size >= 8.5), `rendered overlay text sizes: ${appliedTextSizes.join(", ")}`);
  assert.match(css, /\.draft-column-surface-overlay,\s*\.draft-column-surface-overlay \*\s*\{[^}]*font-size:\s*8\.5px/s);
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
      expected: ["Untaken 1", "Untaken 2", "Untaken 3", "Untaken 4", "Untaken 5", "Untaken 6", "Untaken 7", "Untaken 8"],
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
      expected: ["Pickable 1", "Pickable 2", "Pickable 3", "Pickable 4", "Pickable 5", "Pickable 6", "Pickable 7", "Pickable 8"],
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
  assert.equal((overlay.match(/class="draft-counter-row"/g) ?? []).length, 3);
  assert.doesNotMatch(overlay, /Medusa/);
  assert.equal((overlay.match(/class="draft-row /g) ?? []).length, 5, "8 rows minus 3 counter rows");
  assert.ok(overlay.indexOf("draft-counter-block") < overlay.indexOf("draft-row-list"), "counters sit above the meta list");
  assert.match(overlay, /ไม่ใช่สถิติ/);
  const counterText = overlay.slice(overlay.indexOf("draft-counter-block"), overlay.indexOf("draft-column-subtitle"));
  assert.doesNotMatch(counterText, /%/, "counters never show a percentage");
  assert.doesNotMatch(overlay, /<(?:button|input|select|a)(?:\s|>)/i);

  const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
  assert.equal((deck.match(/class="draft-counter-row"/g) ?? []).length, 4, "the deck keeps every host counter row");
  assert.match(deck, /Echo Slam/);
});

test("without a ready counter block the overlay shows no counter section and keeps eight meta rows", async () => {
  const DraftColumnView = await loadDraftColumnView();
  for (const state of ["build_disabled", "draft_bar_unavailable", "sides_unknown", "no_enemy"]) {
    const view = parseDraftColumnView(counterPayload(state, []));
    assert.equal(view.suggestions?.counters?.state, state);
    const overlay = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "overlay" }));
    assert.doesNotMatch(overlay, /draft-counter-block/, state);
    assert.equal((overlay.match(/class="draft-row /g) ?? []).length, 8, state);
    const deck = renderToStaticMarkup(React.createElement(DraftColumnView, { view, surface: "deck" }));
    assert.match(deck, /draft-counter-status/, state);
  }
  const sides = renderToStaticMarkup(React.createElement(DraftColumnView, {
    view: parseDraftColumnView(counterPayload("sides_unknown", [])), surface: "deck",
  }));
  assert.match(sides, /ยังไม่รู้ว่าฮีโร่ไหนอยู่ฝั่งตรงข้าม/);
});

test("the overlay counter block fits inside the three meta rows it replaces", () => {
  const css = readFileSync(new URL("../src/draft/draft-column.css", import.meta.url), "utf8");
  const rowHeight = Number(css.match(/--draft-overlay-row-height:\s*(\d+)px/)![1]);
  const block = css.match(/\.draft-column-surface-overlay\s+\.draft-counter-block\s*\{([^}]*)\}/s)![1]!;
  const maxHeight = Number(block.match(/max-height:\s*(\d+)px/)![1]);
  assert.ok(maxHeight <= 3 * rowHeight, `counter block ${maxHeight}px vs ${3 * rowHeight}px`);
  const part = (selector: string) => Number(css.match(new RegExp(String.raw`\.draft-column-surface-overlay\s+\.${selector}\s*\{[^}]*height:\s*(\d+)px`, "s"))![1]);
  const content = part("draft-counter-heading") + 3 * part("draft-counter-row") + part("draft-counter-source");
  assert.ok(content <= maxHeight, `counter content ${content}px within ${maxHeight}px`);
});
