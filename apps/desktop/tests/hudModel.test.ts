import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const model = await import("../src/hudModel.ts").catch(() => null) as Record<string, any> | null;
const runtime = await import("../src/hudRuntime.ts").catch(() => null) as Record<string, any> | null;
const mock = await import("../src/mock/hudState.ts").catch(() => null) as Record<string, any> | null;

const validHud = () => ({
  schema: 1,
  visible: true,
  fields: {
    kills: { value: 3, availability: "fresh" },
    deaths: { value: 2, availability: "fresh" },
    assists: { value: 5, availability: "fresh" },
    gold: { value: 1_234, availability: "fresh" },
    gpm: { value: 456, availability: "fresh" },
    xpm: { value: 789, availability: "fresh" },
  },
  net_worth: { value: 12_345, availability: "fresh", approximate: false },
});

function api<T extends Record<string, any>>(module: T | null, name: string): any {
  assert.ok(module, `${name} module is implemented`);
  assert.equal(typeof module[name], "function", `${name} is exported`);
  return module[name];
}

function assertAllUnknown(state: any, visible = true) {
  assert.equal(state.visible, visible);
  for (const field of Object.values(state.fields) as any[]) {
    assert.deepEqual(field, { value: null, availability: "unknown" });
  }
  assert.deepEqual(state.net_worth, { value: null, availability: "unknown", approximate: false });
}

function riskView(overrides: Record<string, unknown> = {}) {
  const { state: stateOverrides, ...viewOverrides } = overrides;
  return {
    state: {
      meter: 2,
      gsi_source: true,
      vision_source: true,
      visible: 3,
      baseline: 2,
      events: [],
      ...(stateOverrides as Record<string, unknown> | undefined),
    },
    lastVisionEvent: null,
    ...viewOverrides,
  };
}

test("HUD parser accepts the exact schema and preserves fresh integer fields", () => {
  const parseHudState = api(model, "parseHudState");
  const parsed = parseHudState(validHud());
  assert.equal(parsed.schema, 1);
  assert.equal(parsed.visible, true);
  assert.deepEqual(parsed.fields.kills, { value: 3, availability: "fresh" });
  assert.deepEqual(parsed.net_worth, { value: 12_345, availability: "fresh", approximate: false });
});

test("HUD parser treats null, primitives and arrays as a visible wholly unknown state", () => {
  const parseHudState = api(model, "parseHudState");
  for (const payload of [null, 7, "state", [], false]) assertAllUnknown(parseHudState(payload));
});

test("HUD parser rejects unsupported schema versions without retaining any chip value", () => {
  const parseHudState = api(model, "parseHudState");
  assertAllUnknown(parseHudState({ ...validHud(), schema: 2 }));
});

test("HUD parser fails closed when visible has the wrong type", () => {
  const parseHudState = api(model, "parseHudState");
  assertAllUnknown(parseHudState({ ...validHud(), visible: 1 }));
});

test("HUD parser keeps an explicit hide when another part of the payload is malformed", () => {
  const parseHudState = api(model, "parseHudState");
  assertAllUnknown(parseHudState({ ...validHud(), visible: false, fields: [] }), false);
});

test("HUD parser fails closed when fields has the wrong shape", () => {
  const parseHudState = api(model, "parseHudState");
  assertAllUnknown(parseHudState({ ...validHud(), fields: [] }));
});

test("HUD parser fails closed when net_worth is missing", () => {
  const parseHudState = api(model, "parseHudState");
  const { net_worth: _removed, ...payload } = validHud();
  assertAllUnknown(parseHudState(payload));
});

test("HUD parser fails closed when net_worth has the wrong shape", () => {
  const parseHudState = api(model, "parseHudState");
  assertAllUnknown(parseHudState({ ...validHud(), net_worth: [] }));
});

test("HUD parser fails closed when a required stat is missing", () => {
  const parseHudState = api(model, "parseHudState");
  const payload = validHud();
  delete (payload.fields as Record<string, unknown>).kills;
  assertAllUnknown(parseHudState(payload));
});

test("HUD parser fails closed on extra top-level or field keys", () => {
  const parseHudState = api(model, "parseHudState");
  assertAllUnknown(parseHudState({ ...validHud(), extra: true }));
  const payload = validHud();
  (payload.fields.kills as Record<string, unknown>).extra = true;
  assertAllUnknown(parseHudState(payload));
});

test("HUD parser fails closed when a field omits value or availability", () => {
  const parseHudState = api(model, "parseHudState");
  const withoutValue = validHud();
  delete (withoutValue.fields.kills as Record<string, unknown>).value;
  assertAllUnknown(parseHudState(withoutValue));
  const withoutAvailability = validHud();
  delete (withoutAvailability.fields.kills as Record<string, unknown>).availability;
  assertAllUnknown(parseHudState(withoutAvailability));
});

test("HUD parser fails closed when availability is not a string", () => {
  const parseHudState = api(model, "parseHudState");
  const payload = validHud();
  (payload.fields.gold as Record<string, unknown>).availability = 1;
  assertAllUnknown(parseHudState(payload));
});

test("HUD parser fails closed on an unrecognized availability", () => {
  const parseHudState = api(model, "parseHudState");
  const payload = validHud();
  (payload.fields.xpm as Record<string, unknown>).availability = "future";
  assertAllUnknown(parseHudState(payload));
});

test("HUD parser treats string stat values as a whole-state shape error", () => {
  const parseHudState = api(model, "parseHudState");
  const payload = validHud();
  (payload.fields.gold as Record<string, unknown>).value = "1234";
  assertAllUnknown(parseHudState(payload));
});

test("HUD parser treats boolean, array and object stat values as a whole-state shape error", () => {
  const parseHudState = api(model, "parseHudState");
  for (const value of [true, [], {}]) {
    const payload = validHud();
    (payload.fields.kills as Record<string, unknown>).value = value;
    assertAllUnknown(parseHudState(payload));
  }
});

test("HUD parser makes a negative stat unknown without poisoning other fields", () => {
  const parseHudState = api(model, "parseHudState");
  const payload = validHud();
  (payload.fields.deaths as Record<string, unknown>).value = -1;
  const parsed = parseHudState(payload);
  assert.deepEqual(parsed.fields.deaths, { value: null, availability: "unknown" });
  assert.deepEqual(parsed.fields.kills, { value: 3, availability: "fresh" });
});

test("HUD parser makes fractional and non-finite stat values unknown", () => {
  const parseHudState = api(model, "parseHudState");
  for (const value of [2.5, Number.NaN, Number.POSITIVE_INFINITY]) {
    const payload = validHud();
    (payload.fields.assists as Record<string, unknown>).value = value;
    assert.deepEqual(parseHudState(payload).fields.assists, { value: null, availability: "unknown" });
  }
});

test("HUD parser makes an invalid net worth value unknown while preserving other fields", () => {
  const parseHudState = api(model, "parseHudState");
  const payload = validHud();
  payload.net_worth = { value: -10, availability: "fresh", approximate: false };
  const parsed = parseHudState(payload);
  assert.deepEqual(parsed.net_worth, { value: null, availability: "unknown", approximate: false });
  assert.deepEqual(parsed.fields.gold, { value: 1_234, availability: "fresh" });
});

test("HUD parser accepts only the five declared availability values", () => {
  const parseHudState = api(model, "parseHudState");
  for (const availability of ["fresh", "stale", "unknown", "invalid", "unsupported"]) {
    const payload = validHud();
    (payload.fields.kills as Record<string, unknown>).availability = availability;
    assert.equal(parseHudState(payload).fields.kills.availability, availability);
  }
});

test("meter level zero lights only the first (grey) tick and uses the no-signal label", () => {
  const meterModel = api(model, "meterModel");
  assert.deepEqual(meterModel(riskView({ state: { meter: 0 } })), {
    level: 0, ticksLit: 1, tone: "none", label: "No signal", blind: false,
  });
});

test("meter level one lights two blue ticks and keeps a text label", () => {
  const meterModel = api(model, "meterModel");
  assert.deepEqual(meterModel(riskView({ state: { meter: 1 } })), {
    level: 1, ticksLit: 2, tone: "ally", label: "Out of sight", blind: false,
  });
});

test("meter level two lights three amber ticks", () => {
  const meterModel = api(model, "meterModel");
  assert.deepEqual(meterModel(riskView({ state: { meter: 2 } })), {
    level: 2, ticksLit: 3, tone: "warn", label: "Risky", blind: false,
  });
});

test("meter level three lights all four red ticks", () => {
  const meterModel = api(model, "meterModel");
  assert.deepEqual(meterModel(riskView({ state: { meter: 3 } })), {
    level: 3, ticksLit: 4, tone: "critical", label: "Danger", blind: false,
  });
});

test("meter is blind when no risk view or risk state exists", () => {
  const meterModel = api(model, "meterModel");
  for (const view of [null, { state: null, lastVisionEvent: null }]) {
    assert.deepEqual(meterModel(view), {
      level: null, ticksLit: 0, tone: "unknown", label: "Data not ready", blind: true,
    });
  }
});

test("meter is blind when capture or GSI source health is missing", () => {
  const meterModel = api(model, "meterModel");
  for (const state of [{ vision_source: false }, { gsi_source: false }]) {
    const result = meterModel(riskView({ state }));
    assert.equal(result.blind, true);
    assert.equal(result.ticksLit, 0);
    assert.equal(result.label, "Data not ready");
  }
});

test("meter is blind when the risk level is unknown", () => {
  const meterModel = api(model, "meterModel");
  const result = meterModel(riskView({ state: { meter: null } }));
  assert.equal(result.level, null);
  assert.equal(result.ticksLit, 0);
  assert.equal(result.blind, true);
});

test("an active risk alert or danger remains visible when source health is blind", () => {
  const meterModel = api(model, "meterModel");
  const byEvent = meterModel(riskView({ state: { meter: 1, gsi_source: false, events: ["risk_alert"] } }));
  assert.equal(byEvent.level, 3);
  assert.equal(byEvent.tone, "critical");
  assert.equal(byEvent.blind, false);
  const byDangerLevel = meterModel(riskView({ state: { meter: 3, vision_source: false } }));
  assert.equal(byDangerLevel.level, 3);
  assert.equal(byDangerLevel.tone, "critical");
});

test("K/D/A formats each unavailable part as an em dash", () => {
  const formatChip = api(model, "formatChip");
  const fields = {
    kills: { value: 3, availability: "fresh" },
    deaths: { value: null, availability: "unknown" },
    assists: { value: 5, availability: "stale" },
  };
  assert.deepEqual(formatChip("kda", fields), { text: "3/—/5", stale: true });
});

test("stat chips format integers with thousands separators", () => {
  const formatChip = api(model, "formatChip");
  assert.deepEqual(formatChip("gold", { value: 1_234_567, availability: "fresh" }), { text: "1,234,567", stale: false });
  assert.equal(formatChip("gpm", { value: 12_345, availability: "fresh" }).text, "12,345");
  assert.equal(formatChip("xpm", { value: 6789, availability: "fresh" }).text, "6,789");
});

test("stale values remain visible and carry a dim stale flag", () => {
  const formatChip = api(model, "formatChip");
  assert.deepEqual(formatChip("gold", { value: 987, availability: "stale" }), { text: "987", stale: true });
});

test("unknown, invalid and unsupported chip values show a dash, never zero", () => {
  const formatChip = api(model, "formatChip");
  for (const availability of ["unknown", "invalid", "unsupported"]) {
    assert.equal(formatChip("gold", { value: 0, availability }).text, "—");
  }
  assert.equal(formatChip("gold", { value: null, availability: "fresh" }).text, "—");
  assert.equal(formatChip("gold", { value: -1, availability: "fresh" }).text, "—");
});

test("net worth: fresh and stale integers show, approximate gets a leading ~, unknown states show a dash", () => {
  const formatChip = api(model, "formatChip");
  assert.deepEqual(formatChip("nw", { value: 12_345, availability: "fresh", approximate: false }), { text: "12,345", stale: false });
  assert.deepEqual(formatChip("nw", { value: 12_345, availability: "stale", approximate: false }), { text: "12,345", stale: true });
  assert.equal(formatChip("nw", { value: 12_345, availability: "fresh", approximate: true }).text, "~12,345");
  for (const availability of ["unknown", "invalid", "unsupported"]) {
    assert.equal(formatChip("nw", { value: 12_345, availability, approximate: false }).text, "—");
  }
  assert.equal(formatChip("nw", { value: null, availability: "fresh", approximate: false }).text, "—");
  assert.equal(formatChip("nw", { value: -5, availability: "fresh", approximate: false }).text, "—");
  // The approximate marker is net-worth only: other chips never get a tilde.
  assert.equal(formatChip("gold", { value: 1_000, availability: "fresh", approximate: true }).text, "1,000");
});

test("net worth parser requires the approximate flag and drops it when the value is unknown", () => {
  const parseHudState = api(model, "parseHudState");
  const withNw = (net_worth: unknown) => parseHudState({ ...validHud(), net_worth });
  assert.deepEqual(withNw({ value: 9_000, availability: "fresh", approximate: true }).net_worth, { value: 9_000, availability: "fresh", approximate: true });
  assert.deepEqual(withNw({ value: null, availability: "unknown", approximate: true }).net_worth, { value: null, availability: "unknown", approximate: false });
  assertAllUnknown(withNw({ value: 9_000, availability: "fresh" }));
  assertAllUnknown(withNw({ value: 9_000, availability: "fresh", approximate: "yes" }));
  assertAllUnknown(withNw({ value: 9_000, availability: "fresh", approximate: false, extra: 1 }));
});

test("HUD labels are centralized in Thai copy", () => {
  assert.ok(model, "hud model is implemented");
  assert.ok(model!.hudCopy, "HUD copy is exported");
  assert.deepEqual(model!.hudCopy.chipLabels, { kda: "K/D/A", gold: "Gold", gpm: "GPM", nw: "NW", xpm: "XPM" });
  assert.equal(model!.hudCopy.meterTitle, "Risk");
});

test("default HUD layout matches the requested six module positions and scales", () => {
  assert.ok(model, "hud model is implemented");
  assert.deepEqual(model!.HUD_DEFAULT_LAYOUT, [
    { id: "gmeter", x_percent: 50, y_percent: 5.5, scale: 0.65, enabled: true },
    { id: "kda", x_percent: 9.64, y_percent: 2.2, scale: 0.78, enabled: true },
    { id: "gold", x_percent: 13.56, y_percent: 2.2, scale: 0.78, enabled: true },
    { id: "gpm", x_percent: 17.23, y_percent: 2.2, scale: 0.78, enabled: true },
    { id: "nw", x_percent: 21.02, y_percent: 2.2, scale: 0.78, enabled: true },
    { id: "xpm", x_percent: 24.82, y_percent: 2.2, scale: 0.78, enabled: true },
  ]);
});

test("visible false renders nothing and does not evaluate visible content", () => {
  const renderWhenHudVisible = api(model, "renderWhenHudVisible");
  let rendered = false;
  assert.equal(renderWhenHudVisible(false, () => { rendered = true; return "strip"; }), null);
  assert.equal(rendered, false);
  assert.equal(renderWhenHudVisible(true, () => "strip"), "strip");
});

test("HUD CSS prevents page scrolling and keeps the strip click-through", () => {
  const css = readFileSync(new URL("../src/hud.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const selectors = new Map<string, Map<string, string>[]>();
  for (const match of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const declarations = new Map<string, string>();
    for (const declaration of match[2].split(";")) {
      const separator = declaration.indexOf(":");
      if (separator >= 0) declarations.set(declaration.slice(0, separator).trim(), declaration.slice(separator + 1).trim());
    }
    for (const selector of match[1].split(",").map((item) => item.trim())) {
      const rules = selectors.get(selector) ?? [];
      rules.push(declarations);
      selectors.set(selector, rules);
    }
  }
  for (const selector of ["html.hud-body", "body.hud-body", "body.hud-body #root"]) {
    const rules = selectors.get(selector) ?? [];
    assert.ok(rules.some((rule) => ["hidden", "clip"].includes(rule.get("overflow") ?? "")
      || ["hidden", "clip"].includes(rule.get("overflow-y") ?? "")), `${selector} clips overflow`);
    assert.ok(!rules.some((rule) => ["auto", "scroll"].includes(rule.get("overflow") ?? "")
      || ["auto", "scroll"].includes(rule.get("overflow-y") ?? "")), `${selector} has no scrolling overflow`);
  }
  for (const selector of [".hud-stage", ".hud-stage *"]) {
    assert.ok(selectors.get(selector)?.some((rule) => rule.get("pointer-events") === "none"));
  }
});

test("HUD route sets its transparent body class and mounts the stage without the app shell", () => {
  const source = readFileSync(new URL("../src/main.tsx", import.meta.url), "utf8");
  assert.match(source, /hudView\s*=\s*params\.get\("view"\)\s*===\s*"hud"/);
  assert.match(source, /classList\.toggle\("hud-body",\s*hudView\)/);
  assert.match(source, /hudView\s*\?\s*\(\s*<HudStage/);
  assert.match(source, /if\s*\(!overlayView\s*&&\s*!hudView\)\s*installFrontendLogging/);
});

test("HUD subscription wraps a fake event listener and returns its unlisten function", async () => {
  const createHudStateSubscriber = api(runtime, "createHudStateSubscriber");
  let listenedTo = "";
  let listener: ((event: { payload: unknown }) => void) | null = null;
  let unlistened = false;
  const stop = () => { unlistened = true; };
  const subscribe = createHudStateSubscriber(
    () => true,
    async (event: string, callback: (event: { payload: unknown }) => void) => {
      listenedTo = event;
      listener = callback;
      return stop;
    },
  );
  let received: unknown;
  const unlisten = await subscribe((payload: unknown) => { received = payload; });
  const payload = validHud();
  listener!({ payload });
  assert.equal(listenedTo, "hud://state");
  assert.deepEqual(received, payload);
  unlisten();
  assert.equal(unlistened, true);
});

test("HUD subscription is a no-op when no host is available", async () => {
  const createHudStateSubscriber = api(runtime, "createHudStateSubscriber");
  let listenCount = 0;
  const subscribe = createHudStateSubscriber(() => false, async () => { listenCount += 1; return () => {}; });
  const unlisten = await subscribe(() => {});
  unlisten();
  assert.equal(listenCount, 0);
});

test("mock HUD payload round-trips through the strict parser with fresh sample numbers", () => {
  assert.ok(mock, "mock HUD state is implemented");
  const parseHudState = api(model, "parseHudState");
  const parsed = parseHudState(mock!.mockHudStatePayload);
  assert.equal(parsed.visible, true);
  assert.equal(parsed.fields.kills.availability, "fresh");
  assert.equal(parsed.fields.gold.value, 1_234);
  assert.equal(parsed.net_worth.availability, "fresh");
});

test("mock HUD preview emits periodically and uses the live synthetic risk scenario", () => {
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  const liveMock = readFileSync(new URL("../src/live/mock.ts", import.meta.url), "utf8");
  assert.match(harness, /HudMockHarness/);
  assert.match(harness, /deliver\(HUD_STATE_EVENT, mockHudStatePayload\)/);
  assert.match(harness, /setInterval\([^\n]*HUD_MOCK_INTERVAL_MS/);
  assert.match(harness, /HUD_STATE_EVENT\s*=\s*"hud:\/\/state"/);
  assert.match(liveMock, /meter: 2/);
  assert.match(liveMock, /gsi_source: scenario !== "unknown"/);
  assert.match(liveMock, /vision_source: visionAvailable/);
});

test("HUD component uses the shared risk hook and starts live collection", () => {
  const source = readFileSync(new URL("../src/HudStage.tsx", import.meta.url), "utf8");
  assert.match(source, /useRisk\(/);
  assert.match(source, /startLiveCollection\(/);
  const riskSource = readFileSync(new URL("../src/live/useRisk.ts", import.meta.url), "utf8");
  assert.match(riskSource, /captureGatedRiskView\(state, lastVisionEvent, capture\)/);
  assert.match(riskSource, /export function useRisk\(\): RiskView \| null/);
  assert.match(source, /role="status"/);
  assert.match(source, /aria-live="off"/);
  assert.match(source, /renderWhenHudVisible\(state\.visible/);
});


test("HUD copy: English is shown first, Thai stays one switch away, unknown never reads as safe", async () => {
  const copy = await import("../src/hudCopy.ts");
  assert.equal(copy.HUD_LANGUAGE, "en");
  assert.equal(copy.hudCopy, copy.hudCopyTables.en);
  assert.equal(copy.hudCopyTables.th.meterLabels.unavailable, "ข้อมูลยังไม่พร้อม");
  assert.equal(copy.hudCopyTables.th.chipLabels.gold, "ทอง");
  for (const table of [copy.hudCopyTables.en, copy.hudCopyTables.th]) {
    assert.deepEqual(Object.keys(table.meterLabels).sort(), ["danger", "none", "risk", "unavailable", "unseen"]);
    assert.doesNotMatch(table.meterLabels.unavailable, /safe|ปลอดภัย/i);
    assert.doesNotMatch(JSON.stringify(table), /G-Maiden/i);
  }
});

test("F3: parseHudState accepts only the optional synthetic marker true", () => {
  const parseHudState = api(model, "parseHudState");
  const marked = parseHudState({ ...validHud(), synthetic: true });
  assert.equal(marked.synthetic, true);
  assert.equal(marked.visible, true);
  assert.equal(parseHudState(validHud()).synthetic, undefined);
  for (const bad of [false, 1, "true", null]) {
    const parsed = parseHudState({ ...validHud(), synthetic: bad });
    assert.equal(parsed.synthetic, undefined, String(bad));
    assertAllUnknown(parsed);
  }
});

