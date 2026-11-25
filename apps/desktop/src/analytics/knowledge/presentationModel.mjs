// Pure presentation helpers for Analytics knowledge panels.
const laneOrder = ["top", "mid", "bot", "unknown"];

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
