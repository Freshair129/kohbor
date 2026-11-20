// @ts-ignore Explicit source extensions support the Node test runner.
import { parseMediaEventMap, type EventSound } from "./eventMap.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { mapPackSummaries, type PackSummary } from "./model.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { MediaAuditionPlaybackEvent } from "../runtime.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { MediaPackSummaries } from "../mediaPackSummaries.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { EventVariant, VariantAuditionResult } from "./variant-model.ts";

export interface BrowseVariant extends EventVariant { visualUrl: string | null }
export interface BrowseBinding extends Omit<EventSound, "variants"> { variants: BrowseVariant[] }
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
  visualUrl: string | null;
}
export interface BrowseEndedEvent { playbackId: string; installId: string; variantId: string; reason: string }

export interface BrowseApiPorts {
  summaries: () => Promise<MediaPackSummaries>;
  detail: (installId: string) => Promise<unknown>;
  visualUrl: (installId: string, eventId: string, tier: number | null, variantId: string) => Promise<unknown>;
  audition: (request: BrowseAuditionRequest) => Promise<unknown>;
  subscribe: (onEvent: (event: MediaAuditionPlaybackEvent) => void) => Promise<() => void>;
}
export interface BrowseApi {
  summaries: () => Promise<PackSummary[] | null>;
  detail: (pack: PackSummary) => Promise<BrowsePackDetail | null>;
  visualUrl: (installId: string, eventId: string, tier: number | null, variantId: string) => Promise<string | null>;
  audition: (request: BrowseAuditionRequest) => Promise<VariantAuditionResult>;
  subscribe: (onEvent: (event: BrowseStartedEvent | BrowseEndedEvent) => void) => Promise<() => void>;
}

function object(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function parseDetail(value: unknown, pack: PackSummary): BrowsePackDetail | null {
  const parsed = parseMediaEventMap(value);
  if (!parsed || parsed.active?.install_id !== pack.installId || parsed.active.package_id !== pack.packageId
    || parsed.active.revision !== pack.revision) return null;
  return {
    installId: pack.installId, packageId: pack.packageId, revision: pack.revision, matchActive: parsed.match_active,
    bindings: parsed.bindings.map(binding => ({ ...binding, variants: binding.variants.map(variant => ({ ...variant, visualUrl: null })) })),
  };
}

function parseAuditionResult(value: unknown): VariantAuditionResult | null {
  const result = object(value);
  if (!result || typeof result.audio_started !== "boolean" || typeof result.audio_ended !== "boolean"
    || !(result.audio_end_reason === null || typeof result.audio_end_reason === "string")
    || !["shown", "failed", "none", "unknown"].includes(String(result.visual_status))
    || !(result.visual_reason === null || typeof result.visual_reason === "string")
    || !(result.video_error_code === null || (typeof result.video_error_code === "number" && Number.isSafeInteger(result.video_error_code)))) return null;
  return result as unknown as VariantAuditionResult;
}

export function createBrowseApi(ports: BrowseApiPorts): BrowseApi {
  const playback = new Map<string, { installId: string; variantId: string }>();
  let requested: { installId: string; variantId: string } | null = null;
  return {
    async summaries() { return mapPackSummaries(await ports.summaries()); },
    async detail(pack) { return pack.active ? parseDetail(await ports.detail(pack.installId), pack) : null; },
    async visualUrl(installId, eventId, tier, variantId) {
      const result = object(await ports.visualUrl(installId, eventId, tier, variantId));
      const url = result?.visual_url;
      return typeof url === "string" && url.startsWith(`kohbor-media://localhost/v1/${installId}/`) ? url : null;
    },
    async audition(request) {
      if (request.overlay && !request.pack.active) throw new Error("OVERLAY_ONLY_ACTIVE_PACK");
      requested = { installId: request.pack.installId, variantId: request.variantId };
      const result = parseAuditionResult(await ports.audition(request));
      if (!result) throw new Error("AUDITION_RESULT_UNREADABLE");
      return result;
    },
    async subscribe(onEvent) {
      return ports.subscribe(event => {
        if (event.kind === "started") {
          const active = requested;
          if (!active) return;
          playback.set(event.playback_id, active);
          onEvent({ playbackId: event.playback_id, installId: active.installId, variantId: active.variantId,
            startedMonotonicMs: event.at_ms, durationMs: event.audio_duration_ms,
            hasVisual: event.visual_url !== null, visualUrl: event.visual_url });
        } else {
          const active = playback.get(event.playback_id);
          if (!active) return;
          playback.delete(event.playback_id);
          if (requested?.installId === active.installId && requested.variantId === active.variantId) requested = null;
          onEvent({ playbackId: event.playback_id, ...active, reason: event.reason });
        }
      });
    },
  };
}

export function createBrowseAuditionPort(
  active: (request: BrowseAuditionRequest) => Promise<unknown>,
  installed: (request: BrowseAuditionRequest) => Promise<unknown>,
): BrowseApiPorts["audition"] {
  return request => request.pack.active ? active(request) : installed(request);
}

const tauriBrowsePorts: BrowseApiPorts = {
  summaries: async () => (await import("../runtime")).mediaPackSummaries(),
  detail: async () => (await import("../runtime")).mediaEventMap(),
  visualUrl: async (installId, eventId, tier, variantId) => (await import("../runtime")).mediaVariantVisualUrl(installId, eventId, tier, variantId),
  audition: createBrowseAuditionPort(
    request => import("../runtime").then(runtime => runtime.mediaAuditionVariant(request.eventId, request.tier, request.variantId, request.overlay)),
    request => import("../runtime").then(runtime => runtime.mediaAuditionInstalledVariant(request.pack.installId, request.eventId, request.tier, request.variantId)),
  ),
  subscribe: onEvent => import("../runtime").then(runtime => runtime.subscribeMediaAuditionEvents(onEvent)),
};

export const browseApi = createBrowseApi(tauriBrowsePorts);
