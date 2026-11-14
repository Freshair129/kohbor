// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { CaptureStatus } from "./types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { PreviewMetrics } from "./preview.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { deriveCaptureDisplay } from "./captureDisplay.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { formatNullable } from "./state.ts";

export interface CaptureObservation { value: CaptureStatus | null; receivedAt: number | null }
export interface HostCounterRates {
  elapsedMs: number | null;
  sentDelta: number | null;
  grabDelta: number | null;
  sentPerSecond: number | null;
  grabPerSecond: number | null;
}

/** Observability only: never replaces fps_measured or the preview pipeline's rate calculation. */
export function deriveHostCounterRates(previous: CaptureObservation | null, current: CaptureObservation): HostCounterRates {
  const unknown: HostCounterRates = { elapsedMs: null, sentDelta: null, grabDelta: null, sentPerSecond: null, grabPerSecond: null };
  const before = previous?.value; const after = current.value;
  if (!before || !after || previous?.receivedAt == null || current.receivedAt === null
    || before.phase === "idle" || after.phase === "idle"
    || before.calibration_generation === null || before.calibration_generation !== after.calibration_generation) return unknown;
  const elapsedMs = current.receivedAt - previous.receivedAt;
  if (!Number.isFinite(elapsedMs) || elapsedMs <= 0) return unknown;
  const delta = (a: number | null, b: number | null): number | null =>
    a !== null && b !== null && Number.isSafeInteger(a) && Number.isSafeInteger(b) && a >= 0 && b >= a ? b - a : null;
  const rate = (count: number | null): number | null => {
    if (count === null) return null;
    const result = count * 1000 / elapsedMs;
    return Number.isFinite(result) ? result : null;
  };
  const sentDelta = delta(before.sent_total, after.sent_total);
  const grabDelta = delta(before.grab_total, after.grab_total);
  return { elapsedMs, sentDelta, grabDelta, sentPerSecond: rate(sentDelta), grabPerSecond: rate(grabDelta) };
}

export function previewRateTooltip(locale: "th" | "en"): string {
  return locale === "th"
    ? "ส่ง: โฮสต์นับในวินาทีล่าสุด; รับ/วาด: วัดในหน้าต่างด้วยนาฬิกาและช่วงเก็บตัวอย่างของหน้าต่าง"
    : "Sent: the host's last-second count; received/painted: measured in the window using its own clock and sample window.";
}

export interface DiagnosticRow { key: string; label: string; value: string; unit: string }
export function derivePreviewDiagnostics(capture: CaptureStatus | null, metrics: PreviewMetrics, counterRates: HostCounterRates | null, locale: "th" | "en") {
  const { capture: host, metrics: windowMetrics } = deriveCaptureDisplay(capture, metrics);
  const rates = capture === null || capture.phase === "idle" ? null : counterRates;
  const frames = locale === "th" ? "เฟรม" : "frames";
  const perSecond = locale === "th" ? "เฟรม/วิ" : "frames/s";
  const row = (key: string, label: string, value: number | null, unit: string, digits?: number): DiagnosticRow =>
    ({ key, label, value: formatNullable(value, digits), unit });
  return {
    title: locale === "th" ? "ตัวนับและอัตราภาพ" : "Preview counters and rates",
    host: {
      source: locale === "th" ? "โฮสต์" : "Host",
      note: locale === "th" ? "Δ/วิ จาก status ติดต่อกัน คำนวณด้วยนาฬิกาหน้าต่าง" : "Δ/s uses consecutive status counters and the window clock.",
      rows: [
        row("sent_total", "sent_total", host?.sent_total ?? null, frames),
        row("grab_total", "grab_total", host?.grab_total ?? null, frames),
        row("sent_delta_per_second", "Δ sent_total / s", rates?.sentPerSecond ?? null, perSecond, 1),
        row("grab_delta_per_second", "Δ grab_total / s", rates?.grabPerSecond ?? null, perSecond, 1),
        row("fps_measured", locale === "th" ? "fps_measured · วินาทีล่าสุด" : "fps_measured · last second", host?.fps_measured ?? null, "fps", 1),
        row("grab_fps", "grab_fps", host?.grab_fps ?? null, "fps", 1),
        row("grab_ms_mean", "grab_ms_mean", host?.grab_ms_mean ?? null, "ms", 2),
        row("analysis_ms_mean", "analysis_ms_mean", host?.analysis_ms_mean ?? null, "ms", 2),
      ],
    },
    window: {
      source: locale === "th" ? "หน้าต่าง" : "Window",
      note: locale === "th" ? "รับ/วาดใช้ช่วง sample ของหน้าต่าง; status Δt คือช่วงระหว่าง status สองครั้ง" : "Receive/paint use the window sample interval; status Δt is the time between two status arrivals.",
      rows: [
        row("received_total", "received_total", windowMetrics.received_total, frames),
        row("painted_total", "painted_total", windowMetrics.painted_total, frames),
        row("received_fps", "received_fps", windowMetrics.received_fps, "fps", 1),
        row("painted_fps", "painted_fps", windowMetrics.painted_fps, "fps", 1),
        row("status_interval_ms", "status Δt", rates?.elapsedMs ?? null, "ms"),
        row("status_sent_delta", "status Δ sent_total", rates?.sentDelta ?? null, frames),
        row("status_grab_delta", "status Δ grab_total", rates?.grabDelta ?? null, frames),
        row("dropped_count", "dropped_count", windowMetrics.dropped_count, frames),
        row("parse_paint_ms_mean", "parse_paint_ms_mean", windowMetrics.parse_paint_ms_mean, "ms", 2),
      ],
    },
  };
}
