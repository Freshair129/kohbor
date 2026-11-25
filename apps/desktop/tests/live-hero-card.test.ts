// Slice R5 (DEC-REPLAY-INTEGRATION item 10): replay-style hero cards on the centred Live deck.
// Owner amendment 2026-11-24: every card (own hero and the nine others) has the replay card's full structure; values
// without a source are "—" or unknown slots. Pure-logic tests only: card view model from a GSI snapshot, unknown
// handling for allies/enemies, icon resolver fallback.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { parseOwnSnapshot } from "../src/ownSnapshot.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { deriveOwnHeroViewModel } from "../src/deck/ownHeroModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { deriveLoadoutViewModel } from "../src/deck/loadoutModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatCooldownValue } from "../src/deck/cooldownModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { allyHeroCards, enemyHeroCards, type HeroCard } from "../src/live/secondMonitorCards.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { createDraftRosterMemory, rememberDraftFrame } from "../src/live/secondMonitorModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { createIconResolver, iconFileName, noIcons } from "../src/live/dotaIcons.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { compactCard, formatGold, joinKnown, ownFullCard, selectLiveDerivedRules, slotColor, statCells, teamFullCard, tileText, UNKNOWN } from "../src/live/heroCardModel.ts";
import * as heroCardModel from "../src/live/heroCardModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftBarFrame } from "../src/deck/draftBarModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { StatsData } from "../src/stats/derivedStats.ts";

const fixture = () => JSON.parse(readFileSync(new URL("../../../fixtures/own-fresh-at-boundary.json", import.meta.url), "utf8"));
const field = (value: unknown) => ({ value, availability: "fresh" });
const item = (slot: string, name: string | null, changes = {}) => ({ slot, name, cooldown: null, charges: null, can_cast: true, passive: false, ...changes });
const ability = (slot: number, name: string, changes = {}) =>
  ({ slot, name, level: 1, cooldown: 0, ultimate: false, can_cast: true, passive: false, active: true, ...changes });

/** The boundary fixture plus stats and a loadout, as a mid-game GSI snapshot. */
function liveSnapshot(firstItem = "item_blink") {
  const s = fixture();
  Object.assign(s.own.fields, {
    hero_id: field("npc_dota_hero_lion"),
    gold: field(12345), kills: field(4), deaths: field(2), assists: field(9), last_hits: field(87), denies: field(6),
    gpm: field(412), xpm: field(520),
    items: field([
      item("inventory0", firstItem, { cooldown: 12 }), item("inventory1", null), item("inventory2", "item_magic_wand", { charges: 7 }),
      item("inventory3", "item_boots"), item("inventory4", null), item("inventory5", null),
      item("backpack0", "item_ward_observer", { charges: 2 }), item("backpack1", null), item("backpack2", null),
      item("teleport", "item_tpscroll", { cooldown: 70 }), item("neutral", "item_arcane_ring"),
    ]),
    abilities: field([
      ability(0, "lion_impale", { level: 3 }), ability(1, "lion_voodoo", { level: 2, cooldown: 9, can_cast: false }),
      ability(2, "lion_mana_drain", { level: 0, can_cast: false }), ability(3, "lion_finger_of_death", { level: 2, cooldown: 45, ultimate: true, can_cast: false }),
      ability(4, "plus_high_five", { level: 1, cooldown: 0 }), ability(5, "lion_passive_thing", { passive: true }),
    ]),
  });
  const parsed = parseOwnSnapshot(s);
  assert.ok(parsed, "fixture must stay admissible");
  return parsed;
}

function card(snapshot = liveSnapshot(), icons = noIcons, slot: number | null = null) {
  const loadout = deriveLoadoutViewModel(snapshot, false);
  return ownFullCard({ own: deriveOwnHeroViewModel(snapshot), items: loadout.items.slots, itemsStale: loadout.items.stale,
    abilities: loadout.abilities.tiles, abilitiesStale: loadout.abilities.stale, slot, icons });
}

const liveStatsPack = JSON.parse(readFileSync(new URL("../src/assets/dota/6989/stats.json", import.meta.url), "utf8")) as StatsData;
function derivedCard(firstItem = "item_blink", options: { stale?: boolean; itemsStale?: boolean } = {}) {
  const snapshot = liveSnapshot(firstItem);
  const loadout = deriveLoadoutViewModel(snapshot, false);
  return ownFullCard({ own: deriveOwnHeroViewModel(snapshot), items: loadout.items.slots, itemsStale: options.itemsStale ?? false,
    abilities: loadout.abilities.tiles, abilitiesStale: false, slot: 0, stale: options.stale,
    icons: noIcons, statsData: liveStatsPack, gameBuild: null });
}

test("own hero HP and mana bars expose only fresh derived regeneration with compact one-decimal formatting", () => {
  const view = derivedCard();
  assert.equal(view.hpRegenText, "+5.2");
  assert.equal(view.manaRegenText, null, "mana regen lacks the INT rule, so the bar shows nothing rather than an incomplete total");
  assert.equal(heroCardModel.formatRegenText?.(6.5), "+6.5");
  assert.equal(heroCardModel.formatRegenText?.(6), "+6", "the existing numeric format drops a trailing .0");
  assert.equal(heroCardModel.formatRegenText?.(null), null);
  assert.equal(derivedCard("item_unobserved_test_item").hpRegenText, null);
  assert.equal(derivedCard("item_unobserved_test_item").manaRegenText, null);
  assert.equal(derivedCard("item_blink", { stale: true }).hpRegenText, null);
  assert.equal(derivedCard("item_blink", { stale: true }).manaRegenText, null);
  assert.equal(derivedCard("item_blink", { itemsStale: true }).hpRegenText, null);
  assert.equal(derivedCard("item_blink", { itemsStale: true }).manaRegenText, null);
  assert.equal(card().hpRegenText, null, "a missing stats pack never turns into +0");
  assert.equal(card().manaRegenText, null);
});

test("compact own field stats keep the eight visible derived cells and omit REG", () => {
  const cells = heroCardModel.compactOwnFieldStats?.(derivedCard().stats);
  assert.deepEqual(cells?.map(cell => cell.key), ["arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"]);
  assert.equal(cells?.some(cell => cell.key === "reg"), false, "regen is carried by the HP and mana bars");
});

test("regen bars keep bilingual derived-estimate tooltips and reserve a right-aligned 12 px value slot", () => {
  const component = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(component, /ฟื้นฟูพลังชีวิต \$\{amount\} ต่อวินาที/);
  assert.match(component, /ฟื้นฟูมานา \$\{amount\} ต่อวินาที/);
  assert.match(component, /HP regeneration \$\{amount\} per second/);
  assert.match(component, /Mana regeneration \$\{amount\} per second/);
  assert.match(component, /Derived estimate from the stat rules/);
  assert.match(component, /Sources: \$\{sources\.length > 0/);
  assert.match(component, /without temporary effects/);
  assert.match(css, /\.lhc-bar-num\.has-regen\s*\{[^}]*right:\s*42px/);
  assert.match(css, /\.lhc-bar-regen\s*\{[^}]*right:\s*4px[^}]*font:\s*600 12px/);

  for (const barWidth of [104, 121, 161, 169, 228]) {
    const valueSlot = barWidth - 42;
    const compactHpText = 8 * 7.2; // 850/1200 in the 12 px monospace bar font.
    const regenSlot = 5 * 7.2 + 4; // +6.5 plus its 4 px inset.
    assert.ok(compactHpText <= valueSlot, `${barWidth}px bar: HP value fits left of regen`);
    assert.ok(regenSlot <= 42, `${barWidth}px bar: regen stays inside its reserved right slot`);
  }
});

test("Lion level-12 GSI hero and item IDs populate labelled estimates with a truthful newest-rules fallback", () => {
  const view = derivedCard();
  assert.equal(view.heroId, "npc_dota_hero_lion");
  assert.equal(view.levelText, "12");
  assert.equal(view.derivedBasis?.label, "B? · P6989 · R7.42*");
  assert.match(view.derivedBasis?.title ?? "", /Game build unknown; stats pack build 6989; rules 7\.42 selected as the newest supported fallback/);
  const cells = new Map(view.stats.map(cell => [cell.key, cell]));
  for (const key of ["str", "agi", "int", "dmg", "reg", "arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"] as const) {
    const cell = cells.get(key);
    assert.ok(cell?.derived, `${key} is labelled as a derived value`);
    assert.notEqual(cell.value, UNKNOWN, `${key} has an output from the Lion fixture`);
    assert.ok(cell.sources.length > 0, `${key} keeps derivation sources`);
  }
  assert.equal(cells.get("gpm")?.value, "412", "observed GPM stays the GSI value");
  assert.equal(cells.get("xpm")?.value, "520", "observed XPM stays the GSI value");
  const title = cells.get("arm")?.basis ?? "";
  assert.match(title, /Game build unknown; stats pack build 6989/);
  assert.equal(selectLiveDerivedRules(null, liveStatsPack).rules.patch, "7.42");
});

test("an item unknown to the installed stats pack keeps dependent derived values as em dashes", () => {
  const view = derivedCard("item_unobserved_test_item");
  const cells = new Map(view.stats.map(cell => [cell.key, cell]));
  for (const key of ["str", "agi", "int", "dmg", "reg", "arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"] as const) {
    assert.equal(cells.get(key)?.value, UNKNOWN, `${key} remains unknown when an item ID cannot be resolved`);
  }
  assert.equal(cells.get("gpm")?.value, "412");
  assert.equal(cells.get("xpm")?.value, "520");
});

test("derived stat markers expose their source list and the temporary-effects caveat at 12 px", () => {
  const component = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(component, /Sources: \$\{sources\}/);
  assert.match(component, /ไม่รวมผลชั่วคราว \/ without temporary effects/);
  assert.match(css, /\.lhc-stat\.is-derived:not\(\.is-unknown\) dd\s*\{[^}]*text-decoration:\s*underline dotted/);
  assert.match(css, /\.lhc-field-card\.lhc-field-own \.lhc-stat dt, \.lhc-field-card\.lhc-field-own \.lhc-stat dd\s*\{[^}]*font-size:\s*12px/);
  assert.match(component, /statsData: LIVE_STATS_DATA, gameBuild: null/);
});

test("each stat cell maps to an icon and bilingual accessible label without changing unknown values", () => {
  const cells = statCells();
  assert.deepEqual(cells.map(cell => cell.label), STAT_LABELS);
  for (const cell of cells) {
    assert.ok(cell.iconIds.length > 0, `${cell.key} maps to at least one icon`);
    assert.ok(cell.accessibleLabel.includes(cell.label), `${cell.key} accessible label retains its stat name`);
    assert.match(cell.accessibleLabel, /[ก-๙]/, `${cell.key} label includes Thai`);
    assert.match(cell.accessibleLabel, /\([^)]+\)/, `${cell.key} label includes English`);
    assert.equal(cell.value, UNKNOWN, `${cell.key} remains unknown without a value source`);
  }
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.match(css, /\.lhc-stat dt\s*\{[^}]*height:\s*12px/);
  assert.match(css, /\.lhc-stat dt > img\s*\{[^}]*width:\s*12px;\s*height:\s*12px/);
});

test("own hero: identity, level, K/D/A, LH/DN, gold, HP and mana map from GSI", () => {
  const view = card();
  assert.equal(view.variant, "full");
  assert.equal(view.heroId, "npc_dota_hero_lion");
  assert.equal(view.heroName, "Lion");
  assert.equal(view.levelText, "12");
  assert.equal(view.kdaText, "4/2/9");
  assert.equal(view.lhdnText, "87/6");
  assert.equal(view.gold, 12345);
  assert.equal(view.goldText, "12.3k");
  assert.equal(view.ratesText, "412 / 520");
  assert.equal(view.hpText, "850 / 1200");
  assert.ok(Math.abs((view.hpPercent ?? 0) - 70.83) < 0.01);
  assert.equal(view.hpLow, false);
  assert.equal(view.manaPercent, 43);
  assert.equal(view.manaText, "43%");
  assert.equal(view.alive, true);
  assert.equal(view.respawnText, null);
  assert.equal(view.portraitSrc, null, "no bundled icon set yet: runtime portrait fallback");
  assert.equal(view.color, null, "no draft slot: team colour, not a guessed slot colour");
  assert.equal(card(liveSnapshot(), noIcons, 7).color, slotColor(7));
});

test("own hero: abilities keep level, cooldown and castability; passives are dropped; talents go to extras", () => {
  const view = card();
  assert.ok(view.abilities);
  assert.deepEqual(view.abilities.hero.map(slot => slot.rawId), ["lion_impale", "lion_voodoo", "lion_mana_drain", "lion_finger_of_death"]);
  const [impale, voodoo, drain, finger] = view.abilities.hero;
  assert.equal(impale?.cast, "ready");
  assert.equal(impale?.level, 3);
  assert.equal(impale?.maxLevel, 4);
  assert.equal(voodoo?.cast, "cooldown");
  assert.equal(voodoo?.cooldownText, formatCooldownValue(9));
  assert.equal(drain?.cast, "not_learned");
  assert.equal(drain?.cooldownText, null);
  assert.equal(finger?.ultimate, true);
  assert.equal(finger?.maxLevel, 6);
  assert.equal(finger?.cooldownText, "45");
  assert.deepEqual(view.abilities.extras.map(slot => slot.rawId), ["plus_high_five"]);
});

test("own hero: six main items, backpack, neutral and TP with charges and cooldowns", () => {
  const view = card();
  assert.ok(view.items);
  assert.deepEqual(view.items.main.map(slot => slot.state), ["filled", "empty", "filled", "filled", "empty", "empty"]);
  assert.equal(view.items.main[0]?.rawId, "item_blink");
  assert.equal(view.items.main[0]?.cooldownText, "12");
  assert.equal(view.items.main[2]?.charges, 7);
  assert.equal(view.items.backpack.length, 3);
  assert.equal(view.items.backpack[0]?.charges, 2);
  assert.equal(view.items.teleport.rawId, "item_tpscroll");
  assert.equal(view.items.teleport.cooldownText, "70");
  assert.equal(view.items.neutral.rawId, "item_arcane_ring");
});

const STAT_LABELS = ["STR", "AGI", "INT", "DMG", "REG", "ARM", "MR", "AMP", "AS", "MS", "EV", "SR", "LS", "GPM", "XPM"];

test("own hero: fields GSI did not send stay unknown; every section is present with unknown slots, never zeros", () => {
  const raw = parseOwnSnapshot(fixture());
  assert.ok(raw);
  const view = card(raw);
  assert.equal(view.kdaText, UNKNOWN);
  assert.equal(view.lhdnText, UNKNOWN);
  assert.equal(view.goldText, UNKNOWN);
  assert.equal(view.gold, null);
  assert.equal(view.ratesText, UNKNOWN);
  // No abilities or items from GSI: the full rows still render, as unknown slots (not empty items, not zeros).
  assert.equal(view.abilities.known, false);
  assert.deepEqual(view.abilities.hero.map(slot => [slot.state, slot.ultimate]), [["unknown", false], ["unknown", false], ["unknown", false], ["unknown", true]]);
  assert.equal(view.items.known, false);
  assert.equal(view.items.main.length, 6);
  assert.equal(view.items.backpack.length, 3);
  for (const slot of [...view.items.main, ...view.items.backpack, view.items.teleport, view.items.neutral]) {
    assert.equal(slot.state, "unknown"); assert.equal(slot.rawId, null); assert.equal(slot.charges, null); assert.equal(slot.cooldown, null);
  }
  assert.deepEqual(view.buffs, [], "no buff source on Live: the chip row stays empty");
  // Stat rows are present in the replay order; GSI sends no attributes, so every one is a dash, never a computed value.
  assert.deepEqual(view.stats.map(cell => cell.label), STAT_LABELS);
  assert.ok(view.stats.every(cell => cell.value === UNKNOWN && cell.gain === null));
  // Partly known values keep the unknown part visible instead of a zero.
  assert.equal(joinKnown([3, null, 1]), "3/—/1");
  assert.equal(formatGold(9850), "9,850");
  assert.equal(formatGold(null), UNKNOWN);
});

test("own hero in a match: stat cells stay dashes except the GSI rates", () => {
  const view = card();
  assert.equal(view.items.known, true);
  assert.equal(view.abilities.known, true);
  const values = Object.fromEntries(view.stats.map(cell => [cell.label, cell.value]));
  assert.equal(values.GPM, "412");
  assert.equal(values.XPM, "520");
  for (const label of STAT_LABELS.filter(label => label !== "GPM" && label !== "XPM")) assert.equal(values[label], UNKNOWN, label);
  assert.deepEqual(statCells().map(cell => cell.value), STAT_LABELS.map(() => UNKNOWN));
});

test("own hero: dead shows the respawn time; stale abilities never show a cooldown as current", () => {
  const s = fixture();
  s.own.fields.alive = field(false); s.own.fields.respawn_seconds = field(23);
  const dead = parseOwnSnapshot(s);
  assert.ok(dead);
  assert.equal(card(dead).respawnText, "23");
  const snapshot = liveSnapshot();
  const loadout = deriveLoadoutViewModel(snapshot, false);
  const stale = ownFullCard({ own: deriveOwnHeroViewModel(snapshot), items: loadout.items.slots, itemsStale: true,
    abilities: loadout.abilities.tiles, abilitiesStale: true, stale: true, icons: noIcons });
  assert.equal(stale.stale, true);
  assert.ok(stale.abilities?.hero.every(slot => slot.cooldownText === null && (slot.cast === "unknown" || slot.cast === "not_learned")));
});

// ---- allies and enemies: identity only from same-session draft memory, everything else unknown ----
const ROSTER = ["npc_dota_hero_lion", "npc_dota_hero_axe", null, "npc_dota_hero_juggernaut", "npc_dota_hero_earthshaker",
  "npc_dota_hero_sniper", "npc_dota_hero_pudge", null, "npc_dota_hero_crystal_maiden", "npc_dota_hero_zuus"];
function draftFrame(ids: (string | null)[]): DraftBarFrame {
  return {
    active: true, detail: null, bgra: null,
    slots: ids.map((heroId, slot) => ({ slot, team: slot < 5 ? "radiant" : "dire", hero_id: heroId,
      confidence: heroId ? 0.9 : 0, reason: heroId ? "identified" : "low_confidence", title: "", text: "" })),
  } as unknown as DraftBarFrame;
}
const memory = rememberDraftFrame(createDraftRosterMemory(), 5, draftFrame(ROSTER));

test("enemy compact cards: draft names only, never vitals, skills or another hero's icon", () => {
  const cards = enemyHeroCards(memory, 5, "radiant").map(c => compactCard(c, noIcons));
  assert.deepEqual(cards.map(c => c.heroName), ["Sniper", "Pudge", null, "Crystal Maiden", "Zeus"]);
  assert.deepEqual(cards.map(c => c.slot), [5, 6, 7, 8, 9]);
  for (const c of cards) {
    assert.equal(c.variant, "compact");
    assert.equal(c.side, "enemy");
    assert.equal(c.hpPercent, null); assert.equal(c.manaPercent, null); assert.equal(c.vitalsSource, null); assert.equal(c.ult, null);
    assert.equal(c.portraitSrc, null);
  }
  assert.equal(cards[2]?.heroId, null, "unidentified slot stays unknown");
  assert.equal(cards[0]?.color, slotColor(5));
  // Even if a card arrived with values, an enemy compact card shows none of them.
  const forged: HeroCard = { key: "E1", side: "enemy", slot: 5, heroId: "npc_dota_hero_sniper", heroName: "Sniper", nameSource: "draft",
    hpPercent: 50, manaPercent: 50, vitalsSource: "peer",
    skills: [{ ultimate: false, state: "ready", seconds: null, source: "peer" }, { ultimate: false, state: "ready", seconds: null, source: "peer" },
      { ultimate: false, state: "ready", seconds: null, source: "peer" }, { ultimate: true, state: "ready", seconds: null, source: "peer" }] };
  const shown = compactCard(forged, noIcons);
  assert.equal(shown.hpPercent, null); assert.equal(shown.vitalsSource, null); assert.equal(shown.ult, null);
});

test("ally compact cards: own hero excluded, unknown without a draft match, no guessed side or slot colour", () => {
  const allies = allyHeroCards({ memory, epoch: 5, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion" }).map(c => compactCard(c, noIcons));
  assert.deepEqual(allies.map(c => c.heroName), ["Axe", null, "Juggernaut", "Earthshaker"]);
  assert.ok(allies.every(c => c.side === "ally" && c.hpPercent === null && c.vitalsSource === null && c.ult === null));
  const unmatched = allyHeroCards({ memory, epoch: 5, ownTeam: "radiant", ownHeroId: "npc_dota_hero_tiny" }).map(c => compactCard(c, noIcons));
  assert.ok(unmatched.every(c => c.heroName === null && c.slot === null && c.color === null));
  const otherSession = enemyHeroCards(memory, 6, "radiant").map(c => compactCard(c, noIcons));
  assert.ok(otherSession.every(c => c.heroName === null), "another session's identities never show");
  const noTeam = enemyHeroCards(memory, 5, null).map(c => compactCard(c, noIcons));
  assert.ok(noTeam.every(c => c.heroName === null && c.color === null));
});

test("ally compact cards show peer vitals and the ultimate dot only when that source exists", () => {
  const allies = allyHeroCards({ memory, epoch: 5, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion",
    peer: { 1: { hpPercent: 64, manaPercent: 30, skills: [null, null, null], ultimate: null } },
    topbar: { ally_ult_state: { 3: "ready_no_mana", 4: "bogus" as never } } }).map(c => compactCard(c, noIcons));
  assert.equal(allies[0]?.hpPercent, 64);
  assert.equal(allies[0]?.vitalsSource, "peer");
  assert.deepEqual(allies[2]?.ult, { state: "ready_no_mana", source: "topbar", seconds: null });
  assert.equal(allies[3]?.ult, null, "unrecognised dot value is unknown");
});

test("team cards have the full replay structure with every value unknown; enemies never show a value", () => {
  const enemies = enemyHeroCards(memory, 5, "radiant").map(c => teamFullCard(c, noIcons));
  const allies = allyHeroCards({ memory, epoch: 5, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion" }).map(c => teamFullCard(c, noIcons));
  assert.deepEqual(enemies.map(c => c.heroName), ["Sniper", "Pudge", null, "Crystal Maiden", "Zeus"]);
  assert.deepEqual(allies.map(c => c.heroName), ["Axe", null, "Juggernaut", "Earthshaker"]);
  for (const c of [...enemies, ...allies]) {
    assert.equal(c.variant, "full");
    for (const text of [c.levelText, c.kdaText, c.lhdnText, c.goldText, c.ratesText, c.hpText, c.manaText]) assert.equal(text, UNKNOWN);
    assert.equal(c.gold, null); assert.equal(c.hpPercent, null); assert.equal(c.manaPercent, null);
    assert.equal(c.abilities.hero.length, 4);
    assert.ok(c.abilities.hero.every(slot => slot.state === "unknown" && slot.cast === "unknown" && slot.cooldownText === null));
    assert.equal(c.items.main.length, 6); assert.equal(c.items.backpack.length, 3);
    assert.ok([...c.items.main, ...c.items.backpack, c.items.teleport, c.items.neutral].every(slot => slot.state === "unknown"));
    assert.deepEqual(c.stats.map(cell => cell.label), STAT_LABELS);
    assert.ok(c.stats.every(cell => cell.value === UNKNOWN));
    assert.deepEqual(c.buffs, []);
  }
  assert.equal(enemies[0]?.color, slotColor(5));
  assert.equal(enemies[2]?.heroId, null);
  // A forged enemy card with values still shows none of them; an ally keeps only admitted peer values.
  const forged: HeroCard = { key: "E1", side: "enemy", slot: 5, heroId: "npc_dota_hero_sniper", heroName: "Sniper", nameSource: "draft",
    hpPercent: 50, manaPercent: 50, vitalsSource: "peer",
    skills: [{ ultimate: false, state: "cooldown", seconds: 5, source: "peer" }, { ultimate: false, state: "ready", seconds: null, source: "peer" },
      { ultimate: false, state: "ready", seconds: null, source: "peer" }, { ultimate: true, state: "ready", seconds: null, source: "peer" }] };
  const shown = teamFullCard(forged, noIcons);
  assert.equal(shown.hpPercent, null); assert.equal(shown.hpText, UNKNOWN); assert.equal(shown.ult, null);
  assert.ok(shown.abilities.hero.every(slot => slot.cast === "unknown" && slot.cooldownText === null));
  const shared = teamFullCard({ ...forged, key: "A1", side: "ally", slot: 1 }, noIcons);
  assert.equal(shared.hpText, "50%");
  assert.equal(shared.abilities.hero[0]?.cast, "cooldown");
  assert.equal(shared.abilities.hero[0]?.cooldownText, formatCooldownValue(5));
  assert.equal(shared.abilities.hero[1]?.cast, "ready");
  assert.equal(shared.levelText, UNKNOWN, "no level source even with peer vitals");
});

// ---- icon resolver ----
test("icon resolver maps internal names to the newest bundled build and never substitutes another icon", () => {
  assert.equal(iconFileName("hero", "npc_dota_hero_antimage"), "antimage");
  assert.equal(iconFileName("item", "item_blink"), "blink");
  assert.equal(iconFileName("item", "item_recipe_black_king_bar"), "recipe");
  assert.equal(iconFileName("ability", "antimage_mana_break"), "antimage_mana_break");
  assert.equal(iconFileName("hero", "../secret"), null);
  assert.equal(iconFileName("item", null), null);

  const resolve = createIconResolver({
    "../assets/dota/7.40/heroes/lion.png": "/old/lion.png",
    "../assets/dota/7.41/heroes/lion.png": "/new/lion.png",
    "../assets/dota/7.41/heroes/lion.webp": "/new/lion.webp",
    "../assets/dota/7.41/items/blink.png": "/new/blink.png",
    "../assets/dota/7.41/items/recipe.png": "/new/recipe.png",
    "../assets/dota/7.41/abilities/lion_impale.png": "/new/lion_impale.png",
    "../assets/dota/7.41/other/thing.png": "/ignored.png",
    "../assets/elsewhere/lion.png": "/ignored2.png",
  });
  assert.equal(resolve("hero", "npc_dota_hero_lion"), "/new/lion.webp");
  assert.equal(resolve("item", "item_blink"), "/new/blink.png");
  assert.equal(resolve("item", "item_recipe_blink"), "/new/recipe.png");
  assert.equal(resolve("ability", "lion_impale"), "/new/lion_impale.png");
  assert.equal(resolve("hero", "npc_dota_hero_axe"), null, "missing icon: null, never another hero");
  assert.equal(resolve("ability", "lion"), null, "kinds never cross");
  assert.equal(resolve("hero", null), null);
  assert.equal(createIconResolver({})("hero", "npc_dota_hero_lion"), null);
  assert.equal(createIconResolver(undefined)("item", "item_blink"), null);
  assert.equal(noIcons("hero", "npc_dota_hero_lion"), null);

  const view = card(liveSnapshot(), resolve);
  assert.equal(view.portraitSrc, "/new/lion.webp");
  assert.equal(view.items?.main[0]?.src, "/new/blink.png");
  assert.equal(view.items?.main[2]?.src, null, "no icon: name tile text instead");
  assert.equal(view.items?.main[2]?.text, tileText(view.items?.main[2]?.name ?? null));
  assert.equal(view.abilities?.hero[0]?.src, "/new/lion_impale.png");
  const enemy = compactCard(enemyHeroCards(memory, 5, "radiant")[0]!, createIconResolver({ "../assets/dota/1/heroes/sniper.png": "/s.png" }));
  assert.equal(enemy.portraitSrc, "/s.png");
  const unknownEnemy = compactCard(enemyHeroCards(memory, 5, "radiant")[2]!, resolve);
  assert.equal(unknownEnemy.portraitSrc, null);
});

test("the icon set is bound once through import.meta.glob and the card source holds no remote URL or web font", () => {
  const set = readFileSync(new URL("../src/live/dotaIconSet.ts", import.meta.url), "utf8");
  assert.match(set, /import\.meta\.glob\("\.\.\/assets\/dota\/\*\*\/\*\.\{png,webp\}", \{ eager: true, query: "\?url", import: "default" \}\)/);
  const sources = ["../src/live/HeroCard.tsx", "../src/live/heroCard.css", "../src/live/heroCardModel.ts", "../src/live/dotaIcons.ts"]
    .map(path => readFileSync(new URL(path, import.meta.url), "utf8")).join("\n");
  assert.doesNotMatch(sources, /https?:\/\/|@import|@font-face|fonts\.googleapis/);
  assert.doesNotMatch(sources, /Math\.random|invoke\(/);
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  // The .rc rules are the replay/Analytics reference card and carry the approved 12 px-floor exemption; Live keeps the floor.
  const liveCards = css.split("}").filter(rule => !/^\s*[^{}]*\.rc\b/.test(rule.slice(0, rule.indexOf("{")))).join("}");
  assert.doesNotMatch(liveCards, /font-size:\s*(?:[0-9]|1[01])px/, "no text below 12 px on the Live cards");
  assert.doesNotMatch(liveCards, /font:\s*(?:\d+\s+)?(?:[0-9]|1[01])px/, "no text below 12 px on the Live cards (shorthand)");
  assert.doesNotMatch(css, /overflow-x:\s*(?:auto|scroll)/, "no horizontal scrolling");
});
