// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { bgraToRgba, parseFrame } from "./frame.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { ParsedFrame } from "./types.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { formatNullable } from "./state.ts";

export interface PreviewMetrics {
  received_fps: number | null;
  painted_fps: number | null;
  dropped_count: number | null;
  parse_paint_ms_mean: number | null;
  received_total: number;
  painted_total: number;
}

export function unknownPreviewMetrics(): PreviewMetrics {
  return { received_fps: null, painted_fps: null, dropped_count: null,
    parse_paint_ms_mean: null, received_total: 0, painted_total: 0 };
}

/** Monotonic counters; delayed sample timers use the actual elapsed time. */
export function createPreviewPipeline({ generation, now, paint }: {
  generation: number;
  now: () => number;
  paint: (frame: ParsedFrame) => boolean;
}) {
  let received = 0;
  let painted = 0;
  let dropped = 0;
  let durationMs = 0;
  let pending: ParsedFrame | null = null;
  let painting = false;
  let sampledAt = now();
  let previousReceived = 0;
  let previousPainted = 0;
  let previousDuration = 0;
  let metrics = unknownPreviewMetrics();
  let error: string | null = null;

  function drain() {
    if (painting) return;
    painting = true;
    try {
      while (pending) {
        const candidate = pending;
        pending = null;
        if (paint(candidate)) {
          painted += 1;
        } else {
          if (pending) {
            dropped += 1;
            continue;
          }
          pending = candidate;
          break;
        }
      }
    } catch (failure) {
      dropped += 1;
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      painting = false;
    }
  }

  return {
    receive(buffer: ArrayBuffer) {
      const started = now();
      const frame = parseFrame(buffer);
      if (!frame) return;
      received += 1;
      if (frame.header.calibration_generation !== generation) {
        dropped += 1;
      } else {
        if (pending) dropped += 1;
        pending = frame;
        drain();
      }
      durationMs += Math.max(0, now() - started);
    },
    sample(): PreviewMetrics {
      const at = now();
      const elapsed = at - sampledAt;
      if (elapsed < 1_000) return metrics;
      const messages = received - previousReceived;
      metrics = {
        received_fps: messages * 1_000 / elapsed,
        painted_fps: (painted - previousPainted) * 1_000 / elapsed,
        dropped_count: dropped,
        parse_paint_ms_mean: messages > 0 ? (durationMs - previousDuration) / messages : null,
        received_total: received,
        painted_total: painted,
      };
      sampledAt = at;
      previousReceived = received;
      previousPainted = painted;
      previousDuration = durationMs;
      return metrics;
    },
    totals: () => ({ received_total: received, painted_total: painted, dropped_count: dropped }),
    getError: () => error,
  };
}

interface CanvasSurface {
  width: number;
  height: number;
  getContext(kind: "2d"): Pick<CanvasRenderingContext2D, "createImageData" | "putImageData"> | null;
}

/** Only a new surface or dimensions allocate. Swizzle directly into ImageData.data. */
export function createCanvasPainter(getCanvas: () => CanvasSurface | null) {
  let image: ImageData | null = null;
  let previousCanvas: CanvasSurface | null = null;
  return (frame: ParsedFrame): boolean => {
    const canvas = getCanvas();
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return false;
    const { width, height } = frame.header;
    if (canvas !== previousCanvas || !image || image.width !== width || image.height !== height) {
      canvas.width = width;
      canvas.height = height;
      image = context.createImageData(width, height);
      previousCanvas = canvas;
    }
    bgraToRgba(frame, image.data);
    context.putImageData(image, 0, 0);
    return true;
  };
}

export function visibilityText(state: string | null, locale: "th" | "en"): string | null {
  if (state !== "hidden") return null;
  return locale === "th"
    ? "หน้าต่างถูกซ่อนหรือถูกบัง — ภาพอาจช้า"
    : "window hidden or covered — preview may be slow";
}

export function previewRateText(sent: number | null, metrics: Pick<PreviewMetrics, "received_fps" | "painted_fps"> | null,
  locale: "th" | "en"): string {
  const received = formatNullable(metrics?.received_fps ?? null, 1);
  const painted = formatNullable(metrics?.painted_fps ?? null, 1);
  return locale === "th"
    ? `ส่ง ${formatNullable(sent, 1)} · รับ ${received} · วาด ${painted} fps`
    : `sent ${formatNullable(sent, 1)} · received ${received} · painted ${painted} fps`;
}
