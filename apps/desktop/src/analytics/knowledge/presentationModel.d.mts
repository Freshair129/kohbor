export interface LinePoint { x: number; y: number }
export function buildLineSegments(values: Array<number | null | undefined>, width: number, height: number, domain?: [number, number]): LinePoint[][];
export function matchupRows<T extends { side?: string; slot?: number; lane?: string | null }>(heroes: T[]): Array<{ lane: "top" | "mid" | "bot" | "unknown"; heroes: T[] }>;
export function laneBadgeLabel(outcome: string | null | undefined): string;
export function reportStaleReasons(report: Record<string, unknown>, current: Record<string, unknown>): string[];
export interface LaneExpectationReading { trialLabel: string; score: number | null; expectation: number | null; source: string | null; verdict: string | null; explanation: string | null; reason: string | null; personalAverage: number | null }
export function laneExpectationReading(lane: unknown, side: "radiant" | "dire" | string): LaneExpectationReading;
export function showLaneExpectationInStrip(heroes: unknown[] | null | undefined, maximumCells: number): boolean;
export interface MatchResultPresentation { text: string; source: string | null; conflict: boolean; warning: string | null }
export function matchResultPresentation(result: unknown): MatchResultPresentation;
export interface DraftEntryView { order: number; team: string; kind: string; hero: string }
export function orderedDraftEntries(draft: unknown): DraftEntryView[] | null;
export function heroMetricValue(value: unknown): string;
export function heroComparisonEngineMetric(hero: unknown, metric: "hd" | "gpm" | string): string;
