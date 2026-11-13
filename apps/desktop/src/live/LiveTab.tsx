import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { DangerMeter } from "./DangerMeter";
import { EnemySeen } from "./EnemySeen";
import { liveCopy, phaseLabel, type LiveLocale } from "./copy";
import { formatNullable, showAutoLayoutUnconfirmed } from "./state";
import { liveMockRequested } from "./mockMode";
import { ResourcePanel } from "./ResourcePanel";
import { useCaptureStatus } from "./useCaptureStatus";
import { useMinimap } from "./useMinimap";
import { useRisk } from "./useRisk";
import "./live.css";

export function LiveTab({ locale = "th" }: { locale?: LiveLocale }) {
  const text = liveCopy(locale);
  const capture = useCaptureStatus();
  const risk = useRisk();
  const canShowPicture = capture !== null && ["live", "stale"].includes(capture.phase);
  const minimap = useMinimap(capture?.calibration_generation ?? null, canShowPicture);
  const [invalidating, setInvalidating] = useState(false);
  const [invalidateError, setInvalidateError] = useState<string | null>(null);
  const mock = liveMockRequested();

  const invalidate = async () => {
    setInvalidating(true);
    setInvalidateError(null);
    try {
      if (mock) {
        const mockDriver = await import("./mock");
        mockDriver.invalidateMock();
      } else {
        await invoke("capture_invalidate");
      }
    } catch {
      setInvalidateError(text.invalidateFailed);
    } finally {
      setInvalidating(false);
    }
  };

  const phase = capture?.phase ?? null;
  const age = formatNullable(capture?.frame_age_ms ?? null);
  const statusBadge = `${phaseLabel(phase, locale)} · ${formatNullable(capture?.fps_measured ?? null, 1)} fps · ${age} ms`;
  const stale = phase === "stale";

  let plate: string | null = null;
  let plateTone = "";
  switch (phase) {
    case "unsupported":
      plate = text.unsupported;
      break;
    case "coverage_unavailable":
      plate = text.noVisibility;
      plateTone = " is-critical";
      break;
    case "lost":
      plate = text.lost;
      plateTone = " is-warning";
      break;
    case "calibrating":
      plate = text.calibrating;
      break;
    case "stale":
      plate = text.stale(age);
      break;
    default:
      if (!canShowPicture || !minimap.hasFrame) plate = text.waitingFrame;
  }

  return (
    <main className="live-tab" lang={locale}>
      <header className="live-header">
        <h1>{text.title}</h1>
        {mock && <span className="live-experimental">{text.mockPreview}</span>}
      </header>

      <section className="live-map-card" aria-labelledby="live-map-heading">
        <div className="live-map-head">
          <h2 id="live-map-heading">{text.mapLabel}</h2>
          <span className="live-map-badge mono" role="status">{statusBadge}</span>
          {showAutoLayoutUnconfirmed(capture) && (
            <span className="live-auto-layout-tag">{text.autoLayoutUnconfirmed}</span>
          )}
        </div>
        <div className="live-map-box" aria-label={text.mapLabel}>
          <canvas
            ref={minimap.canvasRef}
            className={`live-map-canvas${stale ? " is-stale" : ""}`}
            aria-hidden="true"
          />
          {plate && <div className={`live-map-plate${plateTone}`}>{plate}</div>}
        </div>
        {invalidateError && <p className="live-error" role="alert">{invalidateError}</p>}
        <div className="live-map-actions">
          <button type="button" onClick={() => void invalidate()} disabled={invalidating} aria-busy={invalidating}>
            {text.mapWrong}
          </button>
        </div>
      </section>

      <div className="live-readings">
        <section className="live-danger-panel" aria-labelledby="live-danger-heading">
          <h2 id="live-danger-heading">{text.danger}</h2>
          <DangerMeter state={risk?.state ?? null} locale={locale} />
        </section>
        <section className="live-enemy-panel" aria-label={text.enemyCount("—")}>
          <EnemySeen risk={risk} locale={locale} />
        </section>
      </div>

      <ResourcePanel locale={locale} />
    </main>
  );
}
