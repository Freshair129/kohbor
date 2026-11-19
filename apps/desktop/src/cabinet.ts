// @ts-ignore Vite and Node both import the canonical catalog as JSON.
import catalog from "../../../contracts/event-catalog.json" with { type: "json" };
import type { Phase } from "./copy.ts";

export type CabinetBindingState = "ready" | "unmapped" | "unknown";

export interface CabinetBinding {
  event_id: string;
  tier: number | null;
  mapped: boolean;
  duration_ms: number | null;
  has_visual: boolean;
  state: CabinetBindingState;
}

export interface CabinetActivePack {
  install_id: string;
  package_id: string;
  revision: number;
}

export interface MediaCabinet {
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

const expectedCatalogKeys = new Set(
  catalog.events.flatMap((event) => {
    if (event.status !== "active" || event.voiced_default !== true) return [];
    const tiers = event.tier_range
      ? Array.from({ length: event.tier_range.max - event.tier_range.min + 1 }, (_, index) => event.tier_range!.min + index)
      : [null];
    return tiers.map((tier) => `${event.id}:${tier ?? "none"}`);
  }),
);

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => Object.hasOwn(value, key));
}

function matches(pattern: RegExp, value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = pattern.exec(value);
  return match !== null && match[0] === value;
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

function parseBinding(value: unknown): CabinetBinding | null {
  const binding = record(value);
  if (!binding || !hasExactKeys(binding, [
    "event_id", "tier", "mapped", "duration_ms", "has_visual", "state",
  ])) return null;
  if (!matches(/^[a-z][a-z0-9_]{0,79}$/, binding.event_id)) return null;
  if (binding.tier !== null && (typeof binding.tier !== "number"
    || !Number.isInteger(binding.tier) || binding.tier < 1 || binding.tier > 100)) return null;
  if (typeof binding.mapped !== "boolean" || typeof binding.has_visual !== "boolean") return null;
  if (binding.duration_ms !== null && (typeof binding.duration_ms !== "number"
    || !Number.isSafeInteger(binding.duration_ms) || binding.duration_ms < 0)) return null;
  if (binding.state !== "ready" && binding.state !== "unmapped" && binding.state !== "unknown") return null;
  if (binding.state === "ready" && (!binding.mapped || binding.duration_ms === null)) return null;
  if (binding.state === "unmapped" && (binding.mapped || binding.duration_ms !== null || binding.has_visual)) return null;
  if (binding.state === "unknown" && (binding.duration_ms !== null || binding.has_visual)) return null;
  if (!expectedCatalogKeys.has(`${binding.event_id}:${binding.tier ?? "none"}`)) return null;
  return {
    event_id: binding.event_id,
    tier: binding.tier,
    mapped: binding.mapped,
    duration_ms: binding.duration_ms,
    has_visual: binding.has_visual,
    state: binding.state,
  };
}

/** Parse the host catalog snapshot strictly; malformed or stale data stays unknown. */
export function parseMediaCabinet(value: unknown): MediaCabinet | null {
  const source = record(value);
  if (!source || !hasExactKeys(source, ["active", "match_active", "bindings"])) return null;
  const active = parseActive(source.active);
  if (active === undefined || typeof source.match_active !== "boolean" || !Array.isArray(source.bindings)) return null;
  const bindings: CabinetBinding[] = [];
  const keys = new Set<string>();
  for (const value of source.bindings) {
    const binding = parseBinding(value);
    if (!binding) return null;
    const key = `${binding.event_id}:${binding.tier ?? "none"}`;
    if (keys.has(key)) return null;
    keys.add(key);
    bindings.push(binding);
  }
  if (keys.size !== expectedCatalogKeys.size || [...expectedCatalogKeys].some((key) => !keys.has(key))) return null;
  if (active === null && bindings.some((binding) => binding.state !== "unknown")) return null;
  return { active, match_active: source.match_active, bindings };
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
    mapped: cabinet.bindings.filter((binding) => binding.state === "ready").length,
    total: cabinet.bindings.length,
  };
}

export type CabinetBlockCode =
  | "CAPABILITY_UNAVAILABLE"
  | "AUDITION_MATCH_ACTIVE"
  | "AUDITION_NO_ACTIVE_PACK"
  | "AUDITION_UNMAPPED"
  | "AUDITION_MEDIA_STATE_UNKNOWN"
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
  if (busy) return "AUDITION_BUSY";
  return null;
}
