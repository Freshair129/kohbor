// @ts-ignore Explicit source extensions support the Node test runner.
import { packsPageCopy, type Locale } from "../copy.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { isAuditionOutcomeRecord, isDemoOutcomeRecord, type AudioHistory } from "../history.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { soundLabel, type SoundKey, type SoundSelection } from "./eventMap.ts";

export const PACKS_TABS = ["installed", "events", "recap"] as const;
export type PacksTab = (typeof PACKS_TABS)[number];

export function nextPacksTab(current: PacksTab = "installed", key: string): PacksTab | null {
  const index = PACKS_TABS.indexOf(current);
  if (key === "Home") return PACKS_TABS[0];
  if (key === "End") return PACKS_TABS[2];
  if (key === "ArrowLeft") return PACKS_TABS[(index + 2) % 3]!;
  if (key === "ArrowRight") return PACKS_TABS[(index + 1) % 3]!;
  return null;
}

export interface PackBinding extends SoundKey {
  selection: SoundSelection;
  variantCount: number;
  bannerCount: number;
}

export interface PackSummary {
  installId: string;
  packageId: string;
  revision: number;
  active: boolean;
  pending: boolean;
  name: string | null;
  problem: "metadata" | "cover" | null;
  coverUrl: string | null;
  counts: { events: number; voices: number; banners: number } | null;
  bindings: PackBinding[];
}

function object(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown> : null;
}

function count(value: unknown, min: number, max: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}

function parseBinding(value: unknown): PackBinding | null {
  const row = object(value);
  if (!row || typeof row.event_id !== "string" || !/^[a-z][a-z0-9_]{0,79}$/u.test(row.event_id)
    || (row.tier !== null && !count(row.tier, 1, 100))
    || !count(row.variant_count, 1, 16) || !count(row.banner_count, 0, row.variant_count)) return null;
  const selection = row.selection;
  if (selection !== null && selection !== "random" && selection !== "shuffle" && selection !== "sequence") return null;
  return { event_id: row.event_id, tier: row.tier, selection, variantCount: row.variant_count, bannerCount: row.banner_count };
}

export function parsePackSummaries(value: unknown): PackSummary[] | null {
  const source = object(value);
  if (source?.schema_version !== "1.0.0" || !Array.isArray(source.records)) return null;
  const results: PackSummary[] = [];
  const seen = new Set<string>();
  for (const item of source.records) {
    const row = object(item);
    if (!row || typeof row.install_id !== "string" || !/^ins_[0-9]{10}$/u.test(row.install_id)
      || seen.has(row.install_id) || typeof row.package_id !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/u.test(row.package_id)
      || !count(row.revision, 1, Number.MAX_SAFE_INTEGER) || typeof row.active !== "boolean" || typeof row.pending !== "boolean") return null;
    seen.add(row.install_id);
    const pack: PackSummary = {
      installId: row.install_id, packageId: row.package_id, revision: row.revision, active: row.active,
      pending: row.pending, name: null, problem: "metadata", coverUrl: null, counts: null, bindings: [],
    };
    results.push(pack);
    if (row.problem !== null && row.problem !== "COVER_INVALID") continue;
    if (typeof row.display_name !== "string" || !row.display_name.trim() || [...row.display_name].length > 96
      || !Array.isArray(row.bindings) || row.bindings.length > 128) continue;
    const bindings = row.bindings.map(parseBinding);
    if (bindings.some(binding => binding === null)) continue;
    const valid = bindings as PackBinding[];
    if (new Set(valid.map(binding => `${binding.event_id}:${binding.tier}`)).size !== valid.length) continue;
    const events = new Set(valid.map(binding => binding.event_id)).size;
    const voices = valid.reduce((total, binding) => total + binding.variantCount, 0);
    const banners = valid.reduce((total, binding) => total + binding.bannerCount, 0);
    if (row.event_count !== events || row.binding_count !== valid.length
      || row.variant_count !== voices || row.banner_count !== banners) continue;
    const cover = typeof row.cover_url === "string" && row.cover_url.match(/^kohbor-media:\/\/localhost\/v1\/cover\/(ins_[0-9]{10})\/[0-9a-f]{64}$/u);
    pack.name = row.display_name.trim();
    pack.bindings = valid;
    pack.counts = { events, voices, banners };
    pack.coverUrl = row.problem === null && banners > 0 && cover && cover[0] === row.cover_url && cover[1] === pack.installId ? cover[0] : null;
    pack.problem = row.problem === "COVER_INVALID" || (banners > 0 && !pack.coverUrl) ? "cover" : null;
  }
  return results;
}

export function cardView(pack: PackSummary, locale: Locale) {
  const text = packsPageCopy(locale);
  const name = pack.name ?? text.unnamed;
  return {
    name,
    initial: Array.from(pack.name ?? "")[0] ?? "—",
    status: pack.problem ? "problem" as const : pack.active ? "active" as const : "installed" as const,
    reason: pack.problem ? text.problems[pack.problem] : null,
    revision: text.revision(pack.revision),
    coverUrl: pack.coverUrl,
    summary: pack.counts ? text.summary(pack.counts.events, pack.counts.voices, pack.counts.banners) : text.summaryUnknown,
  };
}

export function packBindingView(row: PackBinding, locale: Locale) {
  const text = packsPageCopy(locale);
  return {
    ...soundLabel(row, locale),
    voices: text.voices(row.variantCount),
    rotation: row.variantCount > 1 && row.selection ? text.rotation[row.selection] : null,
    banners: text.banners(row.bannerCount),
  };
}

export function sessionAudio(history: AudioHistory | null, epoch?: number | null) {
  const eligible = history?.records.filter(row =>
    row.event_type !== null && !isAuditionOutcomeRecord(row) && !isDemoOutcomeRecord(row)) ?? [];
  const latestEpoch = epoch ?? eligible[0]?.session_epoch;
  const seen = new Set<string>();
  return eligible.filter(row => {
    if (row.session_epoch !== latestEpoch || seen.has(row.instance_id)) return false;
    seen.add(row.instance_id);
    return true;
  });
}
