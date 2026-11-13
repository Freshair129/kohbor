import { liveCopy, type LiveLocale } from "./copy";
import { emptyResourceSummary, formatMegabytes, formatNullable } from "./state";
import type { ResourceMetricKey, ResourceMetrics, ResourceSummary } from "./types";
import { useResources } from "./useResources";
import "./live.css";

type UnitKind = "bytes" | "percent" | "fps" | "age";

const ROWS: { key: ResourceMetricKey; unit: UnitKind }[] = [
  { key: "appGroupBytes", unit: "bytes" },
  { key: "hostGpuBytes", unit: "bytes" },
  { key: "webviewGpuBytes", unit: "bytes" },
  { key: "gpuLoadPercent", unit: "percent" },
  { key: "cpuPercent", unit: "percent" },
  { key: "captureFps", unit: "fps" },
  { key: "frameAgeMs", unit: "age" },
];

function formatMetric(value: number | null, unit: UnitKind, locale: LiveLocale): string {
  if (unit === "bytes") return formatMegabytes(value);
  const formatted = formatNullable(value, unit === "age" ? 0 : 1);
  if (formatted === "—") return formatted;
  const text = liveCopy(locale);
  const unitText = unit === "percent" ? text.percentUnit : unit === "fps" ? text.fpsUnit : text.ageUnit;
  return `${formatted} ${unitText}`;
}

function valueFor(metrics: ResourceMetrics | null, key: ResourceMetricKey): number | null {
  return metrics?.[key] ?? null;
}

function summaryFor(summary: ResourceSummary, key: ResourceMetricKey) {
  return summary[key];
}

function ResourceRow({
  metric,
  current,
  summary,
  locale,
}: {
  metric: (typeof ROWS)[number];
  current: ResourceMetrics | null;
  summary: ResourceSummary;
  locale: LiveLocale;
}) {
  const text = liveCopy(locale);
  const aggregate = summaryFor(summary, metric.key);
  const currentValue = current?.[metric.key] ?? null;
  const notMeasuredYet = (metric.key === "webviewGpuBytes" || metric.key === "gpuLoadPercent") &&
    currentValue === null && aggregate.peak === null && aggregate.mean === null;
  return (
    <div className="live-resource-row" role="row">
      <span role="rowheader" className="live-resource-name">
        {text.metric[metric.key]}{notMeasuredYet ? ` ${text.notMeasuredYet}` : ""}
      </span>
      <span role="cell" className="mono">{formatMetric(valueFor(current, metric.key), metric.unit, locale)}</span>
      <span role="cell" className="mono">{formatMetric(aggregate.peak, metric.unit, locale)}</span>
      <span role="cell" className="mono">{formatMetric(aggregate.mean, metric.unit, locale)}</span>
    </div>
  );
}

export function ResourcePanel({ locale }: { locale: LiveLocale }) {
  const text = liveCopy(locale);
  const { current, summary, reset } = useResources();
  const visibleSummary = current ? summary : emptyResourceSummary();
  return (
    <section className="live-resource-panel" aria-labelledby="live-resource-title" lang={locale}>
      <header className="live-panel-head">
        <h2 id="live-resource-title">{text.resourceTitle}</h2>
        <button type="button" onClick={reset}>{text.reset}</button>
      </header>
      <div className="live-resource-table" role="table" aria-label={text.resourceTitle}>
        <div className="live-resource-row live-resource-head" role="row">
          <span role="columnheader" />
          <span role="columnheader">{text.current}</span>
          <span role="columnheader">{text.peak}</span>
          <span role="columnheader">{text.mean}</span>
        </div>
        {ROWS.map((metric) => (
          <ResourceRow key={metric.key} metric={metric} current={current} summary={visibleSummary} locale={locale} />
        ))}
      </div>
    </section>
  );
}
