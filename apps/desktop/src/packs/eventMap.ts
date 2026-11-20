// @ts-ignore Vite and Node both import the canonical catalog as JSON.
import catalog from "../../../../contracts/event-catalog.json" with { type: "json" };
// @ts-ignore Explicit source extensions support the Node test runner.
import { packsPageCopy, type Locale, type Phase } from "../copy.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { parseEventMapVariant, type EventMapSchemaVersion } from "./event-map-variant.ts";
import type { EventVariant } from "./variant-model.ts";

export type EventSoundState =
  | "ready"
  | "inherited"
  | "unmapped"
  | "invalid_media"
  | "detector_unavailable"
  | "not_voiced"
  | "unknown";

export type EventSoundVariant = EventVariant;

export interface SoundKey {
  event_id: string;
  tier: number | null;
}

export type SoundSelection = "random" | "shuffle" | "sequence" | null;

export interface EventSound extends SoundKey {
  state: EventSoundState;
  reason: string;
  resolved_from_tier: number | null;
  selection: SoundSelection;
  variants: EventSoundVariant[];
}

export interface EventMapActivePack {
  install_id: string;
  package_id: string;
  revision: number;
}

export interface MediaEventMap {
  schema_version: EventMapSchemaVersion;
  active: EventMapActivePack | null;
  match_active: boolean;
  bindings: EventSound[];
}

export type EventMapLoadResult =
  | { state: "ok"; eventMap: MediaEventMap }
  | { state: "unreadable" }
  | { state: "failed"; error: unknown };

export function beginEventMapRefresh(
  request: () => Promise<unknown>,
  commit: (result: EventMapLoadResult) => void,
): () => void {
  let current = true;
  const refresh = async () => {
    try {
      const eventMap = parseMediaEventMap(await request());
      if (current) commit(eventMap ? { state: "ok", eventMap } : { state: "unreadable" });
    } catch (error) {
      if (current) commit({ state: "failed", error });
    }
  };
  void refresh();
  return () => { current = false; };
}

export function eventMapPhaseBlocksAudition(phase: Phase | null): boolean {
  return phase === "draft" || phase === "pregame" || phase === "live" || phase === "post_game";
}

export interface EventMapPackRef {
  install_id: string;
  package_id: string;
  revision: number;
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
const catalogLabels = new Map(catalog.events.map(event => [event.id, event.labels]));
const GROUPS = ["kill", "multi", "streak", "match", "warnings", "other"] as const;
type SoundGroup = (typeof GROUPS)[number];
export const BROWSE_EVENT_CATEGORIES = ["kill", "multi", "streak", "match", "warnings"] as const;
export type BrowseEventCategory = (typeof BROWSE_EVENT_CATEGORIES)[number];
const groupByEvent = new Map<string, SoundGroup>([
  ["hero_kill", "kill"], ["first_blood", "kill"], ["hero_death", "kill"], ["hero_respawn", "kill"],
  ["multi_kill", "multi"], ["streak", "streak"],
  ["match_started", "match"], ["match_ended", "match"], ["level_milestone", "match"],
  ["health_low", "warnings"], ["mana_low", "warnings"], ["health_danger", "warnings"],
  ["enemy_missing", "warnings"], ["risk_alert", "warnings"], ["risk_revision", "warnings"], ["risk_unavailable", "warnings"],
]);

export function soundKey(row: SoundKey) {
  return `${row.event_id}:${row.tier ?? "none"}`;
}

export function groupSoundRows<T extends SoundKey>(rows: readonly T[]) {
  return GROUPS.flatMap(id => {
    const items = rows.filter(row => (groupByEvent.get(row.event_id) ?? "other") === id)
      .sort((a, b) => a.event_id === b.event_id
        ? (a.tier ?? 0) - (b.tier ?? 0)
        : a.event_id === "hero_kill" ? -1 : b.event_id === "hero_kill" ? 1
          : a.event_id.localeCompare(b.event_id));
    return items.length ? [{ id, rows: items }] : [];
  });
}

export function browseCategoryRows(category: BrowseEventCategory): SoundKey[] {
  return catalogRows.filter(row => (groupByEvent.get(row.event_id) ?? "other") === category);
}

export function soundLabel(row: SoundKey, locale: Locale) {
  const text = packsPageCopy(locale);
  const localized = catalogLabels.get(row.event_id)?.[locale] ?? text.unknownEvent;
  const tiers = text.gameTiers[row.event_id as keyof typeof text.gameTiers] as Record<number, string> | undefined;
  const gameTitle = row.tier === null ? undefined : tiers?.[row.tier];
  return {
    title: gameTitle ?? localized,
    detail: gameTitle ? localized : null,
    tier: row.tier === null ? null : text.tier(row.tier),
  };
}

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

function parseActive(value: unknown): EventMapActivePack | null | undefined {
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

function parseBinding(value: unknown, version: EventMapSchemaVersion): EventSound | null {
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
  const variants = binding.variants.map(variant => parseEventMapVariant(variant, version));
  if (variants.some((variant) => variant === null)) return null;
  const parsedVariants = variants as EventSoundVariant[];
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
  };
}

function rowKey(value: unknown): string | null {
  const row = record(value);
  if (!row || !matches(/^[a-z][a-z0-9_]{0,79}$/, row.event_id)) return null;
  if (row.tier !== null && (typeof row.tier !== "number"
    || !Number.isInteger(row.tier) || row.tier < 1 || row.tier > 100)) return null;
  return `${row.event_id}:${row.tier ?? "none"}`;
}

function unknownBinding(eventId: string, tier: number | null, reason: string): EventSound {
  return {
    event_id: eventId,
    tier,
    state: "unknown",
    reason,
    resolved_from_tier: null,
    selection: null,
    variants: [],
  };
}

function withoutActivePack(binding: EventSound): EventSound {
  if (binding.state === "not_voiced" || binding.state === "unknown") {
    return binding;
  }
  if (binding.state === "detector_unavailable") {
    return { ...binding, variants: [], selection: null };
  }
  return unknownBinding(binding.event_id, binding.tier, "MEDIA_STATE_UNKNOWN");
}

/** Parse the versioned host map. A bad row only makes its own catalog row unknown. */
export function parseMediaEventMap(value: unknown): MediaEventMap | null {
  const source = record(value);
  if (!source || !hasOnlyKeys(source, ["schema_version", "active", "events"], ["match_active", "media_match_active"])) return null;
  const flags = [source.match_active, source.media_match_active].filter(flag => flag !== undefined);
  if (flags.length === 0 || flags.some(flag => typeof flag !== "boolean")) return null;
  if (source.schema_version !== "1.0.0" && source.schema_version !== "1.1.0") return null;
  const version = source.schema_version as EventMapSchemaVersion;
  const active = parseActive(source.active);
  if (active === undefined || !Array.isArray(source.events)) return null;

  const rows = new Map<string, EventSound>();
  const invalidKeys = new Set<string>();
  const seenKeys = new Set<string>();
  const unexpectedUnknowns = new Map<string, EventSound>();
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
    const binding = parseBinding(value, version);
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
    schema_version: version,
    active,
    match_active: flags.includes(true),
    bindings,
  };
}

export function eventMapMatchesPack(
  eventMap: MediaEventMap,
  activePack: EventMapPackRef | null | undefined,
): boolean {
  if (activePack === undefined) return true;
  if (activePack === null) return eventMap.active === null;
  return eventMap.active?.install_id === activePack.install_id
    && eventMap.active.package_id === activePack.package_id
    && eventMap.active.revision === activePack.revision;
}

export type EventMapBlockCode =
  | "CAPABILITY_UNAVAILABLE"
  | "AUDITION_MATCH_ACTIVE"
  | "AUDITION_NO_ACTIVE_PACK"
  | "AUDITION_UNMAPPED"
  | "AUDITION_MEDIA_INVALID"
  | "AUDITION_MEDIA_STATE_UNKNOWN"
  | "AUDITION_EVENT_UNAVAILABLE"
  | "AUDITION_BUSY";

export function eventMapPlayBlock(
  eventMap: MediaEventMap | null,
  binding: EventSound,
  hostDisabled: boolean,
  busy: boolean,
  phaseMatchActive = false,
): EventMapBlockCode | null {
  if (hostDisabled) return "CAPABILITY_UNAVAILABLE";
  if (!eventMap) return "AUDITION_MEDIA_STATE_UNKNOWN";
  if (phaseMatchActive || eventMap.match_active) return "AUDITION_MATCH_ACTIVE";
  if (!eventMap.active) return "AUDITION_NO_ACTIVE_PACK";
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

export async function auditionSound<T>(
  command: (eventId: string, tier: number | null) => Promise<T>,
  row: SoundKey,
  blocked: string | null,
): Promise<T | null> {
  if (blocked !== null) return null;
  return command(row.event_id, row.tier);
}
