export type HeroBenchmarkCurve = Array<{ percentile: number; value: number }>;

export type HeroBenchmarkMetric = {
  key: string;
  label: string;
  value: number | null;
  percentile: number | null;
};

export function interpolatePercentile(value: number | null, curve: HeroBenchmarkCurve | null | undefined): number | null;

export function buildHeroBenchmarksViewModel(input?: {
  enabled?: boolean;
  requestState?: "disabled" | "unknown" | "loading" | "ready" | "offline" | "budget_exhausted";
  budgetUsed?: number;
  benchmarks?: {
    gold_per_min?: HeroBenchmarkCurve | null;
    xp_per_min?: HeroBenchmarkCurve | null;
    last_hits_per_min?: HeroBenchmarkCurve | null;
    hero_damage_per_min?: HeroBenchmarkCurve | null;
    kills_per_min?: HeroBenchmarkCurve | null;
  } | null;
  observed?: Record<string, number | null | undefined>;
  fetchedAt?: number | null;
}): {
  state: "disabled" | "unknown" | "loading" | "ready" | "offline" | "budget_exhausted";
  budgetUsed: number;
  budgetText: string;
  attribution: string;
  population: string;
  fetchedAt: number | null;
  metrics: HeroBenchmarkMetric[];
};

export function observedHeroBenchmarkMetrics(
  hero: {
    rates?: { gpm?: unknown; xpm?: unknown };
    final?: Record<string, unknown>;
    hero_damage?: unknown;
  } | null | undefined,
  durationSeconds: number | null | undefined,
): Record<"gpm" | "xpm" | "lhpm" | "hdpm" | "kpm", number | null>;
