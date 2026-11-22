import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPlaybackState, parsePlaybackEvent, reducePlayback, tickPlayback, type PlaybackEndedEvent, type PlaybackStartedEvent, type PlaybackState } from "./playback";
import { activateArgs, inspectArgs, type ChosenActivationPolicy } from "./packs";
import { probeReportArgs, type MediaProbePendingResponse, type MediaProbeReport } from "./probe";
import type { OwnSnapshot, RuntimeStatus } from "./types";
import { parseOwnSnapshot } from "./ownSnapshot";
import type { HostClockAnchor } from "./hostClock";
import { createAudioPrefsClient } from "./audioPrefs";

const POLL_MS = 1000;
const CALL_TIMEOUT_MS = 2500;
/** Consecutive failed polls after which the screen says it has lost the host. */
const LOST_AFTER_FAILURES = 3;
const MAX_TIMEOUT_MS = 2_147_483_647;

export function subscribePlayback(onEvent: (event: unknown) => void): Promise<() => void> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return Promise.resolve(() => {});
  }
  return listen<unknown>("playback://event", (event) => onEvent(event.payload));
}

export function subscribeObjectiveAlerts(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("hud://objective-alert", (event) => onPayload(event.payload));
}

export function subscribeHudLocale(onLocale: (locale: "th" | "en") => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("hud://locale", (event) => {
    if (event.payload === "th" || event.payload === "en") onLocale(event.payload);
  });
}

export function publishHudLocale(locale: "th" | "en"): Promise<void> {
  return hostAvailable() ? emit("hud://locale", locale).catch(() => {}) : Promise.resolve();
}

export function subscribeHudLayout(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("hud://layout", (event) => onPayload(event.payload));
}

export function objectivePrefsGet(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("objective_prefs_get");
}

export function objectivePrefsPatch(request: Record<string, unknown>): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("objective_prefs_patch", { request });
}

export function shellHotkeyGet(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("shell_hotkey_get");
}

export function shellHotkeySet(enabled: boolean, binding: string): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("shell_hotkey_set", { enabled, binding });
}

/**
 * The local game reference screenshot used as the layout editor background, read through the same
 * dev-gated host path as the Lab (fixed file name, JPEG only, 8 MiB max, never bundled). Builds without
 * Lab, a missing file and any refusal all give null, which means the grid fallback.
 */
export async function layoutReferenceImage(): Promise<string | null> {
  if (!hostAvailable()) return null;
  try {
    const value = await invoke<unknown>("lab_reference_image");
    return typeof value === "string" && value.startsWith("data:image/jpeg;base64,") ? value : null;
  } catch {
    return null;
  }
}

export function layoutGet(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("layout_get");
}

export function layoutPatch(request: Record<string, unknown>): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("layout_patch", { request });
}

export function layoutReset(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
  return invoke<unknown>("layout_reset");
}

export function subscribeOverlayStatus(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("overlay://status", (event) => onPayload(event.payload));
}

export function subscribeOverlayDemo(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("overlay://demo", (event) => onPayload(event.payload));
}

export function subscribeOverlayContentVisibility(onVisible: (visible: boolean) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("overlay://content-visibility", (event) => {
    const payload = event.payload;
    if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return;
    const visible = (payload as Record<string, unknown>).visible;
    if (typeof visible === "boolean") onVisible(visible);
  });
}

export function emitOverlayReady(): Promise<void> {
  return hostAvailable() ? emit("overlay://ready", {}) : Promise.resolve();
}

export function emitOverlayVisibility(visible: boolean): Promise<void> {
  return hostAvailable() ? emit("overlay://visible", { visible }) : Promise.resolve();
}

export function emitOverlayVideoDiagnostic(payload: {
  playback_id: string;
  event_type: string | null;
  outcome: "started" | "failed";
  error_code: number | null;
  reason_code: "NONE" | "VIDEO_ERROR" | "VIDEO_URL_UNAVAILABLE" | "VIDEO_START_TIMEOUT";
}): Promise<void> {
  return hostAvailable()
    ? emit("overlay://video-diagnostic", payload).catch(() => {})
    : Promise.resolve();
}

export function reportMediaProbe(report: MediaProbeReport): Promise<{ accepted: boolean } | null> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return Promise.resolve(null);
  return invoke<{ accepted: boolean }>("media_probe_report", probeReportArgs(report));
}

export function demoAvailable(): Promise<boolean> {
  if (!hostAvailable()) return Promise.resolve(false);
  return invoke<boolean>("demo_available").catch(() => false);
}

export function demoTrigger(eventId: string, tier: number | null): Promise<void> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<void>("demo_trigger", { eventId, tier });
}

function hostAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const audioPrefsClient = createAudioPrefsClient({
  invoke: (command, args) => {
    if (!hostAvailable()) return Promise.reject(new Error("CAPABILITY_UNAVAILABLE"));
    return invoke<unknown>(command, args);
  },
  listen: (event, onPayload) => {
    if (!hostAvailable()) return Promise.resolve(() => {});
    return listen<unknown>(event, (value) => onPayload(value.payload));
  },
});

/**
 * Pack-store commands. Outside the Tauri webview there is no host, so each call
 * rejects with CAPABILITY_UNAVAILABLE; a missing host is never shown as an empty store.
 */
/** In-memory outcome history kept by the host; rejects with CAPABILITY_UNAVAILABLE outside the webview. */
export function audioHistory(limit: number): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("audio_history", { limit });
}

export function matchRecapCommand(command: "match_recap" | "clear_match_recap"): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>(command);
}

/** Build metadata is optional on older hosts; callers retain unknown on failure. */
export function appVersion(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("app_version");
}

/** Manual updater IPC. The renderer never supplies an endpoint or installation source. */
export function updateCommand(command: "update_status" | "update_check" | "update_install" | "update_open_release_page"): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  if (command === "update_install") return invoke<unknown>(command, { confirm: true });
  return invoke<unknown>(command);
}

export function mediaList(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_list");
}

export { parseMediaPackSummaries } from "./mediaPackSummaries";
export type { MediaPackBindingSummary, MediaPackPresentationSummary, MediaPackSummaryV1, MediaPackSummaryV1_1, MediaPackSummaries } from "./mediaPackSummaries";
import { parseMediaPackSummaries } from "./mediaPackSummaries";
import type { MediaPackSummaries } from "./mediaPackSummaries";

export function mediaPackSummaries(): Promise<MediaPackSummaries> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_pack_summaries").then(value => {
    const parsed = parseMediaPackSummaries(value);
    if (!parsed) throw new Error("MEDIA_SUMMARY_INVALID");
    return parsed;
  });
}

export function mediaEventMap(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_event_map");
}

export function mediaInstalledEventMap(installId: string): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_installed_event_map", { installId });
}

export interface MediaAuditionResult {
  audio_started: boolean;
  audio_ended: boolean;
  audio_end_reason: string | null;
  visual_status: "shown" | "failed" | "none" | "unknown";
  visual_reason: string | null;
  video_error_code: number | null;
}

export function mediaAudition(eventId: string, tier: number | null, overlay = false): Promise<MediaAuditionResult> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<MediaAuditionResult>("media_audition", { eventId, tier, overlay });
}

export function mediaAuditionVariant(eventId: string, tier: number | null, variantId: string, overlay = false): Promise<MediaAuditionResult> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<MediaAuditionResult>("media_audition_variant", { eventId, tier, variantId, overlay });
}

export function mediaAuditionInstalledVariant(installId: string, eventId: string, tier: number | null, variantId: string): Promise<MediaAuditionResult> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<MediaAuditionResult>("media_audition_installed_variant", { installId, eventId, tier, variantId });
}

export interface MediaVariantVisualUrl {
  visual_url: string | null;
}

function summaryObject(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function validMediaVisualUrl(value: unknown): value is string {
  return typeof value === "string"
    && /^kohbor-media:\/\/localhost\/v1\/ins_[0-9]{10}\/[a-f0-9]{64}$/u.test(value);
}

export function parseMediaVariantVisualUrl(value: unknown): MediaVariantVisualUrl | null {
  const row = summaryObject(value);
  if (!row || Object.keys(row).some(key => key !== "visual_url")
    || !(row.visual_url === null || validMediaVisualUrl(row.visual_url))) return null;
  return { visual_url: row.visual_url as string | null };
}

export function mediaVariantVisualUrl(installId: string, eventId: string, tier: number | null, variantId: string): Promise<MediaVariantVisualUrl> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_variant_visual_url", { installId, eventId, tier, variantId }).then(value => {
    const parsed = parseMediaVariantVisualUrl(value);
    if (!parsed) throw new Error("MEDIA_VARIANT_VISUAL_URL_INVALID");
    return parsed;
  });
}

export type MediaAuditionPlaybackEvent =
  | (PlaybackStartedEvent & { event_type: "audition" })
  | PlaybackEndedEvent;

export function parseMediaAuditionPlaybackEvent(value: unknown): MediaAuditionPlaybackEvent | null {
  const event = parsePlaybackEvent(value);
  if (!event) return null;
  if (event.kind === "started") {
    if (event.event_type !== "audition" || (event.visual_url !== null && !validMediaVisualUrl(event.visual_url))) return null;
    return event as PlaybackStartedEvent & { event_type: "audition" };
  }
  return event;
}

export function subscribeMediaAuditionEvents(onEvent: (event: MediaAuditionPlaybackEvent) => void): Promise<() => void> {
  const active = new Set<string>();
  return subscribePlayback(payload => {
    const event = parseMediaAuditionPlaybackEvent(payload);
    if (!event) return;
    if (event.kind === "started") {
      active.add(event.playback_id);
      onEvent(event);
    } else if (active.has(event.playback_id)) {
      active.delete(event.playback_id);
      onEvent(event);
    }
  });
}

export function subscribeAuditionMatch(onActive: (active: boolean) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("media://audition-match", ({ payload }) => {
    if (typeof payload === "boolean") onActive(payload);
  });
}

export function mediaInspect(sourcePath: string): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_inspect", inspectArgs(sourcePath));
}

export function mediaInstall(request: Record<string, unknown>): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_install", { request });
}

export function mediaActivate(requestId: string, installId: string, policy: ChosenActivationPolicy): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_activate", activateArgs(requestId, installId, policy));
}

export function mediaProbePending(): Promise<MediaProbePendingResponse> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return Promise.resolve([]);
  return invoke<MediaProbePendingResponse>("media_probe_pending");
}

export function subscribeMediaProbeRequest(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return Promise.resolve(() => {});
  }
  return listen<unknown>("media://probe-request", (event) => onPayload(event.payload));
}

/** Lifecycle rendering only; native remains the sole owner of playback selection. */
export interface PlaybackView extends PlaybackState { clockAnchor: HostClockAnchor | null }

export function usePlayback(): PlaybackView {
  const [state, setState] = useState(createPlaybackState);
  const [clockAnchor, setClockAnchor] = useState<HostClockAnchor | null>(null);
  const eventClockOffset = useRef<number | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribePlayback((input) => {
      const event = parsePlaybackEvent(input);
      if (event) setClockAnchor(previous => previous && event.at_ms < previous.hostMs
        ? null : { hostMs: event.at_ms, localMs: performance.now() });
      if (event?.kind === "started") eventClockOffset.current = performance.now() - event.at_ms;
      setState((current) => reducePlayback(current, event ?? input));
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // A missing event listener cannot be replaced with renderer-side playback.
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    const deadlines = [state.audioEndAtMs, state.failSafeAtMs].filter((value): value is number => value !== null);
    if (deadlines.length === 0) return;
    const deadline = Math.min(...deadlines);

    const nowInEventClock = performance.now() - (eventClockOffset.current ?? performance.now());
    let remaining = Math.max(0, deadline - nowInEventClock);
    let timer = 0;
    const schedule = () => {
      const wait = Math.min(remaining, MAX_TIMEOUT_MS);
      timer = window.setTimeout(() => {
        remaining -= wait;
        if (remaining > 0) {
          schedule();
          return;
        }
        setState((current) =>
          current.currentPlaybackId === state.currentPlaybackId ? tickPlayback(current, deadline) : current,
        );
      }, wait);
    };
    schedule();
    return () => window.clearTimeout(timer);
  }, [state.currentPlaybackId, state.failSafeAtMs, state.audioEndAtMs]);

  return { ...state, clockAnchor };
}

function withTimeout<T>(work: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const id = window.setTimeout(() => reject(new Error("timeout")), ms);
    work.then(
      (value) => {
        window.clearTimeout(id);
        resolve(value);
      },
      (error: unknown) => {
        window.clearTimeout(id);
        reject(error);
      },
    );
  });
}

export interface Runtime {
  status: RuntimeStatus | null;
  snapshot: OwnSnapshot | null;
  /**
   * True when the host has stopped answering. The last values stay on screen but
   * must be presented as not current: a silent poll failure would otherwise leave
   * an old "fresh" reading looking live.
   */
  hostLost: boolean;
  /** Runs setup. Resolves to an error code, or null on success. */
  setup: (replace: boolean) => Promise<string | null>;
}

export function useRuntime(): Runtime {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [snapshot, setSnapshot] = useState<OwnSnapshot | null>(null);
  const [failures, setFailures] = useState(0);
  const refresh = useRef<() => void>(() => {});

  useEffect(() => {
    let alive = true;
    let running = false;
    let timer: number | undefined;

    const tick = async () => {
      if (running || !alive) return;
      running = true;
      window.clearTimeout(timer);
      timer = undefined;
      try {
        const [s, o] = await withTimeout(
          Promise.all([invoke<RuntimeStatus>("runtime_get_status"), invoke<unknown>("own_snapshot")]),
          CALL_TIMEOUT_MS,
        );
        const parsedSnapshot = parseOwnSnapshot(o);
        if (!parsedSnapshot) {
          if (alive) setSnapshot(null);
          throw new Error("OWN_SNAPSHOT_UNREADABLE");
        }
        if (alive) {
          setStatus(s);
          setSnapshot(parsedSnapshot);
          setFailures(0);
        }
      } catch {
        // Never fabricate readiness from a failed poll; count it instead.
        if (alive) setFailures((n) => n + 1);
      } finally {
        running = false;
      }
      // A hidden window does not poll; visibilitychange resumes it.
      if (alive && !document.hidden) timer = window.setTimeout(() => void tick(), POLL_MS);
    };

    const onVisibility = () => {
      if (!document.hidden) void tick();
    };
    refresh.current = () => void tick();
    document.addEventListener("visibilitychange", onVisibility);
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);

  const setup = useCallback(async (replace: boolean) => {
    try {
      await invoke("gsi_setup", { replace });
      return null;
    } catch (e) {
      return String(e);
    } finally {
      refresh.current();
    }
  }, []);

  return { status, snapshot, hostLost: failures >= LOST_AFTER_FAILURES, setup };
}
