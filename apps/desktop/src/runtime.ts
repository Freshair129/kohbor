import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { createPlaybackState, parsePlaybackEvent, reducePlayback, tickPlayback, type PlaybackState } from "./playback";
import { activateArgs, inspectArgs, type ChosenActivationPolicy } from "./packs";
import { probeReportArgs, type MediaProbePendingResponse, type MediaProbeReport } from "./probe";
import type { OwnSnapshot, RuntimeStatus } from "./types";
import { parseOwnSnapshot } from "./ownSnapshot";
import type { HostClockAnchor } from "./hostClock";

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

export function subscribeOverlayStatus(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("overlay://status", (event) => onPayload(event.payload));
}

export function subscribeOverlayDemo(onPayload: (payload: unknown) => void): Promise<() => void> {
  if (!hostAvailable()) return Promise.resolve(() => {});
  return listen<unknown>("overlay://demo", (event) => onPayload(event.payload));
}

export function emitOverlayReady(): Promise<void> {
  return hostAvailable() ? emit("overlay://ready", {}) : Promise.resolve();
}

export function emitOverlayVisibility(visible: boolean): Promise<void> {
  return hostAvailable() ? emit("overlay://visible", { visible }) : Promise.resolve();
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

/**
 * Pack-store commands. Outside the Tauri webview there is no host, so each call
 * rejects with CAPABILITY_UNAVAILABLE; a missing host is never shown as an empty store.
 */
/** In-memory outcome history kept by the host; rejects with CAPABILITY_UNAVAILABLE outside the webview. */
export function audioHistory(limit: number): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("audio_history", { limit });
}

/** Build metadata is optional on older hosts; callers retain unknown on failure. */
export function appVersion(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("app_version");
}

export function mediaList(): Promise<unknown> {
  if (!hostAvailable()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<unknown>("media_list");
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
