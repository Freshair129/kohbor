import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import test from "node:test";

type Model = typeof import("../src/draft/draftColumnModel.ts");

async function loadModel(): Promise<Model> {
  let loaded: Model | null = null;
  try {
    loaded = await import("../src/draft/draftColumnModel.ts");
  } catch {
    // Turn a missing feature into an intentional assertion failure during the RED phase.
  }
  assert.ok(loaded, "draftColumnModel.ts must export the draft view model");
  return loaded;
}

function readDraftSource(relative: string): string {
  let source: string | null = null;
  try {
    source = readFileSync(new URL(`../src/draft/${relative}`, import.meta.url), "utf8");
  } catch {
    // Keep RED failures as assertions when the feature source has not been written yet.
  }
  assert.ok(source !== null, `draft/${relative} must be present`);
  return source;
}

function payload(overrides: Record<string, unknown> = {}) {
  return {
    schema: 1,
    mode: "suggestions",
    overlay: { visible: true, hidden_reason: null },
    phase: "hero_selection",
    placement_verified: true,
    data_state: "live",
    context: { position: 1, bracket: 3 },
    source: {
      provider: "OpenDota", window: null, fetched_at_ms: 1790000000000, failed_at_ms: null,
      failure: null, http_status: null, retry_after_ms: null, table_requests_24h: 2, table_request_budget: 4,
    },
    suggestions: {
      rows: [{ hero_id: 1, name: "Spectre", rank: 1, games: 44465, win_rate_tenths: 531, low_data: false, taken: false }],
      ranked_count: 1,
      draft_bar: { available: false, identified: 0, slots: 10 },
    },
    pick_card: null,
    ...overrides,
  };
}

test("schema 1 parses and every other schema fails closed", async () => {
  const model = await loadModel();
  const parsed = model.parseDraftColumnView(payload());
  assert.equal(parsed.valid, true);
  assert.equal(parsed.mode, "suggestions");
  assert.equal(model.parseDraftColumnView(payload({ schema: 2 })).mode, "hidden");
  assert.equal(model.parseDraftColumnView(null).mode, "hidden");
  assert.equal(model.parseDraftColumnView(payload({ data_state: "future_state" })).mode, "hidden");
});

test("null values stay unknown across context, source timestamps, and hero rates", async () => {
  const model = await loadModel();
  const view = model.parseDraftColumnView(payload({
    context: { position: null, bracket: null },
    source: { ...payload().source, fetched_at_ms: null, failed_at_ms: null },
    suggestions: {
      rows: [{ hero_id: 1, name: "Spectre", rank: null, games: 44465, win_rate_tenths: null, low_data: false, taken: false }],
      ranked_count: 0, draft_bar: { available: false, identified: 0, slots: 10 },
    },
  }));
  assert.equal(view.valid, true);
  assert.equal(view.source?.fetched_at_ms, null);
  assert.equal(view.suggestions?.rows[0]?.win_rate_tenths, null);
  assert.equal(model.formatWinRateTenths(null), "—");
});

test("integer formatting preserves tenths, signs, and thousands separators", async () => {
  const model = await loadModel();
  assert.equal(model.formatWinRateTenths(531), "53.1%");
  assert.equal(model.formatFormTenths(18), "+1.8");
  assert.equal(model.formatFormTenths(-4), "−0.4");
  assert.equal(model.formatGames(1234567), "1,234,567");
  assert.equal(model.formatGames(null), "—");
});

test("fetch times use Thai short months, 24-hour local time, and Gregorian year only when needed", async () => {
  const model = await loadModel();
  const currentYear = new Date().getFullYear();
  const current = new Date(currentYear, 10, 21, 17, 13);
  const previous = new Date(currentYear - 1, 10, 21, 17, 13);
  assert.equal(model.formatDraftTime(current.getTime()), "21 พ.ย. 17:13");
  assert.equal(model.formatDraftTime(previous.getTime()), `21 พ.ย. ${currentYear - 1} 17:13`);
  assert.doesNotMatch(model.formatDraftTime(previous.getTime()), new RegExp(`\\b${currentYear + 542}\\b|\\b\\d{2}\\/\\d{1,2}\\/\\d{2}\\b`));
});

test("negative and oversized statistic integers invalidate the complete payload", async () => {
  const model = await loadModel();
  const input = payload({
    suggestions: {
      rows: [
        { hero_id: 1, name: "Negative", rank: 1, games: -1, win_rate_tenths: -1, low_data: false, taken: false },
        { hero_id: 2, name: "Oversized", rank: 2, games: 1000000001, win_rate_tenths: 1001, low_data: false, taken: false },
      ], ranked_count: 2, draft_bar: { available: false, identified: 0, slots: 10 },
    },
  });
  const view = model.parseDraftColumnView(input);
  assert.equal(view.valid, false, "a malformed row invalidates the complete host payload");
  assert.equal(view.mode, "hidden");
  assert.equal(model.formatWinRateTenths(-1), "—");
  assert.equal(model.formatWinRateTenths(1001), "—");
});

test("one malformed nested object invalidates the whole section-10 payload", async () => {
  const model = await loadModel();
  const validCard = {
    hero_id: 54, name: "Lifestealer", state: "live", games: 48210, win_rate_tenths: 505, form_tenths: 5,
    durations: {
      state: "live", fetched_at_ms: 1790000000000,
      cells: [
        { bin: "short", games: 300, win_rate_tenths: 540 },
        { bin: "mid", games: 400, win_rate_tenths: 510 },
        { bin: "long", games: 500, win_rate_tenths: 470 },
      ],
    },
  };
  const validSuggestions = payload().suggestions;
  const malformed: Array<[string, unknown]> = [
    ["source", payload({ source: { ...payload().source, provider: 17 } })],
    ["context", payload({ context: { position: "1", bracket: 3 } })],
    ["overlay", payload({ overlay: { visible: 1, hidden_reason: null } })],
    ["suggestions", payload({ suggestions: { rows: [], draft_bar: { available: false, identified: 0, slots: 10 } } })],
    ["row", payload({ suggestions: { ...validSuggestions, rows: [{ hero_id: 1, name: "Spectre", rank: 1, games: 20, win_rate_tenths: 531, low_data: false }] } })],
    ["draft_bar", payload({ suggestions: { ...validSuggestions, draft_bar: { available: false, identified: "0", slots: 10 } } })],
    ["pick_card", payload({ mode: "pick_card", suggestions: null, pick_card: { ...validCard, state: "future" } })],
    ["durations", payload({ mode: "pick_card", suggestions: null, pick_card: { ...validCard, durations: { ...validCard.durations, cells: [] } } })],
    ["cell", payload({ mode: "pick_card", suggestions: null, pick_card: { ...validCard, durations: { ...validCard.durations, cells: [{ ...validCard.durations.cells[0], games: "300" }, ...validCard.durations.cells.slice(1)] } } })],
  ];
  for (const [part, fixture] of malformed) {
    const view = model.parseDraftColumnView(fixture);
    assert.equal(view.valid, false, `${part} must fail closed`);
    assert.equal(view.mode, "hidden", `${part} must not leave a partially rendered model`);
  }
});

test("all host rows stay in the model and more than 16 invalidates the payload", async () => {
  const model = await loadModel();
  const rows = Array.from({ length: 11 }, (_, index) => ({
    hero_id: index + 1,
    name: `Hero ${index + 1}`,
    rank: index < 3 ? null : index - 2,
    games: 2000,
    win_rate_tenths: 510,
    low_data: false,
    taken: index < 3,
  }));
  const parsed = model.parseDraftColumnView(payload({ suggestions: { ...payload().suggestions, rows, ranked_count: 8 } }));
  assert.equal(parsed.valid, true);
  assert.equal(parsed.suggestions?.rows.length, 11, "3 taken plus 8 untaken ranked rows must all render");
  assert.equal(parsed.suggestions?.rows[3]?.name, "Hero 4");

  const capped = model.parseDraftColumnView(payload({ suggestions: { ...payload().suggestions, rows: [...rows, ...rows.slice(0, 6)] } }));
  assert.equal(capped.valid, false, "the renderer rejects a host payload outside the section-10 bound");
  assert.equal(capped.mode, "hidden");
});

test("overlay prioritizes up to eight untaken rows and includes only qualifying taken rows", async () => {
  const model = await loadModel();

  const threeTakenEightUntaken = Array.from({ length: 11 }, (_, index) => ({
    hero_id: index + 1,
    name: `Hero ${index + 1}`,
    rank: index + 1,
    games: 2000,
    win_rate_tenths: 510,
    low_data: false,
    taken: index < 3,
  }));
  assert.deepEqual(
    model.overlaySuggestionRows(threeTakenEightUntaken).map((row) => row.name),
    ["Hero 4", "Hero 5", "Hero 6", "Hero 7", "Hero 8", "Hero 9", "Hero 10", "Hero 11"],
    "eight untaken rows fill the overlay before any taken row",
  );

  const twoTakenFiveUntaken = [
    { ...threeTakenEightUntaken[0]!, name: "Taken 1", rank: 1, taken: true },
    { ...threeTakenEightUntaken[1]!, name: "Taken 2", rank: 2, taken: true },
    ...Array.from({ length: 5 }, (_, index) => ({
      ...threeTakenEightUntaken[index + 3]!, name: `Untaken ${index + 1}`, rank: index + 3, taken: false,
    })),
  ];
  assert.deepEqual(
    model.overlaySuggestionRows(twoTakenFiveUntaken).map((row) => row.name),
    ["Taken 1", "Taken 2", "Untaken 1", "Untaken 2", "Untaken 3", "Untaken 4", "Untaken 5"],
    "qualifying struck rows are included when fewer than eight untaken rows exist",
  );

  const twelveUntaken = Array.from({ length: 12 }, (_, index) => ({
    ...threeTakenEightUntaken[index]!, name: `Pickable ${index + 1}`, rank: index + 1, taken: false,
  }));
  assert.deepEqual(
    model.overlaySuggestionRows(twelveUntaken).map((row) => row.name),
    ["Pickable 1", "Pickable 2", "Pickable 3", "Pickable 4", "Pickable 5", "Pickable 6", "Pickable 7", "Pickable 8"],
    "only the first eight untaken rows are drawn",
  );

  const takenBelowPickable = [
    { ...threeTakenEightUntaken[0]!, name: "Untaken 1", rank: 1, taken: false },
    { ...threeTakenEightUntaken[1]!, name: "Untaken 2", rank: 2, taken: false },
    { ...threeTakenEightUntaken[2]!, name: "Taken below", rank: 3, taken: true },
  ];
  assert.deepEqual(
    model.overlaySuggestionRows(takenBelowPickable).map((row) => row.name),
    ["Untaken 1", "Untaken 2"],
    "taken rows below all drawn untaken rows are omitted",
  );
});

test("required section-10 integers reject missing, null, negative, wrong, or out-of-bound values", async () => {
  const model = await loadModel();
  const baseSuggestions = payload().suggestions;
  const baseRow = baseSuggestions.rows[0];
  const invalid: Array<[string, Record<string, unknown>]> = [
    ["table_requests_24h null", payload({ source: { ...payload().source, table_requests_24h: null } })],
    ["table_requests_24h negative", payload({ source: { ...payload().source, table_requests_24h: -1 } })],
    ["table_request_budget null", payload({ source: { ...payload().source, table_request_budget: null } })],
    ["table_request_budget negative", payload({ source: { ...payload().source, table_request_budget: -1 } })],
    ["table_request_budget mismatch", payload({ source: { ...payload().source, table_request_budget: 3 } })],
    ["hero_id null", payload({ suggestions: { ...baseSuggestions, rows: [{ ...baseRow, hero_id: null }] } })],
    ["hero_id negative", payload({ suggestions: { ...baseSuggestions, rows: [{ ...baseRow, hero_id: -1 }] } })],
    ["games null", payload({ suggestions: { ...baseSuggestions, rows: [{ ...baseRow, games: null }] } })],
    ["games negative", payload({ suggestions: { ...baseSuggestions, rows: [{ ...baseRow, games: -1 }] } })],
    ["ranked_count null", payload({ suggestions: { ...baseSuggestions, ranked_count: null } })],
    ["ranked_count negative", payload({ suggestions: { ...baseSuggestions, ranked_count: -1 } })],
    ["draft_bar.identified null", payload({ suggestions: { ...baseSuggestions, draft_bar: { ...baseSuggestions.draft_bar, identified: null } } })],
    ["draft_bar.identified negative", payload({ suggestions: { ...baseSuggestions, draft_bar: { ...baseSuggestions.draft_bar, identified: -1 } } })],
    ["draft_bar.slots null", payload({ suggestions: { ...baseSuggestions, draft_bar: { ...baseSuggestions.draft_bar, slots: null } } })],
    ["draft_bar.slots negative", payload({ suggestions: { ...baseSuggestions, draft_bar: { ...baseSuggestions.draft_bar, slots: -1 } } })],
    ["draft_bar.slots mismatch", payload({ suggestions: { ...baseSuggestions, draft_bar: { ...baseSuggestions.draft_bar, slots: 9 } } })],
  ];
  for (const [field, fixture] of invalid) {
    assert.equal(model.parseDraftColumnView(fixture).valid, false, `${field} must invalidate the complete payload`);
  }
});

test("schema 1 rejects a provider window or a non-contract retry budget", async () => {
  const model = await loadModel();
  assert.equal(model.parseDraftColumnView(payload({ source: { ...payload().source, window: "7.40" } })).valid, false);
  assert.equal(model.parseDraftColumnView(payload({ source: { ...payload().source, table_request_budget: 3 } })).valid, false);
});

test("win rate bands include 490 in rose and 510 in green", async () => {
  const model = await loadModel();
  assert.equal(model.winRateBand(489), "rose");
  assert.equal(model.winRateBand(490), "rose");
  assert.equal(model.winRateBand(510), "green");
  assert.equal(model.winRateBand(511), "green");
  assert.equal(model.winRateBand(null), "neutral");
});

test("suggestion rows withhold ranks for taken picks and rates for low samples", async () => {
  const model = await loadModel();
  const taken = model.suggestionRowView({ hero_id: 1, name: "Spectre", rank: 1, games: 44465, win_rate_tenths: 531, low_data: false, taken: true });
  assert.equal(taken.rank, null);
  assert.equal(taken.taken, true);
  assert.equal(taken.winRateText, "53.1%");
  const low = model.suggestionRowView({ hero_id: 2, name: "Chen", rank: 8, games: 496, win_rate_tenths: 487, low_data: true, taken: false });
  assert.equal(low.rank, null);
  assert.equal(low.gamesText, "496");
  assert.equal(low.winRateText, "—");
  assert.equal(low.lowDataLabel, "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม");

  const inconsistent = model.suggestionRowView({ hero_id: 3, name: "Low count", rank: 2, games: 999, win_rate_tenths: 531, low_data: false, taken: false });
  assert.equal(inconsistent.rank, null);
  assert.equal(inconsistent.winRateText, "—");
  assert.equal(inconsistent.lowDataLabel, "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม");

  const missingCount = model.suggestionRowView({ hero_id: 4, name: "Unknown count", rank: 1, games: null, win_rate_tenths: 540, low_data: false, taken: false });
  assert.equal(missingCount.rank, null);
  assert.equal(missingCount.gamesText, "—");
  assert.equal(missingCount.winRateText, "—");
  assert.equal(missingCount.lowDataLabel, null);
  assert.equal(missingCount.unknownGamesLabel, "ไม่ทราบจำนวนเกม");
});

test("context labels stay explicit when position and bracket are unset", async () => {
  const model = await loadModel();
  const bothUnset = model.parseDraftColumnView(payload({ context: { position: null, bracket: null } }));
  assert.equal(model.positionLabel(bothUnset.context.position), "ทุกตำแหน่ง");
  assert.equal(model.bracketLabel(bothUnset.context.bracket), "ทุกแรงก์");
  assert.equal(model.unsetContextLabel(bothUnset.context), "ยังไม่ได้ตั้งตำแหน่งและแรงก์");
  assert.equal(model.positionLabel(3), "ตำแหน่ง 3 (ค่าประมาณ)");
  assert.equal(model.bracketLabel(8), "Immortal");
});

test("deck hidden reasons use the approved Thai copy", async () => {
  const model = await loadModel();
  assert.equal(model.draftHiddenReasonText("unsupported_output"), "แถบในเกมรองรับ 1920×1080 ที่ 100% เท่านั้นในตอนนี้");
  assert.equal(model.draftHiddenReasonText("list_build_disabled"), "รอเจ้าของตรวจกลุ่มตำแหน่งก่อนเปิดในเกม");
  assert.equal(model.draftHiddenReasonText("bar_off"), "ปิดแถบดราฟต์ในเกมไว้");
  assert.equal(model.draftHiddenReasonText("no_draft"), "แสดงสถิติในช่วงดราฟต์เท่านั้น");
});

test("settings parser keeps only explicit consent and bounded manual filters", async () => {
  const model = await loadModel();
  assert.deepEqual(model.parseDraftStatsPrefs({ hero_stats_consent: false, draft_bar_enabled: true, position: null, bracket: null }), {
    hero_stats_consent: false, draft_bar_enabled: true, position: null, bracket: null,
  });
  assert.equal(model.parseDraftStatsPrefs({ hero_stats_consent: "yes", draft_bar_enabled: true, position: null, bracket: null }), null);
  assert.equal(model.parseDraftStatsPrefs({ hero_stats_consent: true, draft_bar_enabled: true, position: 6, bracket: null }), null);
  assert.equal(model.parseDraftStatsPrefs({ hero_stats_consent: true, draft_bar_enabled: true, position: null, bracket: 9 }), null);
});

test("each public-table data state has its prescribed Thai copy", async () => {
  const model = await loadModel();
  const expected: Array<[string, string]> = [
    ["consent_off", "คำแนะนำฮีโร่ปิดอยู่ · เปิดได้ที่หน้าต่าง Kohbor"],
    ["loading", "กำลังขอสถิติจาก OpenDota…"],
    ["live", "สด"],
    ["stale", "ข้อมูลเก่า"],
    ["fetch_failed", "ดึงข้อมูลไม่ได้"],
    ["rate_limited", "ถึงโควตาของ OpenDota แล้ว"],
    ["provider_error", "ผู้ให้บริการสถิติตอบกลับผิดปกติ"],
    ["bracket_empty", "OpenDota ไม่มีสถิติของแรงก์ Immortal ตอนนี้"],
  ];
  for (const [state, text] of expected) {
    const view = model.parseDraftColumnView(payload({ data_state: state, context: { position: null, bracket: 8 } }));
    assert.equal(model.dataStateText(view), text, state);
  }
});

test("stale state identifies both data and failure times", async () => {
  const model = await loadModel();
  const view = model.parseDraftColumnView(payload({
    data_state: "stale",
    source: { ...payload().source, fetched_at_ms: 1790000000000, failed_at_ms: 1790003600000, failure: "timeout" },
  }));
  const text = model.staleTimesText(view);
  assert.match(text, /ข้อมูลของ/);
  assert.match(text, /ดึงใหม่ไม่ได้เมื่อ/);
  assert.doesNotMatch(text, /undefined|null/);
});

test("pick-card states remain distinct and do not invent metric values", async () => {
  const model = await loadModel();
  const expected: Array<[Parameters<typeof model.pickCardStateText>[0], string]> = [
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
  for (const [state, text] of expected) assert.equal(model.pickCardStateText(state), text, state);
  assert.equal(model.formatFormTenths(null), "—");
});

test("rate-limit deadline is fixed at failure time plus the reported delay", async () => {
  const model = await loadModel();
  const view = model.parseDraftColumnView(payload({
    data_state: "rate_limited",
    source: { ...payload().source, failed_at_ms: 1790000000000, retry_after_ms: 3600000 },
  }));
  const renderAt = (_now: number) => model.retryDeadlineMs(view);
  assert.equal(renderAt(1790000000000), 1790003600000);
  assert.equal(renderAt(1890000000000), 1790003600000);
  assert.equal(model.dataStateDetailText(view), `ลองใหม่ได้หลัง ${model.formatDraftTime(1790003600000)}`);

  const unknownFailure = model.parseDraftColumnView(payload({ source: { ...payload().source, failed_at_ms: null, retry_after_ms: 3600000 } }));
  const unknownDelay = model.parseDraftColumnView(payload({ source: { ...payload().source, failed_at_ms: 1790000000000, retry_after_ms: null } }));
  assert.equal(model.retryDeadlineMs(unknownFailure), null);
  assert.equal(model.retryDeadlineMs(unknownDelay), null);
});

test("column placement follows the measured 180 by 488 reference rectangle", async () => {
  const model = await loadModel();
  assert.deepEqual(model.draftColumnPlacement(1920, 1080), { x: 1728, y: 132, width: 180, height: 488, scale: 1 });
  assert.deepEqual(model.draftColumnPlacement(3840, 2160), { x: 3456, y: 264, width: 360, height: 976, scale: 2 });
});

test("inside the host's 180 by 488 column window the stage fills the window at scale 1", async () => {
  const model = await loadModel();
  // Regression, live test on a050ae8 (2026-11-22): the screen formula inside the column window
  // gave scale 0.452 at x 93, y 60, so the column drew at about 81 px wide.
  assert.deepEqual(model.draftStagePlacement(180, 488), { x: 0, y: 0, width: 180, height: 488, scale: 1 });
  const wrong = model.draftColumnPlacement(180, 488);
  assert.ok(wrong.scale < 0.46, "the screen formula is what shrank the column");
  // A screen-shaped viewport (the browser mock) keeps the measured screen placement.
  assert.deepEqual(model.draftStagePlacement(1920, 1080), { x: 1728, y: 132, width: 180, height: 488, scale: 1 });
  assert.equal(model.draftStagePlacement(0, 488), null);
});

test("draft source contains no prediction language", async () => {
  const root = new URL("../src/draft/", import.meta.url);
  let entries = [] as ReturnType<typeof readdirSync>;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch {
    // Keep RED failures as assertions when the feature source has not been written yet.
  }
  assert.ok(entries.length > 0, "draft source directory must be present");
  const files = entries.filter((entry): entry is import("node:fs").Dirent => typeof entry !== "string" && entry.isFile() && /\.(ts|tsx|css)$/.test(entry.name));
  const source = files.map(file => readFileSync(new URL(file.name, root), "utf8")).join("\n");
  assert.doesNotMatch(source, /โอกาสชนะ/);
});

test("only win-rate formatting adds a percent sign to statistic values", async () => {
  const model = await loadModel();
  const row = model.suggestionRowView({ hero_id: 1, name: "Spectre", rank: 1, games: 44465, win_rate_tenths: 531, low_data: false, taken: false });
  assert.equal(row.winRateText, "53.1%");
  assert.doesNotMatch(row.gamesText, /%/);
  assert.doesNotMatch(model.formatFormTenths(-4), /%/);
  const formatter = readDraftSource("draftColumnModel.ts");
  const view = readDraftSource("DraftColumnView.tsx");
  assert.equal([...formatter.matchAll(/%/g)].length, 2, "only win-rate output and the required 100% support notice use a percent sign");
  assert.match(formatter, /toFixed\(1\)\}%/);
  assert.doesNotMatch(view, /%/);
});

test("overlay stage has no controls or pointer interaction", () => {
  const stage = readDraftSource("DraftStage.tsx");
  const sharedView = readDraftSource("DraftColumnView.tsx");
  const css = readDraftSource("draft-column.css");
  for (const source of [stage, sharedView]) {
    assert.doesNotMatch(source, /<(?:button|input|select|textarea|a)\b|tabIndex\s*=|autoFocus|on(?:Click|Mouse|Pointer|Key)[A-Z]/);
  }
  assert.match(css, /\.draft-stage[\s\S]*?pointer-events:\s*none/);
  assert.doesNotMatch(css, /:hover/);
  assert.match(stage, /draftStagePlacement\(window\.innerWidth,\s*window\.innerHeight\)/);
  assert.match(css, /left:\s*var\(--draft-overlay-x(?:,\s*[^)]+)?\)/);
  assert.match(css, /top:\s*var\(--draft-overlay-y(?:,\s*[^)]+)?\)/);
  assert.match(sharedView, /card\.durations\.state === "stale"/);
  assert.match(css, /\.draft-duration-cell\.is-stale[\s\S]*?opacity:\s*0\.6/);
});

test("counter blocks are parsed strictly and dropped whole when invalid", async () => {
  const { parseCounters, visibleCounterRows, counterHeading, OVERLAY_COUNTER_ROWS } = await import("../src/draft/draftColumnModel.ts");
  const valid = {
    state: "ready", source: "kohbor_reviewed_table", enemies: ["Phantom Lancer"], enemies_identified: 1,
    position_filtered: false,
    rows: [{ hero_id: 7, name: "Earthshaker", against: ["Phantom Lancer"], reason: "Echo Slam" }],
  };
  const parsed = parseCounters(valid);
  assert.ok(parsed);
  assert.equal(counterHeading(parsed), "แก้ทาง Phantom Lancer");
  assert.equal(OVERLAY_COUNTER_ROWS, 3);
  for (const [label, value] of [
    ["unknown state", { ...valid, state: "maybe" }],
    ["other source", { ...valid, source: "opendota" }],
    ["rows while not ready", { ...valid, state: "sides_unknown" }],
    ["too many rows", { ...valid, rows: Array.from({ length: 7 }, () => valid.rows[0]) }],
    ["empty against", { ...valid, rows: [{ ...valid.rows[0], against: [] }] }],
    ["blank reason", { ...valid, rows: [{ ...valid.rows[0], reason: " " }] }],
    ["too many enemies", { ...valid, enemies_identified: 6 }],
  ] as const) {
    assert.equal(parseCounters(value), null, label);
  }
  assert.equal(parseCounters(undefined), null, "an older host without counters gives null");
  assert.deepEqual(visibleCounterRows(null, "overlay"), []);
  assert.deepEqual(visibleCounterRows({ ...parsed, state: "no_enemy", rows: [], enemies: [] }, "deck"), []);
});
