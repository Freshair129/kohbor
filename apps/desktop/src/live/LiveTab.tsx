import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { DangerMeter } from "./DangerMeter";
import { EnemySeen } from "./EnemySeen";
import { liveCopy, phaseLabel, type LiveLocale } from "./copy";
import { captureGatedRiskView, formatNullable, showAutoLayoutUnconfirmed } from "./state";
import { liveMockRequested } from "./mockMode";
import {
  CAPTURE_DISPLAY_RATES,
  applyProposal,
  capturePictureEnabled,
  grow,
  isCapturePaused,
  nudge,
  shrink,
  togglePause,
} from "./captureConfig";
import { ResourcePanel } from "./ResourcePanel";
import { ResourceStrip } from "./ResourceStrip";
import { previewRateText, visibilityText } from "./preview";
import { useCaptureStatus } from "./useCaptureStatus";
import { useCaptureConfig } from "./useCaptureConfig";
import { useMinimap } from "./useMinimap";
import { useRisk } from "./useRisk";
import { usePageVisibility } from "./usePageVisibility";
import { useCanvasBox } from "./useCanvasBox";
import { advanceInvalidation, phaseReasonText, UNSUPPORTED_CAPTURE_REASONS } from "./captureRecovery";
import "./live.css";

export function LiveTab({ locale = "th" }: { locale?: LiveLocale }) {
  const text = liveCopy(locale);
  const capture = useCaptureStatus();
  const captureConfig = useCaptureConfig();
  const risk = useRisk();
  const [invalidating, setInvalidating] = useState(false);
  const [invalidateError, setInvalidateError] = useState<string | null>(null);
  const [armedUntil, setArmedUntil] = useState<number | null>(null);
  const [locallyInvalidated, setLocallyInvalidated] = useState(false);
  const userInvalidated = locallyInvalidated || capture?.phase_reason === "user_invalidated";
  const phase = userInvalidated ? "unsupported" : capture?.phase ?? null;
  const reason = userInvalidated ? "user_invalidated" : capture?.phase_reason ?? null;
  const controlsBusy = invalidating || captureConfig.busy;
  const paused = isCapturePaused(captureConfig.config?.paused, capture?.paused ?? null);
  const canShowPicture = capturePictureEnabled(phase, paused);
  const minimap = useMinimap(capture?.calibration_generation ?? null, canShowPicture);
  const canvasBox = useCanvasBox(minimap.frameSize);
  const visibilityNotice = visibilityText(usePageVisibility(), locale);
  const mock = liveMockRequested();

  useEffect(() => {
    if (armedUntil === null) return;
    const timer = window.setTimeout(() => setArmedUntil(null), Math.max(0, armedUntil - performance.now()));
    return () => window.clearTimeout(timer);
  }, [armedUntil]);

  useEffect(() => {
    // Once the host acknowledges the reason, subsequent host statuses own the recovery state.
    if (locallyInvalidated && capture?.phase_reason === "user_invalidated") setLocallyInvalidated(false);
  }, [locallyInvalidated, capture?.phase_reason]);

  const clearInvalidationAfter = async (action: Promise<boolean>) => {
    setArmedUntil(null);
    if (await action) setLocallyInvalidated(false);
  };

  const retry = () => clearInvalidationAfter(captureConfig.retryCapture());

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
      setLocallyInvalidated(true);
    } catch (failure) {
      setInvalidateError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setInvalidating(false);
    }
  };

  const markWrong = () => {
    const next = advanceInvalidation(armedUntil, performance.now());
    setArmedUntil(next.armedUntil);
    if (next.invoke) void invalidate();
  };

  const chooseMockReason = async (selection: string) => {
    setLocallyInvalidated(false);
    setArmedUntil(null);
    setInvalidateError(null);
    try {
      const driver = await import("./mock");
      driver.setMockCaptureReason(selection);
    } catch (failure) {
      setInvalidateError(failure instanceof Error ? failure.message : String(failure));
    }
  };

  const age = formatNullable(capture?.frame_age_ms ?? null);
  const reasonText = phaseReasonText(phase, reason, locale);
  const phaseText = reasonText
    ? `${phaseLabel(phase, locale)} · ${reasonText}`
    : phaseLabel(phase, locale);
  const statusBadge = `${phaseText} · ${age} ms`;
  const rateBadge = previewRateText(capture?.fps_measured ?? null, minimap.metrics, locale);
  const stale = phase === "stale";

  let plate: string | null = null;
  let plateTone = "";
  if (phase === "unsupported") {
    plate = reasonText;
  } else if (paused) {
    plate = phaseReasonText(phase, "paused", locale);
  } else {
    switch (phase) {
      case "coverage_unavailable":
        plate = text.noVisibility;
        plateTone = " is-critical";
        break;
      case "lost":
        plate = reasonText ?? text.lost;
        plateTone = " is-warning";
        break;
      case "calibrating":
        plate = reasonText ?? text.calibrating;
        break;
      case "stale":
        plate = text.stale(age);
        break;
      default:
        if (!canShowPicture || !minimap.hasFrame) plate = text.waitingFrame;
    }
  }

  const config = captureConfig.config;
  const effectiveRect = config?.effective_rect ?? null;
  const displayedRisk = userInvalidated
    ? captureGatedRiskView(risk?.state ?? null, risk?.lastVisionEvent ?? null, null)
    : risk;
  const mockReason = reason ?? (phase === "unsupported" ? "none" : "live");
  const mockReasonOptions: readonly string[] = [...UNSUPPORTED_CAPTURE_REASONS, "paused", "reopening", "access_lost", "calibrating", "unknown_mock_reason"];
  const proposal = config?.proposal ?? null;
  const proposalPatch = proposal && config?.profile_rect ? applyProposal(proposal, config.profile_rect) : null;
  const proposalVisible = proposal !== null && !captureConfig.proposalDismissed;
  const proposalLabel = config?.proposal_status === "searching"
    ? text.proposalSearching
    : config?.proposal_status === "found"
      ? text.proposalFound
      : config?.proposal_status === "not_found"
        ? text.proposalNotFound
        : text.proposalNone;

  const nudgeCrop = (moveX: -1 | 0 | 1, moveY: -1 | 0 | 1, shiftKey: boolean) => {
    if (!config) return;
    const amount = shiftKey ? 5 : 1;
    const patch = nudge(config.crop.dx, config.crop.dy, { dx: moveX * amount, dy: moveY * amount });
    if (patch.crop) void clearInvalidationAfter(captureConfig.setCrop(patch.crop));
  };

  return (
    <main className="live-tab live-dashboard" lang={locale}>
      <section className="live-map-pane" aria-labelledby="live-map-heading">
        <header className="live-map-toolbar">
          <h1 id="live-map-heading">{text.mapLabel}</h1>
          <span className="live-map-badge mono" role="status" title={statusBadge}>{statusBadge}</span>
          <span className="live-preview-rates mono" title={rateBadge}>{rateBadge}</span>
        </header>
        <div className="live-map-stage" ref={canvasBox.stageRef} aria-label={text.mapLabel}>
          <canvas
            ref={minimap.canvasRef}
            style={canvasBox.canvasStyle}
            className={`live-map-canvas${stale ? " is-stale" : ""}`}
            aria-hidden="true"
          />
          {plate && (
            <div className={`live-map-plate${plateTone}`}>
              <span>{plate}</span>
              {userInvalidated && <button type="button" disabled={controlsBusy} onClick={() => void retry()}>{text.retryCapture}</button>}
            </div>
          )}
        </div>
      </section>
      <aside className="live-sidebar" aria-label={text.title}>
        <section className="live-danger-panel" aria-labelledby="live-danger-heading">
          <h2 id="live-danger-heading">{text.danger}</h2>
          <DangerMeter state={displayedRisk?.state ?? null} locale={locale} />
        </section>
        <section className="live-enemy-panel" aria-label={text.enemyCount("—")}>
          <EnemySeen risk={displayedRisk} locale={locale} />
        </section>
        <ResourceStrip locale={locale} capture={capture} metrics={minimap.metrics} />
        <div className="live-sidebar-status">
          {mock && <span className="live-experimental">{text.mockPreview}</span>}
          {showAutoLayoutUnconfirmed(capture) && <span className="live-auto-layout-tag">{text.autoLayoutUnconfirmed}</span>}
          {visibilityNotice && <span className="live-visibility-notice" role="status">{visibilityNotice}</span>}
          {minimap.error && <span className="live-error" role="alert">{minimap.error}</span>}
          {captureConfig.error && <span className="live-error" role="alert">{captureConfig.error}</span>}
        </div>
        <div className="live-sidebar-actions">
          <details className="live-controls-disclosure">
            <summary>{text.cropSettings}</summary>
            <div className="live-disclosure-body live-controls-popover">
              <section className="live-capture-controls" aria-labelledby="live-capture-controls-heading">
                <h3 id="live-capture-controls-heading">{text.captureControls}</h3>
                <p className="live-effective-rect mono">
                  {text.effectiveRect}: {effectiveRect ? `x ${effectiveRect.x} · y ${effectiveRect.y} · w ${effectiveRect.width} · h ${effectiveRect.height}` : "—"}
                </p>
                <div className="live-capture-control-row">
                  <span>{text.cropControls}</span>
                  <span className="live-crop-offsets mono">
                    {config ? `dx ${config.crop.dx} · dy ${config.crop.dy} · dw ${config.crop.dw} · dh ${config.crop.dh}` : "—"}
                  </span>
                  <div className="live-crop-nudges" role="group" aria-label={text.cropControls}>
                    <button type="button" aria-label={text.cropLeft} disabled={!config || controlsBusy}
                      onClick={(event) => nudgeCrop(-1, 0, event.shiftKey)}>←</button>
                    <button type="button" aria-label={text.cropRight} disabled={!config || controlsBusy}
                      onClick={(event) => nudgeCrop(1, 0, event.shiftKey)}>→</button>
                    <button type="button" aria-label={text.cropUp} disabled={!config || controlsBusy}
                      onClick={(event) => nudgeCrop(0, -1, event.shiftKey)}>↑</button>
                    <button type="button" aria-label={text.cropDown} disabled={!config || controlsBusy}
                      onClick={(event) => nudgeCrop(0, 1, event.shiftKey)}>↓</button>
                  </div>
                  <button type="button" disabled={!config || !grow(config, 2) || controlsBusy}
                    aria-label={text.growCrop} onClick={() => { if (config) { const patch = grow(config, 2); if (patch?.crop) void clearInvalidationAfter(captureConfig.setCrop(patch.crop)); } }}>
                    + 2 px
                  </button>
                  <button type="button" disabled={!config || !shrink(config, 2) || controlsBusy}
                    aria-label={text.shrinkCrop} onClick={() => { if (config) { const patch = shrink(config, 2); if (patch?.crop) void clearInvalidationAfter(captureConfig.setCrop(patch.crop)); } }}>
                    − 2 px
                  </button>
                  <button type="button" disabled={!config || controlsBusy} onClick={() => void clearInvalidationAfter(captureConfig.resetCrop())}>
                    {text.resetCrop}
                  </button>
                </div>
                <div className="live-capture-control-row">
                  <button type="button" disabled={!config || controlsBusy}
                    onClick={() => void captureConfig.proposeCrop()}>{text.detectCrop}</button>
                  <span role="status">{proposalLabel}</span>
                  {proposalVisible && proposal && (
                    <span className="live-proposal-rect mono">
                      x {proposal.x} · y {proposal.y} · w {proposal.width} · h {proposal.height}
                    </span>
                  )}
                  {proposalVisible && proposal && (
                    <>
                      <button type="button" disabled={!proposalPatch || controlsBusy}
                        onClick={() => void clearInvalidationAfter(captureConfig.acceptProposal())}>{text.proposalAccept}</button>
                      <button type="button" disabled={controlsBusy} onClick={captureConfig.dismissProposal}>
                        {text.proposalDismiss}
                      </button>
                    </>
                  )}
                  {config && capture?.crop_confirmed !== true && (
                    <button type="button" disabled={controlsBusy} onClick={() => void clearInvalidationAfter(captureConfig.confirmCrop())}>
                      {text.confirmCrop}
                    </button>
                  )}
                </div>
                <div className="live-capture-control-row">
                  <label className="live-display-rate">
                    <span>{text.displayRate}</span>
                    <select
                      value={config?.display_fps === undefined ? "" : String(config?.display_fps ?? "")}
                      disabled={!config || controlsBusy}
                      onChange={(event) => {
                        const rate = Number(event.currentTarget.value);
                        if (CAPTURE_DISPLAY_RATES.includes(rate as (typeof CAPTURE_DISPLAY_RATES)[number])) {
                          void captureConfig.setDisplayRate(rate as (typeof CAPTURE_DISPLAY_RATES)[number]);
                        }
                      }}
                    >
                      {!config && <option value="" disabled>—</option>}
                      {CAPTURE_DISPLAY_RATES.map((rate) => <option value={rate} key={rate}>{rate} {text.rateUnit}</option>)}
                    </select>
                  </label>
                  <button type="button" disabled={!config || controlsBusy}
                    aria-pressed={paused}
                    onClick={() => {
                      if (!config) return;
                      const nextPause = togglePause(config.paused).paused;
                      if (nextPause !== undefined) void captureConfig.setPaused(nextPause);
                    }}>
                    {paused ? text.resumeCapture : text.pauseCapture}
                  </button>
                </div>
                {captureConfig.error && <p className="live-error" role="alert">{captureConfig.error}</p>}
                {mock && (
                  <label className="live-mock-reason-selector">
                    <span>{text.mockCaptureReason}</span>
                    <select value={mockReason} disabled={controlsBusy} onChange={(event) => void chooseMockReason(event.currentTarget.value)}>
                      <option value="live">{text.mockLive}</option>
                      <option value="none">{text.mockNoReason}</option>
                      {mockReasonOptions.map((code) => <option key={code} value={code}>{code}</option>)}
                      {mockReason !== "none" && mockReason !== "live" && !mockReasonOptions.includes(mockReason) && <option value={mockReason}>{mockReason}</option>}
                    </select>
                  </label>
                )}
                <section className="live-mark-wrong-section" aria-labelledby="live-map-feedback-heading">
                  <h4 id="live-map-feedback-heading">{text.mapFeedback}</h4>
                  <button type="button" onClick={markWrong} disabled={controlsBusy || userInvalidated} aria-busy={invalidating}>
                    {armedUntil !== null ? text.confirmMarkWrong : text.mapWrong}
                  </button>
                  {userInvalidated && <button type="button" disabled={controlsBusy} onClick={() => void retry()}>{text.retryCapture}</button>}
                  {invalidateError && <p className="live-error" role="alert">{invalidateError}</p>}
                </section>
              </section>
            </div>
          </details>
          <details className="live-resource-disclosure">
            <summary>{text.resourceDetails}</summary>
            <div className="live-disclosure-body live-resource-popover">
              <p className="live-preview-diagnostics mono">
                grab {formatNullable(capture?.grab_fps ?? null, 1)} fps · {text.previewDropped} {formatNullable(minimap.metrics.dropped_count)}
                {` · ${text.previewCost} ${formatNullable(minimap.metrics.parse_paint_ms_mean, 2)} ms`}
                {` · ${text.sentTotal} ${formatNullable(capture?.sent_total ?? null)} · ${text.grabTotal} ${formatNullable(capture?.grab_total ?? null)}`}
              </p>
              <ResourcePanel locale={locale} />
            </div>
          </details>
        </div>
      </aside>
    </main>
  );
}
