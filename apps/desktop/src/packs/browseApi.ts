import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
// @ts-ignore Explicit source extensions support the Node test runner.
import { parseMediaEventMap, type EventSound } from "./eventMap.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { parsePackSummaries, type PackSummary } from "./model.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { EventVariant, VariantAuditionResult } from "./variant-model.ts";

const INSTALL_ID = /^ins_[0-9]{10}$/u;
const VISUAL_URL = /^kohbor-media:\/\/localhost\/v1\/visual\/(ins_[0-9]{10})\/([0-9a-f]{64})$/u;
const PLAYBACK_ID = /^[A-Za-z0-9_-]{8,96}$/u;

export interface BrowseVariant extends EventVariant {
  visualUrl: string | null;
}

export interface BrowseBinding extends Omit<EventSound, "variants"> {
  variants: BrowseVariant[];
}

export interface BrowsePackDetail {
  installId: string;
  packageId: string;
  revision: number;
  matchActive: boolean;
  bindings: BrowseBinding[];
}

export interface BrowseAuditionRequest {
  pack: PackSummary;
  eventId: string;
  tier: number | null;
  variantId: string;
  overlay: boolean;
}

export interface BrowseStartedEvent {
  playbackId: string;
  installId: string;
  variantId: string;
  startedMonotonicMs: number;
  durationMs: number;
  hasVisual: boolean;
}

export interface BrowseEndedEvent {
  playbackId: string;
  installId: string;
  variantId: string;
  reason: string;
}

export interface BrowseApiPorts {
  summaries: () => Promise<unknown>;
  detail: (installId: string) => Promise<unknown>;
  visualUrl: (installId: string, variantId: string) => Promise<unknown>;
  audition: (request: { installId: string; eventId: string; tier: number | null; variantId: string; overlay: boolean }) => Promise<unknown>;
  subscribeStarted: (onEvent: (payload: unknown) => void) => Promise<() => void>;
  subscribeEnded: (onEvent: (payload: unknown) => void) => Promise<() => void>;
}

export interface BrowseApi {
  summaries: () => Promise<PackSummary[] | null>;
  detail: (pack: PackSummary) => Promise<BrowsePackDetail | null>;
  visualUrl: (installId: string, variantId: string) => Promise<string | null>;
  audition: (request: BrowseAuditionRequest) => Promise<VariantAuditionResult>;
  subscribeStarted: (onEvent: (event: BrowseStartedEvent) => void) => Promise<() => void>;
  subscribeEnded: (onEvent: (event: BrowseEndedEvent) => void) => Promise<() => void>;
}

function object(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function validVariantId(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9][A-Za-z0-9_.:-]{0,79}$/u.test(value);
}

export function parseBrowseVisualUrl(value: unknown, installId: string): string | null {
  if (value === null) return null;
  if (!INSTALL_ID.test(installId) || typeof value !== "string") return null;
  const match = VISUAL_URL.exec(value);
  return match && match[0] === value && match[1] === installId ? value : null;
}

export function parseBrowsePackDetail(value: unknown, pack: PackSummary): BrowsePackDetail | null {
  const source = object(value);
  if (!source || source.schema_version !== "1.1.0" || source.install_id !== pack.installId
    || source.package_id !== pack.packageId || source.revision !== pack.revision
    || typeof source.match_active !== "boolean" || !Array.isArray(source.events)
    || Object.keys(source).some(key => !["schema_version", "install_id", "package_id", "revision", "match_active", "events"].includes(key))) return null;

  const visualByVariant = new Map<string, string | null>();
  const normalizedEvents: unknown[] = [];
  for (const value of source.events) {
    const event = object(value);
    if (!event || !Array.isArray(event.variants)) return null;
    const normalizedVariants: unknown[] = [];
    for (const value of event.variants) {
      const variant = object(value);
      if (!variant || !validVariantId(variant.variant_id)) return null;
      const hasVisual = variant.has_visual;
      const visual = Object.hasOwn(variant, "visual_url") ? parseBrowseVisualUrl(variant.visual_url, pack.installId) : null;
      if (Object.hasOwn(variant, "visual_url") && variant.visual_url !== null && visual === null) return null;
      if (!hasVisual && visual !== null) return null;
      visualByVariant.set(`${String(event.event_id)}:${String(event.tier)}:${variant.variant_id}`, visual);
      const { visual_url: _visualUrl, ...normalizedVariant } = variant;
      normalizedVariants.push(normalizedVariant);
    }
    normalizedEvents.push({ ...event, variants: normalizedVariants });
  }

  const parsed = parseMediaEventMap({
    schema_version: source.schema_version,
    active: { install_id: pack.installId, package_id: pack.packageId, revision: pack.revision },
    match_active: source.match_active,
    events: normalizedEvents,
  });
  if (!parsed) return null;
  return {
    installId: pack.installId,
    packageId: pack.packageId,
    revision: pack.revision,
    matchActive: parsed.match_active,
    bindings: parsed.bindings.map(binding => ({
      ...binding,
      variants: binding.variants.map(variant => ({
        ...variant,
        visualUrl: visualByVariant.get(`${binding.event_id}:${String(binding.tier)}:${variant.variant_id}`) ?? null,
      })),
    })),
  };
}

export function parseBrowseStartedEvent(value: unknown): BrowseStartedEvent | null {
  const event = object(value);
  if (!event || Object.keys(event).some(key => !["playback_id", "install_id", "variant_id", "started_monotonic_ms", "duration_ms", "has_visual"].includes(key))
    || !PLAYBACK_ID.test(String(event.playback_id ?? "")) || !INSTALL_ID.test(String(event.install_id ?? ""))
    || !validVariantId(event.variant_id) || !Number.isSafeInteger(event.started_monotonic_ms) || (event.started_monotonic_ms as number) < 0
    || !Number.isSafeInteger(event.duration_ms) || (event.duration_ms as number) < 0 || typeof event.has_visual !== "boolean") return null;
  return {
    playbackId: event.playback_id as string,
    installId: event.install_id as string,
    variantId: event.variant_id,
    startedMonotonicMs: event.started_monotonic_ms as number,
    durationMs: event.duration_ms as number,
    hasVisual: event.has_visual,
  };
}

export function parseBrowseEndedEvent(value: unknown): BrowseEndedEvent | null {
  const event = object(value);
  if (!event || Object.keys(event).some(key => !["playback_id", "install_id", "variant_id", "reason"].includes(key))
    || !PLAYBACK_ID.test(String(event.playback_id ?? "")) || !INSTALL_ID.test(String(event.install_id ?? ""))
    || !validVariantId(event.variant_id) || typeof event.reason !== "string" || !/^[A-Z][A-Z0-9_]{1,63}$|^finished$|^cut$|^failed$/u.test(event.reason)) return null;
  return {
    playbackId: event.playback_id as string,
    installId: event.install_id as string,
    variantId: event.variant_id,
    reason: event.reason,
  };
}

function parseAuditionResult(value: unknown): VariantAuditionResult | null {
  const result = object(value);
  if (!result || typeof result.audio_started !== "boolean" || typeof result.audio_ended !== "boolean"
    || !(result.audio_end_reason === null || typeof result.audio_end_reason === "string")
    || (result.visual_status !== "shown" && result.visual_status !== "failed" && result.visual_status !== "none" && result.visual_status !== "unknown")
    || !(result.visual_reason === null || typeof result.visual_reason === "string")
    || !(result.video_error_code === null || (typeof result.video_error_code === "number" && Number.isSafeInteger(result.video_error_code)))) return null;
  return result as unknown as VariantAuditionResult;
}

export function createBrowseApi(ports: BrowseApiPorts): BrowseApi {
  return {
    async summaries() {
      return parsePackSummaries(await ports.summaries());
    },
    async detail(pack) {
      return parseBrowsePackDetail(await ports.detail(pack.installId), pack);
    },
    async visualUrl(installId, variantId) {
      if (!INSTALL_ID.test(installId) || !validVariantId(variantId)) return null;
      return parseBrowseVisualUrl(await ports.visualUrl(installId, variantId), installId);
    },
    async audition(request) {
      if (request.overlay && !request.pack.active) throw new Error("OVERLAY_ONLY_ACTIVE_PACK");
      if (!INSTALL_ID.test(request.pack.installId) || !validVariantId(request.variantId)) throw new Error("AUDITION_REQUEST_INVALID");
      const result = parseAuditionResult(await ports.audition({
        installId: request.pack.installId,
        eventId: request.eventId,
        tier: request.tier,
        variantId: request.variantId,
        overlay: request.overlay,
      }));
      if (!result) throw new Error("AUDITION_RESULT_UNREADABLE");
      return result;
    },
    async subscribeStarted(onEvent) {
      return ports.subscribeStarted(payload => {
        const event = parseBrowseStartedEvent(payload);
        if (event) onEvent(event);
      });
    },
    async subscribeEnded(onEvent) {
      return ports.subscribeEnded(payload => {
        const event = parseBrowseEndedEvent(payload);
        if (event) onEvent(event);
      });
    },
  };
}

const tauriBrowsePorts: BrowseApiPorts = {
  summaries: () => invoke<unknown>("media_pack_summaries"),
  detail: installId => invoke<unknown>("media_pack_detail", { installId }),
  visualUrl: (installId, variantId) => invoke<unknown>("media_pack_visual_url", { installId, variantId }),
  audition: request => invoke<unknown>("media_audition_variant", {
    installId: request.installId, eventId: request.eventId, tier: request.tier,
    variantId: request.variantId, overlay: request.overlay,
  }),
  subscribeStarted: onEvent => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? listen<unknown>("audition://started", event => onEvent(event.payload))
    : Promise.resolve(() => {}),
  subscribeEnded: onEvent => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? listen<unknown>("audition://ended", event => onEvent(event.payload))
    : Promise.resolve(() => {}),
};

export const browseApi = createBrowseApi(tauriBrowsePorts);
