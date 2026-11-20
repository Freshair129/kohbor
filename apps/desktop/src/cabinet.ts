// @ts-ignore Vite and Node both import the canonical catalog as JSON.
import catalog from "../../../contracts/event-catalog.json" with { type: "json" };
import type { Phase } from "./copy.ts";

export type CabinetBindingState =
  | "ready"
  | "inherited"
  | "unmapped"
  | "invalid_media"
  | "detector_unavailable"
  | "not_voiced"
  | "unknown";

export interface CabinetVariant {
  variant_id: string;
  clip_id?: string;
  duration_ms: number;
  has_visual: boolean;
}

export interface CabinetBinding {
  event_id: string;
  tier: number | null;
  state: CabinetBindingState;
  reason: string;
  resolved_from_tier: number | null;
  selection: "random" | "shuffle" | "sequence" | null;
  variants: CabinetVariant[];
  /** Derived display fields retained for the current cabinet component. */
  mapped: boolean;
  duration_ms: number | null;
  has_visual: boolean;
}

export interface CabinetActivePack {
  install_id: string;
  package_id: string;
  revision: number;
}

export interface MediaCabinet {
  schema_version: "1.0.0";
  active: CabinetActivePack | null;
  match_active: boolean;
  bindings: CabinetBinding[];
}

export type CabinetLoadResult =
  | { state: "ok"; cabinet: MediaCabinet }
  | { state: "unreadable" }
  | { state: "failed"; error: unknown };

export function beginCabinetRefresh(
  request: () => Promise<unknown>,
  commit: (result: CabinetLoadResult) => void,
): () => void {
  let current = true;
  const refresh = async () => {
    try {
      const cabinet = parseMediaCabinet(await request());
      if (current) commit(cabinet ? { state: "ok", cabinet } : { state: "unreadable" });
    } catch (error) {
      if (current) commit({ state: "failed", error });
    }
  };
  void refresh();
  return () => { current = false; };
}

export function cabinetPhaseBlocksAudition(phase: Phase | null): boolean {
  return phase === "draft" || phase === "pregame" || phase === "live" || phase === "post_game";
}

export interface CabinetPackRef {
  install_id: string;
  revision: number;
}

export interface CabinetGroup {
  eventId: string;
  bindings: CabinetBinding[];
}

const catalogRows = catalog.events
  .filter((event) => event.status === "active")
  .flatMap((event) => {
    const tiers = event.tier_range
      ? Array.from({ length: event.tier_range.max - event.tier_range.min + 1 }, (_, index) => event.tier_range!.min + index)
      : [null];
    return tiers.map((tier) => ({ event_id: event.id, tier }));
  });
const expectedCatalogKeys = new Set(catalogRows.map(({ event_id, tier }) => `${event_id}:${tier ?? "none"}`));

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => Object.hasOwn(value, key));
}

function hasOnlyKeys(value: Record<string, unknown>, required: readonly string[], optional: readonly string[] = []): boolean {
  const allowed = new Set([...required, ...optional]);
  return required.every((key) => Object.hasOwn(value, key))
    && Object.keys(value).every((key) => allowed.has(key));
}

function matches(pattern: RegExp, value: unknown): value is string {
  return typeof value === "string" && pattern.test(value);
}

function parseActive(value: unknown): CabinetActivePack | null | undefined {
  if (value === null) return null;
  const active = record(value);
  if (!active || !hasExactKeys(active, ["install_id", "package_id", "revision"])) return undefined;
  if (!matches(/^ins_[0-9]{10}$/, active.install_id)
    || !matches(/^[a-z0-9][a-z0-9_.-]{0,79}$/, active.package_id)
    || typeof active.revision !== "number"
    || !Number.isSafeInteger(active.revision)
    || active.revision < 1) return undefined;
  return {
    install_id: active.install_id,
    package_id: active.package_id,
    revision: active.revision,
  };
}

function parseVariant(value: unknown): CabinetVariant | null {
  const variant = record(value);
  if (!variant || !hasOnlyKeys(variant, ["variant_id", "duration_ms", "has_visual"], ["clip_id"])) return null;
  if (!matches(/^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,79}$/, variant.variant_id)
    || typeof variant.duration_ms !== "number"
    || !Number.isSafeInteger(variant.duration_ms)
    || variant.duration_ms < 0
    || variant.duration_ms > 15_000
    || typeof variant.has_visual !== "boolean") return null;
  if (Object.hasOwn(variant, "clip_id")
    && !matches(/^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,127}$/, variant.clip_id)) return null;
  return {
    variant_id: variant.variant_id,
    ...(typeof variant.clip_id === "string" ? { clip_id: variant.clip_id } : {}),
    duration_ms: variant.duration_ms,
    has_visual: variant.has_visual,
  };
}

function parseBinding(value: unknown): CabinetBinding | null {
  const binding = record(value);
  if (!binding || !hasExactKeys(binding, [
    "event_id", "tier", "state", "reason", "resolved_from_tier", "selection", "variants",
  ])) return null;
  if (!matches(/^[a-z][a-z0-9_]{0,79}$/, binding.event_id)) return null;
  if (binding.tier !== null && (typeof binding.tier !== "number"
    || !Number.isInteger(binding.tier) || binding.tier < 1 || binding.tier > 100)) return null;
  if (binding.resolved_from_tier !== null && (typeof binding.resolved_from_tier !== "number"
    || !Number.isInteger(binding.resolved_from_tier)
    || binding.resolved_from_tier < 1
    || binding.resolved_from_tier > 100)) return null;
  if (binding.selection !== null
    && binding.selection !== "random"
    && binding.selection !== "shuffle"
    && binding.selection !== "sequence") return null;
  if (!matches(/^[A-Z][A-Z0-9_]{1,63}$/, binding.reason)) return null;
  if (binding.state !== "ready"
    && binding.state !== "inherited"
    && binding.state !== "unmapped"
    && binding.state !== "invalid_media"
    && binding.state !== "detector_unavailable"
    && binding.state !== "not_voiced"
    && binding.state !== "unknown") return null;
  if (!Array.isArray(binding.variants) || binding.variants.length > 16) return null;
  const variants = binding.variants.map(parseVariant);
  if (variants.some((variant) => variant === null)) return null;
  const parsedVariants = variants as CabinetVariant[];
  if (new Set(parsedVariants.map((variant) => variant.variant_id)).size !== parsedVariants.length) return null;
  if ((binding.state === "ready" || binding.state === "inherited") && parsedVariants.length === 0) return null;
  if (["unmapped", "invalid_media", "not_voiced", "unknown"].includes(binding.state)
    && parsedVariants.length !== 0) return null;
  if (binding.state !== "inherited" && binding.resolved_from_tier !== null) return null;
  if (binding.state === "inherited" && binding.tier !== null && binding.resolved_from_tier !== null
    && binding.resolved_from_tier > binding.tier) return null;

  return {
    event_id: binding.event_id,
    tier: binding.tier,
    state: binding.state,
    reason: binding.reason,
    resolved_from_tier: binding.resolved_from_tier,
    selection: binding.selection,
    variants: parsedVariants,
    mapped: parsedVariants.length > 0,
    duration_ms: parsedVariants[0]?.duration_ms ?? null,
    has_visual: parsedVariants.some((variant) => variant.has_visual),
  };
}

function rowKey(value: unknown): string | null {
  const row = record(value);
  if (!row || !matches(/^[a-z][a-z0-9_]{0,79}$/, row.event_id)) return null;
  if (row.tier !== null && (typeof row.tier !== "number"
    || !Number.isInteger(row.tier) || row.tier < 1 || row.tier > 100)) return null;
  return `${row.event_id}:${row.tier ?? "none"}`;
}

function unknownBinding(eventId: string, tier: number | null, reason: string): CabinetBinding {
  return {
    event_id: eventId,
    tier,
    state: "unknown",
    reason,
    resolved_from_tier: null,
    selection: null,
    variants: [],
    mapped: false,
    duration_ms: null,
    has_visual: false,
  };
}

function withoutActivePack(binding: CabinetBinding): CabinetBinding {
  if (binding.state === "not_voiced" || binding.state === "detector_unavailable" || binding.state === "unknown") {
    return binding;
  }
  return unknownBinding(binding.event_id, binding.tier, "MEDIA_STATE_UNKNOWN");
}

/** Parse the versioned host map. A bad row only makes its own catalog row unknown. */
export function parseMediaCabinet(value: unknown): MediaCabinet | null {
  const source = record(value);
  if (!source || !hasExactKeys(source, ["schema_version", "active", "match_active", "events"])) return null;
  if (source.schema_version !== "1.0.0") return null;
  const active = parseActive(source.active);
  if (active === undefined || typeof source.match_active !== "boolean" || !Array.isArray(source.events)) return null;

  const rows = new Map<string, CabinetBinding>();
  const invalidKeys = new Set<string>();
  const seenKeys = new Set<string>();
  const unexpectedUnknowns = new Map<string, CabinetBinding>();
  for (const value of source.events) {
    const key = rowKey(value);
    if (!key) continue;
    const rawRow = record(value)!;
    const expected = expectedCatalogKeys.has(key);
    if (seenKeys.has(key)) {
      rows.delete(key);
      invalidKeys.add(key);
      if (!expected) unexpectedUnknowns.set(key, unknownBinding(rawRow.event_id as string, rawRow.tier as number | null, "UNEXPECTED_ROW"));
      continue;
    }
    seenKeys.add(key);
    const binding = parseBinding(value);
    if (!binding) {
      invalidKeys.add(key);
      if (!expected) unexpectedUnknowns.set(key, unknownBinding(rawRow.event_id as string, rawRow.tier as number | null, "INVALID_ROW"));
      continue;
    }
    rows.set(key, binding);
  }

  const bindings = catalogRows.map(({ event_id, tier }) => {
    const key = `${event_id}:${tier ?? "none"}`;
    if (invalidKeys.has(key)) return unknownBinding(event_id, tier, "INVALID_ROW");
    const binding = rows.get(key);
    if (!binding) return unknownBinding(event_id, tier, "MISSING_ROW");
    return active === null ? withoutActivePack(binding) : binding;
  });

  for (const [key, binding] of rows) {
    if (expectedCatalogKeys.has(key)) continue;
    bindings.push(unknownBinding(binding.event_id, binding.tier, "UNEXPECTED_ROW"));
  }
  bindings.push(...unexpectedUnknowns.values());

  return {
    schema_version: "1.0.0",
    active,
    match_active: source.match_active,
    bindings,
  };
}

export function groupCabinetBindings(bindings: readonly CabinetBinding[]): CabinetGroup[] {
  const groups = new Map<string, CabinetBinding[]>();
  for (const binding of bindings) {
    const group = groups.get(binding.event_id) ?? [];
    group.push(binding);
    groups.set(binding.event_id, group);
  }
  return [...groups].map(([eventId, items]) => ({ eventId, bindings: items }));
}

export function cabinetMatchesPack(
  cabinet: MediaCabinet,
  activePack: CabinetPackRef | null | undefined,
): boolean {
  if (activePack === undefined) return true;
  if (activePack === null) return cabinet.active === null;
  return cabinet.active?.install_id === activePack.install_id
    && cabinet.active.revision === activePack.revision;
}

export function cabinetSummary(cabinet: MediaCabinet): { mapped: number; total: number } | null {
  if (cabinet.bindings.some((binding) => binding.state === "unknown")) return null;
  return {
    mapped: cabinet.bindings.filter((binding) => binding.state === "ready" || binding.state === "inherited").length,
    total: cabinet.bindings.length,
  };
}

export type CabinetBlockCode =
  | "CAPABILITY_UNAVAILABLE"
  | "AUDITION_MATCH_ACTIVE"
  | "AUDITION_NO_ACTIVE_PACK"
  | "AUDITION_UNMAPPED"
  | "AUDITION_MEDIA_INVALID"
  | "AUDITION_MEDIA_STATE_UNKNOWN"
  | "AUDITION_EVENT_UNAVAILABLE"
  | "AUDITION_BUSY";

export function cabinetPlayBlock(
  cabinet: MediaCabinet | null,
  binding: CabinetBinding,
  hostDisabled: boolean,
  busy: boolean,
  phaseMatchActive = false,
): CabinetBlockCode | null {
  if (hostDisabled) return "CAPABILITY_UNAVAILABLE";
  if (!cabinet) return "AUDITION_MEDIA_STATE_UNKNOWN";
  if (phaseMatchActive || cabinet.match_active) return "AUDITION_MATCH_ACTIVE";
  if (!cabinet.active) return "AUDITION_NO_ACTIVE_PACK";
  if (binding.state === "unknown") return "AUDITION_MEDIA_STATE_UNKNOWN";
  if (binding.state === "unmapped") return "AUDITION_UNMAPPED";
  if (binding.state === "invalid_media") return "AUDITION_MEDIA_INVALID";
  if (binding.state === "not_voiced") return "AUDITION_EVENT_UNAVAILABLE";
  if (binding.state === "detector_unavailable" && binding.reason === "EVENT_UNAVAILABLE" && binding.variants.length === 0) {
    return "AUDITION_EVENT_UNAVAILABLE";
  }
  if (binding.state === "detector_unavailable" && binding.variants.length === 0) return "AUDITION_UNMAPPED";
  if (busy) return "AUDITION_BUSY";
  return null;
}
