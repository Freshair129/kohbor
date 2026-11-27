import assert from "node:assert/strict";
import test from "node:test";
import { buildingMapPoint, buildMatchSummaryViewModel } from "../src/analytics/knowledge/matchSummaryModel.mjs";
import { syntheticMatchSummaryDigest } from "../src/mock/debriefFixture.mjs";

// Synthetic fixture only; it does not represent a live match or acceptance evidence.

test("builds the complete summary header, hero cards, scoreboards, totals, item timings and draft groups", () => {
  const view = buildMatchSummaryViewModel(syntheticMatchSummaryDigest());

  assert.deepEqual(view.header, {
    winner: "radiant",
    winnerText: "Radiant ชนะ",
    winnerSource: "Replay engine",
    warning: null,
    kills: {
      radiant: { value: 41, reason: null },
      dire: { value: 39, reason: null },
    },
    durationSeconds: 2401,
    gameMode: 23,
    gameModeReason: null,
    patch: "7.40",
    patchReason: null,
  });
  assert.equal(view.cards.radiant[0].heroId, "npc_dota_hero_axe");
  assert.equal(view.cards.radiant[0].laneResult, "ชนะเลน · เกณฑ์ชั่วคราว");
  assert.equal(view.cards.radiant[0].economyLabel, "มูลค่าสุทธิ");
  assert.equal(view.cards.radiant[0].economy.value, 1000);
  assert.equal(view.cards.radiant[2].laneResult, null);
  assert.equal(view.cards.radiant[2].laneResultReason, "match ended before 10:00");

  const radiant = view.scoreboard.radiant;
  assert.equal(radiant.rows.length, 5);
  assert.deepEqual(radiant.rows[0].metrics.k, { value: 1, reason: null });
  assert.deepEqual(radiant.rows[0].metrics.lh, { value: 10, reason: null });
  assert.deepEqual(radiant.rows[0].metrics.dn, { value: 1, reason: null });
  assert.equal(radiant.rows[0].metrics.gpm.value, 10);
  assert.equal(radiant.rows[0].metrics.xpm.value, 20);
  assert.equal(radiant.rows[0].metrics.heroDamage.value, 3000);
  assert.deepEqual(radiant.rows[0].items[0], {
    id: "item_blink",
    iconName: "blink",
    purchaseTimeSeconds: 900,
    purchaseMinute: 15,
  });
  assert.equal(radiant.totals.metrics.k.value, 15);
  assert.equal(radiant.totals.metrics.d.value, 20);
  assert.equal(radiant.totals.metrics.nw.value, 15000);
  assert.equal(radiant.totals.metrics.gpm.value, 150);
  assert.equal(radiant.totals.metrics.xpm.value, 300);
  assert.equal(radiant.totals.metrics.heroDamage.value, 45000);
  assert.deepEqual(view.draft.groups.radiant.map((entry) => [entry.order, entry.kind, entry.heroId]), [
    [1, "Pick", "npc_dota_hero_axe"],
    [3, "Ban", "npc_dota_hero_earthshaker"],
  ]);
  assert.deepEqual(view.draft.groups.dire.map((entry) => [entry.order, entry.kind]), [[2, "Ban"]]);
});

test("keeps missing assists unknown with their reason and refuses an incomplete team total", () => {
  const digest = syntheticMatchSummaryDigest();
  digest.heroes[0].final.a = null;
  digest.heroes[0].final.a_reason = "exact assists are not in the bundle";

  const view = buildMatchSummaryViewModel(digest);

  assert.deepEqual(view.scoreboard.radiant.rows[0].metrics.a, {
    value: null,
    reason: "exact assists are not in the bundle",
  });
  assert.equal(view.scoreboard.radiant.totals.metrics.a.value, null);
  assert.equal(view.scoreboard.radiant.totals.metrics.a.reason, "one or more hero values are unknown");
});

test("does not invent a zero purchase minute when a final item has no matching timing", () => {
  const digest = syntheticMatchSummaryDigest();
  digest.heroes[0].items = [];
  digest.heroes[0].final_items = ["item_shadow_blade", null, null, null, null, null];

  const item = buildMatchSummaryViewModel(digest).scoreboard.radiant.rows[0].items[0];

  assert.equal(item.id, "item_shadow_blade");
  assert.equal(item.iconName, "shadow_blade");
  assert.equal(item.purchaseTimeSeconds, null);
  assert.equal(item.purchaseMinute, null);
});

test("keeps an explicitly empty draft distinct from an unknown draft", () => {
  const digest = syntheticMatchSummaryDigest();
  digest.meta.draft = [];

  const draft = buildMatchSummaryViewModel(digest).draft;

  assert.equal(draft.state, "empty");
  assert.deepEqual(draft.groups, { radiant: [], dire: [], unknown: [] });
  assert.equal(draft.reason, null);
});

test("shows an unknown winner without turning it into a loss", () => {
  const digest = syntheticMatchSummaryDigest();
  digest.meta.result = { winner: null, source: null, conflict: false, reason: "winner not in replay" };

  const header = buildMatchSummaryViewModel(digest).header;

  assert.equal(header.winner, null);
  assert.equal(header.winnerText, "— ไม่ทราบ");
  assert.equal(header.warning, null);
});

test("keeps exactly the ten observed heroes in their two five-player card and scoreboard groups", () => {
  const view = buildMatchSummaryViewModel(syntheticMatchSummaryDigest());

  assert.equal(view.cards.radiant.length, 5);
  assert.equal(view.cards.dire.length, 5);
  assert.equal(view.scoreboard.radiant.rows.length, 5);
  assert.equal(view.scoreboard.dire.rows.length, 5);
  assert.equal([...view.cards.radiant, ...view.cards.dire].length, 10);
  assert.deepEqual(view.cards.dire.map((hero) => hero.slot), [0, 1, 2, 3, 4]);
});

test("labels earned gold accurately when net worth is unavailable", () => {
  const digest = syntheticMatchSummaryDigest();
  digest.heroes[0].final.nw = null;
  digest.heroes[0].final.nw_reason = "net-worth samples are missing";

  const view = buildMatchSummaryViewModel(digest);

  assert.equal(view.cards.radiant[0].economyLabel, "ทองที่หาได้");
  assert.equal(view.cards.radiant[0].economy.value, 2000);
  assert.deepEqual(view.scoreboard.radiant.rows[0].metrics.nw, {
    value: null,
    reason: "net-worth samples are missing",
  });
});

test("projects local building positions only when the supplied replay calibration maps inside the minimap", () => {
  assert.deepEqual(buildingMapPoint([25, 75], { size: 100, a: 1, b: 0, c: 1, d: 0 }), { x: 25, y: 75 });
  assert.equal(buildingMapPoint([120, 75], { size: 100, a: 1, b: 0, c: 1, d: 0 }), null);
  assert.equal(buildingMapPoint([25, 75], null), null);
});
