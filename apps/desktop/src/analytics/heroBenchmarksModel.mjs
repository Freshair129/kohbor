export const HERO_BENCHMARK_METRICS = Object.freeze([
  { key: "gold_per_min", label: "ทองต่อนาที", observedKey: "gpm" },
  { key: "xp_per_min", label: "ประสบการณ์ต่อนาที", observedKey: "xpm" },
  { key: "last_hits_per_min", label: "ครีปที่สังหารต่อนาที", observedKey: "lhpm" },
  { key: "hero_damage_per_min", label: "ความเสียหายต่อฮีโร่ต่อนาที", observedKey: "hdpm" },
  { key: "kills_per_min", label: "สังหารต่อนาที", observedKey: "kpm" },
]);

function validPoint(point) {
  return point && Number.isFinite(point.percentile) && point.percentile >= 0 && point.percentile <= 100
    && Number.isFinite(point.value) && point.value >= 0;
}

/** Interpolate one locally observed value between an OpenDota percentile curve. */
export function interpolatePercentile(value, curve) {
  if (!Number.isFinite(value) || value < 0 || !Array.isArray(curve) || curve.length < 2
    || !curve.every(validPoint)) return null;
  const points = [...curve].sort((left, right) => left.percentile - right.percentile);
  for (let index = 1; index < points.length; index += 1) {
    if (points[index - 1].percentile >= points[index].percentile
      || points[index - 1].value > points[index].value) return null;
  }
  if (value <= points[0].value) return points[0].percentile;
  const last = points[points.length - 1];
  if (value >= last.value) return last.percentile;
  for (let index = 1; index < points.length; index += 1) {
    const lower = points[index - 1];
    const upper = points[index];
    if (value <= upper.value) {
      if (upper.value === lower.value) return lower.percentile;
      const ratio = (value - lower.value) / (upper.value - lower.value);
      return lower.percentile + ratio * (upper.percentile - lower.percentile);
    }
  }
  return null;
}

function safeBudget(value) {
  return Number.isFinite(value) ? Math.max(0, Math.min(4, Math.floor(value))) : 0;
}

/** Pure renderer model. Benchmark data is only compared with the replay on this device. */
export function buildHeroBenchmarksViewModel({
  enabled = false,
  requestState = "unknown",
  budgetUsed = 0,
  benchmarks = null,
  observed = {},
  fetchedAt = null,
} = {}) {
  const state = !enabled ? "disabled" : ["loading", "ready", "offline", "budget_exhausted", "unknown"].includes(requestState)
    ? requestState : "unknown";
  const metrics = HERO_BENCHMARK_METRICS.map(({ key, label, observedKey }) => {
    const value = Number.isFinite(observed[observedKey]) && observed[observedKey] >= 0
      ? observed[observedKey] : null;
    return {
      key,
      label,
      value,
      percentile: value === null ? null : interpolatePercentile(value, benchmarks?.[key]),
    };
  });
  return {
    state,
    budgetUsed: safeBudget(budgetUsed),
    budgetText: `ใช้ไป ${safeBudget(budgetUsed)}/4 ใน 24 ชม.`,
    attribution: "ข้อมูลเทียบ: OpenDota",
    population: "ข้อมูลครอบคลุมทุกแรงก์ · คำนวณ percentile ในเครื่อง",
    fetchedAt: Number.isFinite(fetchedAt) && fetchedAt > 0 ? fetchedAt : null,
    metrics,
  };
}

/** Build replay values from fields already present in the local bundle. */
export function observedHeroBenchmarkMetrics(hero, durationSeconds) {
  const durationMinutes = Number.isFinite(durationSeconds) && durationSeconds > 0
    ? durationSeconds / 60 : null;
  const scalar = (value) => {
    if (Number.isFinite(value) && value >= 0) return value;
    if (value && typeof value === "object" && Number.isFinite(value.value) && value.value >= 0) return value.value;
    return null;
  };
  const perMinute = (value) => {
    const total = scalar(value);
    return total === null || durationMinutes === null ? null : total / durationMinutes;
  };
  return {
    gpm: scalar(hero?.rates?.gpm),
    xpm: scalar(hero?.rates?.xpm),
    lhpm: perMinute(hero?.final?.lh),
    hdpm: perMinute(hero?.hero_damage),
    kpm: perMinute(hero?.final?.k),
  };
}
