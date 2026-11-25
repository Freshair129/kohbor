import test from "node:test";
import assert from "node:assert/strict";

const layout = await import("../src/analytics/analyticsLayoutModel.mjs").catch((error) => {
  if (error?.code === "ERR_MODULE_NOT_FOUND") return {};
  throw error;
});
const heroCard = await import("../src/shared/heroCardDisplay.mjs").catch((error) => {
  if (error?.code === "ERR_MODULE_NOT_FOUND") return {};
  throw error;
});
const statusIcons = await import("../src/analytics/statusTrayModel.mjs").catch((error) => {
  if (error?.code === "ERR_MODULE_NOT_FOUND") return {};
  throw error;
});

test("pane math clamps five target window sizes without dropping below the map floor", () => {
  assert.equal(typeof layout.analyticsPaneLayout, "function", "analyticsPaneLayout is implemented");
  const cases = [
    { windowWidth: 1366, windowHeight: 768, scale: 1, sizeClass: "m", density: "compact", mapSide: 374 },
    { windowWidth: 1440, windowHeight: 900, scale: 1, sizeClass: "m", density: "compact", mapSide: 448 },
    { windowWidth: 1920, windowHeight: 1080, scale: 1, sizeClass: "l", density: "full", mapSide: 640 },
    { windowWidth: 2560, windowHeight: 1440, scale: 1, sizeClass: "xl", density: "full", mapSide: 1068 },
    { windowWidth: 1920, windowHeight: 1080, scale: 1.25, sizeClass: "m", density: "compact", mapSide: 440 },
  ];
  for (const item of cases) {
    const page = layout.analyticsPageDimensions(item);
    const result = layout.analyticsPaneLayout({ ...page });
    assert.equal(result.sizeClass, item.sizeClass);
    assert.equal(result.density, item.density);
    assert.ok(result.scale >= 0.85 && result.scale <= 1.2, `${item.windowWidth}: scale ${result.scale}`);
    assert.equal(result.mapSide, item.mapSide, `${item.windowWidth}: map side`);
  }
});

test("pane preferences persist by size class and reset only the current class", () => {
  assert.equal(typeof layout.readPanePreferences, "function", "readPanePreferences is implemented");
  const values = new Map();
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  for (const sizeClass of ["s", "m", "l", "xl"]) {
    const defaults = layout.defaultPanePreferences(sizeClass);
    const changed = { ...defaults, cardWidth: 360, sideWidth: 500, collapsed: { ...defaults.collapsed, radiant: true } };
    layout.writePanePreferences(storage, sizeClass, changed);
    assert.equal(layout.readPanePreferences(storage, sizeClass).cardWidth, 360);
    layout.resetPanePreferences(storage, sizeClass);
    assert.equal(layout.readPanePreferences(storage, sizeClass).cardWidth, defaults.cardWidth);
    assert.equal(layout.readPanePreferences(storage, sizeClass).collapsed.radiant, defaults.collapsed.radiant);
  }
  assert.equal(layout.analyticsPaneLayout({ width: 1000, height: 700 }).collapsed.side, true);
  assert.equal(layout.analyticsPaneLayout({ width: 850, height: 430 }).tooSmall, true);
  assert.deepEqual(layout.toggleCardColumns({ radiant: false, dire: false }), { radiant: true, dire: true });
  assert.deepEqual(layout.toggleCardColumns({ radiant: true, dire: true }), { radiant: false, dire: false });
  assert.deepEqual(layout.toggleCardColumns({ radiant: false, dire: true }), { radiant: true, dire: true });
});

test("layout shortcuts avoid the WebView Ctrl+0 zoom reset", () => {
  assert.equal(typeof layout.analyticsShortcut, "function", "analyticsShortcut is implemented");
  assert.equal(layout.analyticsShortcut({ ctrlKey: true, altKey: false, shiftKey: false, metaKey: false, key: "0" }), null);
  assert.equal(layout.analyticsShortcut({ ctrlKey: true, altKey: true, shiftKey: false, metaKey: false, key: "0" }), "reset-layout");
  assert.equal(layout.analyticsShortcut({ ctrlKey: true, altKey: false, shiftKey: true, metaKey: false, key: "\\" }), "toggle-cards");
});

test("top bar removes whole segments in the approved order at each measured width", () => {
  assert.equal(typeof layout.analyticsTopBarLayout, "function", "analyticsTopBarLayout is implemented");
  assert.deepEqual(layout.analyticsTopBarLayout(1300).picker, ["portrait", "dateTime", "hero", "result", "duration"]);
  assert.deepEqual(layout.analyticsTopBarLayout(1100).picker, ["portrait", "dateTime", "hero", "result"]);
  assert.deepEqual(layout.analyticsTopBarLayout(900).picker, ["portrait", "shortDate", "hero", "result"]);
  const narrow = layout.analyticsTopBarLayout(700);
  assert.deepEqual(narrow.picker, ["portrait", "hero", "resultDot"]);
  assert.deepEqual(narrow.overflow, ["myHero", "cardSettings", "layout"]);
  assert.equal(narrow.truncates, false);
});

test("hero card settings remove hidden cells and reflow rows without placeholders", () => {
  assert.equal(typeof heroCard.reflowHeroCardDisplay, "function", "reflowHeroCardDisplay is implemented");
  const all = heroCard.DEFAULT_HERO_CARD_DISPLAY;
  assert.equal(Object.values(all.show).filter(Boolean).length, 13);
  const hidden = { ...all, show: { ...all.show, attrs: false, dmg: false, reg: false, arm: false, mr: false, status: false } };
  const view = heroCard.reflowHeroCardDisplay(hidden, "full");
  assert.deepEqual(view.visibleStats, ["amp", "as", "ms", "ev", "sr", "ls", "gpm", "xpm"]);
  assert.equal(view.showStatus, false);
  assert.equal(view.emptyRows.includes("attrs"), true);
  assert.equal(view.placeholderCount, 0);
  assert.deepEqual(heroCard.visibleHeroCardRows(hidden, "compact", false, false), ["top", "bars", "ab"]);
  assert.deepEqual(heroCard.visibleHeroCardRows(hidden, "compact", true, false), ["top", "bars", "ab", "it", "dv"]);
  assert.equal(heroCard.visibleHeroCardRows(hidden, "full", true, false).includes("st"), false);
  const values = new Map();
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  heroCard.writeHeroCardDisplay(storage, hidden);
  assert.equal(heroCard.readHeroCardDisplay(storage).show.status, false);
});

test("status tray keeps every crowd-control effect and reports overflow", () => {
  assert.equal(typeof layout.statusTrayLayout, "function", "statusTrayLayout is implemented");
  const effects = [
    { name: "slow", kind: "debuff", remaining: 5 },
    { name: "hex", kind: "control", remaining: 1.2 },
    { name: "stun", kind: "control", remaining: 2.1 },
    { name: "root", kind: "control", remaining: 0.8 },
    { name: "bkb", kind: "buff", remaining: 7 },
    { name: "aegis", kind: "buff", permanent: true },
  ];
  const tray = layout.statusTrayLayout(effects, 2);
  assert.deepEqual(tray.visible.filter(item => item.kind === "control").map(item => item.name), ["stun", "hex", "root"]);
  assert.equal(tray.hiddenCount, 3);
  assert.equal(tray.overflow, "+3");
  const crowded = layout.statusTrayLayout([
    ...effects,
    { name: "disarm", kind: "debuff", remaining: 0.2 },
    { name: "silence", kind: "debuff", remaining: 0.5 },
    { name: "slow-two", kind: "debuff", remaining: 1 },
  ], 3);
  assert.equal(crowded.visible.filter(item => item.kind === "control").length, 3);
  assert.equal(crowded.hiddenCount, crowded.all.length - crowded.visible.length);
  assert.equal(crowded.overflow, "+6");
  assert.equal(crowded.visible.at(-1).name, "root", "the shortest control duration is on the right edge");
});

test("modifier icons use bounded tiers and do not invent unobserved status", () => {
  assert.equal(typeof statusIcons.resolveModifierIcon, "function", "resolveModifierIcon is implemented");
  const icons = new Set(["item:phase_boots", "ability:legion_commander_duel", "ability:earthshaker_fissure"]);
  const has = (kind, name) => icons.has(`${kind}:${name}`);
  assert.deepEqual(statusIcons.resolveModifierIcon({ modifier: "modifier_item_phase_boots_active", iconExists: has }), {
    kind: "item", icon: "phase_boots", glyph: null, tier: 1, inferred: false, sourceHeroId: null,
  });
  assert.equal(statusIcons.resolveModifierIcon({ modifier: "modifier_legion_commander_duel", iconExists: has }).tier, 2);
  const inferred = statusIcons.resolveModifierIcon({ modifier: "modifier_stunned", sourceHeroId: "npc_dota_hero_earthshaker",
    recentCasts: [{ time: 10, name: "earthshaker_fissure", start: 12 }], iconExists: has });
  assert.equal(inferred.tier, 4);
  assert.equal(inferred.inferred, true);
  assert.equal(statusIcons.resolveModifierIcon({ modifier: "modifier_unknown_effect", effectKind: "debuff", iconExists: has }).glyph, "▼");
  assert.match(statusIcons.statusIconTitle({ name: "stun", sourceNames: [], remaining: 1.2, locale: "th" }), /ไม่ทราบที่มา/);
});

test("a selection creates one quote action without changing the active tab", () => {
  assert.equal(typeof layout.selectionQuote, "function", "selectionQuote is implemented");
  const quote = layout.selectionQuote({ text: "Review the fight at 14:18", source: "สรุป › สรุป", fallbackTime: null });
  assert.deepEqual(quote, { text: "Review the fight at 14:18", source: "สรุป › สรุป", time: 858, action: "ask", changesTab: false, noteCount: 1 });
  const untimed = layout.selectionQuote({ text: "  one\nselection  ", source: "ผู้ช่วย", fallbackTime: null });
  assert.equal(untimed.text, "one selection");
  assert.equal(untimed.time, null);
  assert.equal(untimed.noteCount, 1);
  const prompt = layout.selectionPrompt("", [quote, untimed, { ...quote, text: "third" }, { ...quote, text: "fourth" }], "en");
  assert.equal(prompt.includes("Review the fight at 14:18"), false, "only the most recent three quotes are retained");
  assert.match(prompt, /> one selection/);
  assert.match(prompt, /Explain this selected text\.$/);
  assert.equal(quote.changesTab, false);
  assert.equal(quote.noteCount, 1);
});

test("compact density keeps every row for the selected hero", () => {
  assert.equal(typeof layout.heroCardDensity, "function", "heroCardDensity is implemented");
  const compact = layout.heroCardDensity({ columnWidth: 240, height: 712, scale: 1 });
  assert.equal(compact.density, "compact");
  assert.equal(compact.selectedRows, "all");
  assert.equal(compact.scale, 1);
  assert.deepEqual(heroCard.visibleHeroCardRows(heroCard.DEFAULT_HERO_CARD_DISPLAY, compact.density, true, false),
    ["top", "bars", "ab", "it", "st", "dv"]);
  const smallest = layout.heroCardDensity({ columnWidth: 232, height: 478, scale: 1 });
  assert.ok(smallest.scale >= 0.85);
});
