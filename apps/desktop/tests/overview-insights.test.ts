import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  aggregateNemesis, buildLocalHeroStyleTags, localDayRange, parseNemesisReply, todayBestHero,
} from "../src/overview/overviewModel.ts";

const ownHero = "npc_dota_hero_lion";
const opponent = (hero: string) => ({ hero, name: "Player name must never be projected", account_id: 987654321 });
function laneMatch(lane: "top" | "mid" | "bot", hero: string, outcome: string, ownSide: "radiant" | "dire" = "radiant") {
  return {
    own_hero: ownHero,
    lanes: [{
      lane,
      radiant: ownSide === "radiant" ? [ownHero] : [opponent(hero)],
      dire: ownSide === "dire" ? [ownHero] : [opponent(hero)],
      outcome,
    }],
  };
}

function localSeconds(day: number, hour: number, minute = 0) {
  return Math.floor(new Date(2026, 10, day, hour, minute).getTime() / 1000);
}

function historyEntry(hero: string, endedAt: number, result: "win" | "loss" | null = "win", extra = {}) {
  return {
    matchKey: `${hero}-${endedAt}`,
    source: "kohbor" as const,
    hero,
    result,
    end: result === null ? "abandoned" as const : "completed" as const,
    kills: 4,
    deaths: 2,
    assists: 5,
    durationS: 1800,
    endedAt,
    matchType: "unknown" as const,
    ...extra,
  };
}

test("today uses local midnight boundaries and never includes the next local day", () => {
  const now = new Date(2026, 10, 25, 0, 5);
  const range = localDayRange(now);
  assert.deepEqual(range, {
    start: Math.floor(new Date(2026, 10, 25).getTime() / 1000),
    end: Math.floor(new Date(2026, 10, 26).getTime() / 1000),
  });

  const best = todayBestHero([
    historyEntry("npc_dota_hero_axe", range.start - 1),
    historyEntry("npc_dota_hero_lion", range.start),
    historyEntry("npc_dota_hero_lion", range.end - 1, "loss"),
    historyEntry("npc_dota_hero_axe", range.end),
  ], now, 3);
  assert.equal(best.hero?.hero, "npc_dota_hero_lion");
  assert.equal(best.hero?.games, 2);
  assert.equal(best.hero?.basis, "most_played", "fewer than three counted matches use the existing fallback");
});

test("best hero today is unknown with a reason when the local calendar day has no matches", () => {
  const best = todayBestHero([historyEntry("npc_dota_hero_axe", localSeconds(24, 23, 59))], new Date(2026, 10, 25, 12), 3);
  assert.equal(best.hero, null);
  assert.equal(best.reason, "no_matches_today");
});

test("best hero today uses the shared three-counted-game win-rate threshold", () => {
  const now = new Date(2026, 10, 25, 12);
  const start = localDayRange(now).start;
  const best = todayBestHero([
    historyEntry("npc_dota_hero_axe", start, "win"),
    historyEntry("npc_dota_hero_axe", start + 10, "win"),
    historyEntry("npc_dota_hero_axe", start + 20, "loss"),
    historyEntry("npc_dota_hero_lion", start + 30, "win"),
  ], now, 3);
  assert.equal(best.hero?.hero, "npc_dota_hero_axe");
  assert.equal(best.hero?.basis, "win_rate");
  assert.equal(best.hero?.countedGames, 3);
  assert.equal(best.minGames, 3);
});

test("nemesis needs three known lane samples and reports faced, lost, and sample size", () => {
  const reply = { schema_version: "1.0.0", matches: [
    laneMatch("top", "npc_dota_hero_sniper", "dire_won"),
    laneMatch("mid", "npc_dota_hero_sniper", "dire_stomp"),
    laneMatch("bot", "npc_dota_hero_sniper", "dire_won"),
    laneMatch("top", "npc_dota_hero_drow_ranger", "dire_won"),
    laneMatch("mid", "npc_dota_hero_drow_ranger", "dire_won"),
    laneMatch("bot", "npc_dota_hero_axe", "radiant_won"),
    laneMatch("top", "npc_dota_hero_axe", "radiant_won"),
    laneMatch("mid", "npc_dota_hero_axe", "radiant_won"),
  ] };
  const parsed = parseNemesisReply(reply);
  assert.ok(parsed);
  assert.doesNotMatch(JSON.stringify(parsed), /Player name|account_id|987654321/);
  const model = aggregateNemesis(parsed, {});
  assert.deepEqual(model.rows.map(row => [row.hero, row.lost, row.faced, row.sampleSize]), [
    ["npc_dota_hero_sniper", 3, 3, 3],
    ["npc_dota_hero_axe", 0, 3, 3],
  ]);
  assert.equal(model.minLanes, 3);
  assert.equal(model.lowData, true, "heroes below the ranking minimum are disclosed");
});

test("nemesis ties use a stable hero-id order and unknown lane results do not count", () => {
  const parsed = parseNemesisReply({ schema_version: "1.0.0", matches: [
    laneMatch("top", "npc_dota_hero_luna", "dire_won"),
    laneMatch("mid", "npc_dota_hero_luna", "dire_won"),
    laneMatch("bot", "npc_dota_hero_luna", "dire_won"),
    laneMatch("top", "npc_dota_hero_queenofpain", "dire_won"),
    laneMatch("mid", "npc_dota_hero_queenofpain", "dire_won"),
    laneMatch("bot", "npc_dota_hero_queenofpain", "dire_won"),
    laneMatch("top", "npc_dota_hero_axe", "not_a_lane_result"),
  ] });
  assert.ok(parsed);
  const model = aggregateNemesis(parsed, {});
  assert.deepEqual(model.rows.map(row => row.hero), ["npc_dota_hero_luna", "npc_dota_hero_queenofpain"]);
  assert.ok(model.rows.every(row => row.lost === 3 && row.faced === 3));
});

test("nemesis with no local parsed lane data returns no ranked heroes", () => {
  const parsed = parseNemesisReply({ schema_version: "1.0.0", matches: [] });
  assert.ok(parsed);
  const model = aggregateNemesis(parsed, {});
  assert.deepEqual(model.rows, []);
  assert.equal(model.totalSamples, 0);
  assert.equal(model.lowData, true);
  assert.deepEqual(model.styles, []);
});

test("local style tags use only attack type and primary attribute values in the bundled hero stats pack", () => {
  const tags = buildLocalHeroStyleTags({
    sniper: { attack: { type: "ranged" }, primaryAttribute: "agility", roles: ["invented"] },
    axe: { attack: { type: "melee" }, primaryAttribute: "strength" },
    unknown: { attack: { type: "siege" }, primaryAttribute: "unknown" },
  });
  assert.deepEqual(tags.npc_dota_hero_sniper, ["attack:ranged", "attribute:agility"]);
  assert.deepEqual(tags.npc_dota_hero_axe, ["attack:melee", "attribute:strength"]);
  assert.equal(tags.npc_dota_hero_unknown, undefined);

  const parsed = parseNemesisReply({ schema_version: "1.0.0", matches: [
    laneMatch("top", "npc_dota_hero_sniper", "dire_won"),
    laneMatch("mid", "npc_dota_hero_sniper", "dire_won"),
    laneMatch("bot", "npc_dota_hero_sniper", "dire_won"),
  ] });
  assert.ok(parsed);
  const styles = aggregateNemesis(parsed, tags).styles;
  assert.ok(styles.some(style => style.tag === "attack:ranged" && style.losses === 3 && style.samples === 3));
  assert.ok(styles.some(style => style.tag === "attribute:agility" && style.losses === 3 && style.samples === 3));
});

test("recent match list scrolls locally and show-all opens the full list without page scrolling", () => {
  const css = readFileSync(new URL("../src/overview/overview.css", import.meta.url), "utf8");
  const page = readFileSync(new URL("../src/overview/OverviewPage.tsx", import.meta.url), "utf8");
  assert.match(css, /\.ov-table-wrap\s*\{[^}]*max-height:[^;]+;[^}]*overflow(?:-y)?:\s*auto/s);
  assert.match(page, /<dialog[^>]*className="ov-history-dialog"/);
  assert.match(page, /\.showModal\(\)/);
  assert.match(page, /aria-expanded=\{showAll\}/);
  assert.doesNotMatch(css, /\.ov-page\s*\{[^}]*overflow-y:\s*(?:auto|scroll)/s);
});

test("Live HP and mana bars use two full-width stacked rows in every field-card mode", () => {
  const css = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const component = readFileSync(new URL("../src/live/HeroCard.tsx", import.meta.url), "utf8");
  assert.match(component, /<Bar kind="hp"[\s\S]*?<Bar kind="mp"/);
  const bars = css.split("}").filter(rule => /\.lhc-bars/.test(rule.slice(0, rule.indexOf("{")))).join("}");
  assert.match(bars, /grid-template-columns:\s*minmax\(0,\s*1fr\)/);
  assert.doesNotMatch(bars, /grid-template-columns:\s*repeat\(2/);
  assert.match(css, /\.lhc-bar-num\s*\{[^}]*font:\s*600\s+12px\/1/s);
  assert.match(css, /\.lhc-field-card\.lhc-field-own\.is-own-two-row-22\s*\{[^}]*grid-template-rows:\s*16px\s+24px\s+18px\s+18px/s);
  assert.match(css, /\.lhc-field-card\.is-compact-fit\s*\{[^}]*grid-template-rows:\s*14px\s+24px\s+18px\s+18px/s);
  assert.match(css, /\.lhc-field-card\.is-roomy-field\s*\{[^}]*grid-template-rows:\s*28px\s+14px\s+25px/s);
  assert.match(css, /\.sm-host\s+\.sm-card-row\s+\.lhc-field-card\.is-roomy-field\s*\{[^}]*grid-template-rows:\s*24px\s+12px\s+25px\s+24px\s+27px/s);
});
