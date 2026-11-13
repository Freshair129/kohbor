import type { PlaybackState } from "./playback";
import type { RuntimeStatus } from "./types";

export type ChipKind = "ready" | "not_ready" | "muted" | "no_device" | "unknown" | "demo";
export interface ChipView { kind: ChipKind; reason: string | null; text_th: string }
export interface ChipState {
  view: ChipView | null;
  shownUntilMs: number | null;
  lastStatusKey: string | null;
  demoMarkerAtMs: number | null;
  statusView?: ChipView | null;
  demoPlaybackId?: string | null;
}
export type ChipInput =
  | { type: "status"; media: RuntimeStatus["media"] | null; now_ms: number }
  | { type: "demo_marker" | "tick"; now_ms: number }
  | { type: "playback_started"; playback_id?: string; now_ms: number }
  | { type: "playback_ended"; playback_id?: string; now_ms: number };

export const CHIP_SHOW_MS = 8000;
export const DEMO_MARKER_WINDOW_MS = 2000;

const unknownChip: ChipView = { kind: "unknown", reason: null, text_th: "ยังไม่ทราบสถานะ" };
const demoChip: ChipView = { kind: "demo", reason: null, text_th: "DEMO · ไม่ใช่เหตุการณ์จริง" };

export function chipForStatus(media: RuntimeStatus["media"] | null): ChipView {
  if (media === null) return { ...unknownChip };
  if (media.state === "ready" && media.reason === null) {
    return { kind: "ready", reason: null, text_th: "พร้อม · ผ่านการตรวจ" };
  }
  if (media.state === "unavailable") {
    if (media.reason === "AUDIO_MUTED") {
      return { kind: "muted", reason: media.reason, text_th: "ปิดเสียงอยู่ · เสียงเตือนสำคัญจะไม่ดัง" };
    }
    if (media.reason === "CAPABILITY_UNAVAILABLE") {
      return { kind: "no_device", reason: media.reason, text_th: "ระบบเสียงใช้ไม่ได้ในขณะนี้" };
    }
  }
  if (media.state === "not_ready") {
    const copy: Record<string, string> = {
      MEDIA_UNMAPPED: "ยังไม่มีเสียงเตือนสำคัญ",
      MEDIA_INVALID: "สื่อไม่ผ่านการตรวจ",
      MEDIA_PROBE_FAILED: "ยังยืนยันภาพไม่ได้",
    };
    const text = media.reason ? copy[media.reason] : undefined;
    if (text) return { kind: "not_ready", reason: media.reason, text_th: text };
  }
  return { ...unknownChip };
}

export function createChipState(): ChipState {
  return {
    view: null,
    shownUntilMs: null,
    lastStatusKey: null,
    demoMarkerAtMs: null,
    statusView: null,
    demoPlaybackId: null,
  };
}

function isDegraded(view: ChipView): boolean {
  return view.kind === "not_ready" || view.kind === "muted" || view.kind === "no_device";
}

function statusKey(media: RuntimeStatus["media"] | null): string {
  return media === null ? "unknown" : `${media.state}:${media.reason ?? ""}`;
}

function restoreStatus(state: ChipState): ChipView | null {
  return state.statusView ?? null;
}

export function reduceChip(state: ChipState, input: ChipInput): ChipState {
  if (input.type === "status") {
    const view = chipForStatus(input.media);
    const key = statusKey(input.media);
    if (key === state.lastStatusKey) return state;
    return {
      ...state,
      view: state.view?.kind === "demo" ? state.view : view,
      statusView: view,
      shownUntilMs: isDegraded(view) ? input.now_ms + CHIP_SHOW_MS : null,
      lastStatusKey: key,
    };
  }

  if (input.type === "demo_marker") {
    return { ...state, demoMarkerAtMs: input.now_ms };
  }

  if (input.type === "playback_started") {
    const markerAt = state.demoMarkerAtMs;
    const isDemo = markerAt !== null
      && input.now_ms >= markerAt
      && input.now_ms - markerAt <= DEMO_MARKER_WINDOW_MS;
    return {
      ...state,
      view: isDemo ? demoChip : state.view?.kind === "demo" ? restoreStatus(state) : state.view,
      shownUntilMs: isDemo || state.view?.kind === "demo" ? null : state.shownUntilMs,
      demoMarkerAtMs: null,
      demoPlaybackId: isDemo ? input.playback_id ?? null : null,
    };
  }

  if (input.type === "playback_ended") {
    if (state.view?.kind !== "demo") return state;
    if (input.playback_id && state.demoPlaybackId && input.playback_id !== state.demoPlaybackId) return state;
    return {
      ...state,
      view: restoreStatus(state),
      shownUntilMs: state.statusView && isDegraded(state.statusView) ? state.shownUntilMs : null,
      demoMarkerAtMs: null,
      demoPlaybackId: null,
    };
  }

  return state;
}

export function overlayShouldShow(playback: PlaybackState, chip: ChipState, now_ms: number): boolean {
  if (playback.visual !== null) return true;
  return chip.view !== null && chip.shownUntilMs !== null && now_ms < chip.shownUntilMs;
}
