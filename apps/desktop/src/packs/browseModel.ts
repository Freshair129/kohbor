import type { PackSummary } from "./model";

export type BrowsePackSummary = PackSummary;

export interface BrowseFilters {
  official: boolean;
  custom: boolean;
  query: string;
  sort: "active-first" | "name";
}

export function countPackOrigins(packs: readonly BrowsePackSummary[]) {
  return packs.reduce((counts, pack) => {
    if (pack.authorityClass === "system") counts.official += 1;
    else if (pack.authorityClass === "local_import") counts.custom += 1;
    else counts.unknown += 1;
    return counts;
  }, { official: 0, custom: 0, unknown: 0 });
}

export function originFiltersAvailable(packs: readonly BrowsePackSummary[]): boolean {
  return packs.every(pack => pack.authorityClass !== "unknown");
}

export function filterBrowsePacks(packs: readonly BrowsePackSummary[], filters: BrowseFilters): BrowsePackSummary[] {
  const hasUnknownOrigin = !originFiltersAvailable(packs);
  const query = filters.query.trim().toLocaleLowerCase();
  return packs.filter(pack => {
    if (query && !(pack.name ?? "").toLocaleLowerCase().includes(query)) return false;
    if (hasUnknownOrigin) return true;
    if (pack.authorityClass === "system" && !filters.official) return false;
    if (pack.authorityClass === "local_import" && !filters.custom) return false;
    return filters.official || filters.custom;
  }).sort((a, b) => filters.sort === "name"
    ? (a.name ?? "").localeCompare(b.name ?? "", undefined, { sensitivity: "base" })
      || a.installId.localeCompare(b.installId)
    : Number(b.active) - Number(a.active)
      || (a.name ?? "").localeCompare(b.name ?? "", undefined, { sensitivity: "base" })
      || a.installId.localeCompare(b.installId));
}
