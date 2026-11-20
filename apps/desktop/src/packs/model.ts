// @ts-ignore Explicit source extensions support the Node test runner.
import { packsPageCopy, type Locale } from "../copy.ts";
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
  if ((source?.schema_version !== "1.0.0" && source?.schema_version !== "1.1.0") || !Array.isArray(source.records)) return null;
  const version = source.schema_version;
  const results: PackSummary[] = [];
  const seen = new Set<string>();
  for (const item of source.records) {
    const row = object(item);
    if (!row || typeof row.install_id !== "string" || !/^ins_[0-9]{10}$/u.test(row.install_id)
      || seen.has(row.install_id) || typeof row.package_id !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/u.test(row.package_id)
      || !count(row.revision, 1, Number.MAX_SAFE_INTEGER) || typeof row.active !== "boolean" || typeof row.pending !== "boolean") return null;
    seen.add(row.install_id);
    const metadata = version === "1.1.0" ? parseBrowseMetadata(row) : {
      authorityClass: "unknown" as const,
      publisherClaim: null,
      installedAt: null,
      sizeBytes: null,
      gameId: null,
      presentation: { description: null, author: null, languages: null, coverUrl: null },
    };
    if (!metadata) return null;
    const pack: PackSummary = {
      installId: row.install_id, packageId: row.package_id, revision: row.revision, active: row.active,
      pending: row.pending, name: null, problem: "metadata", coverUrl: null, counts: null, bindings: [],
      ...metadata,
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
    const rawCover = typeof row.cover_url === "string" ? row.cover_url : metadata.presentation.coverUrl;
    const cover = typeof rawCover === "string" && rawCover.match(/^kohbor-media:\/\/localhost\/v1\/cover\/(ins_[0-9]{10})\/[0-9a-f]{64}$/u);
    pack.name = row.display_name.trim();
    pack.bindings = valid;
    pack.counts = { events, voices, banners };
    pack.coverUrl = row.problem === null && banners > 0 && cover && cover[0] === rawCover && cover[1] === pack.installId ? cover[0] : null;
    pack.problem = row.problem === "COVER_INVALID" || (banners > 0 && !pack.coverUrl) ? "cover" : null;
  }
  return results;
}

function parseBrowseMetadata(row: Record<string, unknown>): Pick<PackSummary,
  "authorityClass" | "publisherClaim" | "installedAt" | "sizeBytes" | "gameId" | "presentation"> | null {
  if ((row.authority_class !== "system" && row.authority_class !== "local_import")
    || !(row.publisher_claim === null || validDisplayText(row.publisher_claim, 96))
    || !(row.installed_at === null || validTimestamp(row.installed_at))
    || !(row.size_bytes === null || count(row.size_bytes, 0, Number.MAX_SAFE_INTEGER))
    || row.game_id !== "dota2") return null;
  const presentation = row.presentation === undefined || row.presentation === null ? null : object(row.presentation);
  if (row.presentation !== undefined && row.presentation !== null && !presentation) return null;
  if (presentation && Object.keys(presentation).some(key => !["description", "author", "languages", "cover"].includes(key))) return null;
  const description = presentation?.description;
  const author = presentation?.author;
  const languages = presentation?.languages;
  const cover = presentation?.cover;
  if (!(description === undefined || description === null || validDisplayText(description, 500))
    || !(author === undefined || author === null || validDisplayText(author, 80))
    || !(languages === undefined || languages === null || validLanguages(languages))
    || !(cover === undefined || cover === null || validCoverUrl(cover))) return null;
  return {
    authorityClass: row.authority_class,
    publisherClaim: row.publisher_claim,
    installedAt: row.installed_at,
    sizeBytes: row.size_bytes,
    gameId: "dota2",
    presentation: {
      description: typeof description === "string" ? description : null,
      author: typeof author === "string" ? author : null,
      languages: Array.isArray(languages) ? languages as string[] : null,
      coverUrl: typeof cover === "string" ? cover : null,
    },
  };
}

function validTimestamp(value: unknown): value is string {
  return typeof value === "string"
    && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/u.test(value)
    && Number.isFinite(Date.parse(value));
}

function validDisplayText(value: unknown, max: number): value is string {
  return typeof value === "string" && value.length > 0 && [...value].length <= max
    && !/[\u0000-\u001f\u007f-\u009f\r\n]/u.test(value);
}

function validLanguages(value: unknown): value is string[] {
  return Array.isArray(value) && value.length >= 1 && value.length <= 8
    && value.every(tag => typeof tag === "string" && /^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/u.test(tag));
}

function validCoverUrl(value: unknown): value is string {
  return typeof value === "string" && /^kohbor-media:\/\/localhost\/v1\/cover\/ins_[0-9]{10}\/[0-9a-f]{64}$/u.test(value);
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
