import { StrictMode, useEffect, useRef, useState, type ComponentType } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { MediaProbeContext } from "./MediaProbeContext";
import {
  createChipState,
  overlayShouldShow,
  reduceChip,
  type ChipState,
} from "./overlayModel";
import { OverlayStage } from "./OverlayStage";
import {
  emitOverlayReady,
  emitOverlayVisibility,
  subscribeOverlayDemo,
  subscribeOverlayStatus,
  usePlayback,
} from "./runtime";
import type { RuntimeStatus } from "./types";
import "./styles.css";
import { installFrontendLogging } from "./logs/frontend";
import { logHostInvoke } from "./logs/host";

const root = document.getElementById("root");
const params = new URLSearchParams(window.location.search);
const overlayView = params.get("view") === "overlay";
if (!overlayView) installFrontendLogging(window, logHostInvoke);
document.documentElement.classList.toggle("overlay-body", overlayView);
document.body.classList.toggle("overlay-body", overlayView);

function mediaFromOverlayStatus(payload: unknown): RuntimeStatus["media"] | null {
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return null;
  const media = (payload as Record<string, unknown>).media;
  if (typeof media !== "object" || media === null || Array.isArray(media)) return null;
  const { state, reason } = media as Record<string, unknown>;
  if (state !== "ready" && state !== "not_ready" && state !== "unavailable") return null;
  if (reason !== null && typeof reason !== "string") return null;
  return { state, reason } as RuntimeStatus["media"];
}

function isDemoMarker(payload: unknown): boolean {
  return typeof payload === "object" && payload !== null && !Array.isArray(payload)
    && (payload as Record<string, unknown>).source === "demo";
}

function OverlayView({ MockHarness }: { MockHarness?: ComponentType }) {
  const playback = usePlayback();
  const [chip, setChip] = useState<ChipState>(createChipState);
  const [nowMs, setNowMs] = useState(() => performance.now());
  const previousVisualId = useRef<string | null>(null);
  const visualId = playback.visual?.playback_id ?? null;
  const currentVisualId = useRef<string | null>(visualId);
  currentVisualId.current = visualId;

  useEffect(() => {
    let disposed = false;
    let stopStatus: (() => void) | undefined;
    let stopDemo: (() => void) | undefined;
    const status = subscribeOverlayStatus((payload) => {
      const media = mediaFromOverlayStatus(payload);
      if (!media) return;
      const at = performance.now();
      setNowMs(at);
      setChip((state) => reduceChip(state, { type: "status", media, now_ms: at }));
    });
    const demo = subscribeOverlayDemo((payload) => {
      if (!isDemoMarker(payload)) return;
      const at = performance.now();
      setNowMs(at);
      setChip((state) => {
        const marked = reduceChip(state, { type: "demo_marker", now_ms: at });
        const currentPlaybackId = currentVisualId.current;
        return currentPlaybackId
          ? reduceChip(marked, {
              type: "playback_started",
              playback_id: currentPlaybackId,
              now_ms: at,
            })
          : marked;
      });
    });
    void Promise.all([status, demo]).then(([unlistenStatus, unlistenDemo]) => {
      if (disposed) {
        unlistenStatus();
        unlistenDemo();
        return;
      }
      stopStatus = unlistenStatus;
      stopDemo = unlistenDemo;
      void emitOverlayReady().catch(() => {});
    }).catch(() => {
      // No renderer-side status is fabricated when host event registration fails.
    });
    return () => {
      disposed = true;
      stopStatus?.();
      stopDemo?.();
    };
  }, []);

  useEffect(() => {
    const at = performance.now();
    setNowMs(at);
    const previous = previousVisualId.current;
    if (visualId && visualId !== previous) {
      setChip((state) => reduceChip(state, { type: "playback_started", playback_id: visualId, now_ms: at }));
    } else if (!visualId && previous) {
      setChip((state) => reduceChip(state, { type: "playback_ended", playback_id: previous, now_ms: at }));
    }
    previousVisualId.current = visualId;
  }, [visualId]);

  useEffect(() => {
    const deadline = chip.shownUntilMs;
    if (deadline === null || deadline <= nowMs) return;
    const timer = window.setTimeout(() => setNowMs(performance.now()), deadline - nowMs);
    return () => window.clearTimeout(timer);
  }, [chip.shownUntilMs, nowMs]);

  const visible = overlayShouldShow(playback, chip, nowMs);
  useEffect(() => {
    void emitOverlayVisibility(visible).catch(() => {});
  }, [visible]);

  return (
    <>
      <OverlayStage playback={playback} chip={chip.view} />
      {MockHarness ? <MockHarness /> : null}
    </>
  );
}

async function start() {
  const mockHarness = import.meta.env.DEV && params.get("mock") === "1"
    ? await import("./mock/harness")
    : null;
  mockHarness?.installMockHarness();

  if (!root) return;
  const MockScenarioSwitcher = mockHarness?.MockScenarioSwitcher;
  const OverlayMockHarness = mockHarness?.OverlayMockHarness;
  createRoot(root).render(
    <StrictMode>
      {overlayView ? (
        <OverlayView MockHarness={OverlayMockHarness} />
      ) : (
        <>
          <MediaProbeContext />
          <App devScenarioSwitcher={MockScenarioSwitcher ? <MockScenarioSwitcher /> : undefined} />
        </>
      )}
    </StrictMode>,
  );
}

void start();
