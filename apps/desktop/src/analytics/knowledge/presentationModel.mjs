// Pure presentation helpers for Analytics knowledge panels.
const laneOrder = ["top", "mid", "bot", "unknown"];

/** Present the resolved match result without guessing when its sources conflict or are absent. */
export function matchResultPresentation(result) {
  const sources = {
    engine: "Replay engine",
    gsi_history: "Local GSI history",
    opendota_history: "OpenDota history",
  };
  const winner = result?.winner === "radiant" ? "Radiant" : result?.winner === "dire" ? "Dire" : null;
  const conflict = result?.conflict === true;
  return {
    text: winner ?? "—",
    source: winner ? sources[result?.source] ?? null : null,
    conflict,
    warning: conflict ? "ผลแมตช์จากแหล่งข้อมูลขัดแย้งกัน จึงแสดงเป็น —" : null,
  };
}

/** Convert engine draft entries to display rows while preserving their recorded order. */
export function orderedDraftEntries(draft) {
  if (!Array.isArray(draft)) return null;
  return draft.map((entry, index) => {
    const heroId = typeof entry?.hero === "string" ? entry.hero : null;
    const heroToken = heroId?.replace(/^npc_dota_hero_/, "") ?? null;
    const hero = heroToken
      ? heroToken.split("_").map((part) => part ? part[0].toUpperCase() + part.slice(1) : "").join(" ")
      : "—";
    return {
      order: Number.isInteger(entry?.order) && entry.order >= 0 ? entry.order + 1 : index + 1,
      team: entry?.team === "radiant" ? "Radiant" : entry?.team === "dire" ? "Dire" : "—",
      kind: entry?.kind === "pick" ? "Pick" : entry?.kind === "ban" ? "Ban" : "—",
      hero,
    };
  }).sort((left, right) => left.order - right.order);
}

/** Render a known non-negative replay statistic, or the required em dash. */
export function heroMetricValue(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0
    ? Math.round(value).toLocaleString("en-US")
    : "—";
}

/** Show engine-provided comparison metrics without turning unknown values into zero. */
export function heroComparisonEngineMetric(hero, metric) {
  if (metric === "hd") return heroMetricValue(hero?.hero_damage);
  if (metric === "gpm") return heroMetricValue(hero?.rates?.gpm);
  return "—";
}

/** Convert finite samples into independent SVG line segments; nulls break the line. */
export function buildLineSegments(values, width, height, domain) {
  const finite = values.filter((value) => Number.isFinite(value));
  if (finite.length === 0 || width < 0 || height < 0) return [];
  const min = domain?.[0] ?? Math.min(...finite);
  const max = domain?.[1] ?? Math.max(...finite);
  const span = max - min;
  const segments = [];
  let current = [];
  values.forEach((value, index) => {
    if (!Number.isFinite(value)) {
      if (current.length) segments.push(current);
      current = [];
      return;
    }
    const x = values.length <= 1 ? width / 2 : (index * width) / (values.length - 1);
    const y = span === 0 ? height / 2 : ((max - value) * height) / span;
    current.push({ x, y });
  });
  if (current.length) segments.push(current);
  return segments;
}

/** Group hero rows by lane, keeping missing position data in a neutral bucket. */
export function matchupRows(heroes) {
  const rows = laneOrder.map((lane) => ({ lane, heroes: [] }));
  const rowByLane = new Map(rows.map((row) => [row.lane, row]));
  for (const hero of heroes ?? []) {
    const lane = laneOrder.includes(hero?.lane) && hero.lane !== "unknown" ? hero.lane : "unknown";
    rowByLane.get(lane).heroes.push(hero);
  }
  for (const row of rows) {
    row.heroes.sort((left, right) => {
      const side = (left.side === "radiant" ? 0 : 1) - (right.side === "radiant" ? 0 : 1);
      return side || (Number(left.slot) || 0) - (Number(right.slot) || 0);
    });
  }
  return rows;
}

/** Localized label for provisional lane outcomes. */
export function laneBadgeLabel(outcome) {
  const labels = {
    even: "สูสี",
    radiant_won: "ชนะเลน · Radiant",
    dire_won: "ชนะเลน · Dire",
    radiant_stomp: "ชนะขาด · Radiant",
    dire_stomp: "ชนะขาด · Dire",
  };
  if (outcome === "radiant_won" || outcome === "dire_won") return "ชนะเลน · เกณฑ์ชั่วคราว";
  if (outcome === "radiant_stomp" || outcome === "dire_stomp") return "ชนะขาด · เกณฑ์ชั่วคราว";
  return labels[outcome] ? `${labels[outcome]} · เกณฑ์ชั่วคราว` : "— ไม่ทราบ";
}

/** Build one side's trial reading without changing the AV2 raw lane result. */
export function laneExpectationReading(lane, side) {
  const expectation = lane?.expectation && typeof lane.expectation === "object" ? lane.expectation : null;
  const verdict = lane?.verdict_by_side?.[side] ?? (side === "radiant" ? lane?.verdict : null);
  const labels = { above: "เกินคาด", as: "ตามคาด", below: "ต่ำกว่าคาด" };
  return {
    trialLabel: lane?.label ?? "ทดลอง · เกณฑ์ชั่วคราว",
    score: typeof lane?.score?.value === "number" ? lane.score.value : null,
    expectation: typeof expectation?.value === "number" ? expectation.value : null,
    source: typeof expectation?.source_label === "string" ? expectation.source_label : null,
    verdict: verdict && labels[verdict.label]
      ? `${labels[verdict.label]}${verdict.far ? "มาก" : ""}`
      : null,
    explanation: typeof verdict?.text_th === "string" ? verdict.text_th : null,
    reason: typeof lane?.expectation_reason === "string" ? lane.expectation_reason : null,
    personalAverage: lane?.personal_average && typeof lane.personal_average.value === "number"
      ? lane.personal_average.value
      : null,
  };
}

/** Use the lane card when more than four matchup cells would make the strip crowded. */
export function showLaneExpectationInStrip(heroes, maximumCells) {
  return Number.isSafeInteger(maximumCells) && maximumCells >= 0 && (heroes?.length ?? 0) <= maximumCells;
}

/** Return all reasons a persisted report no longer matches current inputs. */
export function reportStaleReasons(report, current) {
  const reasons = [];
  if (report?.bundle_hash !== current?.bundle_hash) reasons.push("bundle_changed");
  if (report?.knowledge_version !== current?.knowledge_version) reasons.push("knowledge_changed");
  if (report?.prompt_version !== current?.prompt_version) reasons.push("prompt_changed");
  if ((report?.own_hero_id ?? null) !== (current?.own_hero_id ?? null)) reasons.push("own_hero_changed");
  return reasons;
}
