import type { PlaybackState } from "./playback";
import type { RuntimeStatus } from "./types";
export type ChipKind = "ready" | "not_ready" | "muted" | "no_device" | "unknown" | "demo";
export interface ChipView { kind: ChipKind; reason: string | null; text_th: string }
export interface ChipState { view: ChipView | null; shownUntilMs: number | null; lastStatusKey: string | null; demoMarkerAtMs: number | null }
export type ChipInput = { type: "status"; media: RuntimeStatus["media"] | null; now_ms: number } | { type: "demo_marker" | "playback_started" | "tick"; now_ms: number };
export const CHIP_SHOW_MS = 8000;
export const DEMO_MARKER_WINDOW_MS = 2000;
export function chipForStatus(_media: RuntimeStatus["media"] | null): ChipView { throw new Error("not implemented"); }
export function createChipState(): ChipState { throw new Error("not implemented"); }
export function reduceChip(_state: ChipState, _input: ChipInput): ChipState { throw new Error("not implemented"); }
export function overlayShouldShow(_playback: PlaybackState, _chip: ChipState, _now_ms: number): boolean { throw new Error("not implemented"); }
