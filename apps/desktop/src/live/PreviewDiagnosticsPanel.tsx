import { derivePreviewDiagnostics, type HostCounterRates } from "./previewDiagnostics";
import type { CaptureStatus } from "./types";
import type { PreviewMetrics } from "./preview";
import type { LiveLocale } from "./copy";

export function PreviewDiagnosticsPanel({ capture, metrics, counterRates, locale }: {
  capture: CaptureStatus | null; metrics: PreviewMetrics; counterRates: HostCounterRates | null; locale: LiveLocale;
}) {
  const view = derivePreviewDiagnostics(capture, metrics, counterRates, locale);
  return <section className="live-preview-diagnostics" aria-labelledby="live-preview-diagnostics-heading">
    <h3 id="live-preview-diagnostics-heading">{view.title}</h3>
    <div className="live-preview-diagnostic-grid">
      {[view.host, view.window].map(group => <section key={group.source} aria-label={group.source}>
        <h4>{group.source}</h4><p>{group.note}</p>
        <dl>{group.rows.map(row => <div key={row.key}>
          <dt>{row.label}</dt><dd><span className="mono">{row.value}</span><small>{row.unit}</small></dd>
        </div>)}</dl>
      </section>)}
    </div>
  </section>;
}
