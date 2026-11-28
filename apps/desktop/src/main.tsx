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
import { startBannerWarmup } from "./bannerWarmup";
import bannerWarmupUrl from "./assets/overlay/banner-warmup.webm";
import { HudStage } from "./HudStage";
import { OrbStage } from "./orb/OrbStage";
import { DraftStage } from "./draft/DraftStage";
import { installDraftMockCommands } from "./draft/draftMock";
import {
  emitOverlayReady,
  emitOverlayVisibility,
  subscribeOverlayDemo,
  subscribeOverlayContentVisibility,
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
const hudView = params.get("view") === "hud";
const orbView = params.get("view") === "orb";
const draftView = params.get("view") === "draft";
if (!overlayView && !hudView && !orbView && !draftView) installFrontendLogging(window, logHostInvoke);
document.documentElement.classList.toggle("overlay-body", overlayView);
document.body.classList.toggle("overlay-body", overlayView);
document.documentElement.classList.toggle("hud-body", hudView);
document.body.classList.toggle("hud-body", hudView);
document.documentElement.classList.toggle("orb-body", orbView);
document.body.classList.toggle("orb-body", orbView);
document.documentElement.classList.toggle("draft-body", draftView);
document.body.classList.toggle("draft-body", draftView);

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
  const windowsNativeOverlay = typeof window !== "undefined"
    && "__TAURI_INTERNALS__" in window
    && typeof navigator !== "undefined"
    && navigator.userAgent.includes("Windows");
  const [hostContentVisible, setHostContentVisible] = useState(false);
  const [hostVisibilityReady, setHostVisibilityReady] = useState(!windowsNativeOverlay);
  const [hostStatusSeen, setHostStatusSeen] = useState(false);
  const stopBannerWarmup = useRef<(() => void) | null>(null);
  const previousVisualId = useRef<string | null>(null);
  const visualId = playback.visual?.playback_id ?? null;
  const currentVisualId = useRef<string | null>(visualId);
  currentVisualId.current = visualId;

  useEffect(() => {
    let disposed = false;
    let stopStatus: (() => void) | undefined;
    let stopDemo: (() => void) | undefined;
    let stopContentVisibility: (() => void) | undefined;
    const status = subscribeOverlayStatus((payload) => {
      const media = mediaFromOverlayStatus(payload);
      if (!media) return;
      const at = performance.now();
      setHostStatusSeen(true);
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
    const contentVisibility = windowsNativeOverlay
      ? subscribeOverlayContentVisibility(setHostContentVisible)
      : Promise.resolve(() => {});
    void Promise.all([status, demo, contentVisibility]).then(([unlistenStatus, unlistenDemo, unlistenContentVisibility]) => {
      if (disposed) {
        unlistenStatus();
        unlistenDemo();
        unlistenContentVisibility();
        return;
      }
      stopStatus = unlistenStatus;
      stopDemo = unlistenDemo;
      stopContentVisibility = unlistenContentVisibility;
      setHostVisibilityReady(true);
      void emitOverlayReady().catch(() => {});
    }).catch(() => {
      // No renderer-side status is fabricated when host event registration fails.
    });
    return () => {
      disposed = true;
      stopStatus?.();
      stopDemo?.();
      stopContentVisibility?.();
    };
  }, [windowsNativeOverlay]);

  // Warm the banner video path once, invisibly, after the host's first status (host set up, window
  // mapped) and before any banner; a banner that starts first ends the warm-up at once.
  useEffect(() => {
    if (!hostStatusSeen || stopBannerWarmup.current || currentVisualId.current) return;
    stopBannerWarmup.current = startBannerWarmup(bannerWarmupUrl);
  }, [hostStatusSeen]);
  useEffect(() => {
    if (visualId) stopBannerWarmup.current?.();
  }, [visualId]);
  useEffect(() => () => stopBannerWarmup.current?.(), []);

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
    if (windowsNativeOverlay && !hostVisibilityReady) return;
    void emitOverlayVisibility(visible).catch(() => {});
  }, [visible, hostVisibilityReady, windowsNativeOverlay]);
  const contentVisible = windowsNativeOverlay ? visible && hostContentVisible : visible;

  return (
    <>
      <OverlayStage playback={playback} chip={chip.view} visible={contentVisible} />
      {MockHarness ? <MockHarness /> : null}
    </>
  );
}

async function start() {
  if (import.meta.env.DEV && params.get("view") === "s1c-native") {
    if (!root) return;
    const { mountNativeAccount } = await import("./account/s1c/native-account.mjs");
    await mountNativeAccount(root);
    return;
  }

  const mockUpdateScenario = params.get("mockUpdate");
  const mockHarnessRequested = params.get("mock") === "1"
    || mockUpdateScenario === "available"
    || mockUpdateScenario === "failed"
    || mockUpdateScenario === "up_to_date"
    || mockUpdateScenario === "downloading";
  const mockHarness = import.meta.env.DEV && mockHarnessRequested
    ? await import("./mock/harness")
    : null;
  mockHarness?.installMockHarness();
  if (mockHarness) installDraftMockCommands();

  if (!root) return;
  const MockScenarioSwitcher = mockHarness?.MockScenarioSwitcher;
  const OverlayMockHarness = mockHarness?.OverlayMockHarness;
  const HudMockHarness = mockHarness?.HudMockHarness;
  createRoot(root).render(
    <StrictMode>
      {draftView ? (
        <DraftStage />
      ) : orbView ? (
        <OrbStage />
      ) : hudView ? (
        <HudStage MockHarness={HudMockHarness} />
      ) : overlayView ? (
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
