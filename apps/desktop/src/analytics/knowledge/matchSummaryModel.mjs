import { matchResultPresentation } from "./presentationModel.mjs";

const sides = ["radiant", "dire"];
const statKeys = ["k", "d", "a", "nw", "lh", "dn", "gpm", "xpm", "heroDamage"];

function finiteMetric(source, key, reasonKey = `${key}_reason`) {
  const value = source?.[key];
  const reason = typeof source?.[reasonKey] === "string" ? source[reasonKey] : null;
  if (typeof value === "number" && Number.isFinite(value) && value >= 0) return { value, reason: null };
  return { value: null, reason: reason ?? "value is unavailable in the digest" };
}

function heroIdOf(value) {
  if (typeof value === "string" && value.length > 0) return value;
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  for (const key of ["id", "hero", "name", "item"]) {
    if (typeof value[key] === "string" && value[key].length > 0) return value[key];
  }
  return null;
}

function iconName(id, prefix) {
  if (typeof id !== "string") return null;
  const withoutPrefix = id.startsWith(prefix) ? id.slice(prefix.length) : id;
  return withoutPrefix.length > 0 ? withoutPrefix : null;
}

function purchaseTimes(items) {
  if (!Array.isArray(items)) return new Map();
  const times = new Map();
  for (const row of items) {
    if (!Array.isArray(row) || row.length < 2) continue;
    const seconds = row[0];
    const id = heroIdOf(row[1]);
    if (typeof seconds !== "number" || !Number.isFinite(seconds) || seconds < 0 || !id) continue;
    const bucket = times.get(id) ?? [];
    bucket.push(seconds);
    times.set(id, bucket);
  }
  return times;
}

function finalItems(hero) {
  if (!Array.isArray(hero?.final_items)) return [];
  const times = purchaseTimes(hero.items);
  return hero.final_items.flatMap((item) => {
    const id = heroIdOf(item);
    if (!id) return [];
    const bucket = times.get(id);
    const purchaseTimeSeconds = bucket?.length ? bucket.shift() : null;
    return [{
      id,
      iconName: iconName(id, "item_"),
      purchaseTimeSeconds,
      purchaseMinute: purchaseTimeSeconds === null ? null : Math.floor(purchaseTimeSeconds / 60),
    }];
  });
}

function laneResult(outcome, side) {
  if (outcome === "even") return "สูสี · เกณฑ์ชั่วคราว";
  const winner = typeof outcome === "string" ? /^(radiant|dire)_(won|stomp)$/.exec(outcome) : null;
  if (!winner) return null;
  const won = winner[1] === side;
  return `${won ? (winner[2] === "stomp" ? "ชนะขาด" : "ชนะเลน") : (winner[2] === "stomp" ? "แพ้ขาด" : "แพ้เลน")} · เกณฑ์ชั่วคราว`;
}

function heroMetrics(hero) {
  const final = hero?.final ?? {};
  const rates = hero?.rates ?? {};
  return {
    k: finiteMetric(final, "k"),
    d: finiteMetric(final, "d"),
    a: finiteMetric(final, "a"),
    nw: finiteMetric(final, "nw"),
    lh: finiteMetric(final, "lh"),
    dn: finiteMetric(final, "dn"),
    gpm: finiteMetric(rates, "gpm"),
    xpm: finiteMetric(rates, "xpm"),
    heroDamage: finiteMetric(hero, "hero_damage"),
  };
}

function heroCard(hero, laneRows) {
  const lane = typeof hero?.lane?.lane === "string" ? hero.lane.lane : null;
  const laneDigest = laneRows.find((row) => row?.lane === lane);
  const metrics = heroMetrics(hero);
  const goldEarned = finiteMetric(hero, "gold_earned");
  const economy = metrics.nw.value !== null ? metrics.nw : goldEarned;
  return {
    heroId: typeof hero?.id === "string" ? hero.id : null,
    heroIconName: iconName(hero?.id, "npc_dota_hero_"),
    hero: typeof hero?.hero === "string" ? hero.hero : null,
    side: hero?.side === "radiant" || hero?.side === "dire" ? hero.side : null,
    slot: Number.isInteger(hero?.slot) ? hero.slot : null,
    lane,
    laneReason: typeof hero?.lane?.reason === "string" ? hero.lane.reason : null,
    laneResult: laneResult(laneDigest?.outcome, hero?.side),
    laneResultReason: typeof laneDigest?.outcome_reason === "string"
      ? laneDigest.outcome_reason
      : laneDigest?.outcome == null ? "lane outcome is not available in the digest" : null,
    kda: { k: metrics.k, d: metrics.d, a: metrics.a },
    economyLabel: metrics.nw.value !== null ? "มูลค่าสุทธิ" : goldEarned.value !== null ? "ทองที่หาได้" : "มูลค่าสุทธิ",
    economy,
  };
}

function scoreboardRow(hero) {
  return {
    heroId: typeof hero?.id === "string" ? hero.id : null,
    heroIconName: iconName(hero?.id, "npc_dota_hero_"),
    hero: typeof hero?.hero === "string" ? hero.hero : null,
    slot: Number.isInteger(hero?.slot) ? hero.slot : null,
    metrics: heroMetrics(hero),
    items: finalItems(hero),
    itemsKnown: Array.isArray(hero?.final_items),
    itemsReason: typeof hero?.final_items_reason === "string" ? hero.final_items_reason : null,
  };
}

function teamTotal(rows) {
  const metrics = Object.fromEntries(statKeys.map((key) => {
    const values = rows.map((row) => row.metrics[key].value);
    if (rows.length !== 5) return [key, { value: null, reason: "team roster does not contain five observed heroes" }];
    if (values.some((value) => value === null)) return [key, { value: null, reason: "one or more hero values are unknown" }];
    return [key, { value: values.reduce((sum, value) => sum + value, 0), reason: null }];
  }));
  return { metrics, playerCount: rows.length };
}

function draftGroups(digest) {
  const raw = digest?.meta?.draft;
  const groups = { radiant: [], dire: [], unknown: [] };
  if (!Array.isArray(raw)) {
    return {
      state: "unknown",
      reason: typeof digest?.meta?.draft_reason === "string" ? digest.meta.draft_reason : "draft is not present in the digest",
      groups,
    };
  }
  const entries = raw.map((entry, index) => {
    const order = Number.isInteger(entry?.order) && entry.order >= 0 ? entry.order + 1 : index + 1;
    const hero = heroIdOf(entry?.hero);
    return {
      order,
      side: sides.includes(entry?.team) ? entry.team : "unknown",
      kind: entry?.kind === "pick" ? "Pick" : entry?.kind === "ban" ? "Ban" : "— ไม่ทราบ",
      heroId: hero,
      heroIconName: iconName(hero, "npc_dota_hero_"),
      hero: hero ? hero.replace(/^npc_dota_hero_/, "").split("_").map((part) => part ? part[0].toUpperCase() + part.slice(1) : "").join(" ") : "— ไม่ทราบ",
    };
  }).sort((left, right) => left.order - right.order);
  for (const entry of entries) groups[entry.side].push(entry);
  return { state: entries.length ? "available" : "empty", reason: null, groups };
}

/** Build data-only view models for the post-match summary without supplying missing values. */
export function buildMatchSummaryViewModel(digest) {
  const meta = digest?.meta ?? {};
  const result = matchResultPresentation(meta.result);
  const rawHeroes = Array.isArray(digest?.heroes) ? digest.heroes : [];
  const laneRows = Array.isArray(digest?.lanes) ? digest.lanes : [];
  const cards = { radiant: [], dire: [] };
  const scoreboard = {
    radiant: { rows: [], totals: null },
    dire: { rows: [], totals: null },
  };
  for (const side of sides) {
    const heroes = rawHeroes
      .filter((hero) => hero?.side === side)
      .sort((left, right) => (left?.slot ?? Number.MAX_SAFE_INTEGER) - (right?.slot ?? Number.MAX_SAFE_INTEGER));
    cards[side] = heroes.map((hero) => heroCard(hero, laneRows));
    scoreboard[side].rows = heroes.map(scoreboardRow);
    scoreboard[side].totals = teamTotal(scoreboard[side].rows);
  }
  const winner = result.text === "Radiant" ? "radiant" : result.text === "Dire" ? "dire" : null;
  const header = {
    winner,
    winnerText: winner === "radiant" ? "Radiant ชนะ" : winner === "dire" ? "Dire ชนะ" : "— ไม่ทราบ",
    winnerSource: result.source,
    warning: result.warning,
    kills: {
      radiant: finiteMetric(meta.kills, "radiant"),
      dire: finiteMetric(meta.kills, "dire"),
    },
    durationSeconds: typeof meta.duration_s === "number" && Number.isFinite(meta.duration_s) && meta.duration_s >= 0 ? meta.duration_s : null,
    gameMode: typeof meta.game_mode === "number" || typeof meta.game_mode === "string" ? meta.game_mode : null,
    gameModeReason: typeof meta.game_mode_reason === "string" ? meta.game_mode_reason : null,
    patch: typeof meta.patch === "string" && meta.patch.length > 0 ? meta.patch : null,
    patchReason: typeof meta.patch_reason === "string" ? meta.patch_reason : null,
  };
  return { header, cards, scoreboard, draft: draftGroups(digest) };
}

/** Convert a local world-coordinate building point with the replay viewer's calibration. */
export function buildingMapPoint(position, calibration) {
  if (!Array.isArray(position) || position.length < 2 || !position.slice(0, 2).every(Number.isFinite)) return null;
  if (!calibration || !Number.isFinite(calibration.size) || calibration.size <= 0
    || ![calibration.a, calibration.b, calibration.c, calibration.d].every(Number.isFinite)) return null;
  const x = ((calibration.a * position[0] + calibration.b) / calibration.size) * 100;
  const y = ((calibration.c * position[1] + calibration.d) / calibration.size) * 100;
  if (x < 0 || x > 100 || y < 0 || y > 100) return null;
  return { x, y };
}

