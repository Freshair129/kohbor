import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  allyHeroCards,
  enemyHeroCards,
  ownDraftSlot,
  ownHeroCard,
  ownSkillSquares,
  type HeroCard,
  // @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
} from "../src/live/secondMonitorCards.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { createDraftRosterMemory, rememberDraftFrame } from "../src/live/secondMonitorModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { DraftBarFrame } from "../src/deck/draftBarModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { AbilityTile } from "../src/deck/loadoutModel.ts";

const ROSTER = [
  "npc_dota_hero_silencer", "npc_dota_hero_axe", "npc_dota_hero_lion", "npc_dota_hero_juggernaut", "npc_dota_hero_earthshaker",
  "npc_dota_hero_sniper", "npc_dota_hero_pudge", "npc_dota_hero_phantom_assassin", "npc_dota_hero_crystal_maiden", null,
];

function draftFrame(ids: (string | null)[]): DraftBarFrame {
  return {
    active: true, detail: null, bgra: null,
    slots: ids.map((heroId, slot) => ({
      slot, team: slot < 5 ? "radiant" : "dire", hero_id: heroId,
      confidence: heroId ? 0.9 : 0, reason: heroId ? "identified" : "low_confidence",
    })),
  } as DraftBarFrame;
}

const memory = rememberDraftFrame(createDraftRosterMemory(), 7, draftFrame(ROSTER));

function tile(slot: number, ultimate: boolean | null, status: AbilityTile["status"], cooldown: number | null = null): AbilityTile {
  return { slot, name: `a${slot}`, rawId: `a${slot}`, level: 1, pips: null, ultimate, status, cooldown, canCast: status === "ready" };
}

function noVitals(card: HeroCard) {
  return card.hpPercent === null && card.manaPercent === null && card.vitalsSource === null;
}

test("every card has a portrait id, a name, HP and mana and exactly four skill squares with the ult last", () => {
  const cards = [
    ownHeroCard({ heroId: "npc_dota_hero_lion", hpPercent: 60, manaPercent: 30, tiles: null }),
    ...allyHeroCards({ memory, epoch: 7, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion" }),
    ...enemyHeroCards(memory, 7, "radiant"),
  ];
  assert.equal(cards.length, 10);
  for (const card of cards) {
    assert.equal(card.skills.length, 4);
    assert.deepEqual(card.skills.map(skill => skill.ultimate), [false, false, false, true]);
  }
});

test("own card: name, HP and mana from GSI; skills from GSI ability tiles only", () => {
  const card = ownHeroCard({
    heroId: "npc_dota_hero_lion", hpPercent: 62.4, manaPercent: 140,
    tiles: [tile(0, false, "ready"), tile(1, false, "cooldown", 7.5), tile(2, false, "unknown"), tile(5, true, "cooldown", 48)],
  });
  assert.equal(card.heroName, "Lion");
  assert.equal(card.nameSource, "gsi");
  assert.equal(card.hpPercent, 62.4);
  assert.equal(card.manaPercent, 100, "clamped, never above 100");
  assert.equal(card.vitalsSource, "gsi");
  assert.deepEqual(card.skills.map(skill => [skill.state, skill.seconds, skill.source]), [
    ["ready", null, "gsi"], ["cooldown", 7.5, "gsi"], ["unknown", null, null], ["cooldown", 48, "gsi"],
  ]);
  const empty = ownHeroCard({ heroId: null, hpPercent: null, manaPercent: null, tiles: null });
  assert.equal(empty.heroName, null);
  assert.ok(noVitals(empty));
  assert.ok(empty.skills.every(skill => skill.state === "unknown" && skill.seconds === null && skill.source === null));
  assert.deepEqual(ownSkillSquares([tile(0, null, "ready")]).map(skill => skill.state), ["unknown", "unknown", "unknown", "unknown"],
    "an ability with unknown ult flag is not placed");
});

test("allies without peer sharing: HP, mana and three skills unknown; the ult square shows only the top-bar dot", () => {
  const unknown = allyHeroCards({ memory, epoch: 7, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion" });
  assert.deepEqual(unknown.map(card => card.heroName), ["Silencer", "Axe", "Juggernaut", "Earthshaker"]);
  assert.deepEqual(unknown.map(card => card.slot), [0, 1, 3, 4]);
  assert.ok(unknown.every(noVitals));
  assert.ok(unknown.every(card => card.skills.every(skill => skill.state === "unknown" && skill.source === null)),
    "no top-bar input: the ult stays unknown");

  const dots = allyHeroCards({
    memory, epoch: 7, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion",
    topbar: { ally_ult_state: { 0: "ready", 1: "ready_no_mana", 2: "ready", 3: "not_ready" } },
  });
  assert.deepEqual(dots.map(card => card.skills[3].state), ["ready", "ready_no_mana", "not_ready", "unknown"],
    "keyed by draft slot; the own slot 2 is skipped; slot 4 has no dot");
  assert.ok(dots.every(card => card.skills[3].seconds === null), "the dot never carries a number");
  assert.ok(dots.every(card => card.skills.slice(0, 3).every(skill => skill.state === "unknown")));
  assert.ok(dots.every(noVitals), "the top bar never gives ally HP or mana");
  assert.equal(dots[0].skills[3].source, "topbar");
  const bogus = allyHeroCards({ memory, epoch: 7, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion",
    topbar: { ally_ult_state: { 0: "stunned" as never } } });
  assert.equal(bogus[0].skills[3].state, "unknown", "an unrecognised dot value is unknown");
});

test("allies with peer sharing: HP, mana and cooldowns from the peer; the peer ult wins over the dot", () => {
  const cards = allyHeroCards({
    memory, epoch: 7, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion",
    peer: { 0: { hpPercent: 80, manaPercent: 45, skills: [{ cooldown: 3, ready: false }, { cooldown: null, ready: true }, null], ultimate: { cooldown: 20, ready: false } } },
    topbar: { ally_ult_state: { 0: "ready", 1: "not_ready" } },
  });
  const [silencer, axe] = cards;
  assert.equal(silencer.hpPercent, 80);
  assert.equal(silencer.vitalsSource, "peer");
  assert.deepEqual(silencer.skills.map(skill => [skill.state, skill.seconds, skill.source]), [
    ["cooldown", 3, "peer"], ["ready", null, "peer"], ["unknown", null, null], ["cooldown", 20, "peer"],
  ]);
  assert.ok(noVitals(axe));
  assert.deepEqual([axe.skills[3].state, axe.skills[3].source], ["not_ready", "topbar"]);
});

test("unmatched own hero: four unknown ally cards and no dot bound to a guessed ally", () => {
  for (const cards of [
    allyHeroCards({ memory, epoch: 7, ownTeam: "radiant", ownHeroId: "npc_dota_hero_tiny", topbar: { ally_ult_state: { 0: "ready" } } }),
    allyHeroCards({ memory, epoch: 7, ownTeam: null, ownHeroId: "npc_dota_hero_lion", topbar: { ally_ult_state: { 0: "ready" } } }),
    allyHeroCards({ memory, epoch: 8, ownTeam: "radiant", ownHeroId: "npc_dota_hero_lion", topbar: { ally_ult_state: { 0: "ready" } } }),
  ]) {
    assert.equal(cards.length, 4);
    assert.ok(cards.every(card => card.slot === null && card.heroName === null && noVitals(card)));
    assert.ok(cards.every(card => card.skills.every(skill => skill.state === "unknown")));
  }
  assert.equal(ownDraftSlot(memory, 7, "radiant", "npc_dota_hero_lion"), 2);
  assert.equal(ownDraftSlot(memory, 7, null, "npc_dota_hero_lion"), 2, "a unique observed slot can locate the GSI hero without guessing the team");
  assert.equal(ownDraftSlot(memory, 8, "radiant", "npc_dota_hero_lion"), null);
  const duplicate = { ...memory, slots: [...memory.slots] };
  duplicate.slots[4] = "npc_dota_hero_lion";
  assert.equal(ownDraftSlot(duplicate, 7, null, "npc_dota_hero_lion"), null, "ambiguous slot identity is not assigned");
});

test("enemy cards: identity from same-session draft only; HP, mana and skills unknown", () => {
  const cards = enemyHeroCards(memory, 7, "radiant");
  assert.deepEqual(cards.map(card => card.heroName), ["Sniper", "Pudge", "Phantom Assassin", "Crystal Maiden", null]);
  assert.deepEqual(cards.map(card => card.key), ["E1", "E2", "E3", "E4", "E5"]);
  assert.ok(cards.every(card => card.side === "enemy" && noVitals(card)));
  assert.ok(cards.every(card => card.skills.every(skill => skill.state === "unknown" && skill.source === null)));
  assert.equal(cards[4].heroId, null);
  assert.ok(enemyHeroCards(memory, 7, null).every(card => card.heroName === null && card.slot === null), "unknown team: no side guessed");
  assert.ok(enemyHeroCards(memory, 8, "radiant").every(card => card.heroName === null), "other session: nothing carried over");
});

test("card sources: unknown renders an em dash, enemies are marked, no screen reading on this branch", () => {
  const deck = readFileSync(new URL("../src/live/SecondMonitorDeck.tsx", import.meta.url), "utf8");
  const model = readFileSync(new URL("../src/live/secondMonitorCards.ts", import.meta.url), "utf8");
  const cardView = readFileSync(new URL("../src/live/heroCardModel.ts", import.meta.url), "utf8");
  const cardComponent = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  assert.match(deck, /CompactHeroCards cards=\{cards\}[^>]*fieldCard/);
  assert.match(deck, /orderBySlot\(\[ownCard, \.\.\.allies\]\)/);
  assert.match(cardView, /export const UNKNOWN = "—"/);
  assert.match(cardView, /value === null \? UNKNOWN/);
  assert.match(cardComponent, /view\.heroName \?\? UNKNOWN/);
  assert.match(cardView, /hpText: percentText\(view\.hpPercent\)/);
  assert.match(cardView, /manaText: percentText\(view\.manaPercent\)/);
  assert.match(cardComponent, /value=\{view\.hpText\}/);
  assert.match(cardComponent, /value=\{view\.manaText\}/);
  assert.match(deck, /data-testid=\{`sm-row-\$\{side\}`\}/);
  assert.match(deck, /side === "ally" \? labels\.ourTeam : labels\.enemyTeam/);
  assert.match(deck, /topbar = null, peer = null/);
  assert.doesNotMatch(model + deck, /invoke\(|getDisplayMedia|captureScreen|Math\.random/);
  const css = readFileSync(new URL("../src/live/secondMonitor.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.doesNotMatch(css, /font-size:\s*(?:[0-9]|1[01])px/, "no text below 12 px");
});
