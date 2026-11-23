// @ts-ignore Explicit source extensions support the Node test runner.
import { packsPageCopy, type Locale } from "../copy.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { soundLabel, type SoundKey, type SoundSelection } from "./eventMap.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { parseMediaPackSummaries } from "../mediaPackSummaries.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { MediaPackSummaries, MediaPackSummaryV1_1 } from "../mediaPackSummaries.ts";

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
  authorityClass: "system" | "local_import" | "unknown";
  publisherClaim: string | null;
  installedAt: string | null;
  sizeBytes: number | null;
  gameId: "dota2" | null;
  presentation: {
    description: string | null;
    author: string | null;
    languages: string[] | null;
    coverUrl: string | null;
  };
}

export function parsePackSummaries(value: unknown): PackSummary[] | null {
  const source = parseMediaPackSummaries(value);
  if (!source) return null;
  return mapPackSummaries(source);
}

export function mapPackSummaries(source: MediaPackSummaries): PackSummary[] {
  return source.records.map(row => {
    const modern = source.schema_version === "1.1.0" ? row as MediaPackSummaryV1_1 : null;
    const bindings: PackBinding[] = row.bindings.map(binding => ({
      event_id: binding.event_id, tier: binding.tier, selection: binding.selection,
      variantCount: binding.variant_count, bannerCount: binding.banner_count,
    }));
    const events = new Set(bindings.map(binding => binding.event_id)).size;
    const voices = bindings.reduce((total, binding) => total + binding.variantCount, 0);
    const banners = bindings.reduce((total, binding) => total + binding.bannerCount, 0);
    // The cover is the optional still image from presentation.cover (MEDIA-PACK-SPEC 1.4.2); banners do not imply one.
    const coverUrl = row.problem === null ? row.cover_url : null;
    return {
      installId: row.install_id, packageId: row.package_id, revision: row.revision, active: row.active, pending: row.pending,
      name: row.problem === "METADATA_UNAVAILABLE" ? null : row.display_name,
      problem: row.problem === "METADATA_UNAVAILABLE" ? "metadata" : row.problem === "COVER_INVALID" ? "cover" : null,
      coverUrl, counts: row.problem === "METADATA_UNAVAILABLE" ? null : { events, voices, banners },
      bindings: row.problem === "METADATA_UNAVAILABLE" ? [] : bindings,
      authorityClass: modern?.authority_class ?? "unknown", publisherClaim: modern?.publisher_claim ?? null,
      installedAt: modern?.installed_at == null ? null : new Date(modern.installed_at).toISOString(),
      sizeBytes: modern?.size_bytes ?? null, gameId: modern?.game_id ?? null,
      presentation: {
        description: modern?.presentation?.description ?? null, author: modern?.presentation?.author ?? null,
        languages: modern?.presentation?.languages ?? null, coverUrl: modern?.presentation?.cover_url ?? null,
      },
    };
  });
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
