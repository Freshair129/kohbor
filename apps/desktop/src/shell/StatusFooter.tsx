import { formatMegabytes, formatNullable } from "../live/state";
import type { ResourceMetrics, ResourceSummary, CaptureStatus } from "../live/types";
import type { PreviewMetrics } from "../live/preview";
import { footerMetric, type FooterTone } from "./statusFooterModel";
import "./statusFooter.css";

const TONE_CLASS: Record<FooterTone, string> = {
  fresh: "fresh", warn: "warn", stale: "stale", off: "off", unknown: "unknown",
};

function Metric({ metric }: { metric: ReturnType<typeof footerMetric> }) {
  return <div className="shell-footer-metric" title={metric.title} data-priority={metric.id}>
    <span className="shell-footer-metric-label">{metric.label}</span>
    <span className="shell-footer-bar" aria-hidden="true"><span style={{ width: `${metric.barPercent}%` }} /></span>
    <span className="shell-footer-value mono" aria-label={`${metric.title}: ${metric.display} ${metric.unit}`.trim()}>
      {metric.display}<small>{metric.unit}</small>
    </span>
  </div>;
}

export function StatusFooter({ statusLabel, tone, resources, resourceSummary, capture, preview, labels }: {
  statusLabel: string;
  tone: FooterTone;
  resources: ResourceMetrics | null;
  resourceSummary: ResourceSummary;
  capture: CaptureStatus | null;
  preview: PreviewMetrics;
  labels: { region: string; cpu: string; ram: string; capture: string; received: string; painted: string;
    cpuTitle: string; ramTitle: string; captureTitle: string; receivedTitle: string; paintedTitle: string };
}) {
  const sample = (value: number | null, peak: number | null) => value === null || peak === null || peak <= 0 ? 0 : value / peak * 100;
  const metrics = [
    footerMetric({ id: "cpu", label: labels.cpu, value: resources?.cpuPercent ?? null,
      display: formatNullable(resources?.cpuPercent ?? null, 1), unit: "%", barPercent: resources?.cpuPercent ?? 0,
      tone: "unknown", title: labels.cpuTitle }),
    footerMetric({ id: "ram", label: labels.ram, value: resources?.appGroupBytes ?? null,
      display: formatMegabytes(resources?.appGroupBytes ?? null), unit: "", barPercent: sample(resources?.appGroupBytes ?? null, resourceSummary.appGroupBytes.peak),
      tone: "unknown", title: labels.ramTitle }),
    footerMetric({ id: "capture", label: labels.capture, value: capture?.fps_measured ?? null,
      display: formatNullable(capture?.fps_measured ?? null, 1), unit: "fps", barPercent: sample(capture?.fps_measured ?? null, resourceSummary.captureFps.peak),
      tone: "unknown", title: labels.captureTitle }),
    footerMetric({ id: "received", label: labels.received, value: preview.received_fps,
      display: formatNullable(preview.received_fps, 1), unit: "fps", barPercent: sample(preview.received_fps, Math.max(preview.received_fps ?? 0, preview.painted_fps ?? 0)),
      tone: "unknown", title: labels.receivedTitle }),
    footerMetric({ id: "painted", label: labels.painted, value: preview.painted_fps,
      display: formatNullable(preview.painted_fps, 1), unit: "fps", barPercent: sample(preview.painted_fps, Math.max(preview.received_fps ?? 0, preview.painted_fps ?? 0)),
      tone: "unknown", title: labels.paintedTitle }),
  ];
  return <footer className="shell-status-footer" role="status" aria-label={labels.region}>
    <span className={`shell-footer-state shell-footer-state--${TONE_CLASS[tone]}`}>
      <span className="shell-footer-dot" aria-hidden="true" />{statusLabel}
    </span>
    <div className="shell-footer-metrics">{metrics.map((metric) => <Metric key={metric.id} metric={metric} />)}</div>
  </footer>;
}
