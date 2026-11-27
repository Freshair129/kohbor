// Synthetic UI-023 sample fixture shared by the match-summary model tests and the mock harness.
function syntheticHero(side, index) {
  const stat = side === "radiant" ? index + 1 : index + 6;
  const name = ["axe", "crystal_maiden", "drow_ranger", "earthshaker", "lich", "lion", "luna", "pudge", "sniper", "zeus"][stat - 1];
  return {
    id: `npc_dota_hero_${name}`,
    hero: name,
    side,
    slot: index,
    lane: { lane: index === 0 ? "top" : index === 1 ? "mid" : "bot" },
    final: {
      k: stat,
      d: stat + 1,
      a: stat + 2,
      nw: stat * 1000,
      lh: stat * 10,
      dn: stat,
      xp: stat * 10000,
    },
    rates: { gpm: stat * 10, xpm: stat * 20 },
    gold_earned: stat * 2000,
    hero_damage: stat * 3000,
    items: [[900, "item_blink"], [1200, "item_black_king_bar"]],
    final_items: ["item_blink", "item_black_king_bar", null, null, null, null],
  };
}

export function syntheticMatchSummaryDigest() {
  return {
    meta: {
      result: { winner: "radiant", source: "engine", conflict: false },
      kills: { radiant: 41, dire: 39 },
      duration_s: 2401,
      game_mode: 23,
      game_mode_reason: null,
      patch: "7.40",
      patch_reason: null,
      draft: [
        { order: 0, team: "radiant", kind: "pick", hero: "npc_dota_hero_axe" },
        { order: 1, team: "dire", kind: "ban", hero: "npc_dota_hero_lion" },
        { order: 2, team: "radiant", kind: "ban", hero: "npc_dota_hero_earthshaker" },
      ],
    },
    heroes: [
      ...Array.from({ length: 5 }, (_, index) => syntheticHero("radiant", index)),
      ...Array.from({ length: 5 }, (_, index) => syntheticHero("dire", index)),
    ],
    lanes: [
      { lane: "top", outcome: "radiant_won" },
      { lane: "mid", outcome: "even" },
      { lane: "bot", outcome: null, outcome_reason: "match ended before 10:00" },
    ],
    advantage_per_min: [
      { minute: 0, radiant_nw_diff: 0, radiant_xp_diff: 0 },
      { minute: 10, radiant_nw_diff: 1200, radiant_xp_diff: 850 },
      { minute: 20, radiant_nw_diff: -400, radiant_xp_diff: 300 },
      { minute: 30, radiant_nw_diff: 2400, radiant_xp_diff: 1800 },
      { minute: 40, radiant_nw_diff: 3900, radiant_xp_diff: 2700 },
    ],
    deaths: [
      { event_ref: "synthetic-kill-1", time_s: 185, killer: "npc_dota_hero_axe", victim: "npc_dota_hero_lion" },
      { event_ref: "synthetic-kill-2", time_s: 615, killer: "npc_dota_hero_pudge", victim: "npc_dota_hero_zeus" },
      { event_ref: "synthetic-kill-3", time_s: 1320, killer: "npc_dota_hero_drow_ranger", victim: "npc_dota_hero_luna" },
    ],
    objectives: {
      buildings: [
        { name: "Radiant mid tower", team: "dire", destroyed: true, destroyed_at_s: 920 },
        { name: "Dire bottom barracks", team: "radiant", destroyed: true, destroyed_at_s: 1880 },
        { name: "Ancient", team: "dire", destroyed: null, destroyed_reason: "synthetic sample leaves health unknown" },
      ],
      wards: [{ name: "Observer ward", team: "radiant", start_s: 730 }],
      other_tracks: [{ name: "Roshan", team: "radiant", t: 1510 }],
    },
    teamfights: [
      { n: "synthetic-fight-1", t: [615, 664], deaths: [{ time_s: 620 }, { time_s: 638 }] },
      { n: "synthetic-fight-2", t: [1320, 1384], deaths: [{ time_s: 1322 }, { time_s: 1350 }, { time_s: 1371 }] },
    ],
  };
}
