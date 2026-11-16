import assert from "node:assert/strict";
import test from "node:test";
import {
  changeHudLayoutDraft,
  clampHudModule,
  commitHudNumber,
  computeHudStripHeightPercent,
  confirmHudLayout,
  createHudLayoutEditorState,
  dragHudModule,
  failHudLayoutAction,
  hudLayoutHasUnsavedChanges,
  hudModuleBox,
  modulesOutsideHudStrip,
  nudgeHudModule,
  overlappingHudModules,
  parseLabHudLayoutReply,
  parseLabHudPreviewReply,
  sameHudLayout,
  updateHudModule,
  HUD_MODULE_REFERENCE_SIZES,
  HUD_REFERENCE_SCREEN,
  type LabHudLayoutReply,
  type LabHudModule,
} from "../src/lab/labHudLayout.ts";
import { createLabHudLayoutApi } from "../src/lab/labRuntime.ts";
import { createLabHudLayoutMock } from "../src/lab/labHudMock.ts";
import { HUD_DEFAULT_LAYOUT, HUD_LAYOUT_BOUNDS } from "../src/hudModel.ts";

const modules = () => structuredClone(HUD_DEFAULT_LAYOUT) as LabHudModule[];
const layoutReply = (overrides: Partial<LabHudLayoutReply> = {}): LabHudLayoutReply => ({
  modules: modules(),
  default: modules(),
  bounds: HUD_LAYOUT_BOUNDS,
  strip_height_percent: 9,
  source: "embedded",
  ...overrides,
});

test("layout reply parser accepts embedded six-module host reply", () => {
  assert.deepEqual(parseLabHudLayoutReply(layoutReply()), layoutReply());
});

test("layout reply parser accepts an override reply with grown strip", () => {
  const value = layoutReply({ source: "override", strip_height_percent: 42 });
  assert.deepEqual(parseLabHudLayoutReply(value), value);
});

test("layout reply parser rejects malformed reply roots and unexpected keys", () => {
  assert.equal(parseLabHudLayoutReply(null), null);
  assert.equal(parseLabHudLayoutReply({ ...layoutReply(), extra: true }), null);
  const { source: _source, ...missing } = layoutReply();
  assert.equal(parseLabHudLayoutReply(missing), null);
});

test("layout reply parser rejects an unknown source and invalid strip height", () => {
  assert.equal(parseLabHudLayoutReply({ ...layoutReply(), source: "future" }), null);
  for (const value of [8, 101, NaN, Infinity]) assert.equal(parseLabHudLayoutReply({ ...layoutReply(), strip_height_percent: value }), null);
});

test("layout reply parser requires exact host bounds", () => {
  const wrong = { x_percent: [0, 99], y_percent: [0, 100], scale: [0.2, 3] };
  assert.equal(parseLabHudLayoutReply({ ...layoutReply(), bounds: wrong }), null);
  assert.equal(parseLabHudLayoutReply({ ...layoutReply(), bounds: { ...HUD_LAYOUT_BOUNDS, extra: true } }), null);
});

test("layout reply parser rejects malformed and duplicate module lists atomically", () => {
  const duplicate = modules();
  duplicate[5]!.id = "gmeter";
  assert.equal(parseLabHudLayoutReply({ ...layoutReply(), modules: duplicate }), null);
  assert.equal(parseLabHudLayoutReply({ ...layoutReply(), default: modules().slice(1) }), null);
});

test("preview reply parser accepts only one finite timestamp field", () => {
  assert.deepEqual(parseLabHudPreviewReply({ preview_until_ms: 12_345 }), { preview_until_ms: 12_345 });
  for (const value of [null, {}, { preview_until_ms: -1 }, { preview_until_ms: Infinity }, { preview_until_ms: 1, extra: true }]) {
    assert.equal(parseLabHudPreviewReply(value), null);
  }
});

test("clamp keeps positions and scale within host bounds", () => {
  assert.deepEqual(clampHudModule({ id: "gmeter", x_percent: -9, y_percent: 104, scale: 9, enabled: true }), {
    id: "gmeter", x_percent: 0, y_percent: 100, scale: 3, enabled: true,
  });
});

test("clamp replaces non-finite numeric draft values with safe centres and scale", () => {
  assert.deepEqual(clampHudModule({ id: "gmeter", x_percent: NaN, y_percent: Infinity, scale: -Infinity, enabled: false }), {
    id: "gmeter", x_percent: 50, y_percent: 50, scale: 1, enabled: false,
  });
});

test("field updates change only the selected module and retain enabled state", () => {
  const updated = updateHudModule(modules(), "kda", { x_percent: 33 });
  assert.equal(updated[1]!.x_percent, 33);
  assert.equal(updated[1]!.enabled, true);
  assert.equal(updated[0]!.x_percent, 50);
});

test("canvas drag converts pixel deltas into screen percent", () => {
  const moved = dragHudModule(modules(), "gmeter", 192, 108, 1920, 1080);
  assert.equal(moved[0]!.x_percent, 60);
  assert.equal(moved[0]!.y_percent, 15.5);
});

test("canvas drag clamps at screen bounds and ignores a zero-size canvas", () => {
  const edge = updateHudModule(modules(), "gmeter", { x_percent: 99, y_percent: 99 });
  assert.equal(dragHudModule(edge, "gmeter", 200, 200, 100, 100)[0]!.x_percent, 100);
  assert.deepEqual(dragHudModule(edge, "gmeter", 50, 50, 0, 100), edge);
});

test("arrow nudge moves by one tenth of a percent", () => {
  const moved = nudgeHudModule(modules(), "gmeter", "ArrowRight", false)!;
  assert.equal(moved[0]!.x_percent, 50.1);
  assert.equal(nudgeHudModule(modules(), "gmeter", "ArrowDown", false)![0]!.y_percent, 5.6);
});

test("shift arrow nudge moves by one percent", () => {
  assert.equal(nudgeHudModule(modules(), "gmeter", "ArrowLeft", true)![0]!.x_percent, 49);
  assert.equal(nudgeHudModule(modules(), "gmeter", "ArrowUp", true)![0]!.y_percent, 4.5);
});

test("nudge clamps at bounds and rejects other keys", () => {
  const atEdge = updateHudModule(modules(), "gmeter", { x_percent: 0 });
  assert.equal(nudgeHudModule(atEdge, "gmeter", "ArrowLeft", true)![0]!.x_percent, 0);
  assert.equal(nudgeHudModule(atEdge, "gmeter", "Enter", false), null);
});

test("module box math uses the requested reference pixel sizes and scale", () => {
  assert.deepEqual(HUD_MODULE_REFERENCE_SIZES, {
    gmeter: { width: 300, height: 36 },
    kda: { width: 90, height: 43 },
    gold: { width: 90, height: 43 },
    gpm: { width: 78, height: 43 },
    nw: { width: 96, height: 43 },
    xpm: { width: 78, height: 43 },
  });
  const box = hudModuleBox({ id: "gmeter", x_percent: 50, y_percent: 50, scale: 2, enabled: true });
  assert.deepEqual(box, { x: 660, y: 504, width: 600, height: 72 });
  assert.equal(box.x + box.width / 2, HUD_REFERENCE_SCREEN.width / 2);
  assert.equal(box.y + box.height / 2, HUD_REFERENCE_SCREEN.height / 2);
});

test("overlap detection reports intersecting enabled module boxes", () => {
  const layout = modules().map((module) => ({ ...module, enabled: false }));
  layout[1] = { ...layout[1]!, x_percent: 50, y_percent: 50, enabled: true };
  layout[2] = { ...layout[2]!, x_percent: 50, y_percent: 50, enabled: true };
  assert.deepEqual(overlappingHudModules(layout), [{ first: "kda", second: "gold" }]);
});

test("overlap detection ignores disabled modules and touching edges", () => {
  const disabled = modules().map((module) => ({ ...module, x_percent: 50, y_percent: 50, enabled: false }));
  assert.deepEqual(overlappingHudModules(disabled), []);
  const touching = modules().map((module) => ({ ...module, enabled: false }));
  touching[1] = { ...touching[1]!, x_percent: 50, y_percent: 50, scale: 1, enabled: true };
  touching[2] = { ...touching[2]!, x_percent: 54.6875, y_percent: 50, scale: 1, enabled: true };
  assert.deepEqual(overlappingHudModules(touching), []);
});

test("strip calculation defaults to nine percent for the default module layout", () => {
  assert.equal(computeHudStripHeightPercent(HUD_DEFAULT_LAYOUT), 9);
});

test("strip calculation grows to the greatest enabled centre Y plus three point five", () => {
  const layout = modules();
  layout[0] = { ...layout[0]!, y_percent: 27.25 };
  assert.equal(computeHudStripHeightPercent(layout), 30.75);
});

test("strip calculation ignores disabled modules", () => {
  const layout = modules().map((module) => ({ ...module, y_percent: 90, enabled: false }));
  assert.equal(computeHudStripHeightPercent(layout), 9);
});

test("strip calculation caps at one hundred percent", () => {
  const layout = modules();
  layout[0] = { ...layout[0]!, y_percent: 100 };
  assert.equal(computeHudStripHeightPercent(layout), 100);
});

test("bottom-edge warning uses scaled box dimensions against computed strip", () => {
  const layout = modules().map((module) => ({ ...module, enabled: false }));
  layout[1] = { ...layout[1]!, y_percent: 12, scale: 3, enabled: true };
  assert.equal(computeHudStripHeightPercent(layout), 15.5);
  assert.deepEqual(modulesOutsideHudStrip(layout), ["kda"]);
});

test("bottom-edge warning ignores disabled modules", () => {
  const layout = modules().map((module) => ({ ...module, y_percent: 96, scale: 3, enabled: false }));
  assert.deepEqual(modulesOutsideHudStrip(layout), []);
});

test("layout equality is order-independent and compares every editable field", () => {
  const original = modules();
  assert.equal(sameHudLayout(original, [...original].reverse()), true);
  assert.equal(sameHudLayout(original, updateHudModule(original, "xpm", { enabled: false })), false);
});

test("unsaved indicator changes with the draft and clears after host confirmation", () => {
  const initial = createHudLayoutEditorState(layoutReply());
  assert.equal(hudLayoutHasUnsavedChanges(initial), false);
  const edited = changeHudLayoutDraft(initial, updateHudModule(initial.draft, "gold", { scale: 1.2 }));
  assert.equal(hudLayoutHasUnsavedChanges(edited), true);
  const confirmed = confirmHudLayout(edited, layoutReply({ modules: edited.draft }));
  assert.equal(hudLayoutHasUnsavedChanges(confirmed), false);
});

test("host failure keeps the last confirmed layout and draft while showing an error", () => {
  const initial = createHudLayoutEditorState(layoutReply());
  const edited = changeHudLayoutDraft(initial, updateHudModule(initial.draft, "gold", { scale: 1.2 }));
  const failed = failHudLayoutAction(edited, "host offline");
  assert.deepEqual(failed.confirmed, initial.confirmed);
  assert.deepEqual(failed.draft, edited.draft);
  assert.equal(failed.error, "host offline");
  assert.equal(hudLayoutHasUnsavedChanges(failed), true);
});

test("fake invoke receives the exact get, apply, reset and preview call shapes", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const api = createLabHudLayoutApi(async (command, args) => {
    calls.push(args === undefined ? { command } : { command, args });
    return command === "lab_hud_preview" ? { preview_until_ms: 1_010 } : layoutReply();
  });
  const modulesToApply = modules();
  await api.get();
  await api.apply(modulesToApply);
  await api.reset();
  await api.preview(10);
  assert.deepEqual(calls, [
    { command: "lab_hud_layout_get" },
    { command: "lab_hud_layout_apply", args: { modules: modulesToApply } },
    { command: "lab_hud_layout_reset" },
    { command: "lab_hud_preview", args: { seconds: 10 } },
  ]);
});

test("fake invoke response parsers reject invalid layout and preview replies", async () => {
  const api = createLabHudLayoutApi(async (command) => command === "lab_hud_preview" ? { preview_until_ms: NaN } : {});
  await assert.rejects(api.get(), /LAB_HUD_BAD_RESPONSE/);
  await assert.rejects(api.apply(modules()), /LAB_HUD_BAD_RESPONSE/);
  await assert.rejects(api.reset(), /LAB_HUD_BAD_RESPONSE/);
  await assert.rejects(api.preview(10), /LAB_HUD_BAD_RESPONSE/);
});

test("preview invoke accepts only whole seconds from one through sixty", async () => {
  const api = createLabHudLayoutApi(async () => ({ preview_until_ms: 2_000 }));
  for (const seconds of [0, 61, 2.5, NaN]) await assert.rejects(api.preview(seconds), /LAB_HUD_PREVIEW_SECONDS/);
  assert.deepEqual(await api.preview(1), { preview_until_ms: 2_000 });
});

test("Lab mock starts with an event and republishes every two seconds", () => {
  const events: Array<{ event: string; payload: unknown }> = [];
  let scheduled: (() => void) | null = null;
  let delay = 0;
  const mock = createLabHudLayoutMock({
    emit: (event, payload) => events.push({ event, payload }),
    schedule: (handler, milliseconds) => {
      scheduled = handler;
      delay = milliseconds;
      return 1 as ReturnType<typeof setInterval>;
    },
    cancel: () => {},
  });
  mock.start();
  assert.equal(events[0]!.event, "hud://layout");
  assert.equal(delay, 2000);
  scheduled!();
  assert.equal(events.length, 2);
  assert.deepEqual(events[0]!.payload, events[1]!.payload);
  mock.stop();
});

test("Lab mock apply and reset return confirmed replies and publish each change", () => {
  const events: unknown[] = [];
  const mock = createLabHudLayoutMock({ emit: (_event, payload) => events.push(payload) });
  const edited = modules();
  edited[0] = { ...edited[0]!, x_percent: 36, y_percent: 18 };
  const applied = mock.apply(edited);
  assert.equal(applied.source, "override");
  assert.equal(applied.strip_height_percent, 21.5);
  assert.equal(events.length, 1);
  assert.equal(mock.reset().source, "embedded");
  assert.equal(events.length, 2);
  assert.equal(mock.get().modules[0]!.x_percent, 50);
});

test("Lab mock preview reports its bounded synthetic display expiry", () => {
  const mock = createLabHudLayoutMock({ emit: () => {}, now: () => 500 });
  assert.deepEqual(mock.preview(10), { preview_until_ms: 10_500 });
  assert.throws(() => mock.preview(61), /LAB_HUD_PREVIEW_SECONDS/);
});

test("Lab mock rejects partial or malformed module application without publishing", () => {
  let emissions = 0;
  const mock = createLabHudLayoutMock({ emit: () => { emissions += 1; } });
  assert.throws(() => mock.apply(modules().slice(1)), /MOCK_HUD_LAYOUT_INVALID/);
  assert.equal(emissions, 0);
  assert.equal(mock.get().source, "embedded");
});

test("F2: edits made while a save is in flight survive the confirmation", () => {
  const confirmed = layoutReply();
  const state0 = createHudLayoutEditorState(confirmed);
  const sent = updateHudModule(state0.draft, "kda", { x_percent: 30 }, HUD_LAYOUT_BOUNDS);
  const state1 = changeHudLayoutDraft(state0, sent);
  const edited = updateHudModule(state1.draft, "gold", { y_percent: 20 }, HUD_LAYOUT_BOUNDS);
  const state2 = changeHudLayoutDraft(state1, edited);
  const reply = layoutReply({ modules: sent, source: "override" });
  const after = confirmHudLayout(state2, reply, sent);
  assert.deepEqual(after.confirmed, reply);
  assert.deepEqual(after.draft, edited, "the newer edit is kept");
  assert.equal(hudLayoutHasUnsavedChanges(after), true);
});

test("F2: unchanged draft is replaced by the confirmed reply; no sent snapshot behaves as before", () => {
  const state0 = createHudLayoutEditorState(layoutReply());
  const sent = updateHudModule(state0.draft, "kda", { x_percent: 30 }, HUD_LAYOUT_BOUNDS);
  const state1 = changeHudLayoutDraft(state0, sent);
  const reply = layoutReply({ modules: sent, source: "override" });
  assert.equal(hudLayoutHasUnsavedChanges(confirmHudLayout(state1, reply, sent)), false);
  assert.equal(hudLayoutHasUnsavedChanges(confirmHudLayout(state1, reply)), false);
});

test("F4: typed numbers commit on blur only and clamp then", () => {
  const bounds = HUD_LAYOUT_BOUNDS.scale;
  assert.equal(commitHudNumber("", bounds), null);
  assert.equal(commitHudNumber("abc", bounds), null);
  assert.equal(commitHudNumber("1", bounds), 1);
  assert.equal(commitHudNumber("0.1", bounds), bounds[0]);
  assert.equal(commitHudNumber("9", bounds), bounds[1]);
  assert.equal(commitHudNumber("-1", HUD_LAYOUT_BOUNDS.x_percent), 0);
  assert.equal(commitHudNumber("12.5", HUD_LAYOUT_BOUNDS.x_percent), 12.5);
});


test("F5/F6: editor source keeps the editor mounted across tabs and exposes slider value attributes", async () => {
  const { readFileSync } = await import("node:fs");
  const editor = readFileSync(new URL("../src/lab/HudLayoutEditor.tsx", import.meta.url), "utf8");
  const lab = readFileSync(new URL("../src/lab/Lab.tsx", import.meta.url), "utf8");
  for (const attribute of ["aria-valuemin", "aria-valuemax", "aria-valuenow", "aria-valuetext"]) assert.ok(editor.includes(attribute), attribute);
  assert.ok(lab.includes("hidden={tab !== \"hud\"}"), "HUD editor is hidden, not unmounted, on tab switch");
  assert.ok(lab.includes("alertdialog"), "closing the Lab with unsaved HUD edits asks first");
});
