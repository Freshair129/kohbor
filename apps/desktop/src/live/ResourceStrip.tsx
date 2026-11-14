import { liveCopy, type LiveLocale } from "./copy";
import { previewRateText, type PreviewMetrics } from "./preview";
import { formatMegabytes, formatNullable } from "./state";
import type { CaptureStatus } from "./types";
import { useResources } from "./useResources";

export function ResourceStrip({ locale, capture, metrics }: {
  locale: LiveLocale;
  capture: CaptureStatus | null;
  metrics: Pick<PreviewMetrics, "received_fps" | "painted_fps">;
}) {
  const { current } = useResources();
  const text = liveCopy(locale);
  return (
    <div className="live-resource-strip mono" aria-label={text.resourceTitle}>
      <span>RAM <strong>{formatMegabytes(current?.appGroupBytes ?? null)}</strong></span>
      <span>CPU <strong>{formatNullable(current?.cpuPercent ?? null, 1)} %</strong></span>
      <span>{previewRateText(capture?.fps_measured ?? null, metrics, locale)}</span>
      <span>{text.metric.frameAgeMs} <strong>{formatNullable(capture?.frame_age_ms ?? null)} ms</strong></span>
    </div>
  );
}
