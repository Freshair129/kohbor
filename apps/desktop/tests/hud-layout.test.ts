import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  applyHudLayoutEvent,
  HUD_DEFAULT_LAYOUT,
  HUD_DEFAULT_LAYOUT_EVENT,
  HUD_DEFAULT_STRIP_HEIGHT_PERCENT,
  hudModulePlacement,
  parseHudLayoutEvent,
  visibleHudModules,
} from "../src/hudModel.ts";
import { createHudLayoutSubscriber } from "../src/hudRuntime.ts";

const validEvent = () => ({
  schema: 1,
  strip_height_percent: 9,
  modules: structuredClone(HUD_DEFAULT_LAYOUT),
});

function mutateModule(index: number, key: string, value: unknown) {
  const event = validEvent();
  (event.modules[index] as Record<string, unknown>)[key] = value;
  return event;
}

test("layout event accepts six unique bounded modules and the minimum strip", () => {
  assert.deepEqual(parseHudLayoutEvent(validEvent()), {
    schema: 1,
    strip_height_percent: 9,
    modules: HUD_DEFAULT_LAYOUT,
  });
});

test("layout event rejects null, primitives and arrays", () => {
  for (const value of [null, false, 4, "layout", []]) assert.equal(parseHudLayoutEvent(value), null);
});

test("layout event rejects a schema other than one", () => {
  assert.equal(parseHudLayoutEvent({ ...validEvent(), schema: 2 }), null);
});

test("layout event rejects missing or extra top-level keys", () => {
  const { modules: _modules, ...missing } = validEvent();
  assert.equal(parseHudLayoutEvent(missing), null);
  assert.equal(parseHudLayoutEvent({ ...validEvent(), extra: true }), null);
});

test("layout event rejects a non-array module collection", () => {
  assert.equal(parseHudLayoutEvent({ ...validEvent(), modules: {} }), null);
});

test("layout event requires exactly six module records", () => {
  const event = validEvent();
  assert.equal(parseHudLayoutEvent({ ...event, modules: event.modules.slice(1) }), null);
  assert.equal(parseHudLayoutEvent({ ...event, modules: [...event.modules, event.modules[0]] }), null);
});

test("layout event rejects duplicate and missing module ids", () => {
  const duplicate = validEvent();
  duplicate.modules[5]!.id = "gmeter";
  assert.equal(parseHudLayoutEvent(duplicate), null);
  const missing = validEvent();
  (missing.modules[5] as Record<string, unknown>).id = "future";
  assert.equal(parseHudLayoutEvent(missing), null);
});

test("layout event rejects an unknown module id and a malformed module row", () => {
  assert.equal(parseHudLayoutEvent(mutateModule(0, "id", "future")), null);
  const event = validEvent();
  event.modules[2] = null as never;
  assert.equal(parseHudLayoutEvent(event), null);
});

test("layout event rejects module records with missing or extra keys", () => {
  const missing = validEvent();
  delete (missing.modules[0] as unknown as Record<string, unknown>).enabled;
  assert.equal(parseHudLayoutEvent(missing), null);
  assert.equal(parseHudLayoutEvent(mutateModule(0, "extra", 1)), null);
});

test("layout event rejects every non-finite module coordinate or scale", () => {
  for (const [key, value] of [["x_percent", NaN], ["y_percent", Infinity], ["scale", -Infinity]] as const) {
    assert.equal(parseHudLayoutEvent(mutateModule(0, key, value)), null, key);
  }
});

test("layout event rejects each coordinate or scale outside its bound", () => {
  for (const [key, value] of [["x_percent", -0.01], ["x_percent", 100.01], ["y_percent", -1], ["y_percent", 101], ["scale", 0.19], ["scale", 3.01]] as const) {
    assert.equal(parseHudLayoutEvent(mutateModule(0, key, value)), null, `${key}=${value}`);
  }
});

test("layout event rejects non-number coordinates and a non-boolean enabled field", () => {
  for (const [key, value] of [["x_percent", "4"], ["y_percent", null], ["scale", true], ["enabled", 1]] as const) {
    assert.equal(parseHudLayoutEvent(mutateModule(0, key, value)), null, key);
  }
});

test("layout event rejects strip height below nine, above one hundred, or non-finite", () => {
  for (const value of [8.99, 100.01, NaN, Infinity, "9"]) {
    assert.equal(parseHudLayoutEvent({ ...validEvent(), strip_height_percent: value }), null);
  }
});

test("HUD layout starts with the existing defaults and nine-percent strip", () => {
  assert.equal(HUD_DEFAULT_LAYOUT_EVENT.strip_height_percent, HUD_DEFAULT_STRIP_HEIGHT_PERCENT);
  assert.deepEqual(HUD_DEFAULT_LAYOUT_EVENT.modules, HUD_DEFAULT_LAYOUT);
});

test("HUD applies a valid event as one whole layout", () => {
  const event = validEvent();
  event.strip_height_percent = 16;
  event.modules[0]!.x_percent = 37;
  const next = applyHudLayoutEvent(HUD_DEFAULT_LAYOUT_EVENT, event);
  assert.equal(next.strip_height_percent, 16);
  assert.equal(next.modules[0]!.x_percent, 37);
});

test("HUD ignores an invalid event and retains its previous complete layout", () => {
  const event = validEvent();
  event.modules[0]!.x_percent = 72;
  const previous = applyHudLayoutEvent(HUD_DEFAULT_LAYOUT_EVENT, event);
  const next = applyHudLayoutEvent(previous, mutateModule(1, "enabled", "false"));
  assert.equal(next, previous);
  assert.equal(next.modules[0]!.x_percent, 72);
});

test("HUD hides disabled modules and keeps enabled modules in input order", () => {
  const event = validEvent();
  event.modules[1]!.enabled = false;
  assert.deepEqual(visibleHudModules(parseHudLayoutEvent(event)!), event.modules.filter((module) => module.enabled));
});

test("default placement preserves the former nine-percent top mapping", () => {
  const item = HUD_DEFAULT_LAYOUT[0];
  const position = hudModulePlacement(item, 9);
  assert.equal(position.left_vw, item.x_percent);
  assert.equal(position.top_vh, (item.y_percent / 9) * 100);
  assert.equal(position.scale, item.scale);
});

test("placement maps screen-relative y into a grown native strip", () => {
  assert.equal(hudModulePlacement({ ...HUD_DEFAULT_LAYOUT[0], y_percent: 12 }, 24).top_vh, 50);
});

test("placement falls back to the default strip for an invalid height", () => {
  assert.equal(hudModulePlacement(HUD_DEFAULT_LAYOUT[0], 8).top_vh, (5.5 / 9) * 100);
});

test("HUD layout subscriber listens to hud://layout and passes complete payloads", async () => {
  let eventName = "";
  let listener: ((event: { payload: unknown }) => void) | null = null;
  const payload = validEvent();
  const subscribe = createHudLayoutSubscriber(() => true, async (event, handler) => {
    eventName = event;
    listener = handler;
    return () => {};
  });
  let received: unknown;
  await subscribe((value) => { received = value; });
  listener!({ payload });
  assert.equal(eventName, "hud://layout");
  assert.deepEqual(received, payload);
});

test("HUD layout subscriber is a no-op without a host", async () => {
  let registrations = 0;
  const subscribe = createHudLayoutSubscriber(() => false, async () => {
    registrations += 1;
    return () => {};
  });
  await subscribe(() => {});
  assert.equal(registrations, 0);
});

test("HUD renderer keeps the default layout without an event and filters disabled modules", () => {
  const source = readFileSync(new URL("../src/HudStage.tsx", import.meta.url), "utf8");
  assert.match(source, /useState<HudLayoutEvent>\(\(\) => \(\{ \.\.\.HUD_DEFAULT_LAYOUT_EVENT, modules: layout \}\)\)/);
  assert.match(source, /visibleHudModules\(hudLayout\)/);
  assert.match(source, /modulePosition\(item, hudLayout\.strip_height_percent\)/);
});

test("non-Lab HUD code contains no Lab command or branding strings", () => {
  for (const path of ["../src/HudStage.tsx", "../src/hudModel.ts", "../src/hudRuntime.ts"]) {
    const source = readFileSync(new URL(path, import.meta.url), "utf8");
    assert.doesNotMatch(source, /lab_status|Kohbor Lab|lab_hud/);
  }
});
