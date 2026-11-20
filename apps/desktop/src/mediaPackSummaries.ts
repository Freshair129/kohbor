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

