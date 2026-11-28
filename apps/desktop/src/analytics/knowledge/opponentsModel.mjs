const HERO_ID = /^npc_dota_hero_[a-z0-9_]+$/;

function record(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value : null;
}

function heroSide(hero) {
  if (hero?.side === "radiant" || hero?.side === "dire") return hero.side;
  if (hero?.team === 2) return "radiant";
  if (hero?.team === 3) return "dire";
  return null;
}

function heroName(hero, id) {
  const given = typeof hero?.name === "string" ? hero.name : typeof hero?.hero === "string" ? hero.hero : "";
  if (given && !given.startsWith("npc_dota_hero_")) return given;
  return id.replace(/^npc_dota_hero_/, "").split("_").filter(Boolean)
    .map(part => part[0]?.toUpperCase() + part.slice(1)).join(" ");
}

function completeGoldMap(opponentStats, fieldName, ownHeroId, expectedIds = null) {
  const field = record(opponentStats?.[fieldName]);
  const byPlayer = record(field?.by_player_hero_id);
  const completeByPlayer = record(field?.complete_by_player_hero_id);
  const values = typeof ownHeroId === "string" ? record(byPlayer?.[ownHeroId]) : null;
  const complete = completeByPlayer
    ? completeByPlayer[ownHeroId] === true
    : field?.complete === true;
  if (!complete || !values) return null;
  const keys = expectedIds ?? Object.keys(values);
  const result = new Map();
  for (const key of keys) {
    const value = values[key];
    if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) return null;
    result.set(key, value);
  }
  return result;
}

function completeOtherGold(opponentStats, ownHeroId) {
  const field = record(opponentStats?.gold_lost_by_killer);
  const completeByPlayer = record(field?.complete_by_player_hero_id);
  const complete = completeByPlayer
    ? completeByPlayer[ownHeroId] === true
    : field?.complete === true;
  const values = record(field?.other_by_player_hero_id);
  const gold = values?.[ownHeroId];
  return complete && typeof gold === "number" && Number.isSafeInteger(gold) && gold >= 0 ? gold : null;
}

function leadersByCount(counts, names) {
  if (counts === null) return { state: "unknown", names: [], heroIds: [], count: null };
  const max = Math.max(0, ...counts.values());
  if (max === 0) return { state: "none", names: [], heroIds: [], count: 0 };
  const ids = [...counts].filter(([, count]) => count === max).map(([id]) => id);
  const winners = ids.map(id => names.get(id) ?? id);
  return { state: "known", names: winners, heroIds: ids, count: max };
}

function leadersByGold(values, names) {
  if (values === null) return { state: "unknown", names: [], heroIds: [], campIds: [], gold: null };
  const max = Math.max(0, ...values.values());
  if (max === 0) return { state: "none", names: [], heroIds: [], campIds: [], gold: 0 };
  const ids = [...values].filter(([, gold]) => gold === max).map(([id]) => id);
  const winners = ids.map(id => names.get(id) ?? id);
  return { state: "known", names: winners, heroIds: ids, campIds: [], gold: max };
}

function leadersByZoneGold(values, names) {
  if (values === null || values.size === 0) {
    return { state: "unknown", names: [], heroIds: [], zoneIds: [], gold: null };
  }
  const max = Math.max(...values.values());
  if (max === 0) return { state: "none", names: [], heroIds: [], zoneIds: [], gold: 0 };
  const zoneIds = [...values].filter(([, gold]) => gold === max).map(([id]) => id);
  return {
    state: "known",
    names: zoneIds.map(id => names.get(id) ?? id),
    heroIds: [],
    zoneIds,
    gold: max,
  };
}

export function buildOpponentsViewModel({ digest = null, matchData = null, ownHeroId = null } = {}) {
  const source = record(matchData) ?? record(digest) ?? {};
  const heroes = Array.isArray(source.heroes) ? source.heroes : [];
  const roster = new Map(heroes.flatMap(hero => {
    if (typeof hero?.id !== "string" || !HERO_ID.test(hero.id)) return [];
    return [[hero.id, { id: hero.id, name: heroName(hero, hero.id), side: heroSide(hero), slot: Number.isInteger(hero.slot) ? hero.slot : Number.isInteger(hero.pid) ? hero.pid : Number.MAX_SAFE_INTEGER }]];
  }));
  const own = typeof ownHeroId === "string" ? roster.get(ownHeroId) : null;
  const enemies = own?.side ? [...roster.values()].filter(hero => hero.side && hero.side !== own.side).sort((a, b) => a.slot - b.slot || a.id.localeCompare(b.id)) : [];
  const names = new Map(enemies.map(hero => [hero.id, hero.name]));
  const enemyIds = enemies.map(hero => hero.id);
  const rawDeaths = Array.isArray(source.deaths) && source.deaths_reason == null ? source.deaths : null;
  const killCounts = rawDeaths === null || !own ? null : new Map(enemyIds.map(id => [id, 0]));
  const deathCounts = rawDeaths === null || !own ? null : new Map(enemyIds.map(id => [id, 0]));
  let otherDeaths = rawDeaths === null || !own ? null : 0;
  let unknownKillerDeaths = rawDeaths === null || !own ? null : 0;
  if (rawDeaths && own && killCounts && deathCounts) {
    for (const death of rawDeaths) {
      const victim = typeof death?.hero === "string" ? death.hero : null;
      const killer = typeof death?.killer === "string" ? death.killer : null;
      const victimHero = victim ? roster.get(victim) : null;
      const killerHero = killer ? roster.get(killer) : null;
      if (killer === own.id && victimHero?.side && victimHero.side !== own.side) {
        killCounts.set(victimHero.id, (killCounts.get(victimHero.id) ?? 0) + 1);
      }
      if (victim === own.id) {
        if (killerHero?.side && killerHero.side !== own.side) {
          deathCounts.set(killerHero.id, (deathCounts.get(killerHero.id) ?? 0) + 1);
        } else if (killerHero?.side === own.side || death.killer_kind === "other" || (killer && !HERO_ID.test(killer))) {
          otherDeaths++;
        } else {
          unknownKillerDeaths++;
        }
      }
    }
  }
  const opponentStats = record(record(digest)?.opponent_stats);
  const killGold = own && enemyIds.length ? completeGoldMap(opponentStats, "kill_gold_by_victim", own.id, enemyIds) : null;
  const goldLost = own && enemyIds.length ? completeGoldMap(opponentStats, "gold_lost_by_killer", own.id, enemyIds) : null;
  const otherGoldLost = own ? completeOtherGold(opponentStats, own.id) : null;
  const jungleGoldField = record(opponentStats?.neutral_kill_gold_by_camp);
  const jungleGoldRaw = own ? completeGoldMap(opponentStats, "neutral_kill_gold_by_camp", own.id) : null;
  const jungleGold = jungleGoldRaw?.size ? jungleGoldRaw : null;
  const zoneNames = new Map(Object.entries(record(jungleGoldField?.zone_names) ?? {}));
  const unattachedByHero = record(jungleGoldField?.unattached_by_player_hero_id);
  const jungleUnattached = own && Number.isSafeInteger(unattachedByHero?.[own.id]) && unattachedByHero[own.id] >= 0
    ? unattachedByHero[own.id]
    : null;
  const mostKills = leadersByCount(killCounts, names);
  const mostDeaths = leadersByCount(deathCounts, names);
  const mostKillGold = leadersByGold(killGold, names);
  const mostGoldLost = leadersByGold(goldLost, names);
  const mostJungleGold = leadersByZoneGold(jungleGold, zoneNames);
  return {
    state: own && enemies.length ? "available" : "unknown",
    reason: !own ? "own_hero_unknown" : !own.side ? "own_side_unknown" : enemies.length ? null : "enemy_team_unknown",
    ownHeroId: own?.id ?? null,
    enemyRows: enemies.map(hero => ({
      heroId: hero.id,
      hero: hero.name,
      kills: killCounts?.get(hero.id) ?? null,
      deaths: deathCounts?.get(hero.id) ?? null,
      killGold: killGold?.get(hero.id) ?? null,
      goldLost: goldLost?.get(hero.id) ?? null,
    })),
    otherDeaths,
    otherGoldLost,
    unknownKillerDeaths,
    jungleUnattached,
    headlines: {
      mostKills,
      mostKillGold,
      mostDeaths,
      mostGoldLost,
      mostJungleGold,
    },
  };
}
