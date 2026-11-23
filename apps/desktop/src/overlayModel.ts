import type { CriticalStripEventType, PlaybackState } from "./playback";
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

const OVERLAY_STAGE_LOGICAL = { width: 720, height: 220 } as const;
const BANNER_BASE_WIDTH_PX = 680;
const BANNER_MAX_WIDTH_FRACTION = 0.72;
const CRITICAL_STRIP_GAP_LOGICAL = 9.4;
const CRITICAL_STRIP_HEIGHT_LOGICAL = 94.1;
const CRITICAL_STRIP_LIMIT_OVERHANG_LOGICAL = 75.3;

export interface OverlayMonitorSize { x: number; y: number; width: number; height: number }
export interface OverlayBannerLayout { x_percent: number; y_percent: number; scale: number; enabled: boolean }
export interface OverlayBox { x: number; y: number; width: number; height: number }
export interface CriticalStackGeometry {
  stage: OverlayBox;
  banner: OverlayBox;
  strip: OverlayBox;
  stripLimitBottom: number;
  window: OverlayBox;
}

const DEFAULT_OVERLAY_BANNER_LAYOUT: OverlayBannerLayout = {
  x_percent: 49.46,
  y_percent: 10.3,
  scale: 0.45,
  enabled: true,
};

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

export function overlayShouldShow(playback: PlaybackState, chip: ChipState, now_ms: number, objectiveAlert = false): boolean {
  if (playback.visual !== null || playback.criticalStrip !== null) return true;
  return objectiveAlert || (chip.view !== null && chip.shownUntilMs !== null && now_ms < chip.shownUntilMs);
}

/** The benign "ready" status is noise during a match: only problem states, unknown and DEMO show. */
export function overlayChipForDisplay(view: ChipView | null): ChipView | null {
  return view === null || view.kind === "ready" ? null : view;
}

export function criticalStripText(eventType: CriticalStripEventType): string {
  return eventType === "health_danger" ? "เลือดน้อย! ถอยก่อน" : "อันตราย! ระวังตัว";
}

function scaled(logical: number, scale: number): number {
  return Math.max(1, Math.round(logical * scale));
}

function clampPosition(position: number, size: number, origin: number, extent: number): number {
  return Math.min(Math.max(position, origin), origin + Math.max(0, extent - size));
}

/** Match the Rust banner layout and return the yielded-video and critical-strip boxes in screen pixels. */
export function criticalStackGeometry(
  monitor: OverlayMonitorSize,
  layout: OverlayBannerLayout = DEFAULT_OVERLAY_BANNER_LAYOUT,
): CriticalStackGeometry | null {
  if (!layout.enabled || monitor.width <= 0 || monitor.height <= 0) return null;
  if (![monitor.x, monitor.y, monitor.width, monitor.height, layout.x_percent, layout.y_percent, layout.scale].every(Number.isFinite)) {
    return null;
  }

  const heightScale = monitor.height / 1080;
  const stageWidthPx = Math.min(BANNER_BASE_WIDTH_PX * heightScale, BANNER_MAX_WIDTH_FRACTION * monitor.width);
  const logicalScale = Math.min(3, Math.max(0.2, layout.scale)) * stageWidthPx / OVERLAY_STAGE_LOGICAL.width;
  const windowWidth = Math.min(scaled(OVERLAY_STAGE_LOGICAL.width, logicalScale), Math.max(1, monitor.width));
  const windowHeight = Math.min(scaled(295, logicalScale), Math.max(1, monitor.height));
  const stageHeight = Math.min(scaled(OVERLAY_STAGE_LOGICAL.height, logicalScale), windowHeight);
  const centerX = monitor.x + monitor.width * Math.min(100, Math.max(0, layout.x_percent)) / 100;
  const centerY = monitor.y + monitor.height * Math.min(100, Math.max(0, layout.y_percent)) / 100;
  const stageX = clampPosition(Math.round(centerX - windowWidth / 2), windowWidth, monitor.x, monitor.width);
  const stageY = clampPosition(Math.round(centerY - stageHeight / 2), windowHeight, monitor.y, monitor.height);
  const stage: OverlayBox = { x: stageX, y: stageY, width: windowWidth, height: stageHeight };
  const banner: OverlayBox = {
    x: stage.x + stage.width * 0.06,
    y: stage.y - stage.height * 0.02,
    width: stage.width * 0.88,
    height: stage.height * 0.88,
  };
  const strip: OverlayBox = {
    x: banner.x,
    y: banner.y + banner.height + CRITICAL_STRIP_GAP_LOGICAL * logicalScale,
    width: banner.width,
    height: CRITICAL_STRIP_HEIGHT_LOGICAL * logicalScale,
  };
  const stripLimitBottom = stage.y + stage.height + CRITICAL_STRIP_LIMIT_OVERHANG_LOGICAL * logicalScale;

  return {
    stage,
    banner,
    strip,
    stripLimitBottom,
    window: { x: stageX, y: stageY, width: windowWidth, height: windowHeight },
  };
}
