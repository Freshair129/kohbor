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

export interface MediaPackBindingSummary {
  event_id: string;
  tier: number | null;
  selection: "random" | "shuffle" | "sequence" | null;
  variant_count: number;
  banner_count: number;
}

export interface MediaPackPresentationSummary {
  description: string | null;
  author: string | null;
  languages: string[];
  cover_url: string | null;
}

export interface MediaPackSummaryV1 {
  install_id: string;
  package_id: string;
  revision: number;
  active: boolean;
  pending: boolean;
  display_name: string | null;
  problem: "METADATA_UNAVAILABLE" | "COVER_INVALID" | null;
  cover_url: string | null;
  event_count: number | null;
  binding_count: number | null;
  variant_count: number | null;
  banner_count: number | null;
  bindings: MediaPackBindingSummary[];
}

export interface MediaPackSummaryV1_1 extends MediaPackSummaryV1 {
  authority_class: "system" | "local_import";
  publisher_claim: string | null;
  installed_at: number | null;
  size_bytes: number | null;
  game_id: "dota2";
  presentation: MediaPackPresentationSummary | null;
}

export type MediaPackSummaries =
  | { schema_version: "1.0.0"; records: MediaPackSummaryV1[] }
  | { schema_version: "1.1.0"; records: MediaPackSummaryV1_1[] };

function summaryObject(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown> : null;
}

function safeSummaryCount(value: unknown, minimum = 0, maximum = Number.MAX_SAFE_INTEGER): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= minimum && value <= maximum;
}

function safeSummaryText(value: unknown, maximum: number): value is string {
  return typeof value === "string" && [...value].length <= maximum
    && !/[\u0000-\u001f\u007f-\u009f\u2028\u2029]/u.test(value);
}

function validSummaryLanguageTag(tag: string): boolean {
  if (tag.length > 63 || !/^[A-Za-z0-9]+(?:-[A-Za-z0-9]+)*$/u.test(tag)) return false;
  const parts = tag.split("-");
  const alpha = (part: string) => /^[A-Za-z]+$/u.test(part);
  const alnum = (part: string) => /^[A-Za-z0-9]+$/u.test(part);
  const language = parts[0]!;
  if (language.toLowerCase() === "x") return parts.length > 1 && parts.slice(1).every(part => part.length <= 8);
  if (language.length < 2 || language.length > 8 || !alpha(language)) return false;
  let index = 1;
  if (language.length <= 3) {
    let count = 0;
    while (index < parts.length && parts[index]!.length === 3 && alpha(parts[index]!) && count < 3) { index += 1; count += 1; }
  }
  if (index < parts.length && parts[index]!.length === 4 && alpha(parts[index]!)) index += 1;
  if (index < parts.length && ((parts[index]!.length === 2 && alpha(parts[index]!))
    || (parts[index]!.length === 3 && /^[0-9]{3}$/u.test(parts[index]!)))) index += 1;
  const variants = new Set<string>();
  while (index < parts.length) {
    const part = parts[index]!;
    const variant = (part.length >= 5 && part.length <= 8 && alnum(part))
      || (part.length === 4 && /^[0-9][A-Za-z0-9]{3}$/u.test(part));
    if (!variant) break;
    const normalized = part.toLowerCase();
    if (variants.has(normalized)) return false;
    variants.add(normalized);
    index += 1;
  }
  const extensions = new Set<string>();
  while (index < parts.length && parts[index]!.length === 1 && parts[index]!.toLowerCase() !== "x") {
    const singleton = parts[index]!.toLowerCase();
    if (extensions.has(singleton)) return false;
    extensions.add(singleton);
    index += 1;
    const start = index;
    while (index < parts.length && parts[index]!.length >= 2 && parts[index]!.length <= 8) index += 1;
    if (index === start) return false;
  }
  if (index < parts.length && parts[index]!.toLowerCase() === "x") {
    index += 1;
    const start = index;
    while (index < parts.length && parts[index]!.length <= 8) index += 1;
    if (index === start) return false;
  }
  return index === parts.length;
}

function validPackCoverUrl(value: unknown, installId: string): value is string {
  return typeof value === "string"
    && value === `kohbor-media://localhost/v1/cover/${installId}/${value.slice(-64)}`
    && /^kohbor-media:\/\/localhost\/v1\/cover\/ins_[0-9]{10}\/[a-f0-9]{64}$/u.test(value);
}

function parsePackBindingSummary(value: unknown): MediaPackBindingSummary | null {
  const row = summaryObject(value);
  if (!row || typeof row.event_id !== "string" || !/^[a-z][a-z0-9_]{0,79}$/u.test(row.event_id)
    || (row.tier !== null && !safeSummaryCount(row.tier, 1, 100))
    || (row.selection !== null && row.selection !== "random" && row.selection !== "shuffle" && row.selection !== "sequence")
    || !safeSummaryCount(row.variant_count, 1, 16)
    || !safeSummaryCount(row.banner_count, 0, row.variant_count)) return null;
  return { event_id: row.event_id, tier: row.tier as number | null, selection: row.selection as MediaPackBindingSummary["selection"], variant_count: row.variant_count, banner_count: row.banner_count };
}

/** Parse both the legacy 1.0.0 shape and the host-owned 1.1.0 pack summary. */
export function parseMediaPackSummaries(value: unknown): MediaPackSummaries | null {
  const source = summaryObject(value);
  if (!source || (source.schema_version !== "1.0.0" && source.schema_version !== "1.1.0") || !Array.isArray(source.records)) return null;
  const version = source.schema_version;
  const seen = new Set<string>();
  const records: Array<MediaPackSummaryV1 | MediaPackSummaryV1_1> = [];
  const legacyKeys = ["install_id", "package_id", "revision", "active", "pending", "display_name", "problem", "cover_url", "event_count", "binding_count", "variant_count", "banner_count", "bindings"];
  const currentKeys = [...legacyKeys, "authority_class", "publisher_claim", "installed_at", "size_bytes", "game_id", "presentation"];
  for (const item of source.records) {
    const row = summaryObject(item);
    if (!row || Object.keys(row).some(key => !(version === "1.0.0" ? legacyKeys : currentKeys).includes(key))
      || typeof row.install_id !== "string" || !/^ins_[0-9]{10}$/u.test(row.install_id) || seen.has(row.install_id)
      || typeof row.package_id !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/u.test(row.package_id)
      || !safeSummaryCount(row.revision, 1) || typeof row.active !== "boolean" || typeof row.pending !== "boolean"
      || (row.display_name !== null && (!safeSummaryText(row.display_name, 96) || !row.display_name.trim()))
      || (row.problem !== null && row.problem !== "METADATA_UNAVAILABLE" && row.problem !== "COVER_INVALID")
      || (row.cover_url !== null && !validPackCoverUrl(row.cover_url, row.install_id))
      || !Array.isArray(row.bindings) || row.bindings.length > 128) return null;
    seen.add(row.install_id);
    const bindings = row.bindings.map(parsePackBindingSummary);
    if (bindings.some(binding => binding === null)) return null;
    const validBindings = bindings as MediaPackBindingSummary[];
    if (new Set(validBindings.map(binding => `${binding.event_id}:${binding.tier}`)).size !== validBindings.length) return null;
    const actualCounts = {
      event_count: new Set(validBindings.map(binding => binding.event_id)).size,
      binding_count: validBindings.length,
      variant_count: validBindings.reduce((total, binding) => total + binding.variant_count, 0),
      banner_count: validBindings.reduce((total, binding) => total + binding.banner_count, 0),
    };
    for (const key of ["event_count", "binding_count", "variant_count", "banner_count"] as const) {
      if (row[key] !== null && !safeSummaryCount(row[key])) return null;
      if (row[key] !== null && row[key] !== actualCounts[key]) return null;
    }
    if (version === "1.0.0") {
      records.push({
        install_id: row.install_id, package_id: row.package_id, revision: row.revision, active: row.active, pending: row.pending,
        display_name: row.display_name as string | null, problem: row.problem as MediaPackSummaryV1["problem"], cover_url: row.cover_url as string | null,
        event_count: row.event_count as number | null, binding_count: row.binding_count as number | null, variant_count: row.variant_count as number | null,
        banner_count: row.banner_count as number | null, bindings: validBindings,
      });
      continue;
    }
    if ((row.authority_class !== "system" && row.authority_class !== "local_import")
      || (row.publisher_claim !== null && (!safeSummaryText(row.publisher_claim, 96) || !row.publisher_claim))
      || (row.installed_at !== null && !safeSummaryCount(row.installed_at))
      || (row.size_bytes !== null && !safeSummaryCount(row.size_bytes))
      || row.game_id !== "dota2") return null;
    let presentation: MediaPackPresentationSummary | null = null;
    if (row.presentation !== null) {
      const item = summaryObject(row.presentation);
      if (!item || Object.keys(item).some(key => !["description", "author", "languages", "cover_url"].includes(key))
        || !(item.description === null || safeSummaryText(item.description, 500))
        || !(item.author === null || safeSummaryText(item.author, 80))
        || !Array.isArray(item.languages) || item.languages.length > 8
        || !item.languages.every(language => typeof language === "string" && validSummaryLanguageTag(language))
        || new Set((item.languages as string[]).map(language => language.toLowerCase())).size !== item.languages.length
        || !(item.cover_url === null || validPackCoverUrl(item.cover_url, row.install_id))
        || item.cover_url !== row.cover_url) return null;
      presentation = {
        description: item.description as string | null,
        author: item.author as string | null,
        languages: item.languages as string[],
        cover_url: item.cover_url as string | null,
      };
    } else if (row.cover_url !== null) return null;
    records.push({
      install_id: row.install_id, package_id: row.package_id, revision: row.revision, active: row.active, pending: row.pending,
      display_name: row.display_name as string | null, problem: row.problem as MediaPackSummaryV1["problem"], cover_url: row.cover_url as string | null,
      event_count: row.event_count as number | null, binding_count: row.binding_count as number | null, variant_count: row.variant_count as number | null,
      banner_count: row.banner_count as number | null, bindings: validBindings,
      authority_class: row.authority_class, publisher_claim: row.publisher_claim as string | null,
      installed_at: row.installed_at as number | null, size_bytes: row.size_bytes as number | null, game_id: "dota2", presentation,
    });
  }
  return version === "1.0.0"
    ? { schema_version: version, records: records as MediaPackSummaryV1[] }
    : { schema_version: version, records: records as MediaPackSummaryV1_1[] };
}

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
