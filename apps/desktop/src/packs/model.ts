// @ts-ignore Explicit source extensions support the Node test runner.
import { packsPageCopy, type Locale } from "../copy.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { soundLabel, type SoundKey, type SoundSelection } from "./eventMap.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { parseMediaPackSummaries } from "../mediaPackSummaries.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { MediaPackSummaries, MediaPackSummaryV1_1, MediaPackSummaryV1_2 } from "../mediaPackSummaries.ts";

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
  equippedSlot: number | null;
  packKind: "basic" | "special" | "mixed" | "unknown";
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

export const UNINSTALL_ERROR_CODES = [
  "UNKNOWN_INSTALL",
  "UNINSTALL_ACTIVE",
  "UNINSTALL_IN_LOADOUT",
  "UNINSTALL_PENDING_LOADOUT",
  "UNINSTALL_PENDING_ACTIVATION",
  "MATCH_ACTIVE",
  "UNINSTALL_FAILED",
] as const;

export type UninstallErrorCode = (typeof UNINSTALL_ERROR_CODES)[number];

export function isPackInUse(pack: Pick<PackSummary, "active" | "equippedSlot">): boolean {
  return pack.active || pack.equippedSlot !== null;
}

export function uninstallErrorCode(error: unknown): UninstallErrorCode {
  const candidate = typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : "";
  return UNINSTALL_ERROR_CODES.includes(candidate as UninstallErrorCode)
    ? candidate as UninstallErrorCode
    : "UNINSTALL_FAILED";
}

export function uninstallCopy(locale: Locale) {
  if (locale === "th") {
    const refusals: Record<UninstallErrorCode, string> = {
      UNKNOWN_INSTALL: "ไม่พบแพ็กที่ติดตั้งแล้ว",
      UNINSTALL_ACTIVE: "กำลังใช้อยู่ — เลือกแพ็กอื่นก่อน",
      UNINSTALL_IN_LOADOUT: "ยังอยู่ในชุดแพ็ก — เลือกแพ็กอื่นก่อน",
      UNINSTALL_PENDING_LOADOUT: "กำลังรอเปลี่ยนชุดแพ็ก — ยกเลิกคำขอก่อน",
      UNINSTALL_PENDING_ACTIVATION: "กำลังรอเปิดใช้งาน — ยกเลิกคำขอก่อน",
      MATCH_ACTIVE: "กำลังแข่งอยู่ — ลบแพ็กได้หลังจบแมตช์",
      UNINSTALL_FAILED: "ลบแพ็กไม่สำเร็จ ลองอีกครั้ง",
    };
    return {
      remove: "ลบแพ็ก",
      inUse: "กำลังใช้อยู่ — เลือกแพ็กอื่นก่อน",
      confirmation: (name: string, revision: number) =>
        `ยืนยันลบแพ็ก “${name}” รุ่น ${revision}?\n\nการลบนี้ย้อนคืนไม่ได้ คลิปเสียงประกาศของแพ็กจะถูกลบจากคอมพิวเตอร์เครื่องนี้ ส่วนไฟล์แพ็กต้นฉบับที่เก็บไว้ที่อื่นจะไม่ถูกแตะต้อง`,
      refusal: (code: string) => refusals[code as UninstallErrorCode] ?? refusals.UNINSTALL_FAILED,
      removing: "กำลังลบ…",
    };
  }

  const refusals: Record<UninstallErrorCode, string> = {
    UNKNOWN_INSTALL: "This pack is no longer installed.",
    UNINSTALL_ACTIVE: "In use — choose another pack first.",
    UNINSTALL_IN_LOADOUT: "This pack is still equipped — choose another pack first.",
    UNINSTALL_PENDING_LOADOUT: "A loadout change is pending — cancel it first.",
    UNINSTALL_PENDING_ACTIVATION: "Activation is pending — cancel it first.",
    MATCH_ACTIVE: "A match is active — remove the pack after it ends.",
    UNINSTALL_FAILED: "Couldn't remove the pack. Try again.",
  };
  return {
    remove: "Remove pack",
    inUse: "In use — choose another pack first.",
    confirmation: (name: string, revision: number) =>
      `Remove “${name}”, revision ${revision}?\n\nThis cannot be undone. The pack’s announcer clips will be deleted from this computer. The original package file stored elsewhere will not be touched.`,
    refusal: (code: string) => refusals[code as UninstallErrorCode] ?? refusals.UNINSTALL_FAILED,
    removing: "Removing…",
  };
}

export function parsePackSummaries(value: unknown): PackSummary[] | null {
  const source = parseMediaPackSummaries(value);
  if (!source) return null;
  return mapPackSummaries(source);
}

export function mapPackSummaries(source: MediaPackSummaries): PackSummary[] {
  return source.records.map(row => {
    const modern = source.schema_version === "1.0.0" ? null : row as MediaPackSummaryV1_1;
    const equipped = source.schema_version === "1.2.0" ? row as MediaPackSummaryV1_2 : null;
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
      equippedSlot: equipped?.equipped_slot ?? (row.active ? 0 : null), packKind: equipped?.pack_kind ?? "unknown",
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
