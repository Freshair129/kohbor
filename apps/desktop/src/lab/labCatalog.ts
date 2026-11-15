// Pure model of the Lab "Event catalog" tab (docs/design/KOHBOR-LAB.md section 12): parsing of
// the host table, Thai labels, filters and formatting. No React, no host calls.

export interface GMaidenRef {
  id: string;
  tier: number | null;
  relation: "same" | "approx" | "tier";
}

export interface LastFired {
  count: number;
  tier: number | null;
  atMs: number;
}

export interface CatalogRow {
  id: string;
  labelTh: string;
  labelEn: string;
  category: string;
  source: string;
  sourceLabelTh: string;
  status: string;
  statusTh: string;
  detectorRef: string;
  tunables: number;
  tunablesEditable: number;
  gMaiden: GMaidenRef[];
  lastFired: LastFired | null;
  mediaMappable: boolean;
  tierRange: [number, number] | null;
}

export interface PossibleEvent {
  id: string;
  labelTh: string;
  status: "possible_unverified" | "proposed_not_built";
  noteTh: string;
}

export interface CatalogView {
  rows: CatalogRow[];
  notInCatalog: PossibleEvent[];
  gMaidenOnly: { id: string; labelTh: string; noteTh: string }[];
  noteTh: string;
}

export interface CatalogFilter {
  category: string;
  source: string;
  status: string;
  text: string;
}

export const ALL = "all";
export const EMPTY_FILTER: CatalogFilter = { category: ALL, source: ALL, status: ALL, text: "" };

export const CATEGORY_TH: Record<string, string> = {
  ordinary: "ทั่วไป",
  critical: "วิกฤต (ความปลอดภัย)",
  notice: "แจ้งให้ทราบ",
  correction: "แก้คำเตือน",
  system: "ระบบ",
  recording: "บันทึก",
  test: "ทดสอบ",
};

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function str(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function count(value: unknown): number | null {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 ? value : null;
}

function parseGMaiden(value: unknown): GMaidenRef[] | null {
  if (!Array.isArray(value)) return null;
  const out: GMaidenRef[] = [];
  for (const item of value) {
    if (!isObject(item) || typeof item.id !== "string") return null;
    const relation = item.relation;
    if (relation !== "same" && relation !== "approx" && relation !== "tier") return null;
    const tier = item.tier === null || item.tier === undefined ? null : item.tier;
    if (tier !== null && typeof tier !== "number") return null;
    out.push({ id: item.id, tier, relation });
  }
  return out;
}

function parseLastFired(value: unknown): LastFired | null | undefined {
  if (value === null || value === undefined) return null;
  if (!isObject(value)) return undefined;
  const fired = count(value.count);
  const atMs = count(value.at_ms);
  const tier = value.tier === null || value.tier === undefined ? null : value.tier;
  if (fired === null || atMs === null || (tier !== null && typeof tier !== "number")) return undefined;
  return { count: fired, tier, atMs };
}

function parseRow(value: unknown): CatalogRow | null {
  if (!isObject(value)) return null;
  const id = str(value.id);
  const labelTh = str(value.label_th);
  const labelEn = str(value.label_en);
  const category = str(value.category);
  const source = str(value.source);
  const sourceLabelTh = str(value.source_label_th);
  const status = str(value.status);
  const statusTh = str(value.status_th);
  const detectorRef = str(value.detector_ref);
  const tunables = count(value.tunables);
  const editable = count(value.tunables_editable);
  const gMaiden = parseGMaiden(value.g_maiden);
  const lastFired = parseLastFired(value.last_fired);
  if (
    id === null || labelTh === null || labelEn === null || category === null || source === null ||
    sourceLabelTh === null || status === null || statusTh === null || detectorRef === null ||
    tunables === null || editable === null || gMaiden === null || lastFired === undefined
  ) {
    return null;
  }
  const range = isObject(value.tier_range) ? value.tier_range : null;
  const tierRange: [number, number] | null =
    range !== null && typeof range.min === "number" && typeof range.max === "number" ? [range.min, range.max] : null;
  return {
    id, labelTh, labelEn, category, source, sourceLabelTh, status, statusTh, detectorRef,
    tunables, tunablesEditable: editable, gMaiden, lastFired,
    mediaMappable: value.media_mappable === true,
    tierRange,
  };
}

export function parseCatalog(value: unknown): CatalogView | null {
  if (!isObject(value) || !Array.isArray(value.events) || !Array.isArray(value.not_in_catalog)) return null;
  const rows: CatalogRow[] = [];
  for (const raw of value.events) {
    const row = parseRow(raw);
    if (row === null) return null;
    rows.push(row);
  }
  const notInCatalog: PossibleEvent[] = [];
  for (const raw of value.not_in_catalog) {
    if (!isObject(raw)) return null;
    const { id, label_th, note_th, status } = raw;
    if (typeof id !== "string" || typeof label_th !== "string" || typeof note_th !== "string") return null;
    if (status !== "possible_unverified" && status !== "proposed_not_built") return null;
    notInCatalog.push({ id, labelTh: label_th, status, noteTh: note_th });
  }
  const gMaidenOnly = Array.isArray(value.g_maiden_only)
    ? value.g_maiden_only.flatMap((raw) =>
        isObject(raw) && typeof raw.id === "string"
          ? [{ id: raw.id, labelTh: String(raw.label_th ?? raw.id), noteTh: String(raw.note_th ?? "") }]
          : [],
      )
    : [];
  return { rows, notInCatalog, gMaidenOnly, noteTh: typeof value.note_th === "string" ? value.note_th : "" };
}

export function filterRows(rows: CatalogRow[], filter: CatalogFilter): CatalogRow[] {
  const needle = filter.text.trim().toLowerCase();
  return rows.filter((row) => {
    if (filter.category !== ALL && row.category !== filter.category) return false;
    if (filter.source !== ALL && row.source !== filter.source) return false;
    if (filter.status !== ALL && row.status !== filter.status) return false;
    if (needle === "") return true;
    const haystack = [row.id, row.labelTh, row.labelEn, ...row.gMaiden.map((g) => g.id)].join(" ").toLowerCase();
    return haystack.includes(needle);
  });
}

/** Distinct values of one column, in first-seen order, for the filter drop-downs. */
export function distinct(rows: CatalogRow[], key: "category" | "source" | "status"): string[] {
  return [...new Set(rows.map((row) => row[key]))];
}

export function summarize(rows: CatalogRow[]): { total: number; detected: number; catalogOnly: number; testOnly: number } {
  const by = (status: string) => rows.filter((row) => row.status === status).length;
  return { total: rows.length, detected: by("detected_today"), catalogOnly: by("catalog_only"), testOnly: by("test_only") };
}

/** Session-clock milliseconds as m:ss (h:mm:ss over an hour). */
export function formatClock(ms: number): string {
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${minutes}:${pad(seconds)}`;
}

export function lastFiredLabel(lastFired: LastFired | null): string {
  if (lastFired === null) return "ยังไม่เคย";
  const tier = lastFired.tier === null ? "" : ` (tier ${lastFired.tier})`;
  return `${lastFired.count} ครั้ง · ล่าสุด ${formatClock(lastFired.atMs)}${tier}`;
}

export function gMaidenLabel(refs: GMaidenRef[]): string {
  if (refs.length === 0) return "ไม่มี (เฉพาะของเรา)";
  const named = refs.map((ref) => (ref.tier === null ? ref.id : `${ref.id} (tier ${ref.tier})`)).join(", ");
  const approx = refs.some((ref) => ref.relation === "approx");
  return approx ? `~ ${named}` : named;
}

export function tunablesLabel(row: CatalogRow): string {
  if (row.tunables === 0) return "—";
  return row.tunablesEditable > 0
    ? `${row.tunablesEditable}/${row.tunables} แก้ได้`
    : `${row.tunables} (ยังแก้ใน Lab ไม่ได้)`;
}

export const POSSIBLE_STATUS_TH: Record<PossibleEvent["status"], string> = {
  possible_unverified: "เป็นไปได้ ยังไม่ตรวจสอบ ยังไม่ได้สร้าง",
  proposed_not_built: "เสนอไว้ (ออกแบบเท่านั้น) ยังไม่ได้สร้าง",
};
