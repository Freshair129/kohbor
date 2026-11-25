import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import type { RiskView } from "./types";
import { DangerMeter } from "./DangerMeter";
import { EnemySeen } from "./EnemySeen";
import { liveCopy, proposalStatusLabel, type LiveLocale } from "./copy";
import { captureGatedRiskView } from "./state";
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
import { useCaptureStatus } from "./useCaptureStatus";
import { useCaptureConfig } from "./useCaptureConfig";
import { useMinimap } from "./useMinimap";
import { useRisk } from "./useRisk";
import { useCanvasBox } from "./useCanvasBox";
import { advanceInvalidation, UNSUPPORTED_CAPTURE_REASONS } from "./captureRecovery";
import { useCaptureCounterRates } from "./useCaptureCounterRates";
import { PreviewDiagnosticsPanel } from "./PreviewDiagnosticsPanel";
import { deriveMinimapPresentation } from "./minimapPresentation";
import { MinimapStage } from "./MinimapStage";
import { deriveMinimapState } from "./minimapStageModel";
import type { OwnSnapshot } from "../types";
import { ScoreboardHeader } from "./ScoreboardHeader";
import type { ScoreSample } from "./scoreboardModel";
import { useScoreSamples } from "./useScoreSamples";
import type { SteamStatus } from "../steam/model";
import "./live.css";
import { DialogButton } from "../shared/Dialog";

export function MinimapPanel({
  locale = "th",
  layout = "standalone",
  snapshot = null,
  gamePhase = null,
  renderAlerts,
  toolbarExtra,
  steamStatus = null,
  mapEdge = "right",
  mapOverlay,
  scoreSamples: sharedScoreSamples,
  stageOverride,
  stageAlerts,
}: {
  locale?: LiveLocale;
  /** "pinned" is second-monitor mode; "field" is the centred in-match map without a side panel. */
  layout?: "standalone" | "deck" | "pinned" | "field" | "settings";
  snapshot?: OwnSnapshot | null;
  gamePhase?: string | null;
  renderAlerts?: (input: { risk: RiskView | null; captureLive: boolean }) => ReactNode;
  toolbarExtra?: ReactNode;
  steamStatus?: SteamStatus | null;
  /** Inner edge used by the left/right second-monitor map stage. */
  mapEdge?: "left" | "right";
  /** Pinned layout only: drawn over the minimap canvas at the same size (UI-018 rune countdowns). */
  mapOverlay?: ReactNode;
  /** Optional shared stream so the scoreboard and pinned kill timeline use identical score samples. */
  scoreSamples?: readonly ScoreSample[];
  /** Pinned layout only: replaces the minimap stage inside the same square (UI-019 draft board). */
  stageOverride?: ReactNode;
  /**
   * Pinned layout only: the alert strip, anchored inside the map stage. The toolbar above the stage wraps to more lines
   * when its notices do not fit, so an offset measured from the pane top would paint over the toolbar text.
   */
  stageAlerts?: (input: { risk: RiskView | null; captureLive: boolean }) => ReactNode;
}) {
  const text = liveCopy(locale);
  const capture = useCaptureStatus();
  const counterRates = useCaptureCounterRates();
  const captureConfig = useCaptureConfig();
  const risk = useRisk();
  const [invalidating, setInvalidating] = useState(false);
  const [invalidateError, setInvalidateError] = useState<string | null>(null);
  const [armedUntil, setArmedUntil] = useState<number | null>(null);
  const [locallyInvalidated, setLocallyInvalidated] = useState(false);
  const userInvalidated = capture?.phase !== "idle" && (locallyInvalidated || capture?.phase_reason === "user_invalidated");
  const phase = userInvalidated ? "unsupported" : capture?.phase ?? null;
  const reason = userInvalidated ? "user_invalidated" : capture?.phase_reason ?? null;
  const controlsBusy = invalidating || captureConfig.busy;
  const paused = isCapturePaused(captureConfig.config?.paused, capture?.paused ?? null);
  const canShowPicture = capturePictureEnabled(phase, paused);
  const minimap = useMinimap(capture?.calibration_generation ?? null, canShowPicture);
  const canvasBox = useCanvasBox(minimap.frameSize, layout === "pinned");
  const mock = liveMockRequested();
  const localScoreSamples = useScoreSamples(snapshot, gamePhase, snapshot?.session_epoch ?? null, false, sharedScoreSamples === undefined);
  const scoreSamples = sharedScoreSamples ?? localScoreSamples;

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

  const presentation = deriveMinimapPresentation(phase, reason, minimap.hasFrame, locale);
  const mapSource = deriveMinimapState(steamStatus, capture, paused || userInvalidated);

  const config = captureConfig.config;
  const effectiveRect = config?.effective_rect ?? null;
  const displayedRisk = userInvalidated
    ? captureGatedRiskView(risk?.state ?? null, risk?.lastVisionEvent ?? null, null)
    : risk;
  const mockReason = reason ?? (phase === "unsupported" ? "none" : "live");
  const mockReasonOptions: readonly string[] = [...UNSUPPORTED_CAPTURE_REASONS, "waiting_for_game", "occluded", "paused", "reopening", "access_lost", "calibrating", "unknown_mock_reason"];
  const proposal = config?.proposal ?? null;
  const proposalPatch = proposal && config?.profile_rect ? applyProposal(proposal, config.profile_rect) : null;
  const proposalVisible = proposal !== null && !captureConfig.proposalDismissed;
  const proposalLabel = proposalStatusLabel(config, locale);

  const nudgeCrop = (moveX: -1 | 0 | 1, moveY: -1 | 0 | 1, shiftKey: boolean) => {
    if (!config) return;
    const amount = shiftKey ? 5 : 1;
    const patch = nudge(config.crop.dx, config.crop.dy, { dx: moveX * amount, dy: moveY * amount });
    if (patch.crop) void clearInvalidationAfter(captureConfig.setCrop(patch.crop));
  };

  const pinned = layout === "pinned" || layout === "field";
  const field = layout === "field";
  const stackedPinned = layout === "pinned";
  const actions = (
        <div className="live-sidebar-actions">
          <DialogButton label={text.cropSettings} closeLabel={text.closeDialog} className="live-dialog">
            <div className="live-controls-popover">
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
          </DialogButton>
          <DialogButton label={text.resourceDetails} closeLabel={text.closeDialog} className="live-dialog">
            <div className="live-resource-popover">
              <PreviewDiagnosticsPanel capture={capture} metrics={minimap.metrics} counterRates={counterRates} locale={locale} />
              <ResourcePanel locale={locale} />
            </div>
          </DialogButton>
        </div>
  );

  const pinnedAlerts = pinned && stageAlerts
    ? <div className="sm-alerts">{stageAlerts({ risk: displayedRisk, captureLive: phase === "live" && !paused })}</div> : null;
  const toolbar = (
    <header className={`live-map-toolbar${field ? " is-field" : ""}${stackedPinned ? " is-pinned" : ""}`}>
      <ScoreboardHeader snapshot={snapshot} phase={gamePhase} locale={locale === "en" ? "en" : "th"} scoreSamples={scoreSamples} />
      {pinned && toolbarExtra && <div className="live-map-tools">{toolbarExtra}</div>}
    </header>
  );
  const stage = pinned && stageOverride ? <div className="live-map-stage live-map-override">{stageOverride}{pinnedAlerts}</div> : <MinimapStage source={mapSource} hasFrame={minimap.hasFrame} covered={presentation.coveredBadge !== null && !paused && !userInvalidated}
    stageRef={canvasBox.stageRef} locale={locale}>
    <canvas
      ref={minimap.canvasRef}
      style={canvasBox.canvasStyle}
      className={`live-map-canvas${presentation.dimmed || paused || userInvalidated ? " is-stale" : ""}`}
      aria-hidden="true"
    />
    {stackedPinned && (gamePhase === "pregame" || gamePhase === "live") && minimap.hasFrame && !stageOverride && mapOverlay
      && <div className="sm-map-overlay" style={{ width: "100%", height: "100%" }}>{mapOverlay}</div>}
    {pinnedAlerts}
  </MinimapStage>;
  const mapPane = (
    <section className="live-map-pane" aria-label={locale === "en" ? "Live minimap and scoreboard" : "แผนที่ย่อสดและสกอร์บอร์ด"}>
      {stackedPinned ? (
        <div className={`live-map-body is-pinned${stageOverride ? " is-draft" : ""}`} data-map-edge={mapEdge}>
          <div className="live-map-pinned-stack">
            {toolbar}
            <div className={stageOverride ? "live-map-draft-slot" : "live-map-square-slot"}>{stage}</div>
          </div>
        </div>
      ) : field ? (
        <>
          {toolbar}
          <div className="live-map-body is-field"><div className="live-map-square-slot">{stage}</div></div>
        </>
      ) : (
        <>{toolbar}{stage}</>
      )}
    </section>
  );

  const panelContents = (
    <>
      {mapPane}
      <aside className={`live-sidebar${layout === "deck" ? " live-sidebar-deck" : ""}`} aria-label={text.title}>
        <section className="live-danger-panel" aria-labelledby="live-danger-heading">
          <h2 id="live-danger-heading">{text.danger}</h2>
          <DangerMeter state={displayedRisk?.state ?? null} locale={locale} />
        </section>
        <section className="live-enemy-panel" aria-label={text.enemyCount("—")}>
          <EnemySeen risk={displayedRisk} locale={locale} />
        </section>
        {layout === "deck" && renderAlerts && <div className="live-deck-alerts">{renderAlerts({ risk: displayedRisk, captureLive: phase === "live" && !paused })}</div>}
      </aside>
    </>
  );

  return layout === "settings" ? (
    <section className="live-capture-settings" aria-label={locale === "en" ? "Minimap capture settings" : "ตั้งค่าแผนที่ย่อและการจับภาพ"}>
      {actions}
    </section>
  ) : layout === "pinned" ? (
    <section className="live-pinned-panel" lang={locale}>
      {mapPane}
      {renderAlerts?.({ risk: displayedRisk, captureLive: phase === "live" && !paused })}
    </section>
  ) : field ? (
    mapPane
  ) : layout === "deck" ? (
    <section className="live-tab live-dashboard live-deck-panel" lang={locale}>
      {panelContents}
    </section>
  ) : panelContents;
}
