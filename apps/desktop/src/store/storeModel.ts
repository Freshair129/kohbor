import type { Locale, Phase } from "../copy.ts";
import { storeCopy } from "../copy.ts";
import { eventMapPhaseBlocksAudition } from "../packs/eventMap.ts";
import type { PackSummary } from "../packs/model.ts";

export type StoreTab = "catalog" | "detail" | "wallet" | "library" | "history";
export type CatalogFilter = "all" | "announcer" | "credits";

export interface StoreSampleVoice {
  id: string;
  name: string;
  durationSec: number;
}

export interface CatalogItem {
  id: string;
  name: string;
  voiceAuthor: string;
  category: "announcer" | "credits";
  language: string;
  description: string;
  samples: StoreSampleVoice[];
  compatibility: {
    kohborVersion: string;
    signatureStatus: string;
  };
  priceDisplay: string;
  priceNote: string;
  badge: string;
  isPlaceholder: boolean;
  buyDisabled: boolean;
  buyLabel: string;
  coverPattern: "waves" | "hexagons" | "rings" | "circuit";
}

export interface TopUpBundle {
  id: string;
  thb: number;
  label: string;
  creditsLabel: string;
  disabled: boolean;
  buttonLabel: string;
}

export interface StoreWalletModel {
  balance: string;
  balanceLabel: string;
  bundles: TopUpBundle[];
  creditsUsageTitle: string;
  creditsUsageBody: string;
}

export interface StoreLibraryItem {
  installId: string;
  packageId: string;
  name: string;
  revision: string;
  status: "installed" | "active";
  eventCount: number;
  variantCount: number;
  bannerCount: number;
  isInstalled: boolean;
  authorityClass: string;
}

export interface StoreHistoryItem {
  id: string;
  date: string;
  title: string;
  amount: string;
  status: string;
}

export function isStorePreviewAvailable(developerModeEnabled: boolean): boolean {
  return developerModeEnabled === true;
}

export function isStoreAuditionBlocked(phase: Phase | null): boolean {
  return eventMapPhaseBlocksAudition(phase);
}

export const PREVIEW_CATALOG_ITEMS: readonly CatalogItem[] = [
  {
    id: "sample-pack-a",
    name: "แพ็กเสียงตัวอย่าง A",
    voiceAuthor: "ผู้พากย์ตัวอย่าง กลุ่ม A",
    category: "announcer",
    language: "ไทย (TH)",
    description: "ชุดเสียงพากย์สังเคราะห์ต้นแบบสำหรับการแข่งขัน Dota 2 ประกอบด้วยการแจ้งเตือน First Blood, Multi-kill, และจังหวะสังหารหลัก",
    samples: [
      { id: "sp-a-1", name: "First Blood (ตัวอย่าง A)", durationSec: 2.4 },
      { id: "sp-a-2", name: "Double Kill (ตัวอย่าง A)", durationSec: 1.8 },
      { id: "sp-a-3", name: "Mega Kill (ตัวอย่าง A)", durationSec: 2.1 },
    ],
    compatibility: {
      kohborVersion: "Kohbor 0.3+",
      signatureStatus: "Ed25519 (ยังไม่เปิดตรวจ)",
    },
    priceDisplay: "— ยังไม่กำหนดราคา",
    priceNote: "ยังไม่กำหนดราคา",
    badge: "ตัวอย่าง",
    isPlaceholder: true,
    buyDisabled: true,
    buyLabel: "ยังไม่เปิดขาย",
    coverPattern: "waves",
  },
  {
    id: "sample-pack-b",
    name: "แพ็กเสียงตัวอย่าง B",
    voiceAuthor: "ผู้พากย์ตัวอย่าง กลุ่ม B",
    category: "announcer",
    language: "ไทย (TH)",
    description: "ชุดเสียงบรรยายสไตล์เกมกลยุทธ์ เน้นจังหวะแจ้งเตือนรูน ป้อมปราการถูกโจมตี และสถานะการเกิดของ Roshan",
    samples: [
      { id: "sp-b-1", name: "Rune Spawn (ตัวอย่าง B)", durationSec: 2.0 },
      { id: "sp-b-2", name: "Tower Under Attack (ตัวอย่าง B)", durationSec: 2.2 },
      { id: "sp-b-3", name: "Victory (ตัวอย่าง B)", durationSec: 3.0 },
    ],
    compatibility: {
      kohborVersion: "Kohbor 0.3+",
      signatureStatus: "Ed25519 (ยังไม่เปิดตรวจ)",
    },
    priceDisplay: "— ยังไม่กำหนดราคา",
    priceNote: "ยังไม่กำหนดราคา",
    badge: "ตัวอย่าง",
    isPlaceholder: true,
    buyDisabled: true,
    buyLabel: "ยังไม่เปิดขาย",
    coverPattern: "hexagons",
  },
  {
    id: "sample-pack-c",
    name: "แพ็กเสียงตัวอย่าง C",
    voiceAuthor: "ผู้พากย์ตัวอย่าง กลุ่ม C",
    category: "announcer",
    language: "อังกฤษ / Thai (EN/TH)",
    description: "ชุดเสียงแบบสองภาษา ผสมผสานศัพท์เทคนิคการเล่น Dota 2 เข้ากับคำอธิบายกระชับสำหรับผู้เล่นระดับแข่งขัน",
    samples: [
      { id: "sp-c-1", name: "Triple Kill (ตัวอย่าง C)", durationSec: 2.1 },
      { id: "sp-c-2", name: "Ultra Kill (ตัวอย่าง C)", durationSec: 2.3 },
      { id: "sp-c-3", name: "Rampage (ตัวอย่าง C)", durationSec: 2.7 },
    ],
    compatibility: {
      kohborVersion: "Kohbor 0.3+",
      signatureStatus: "Ed25519 (ยังไม่เปิดตรวจ)",
    },
    priceDisplay: "— ยังไม่กำหนดราคา",
    priceNote: "ยังไม่กำหนดราคา",
    badge: "ตัวอย่าง",
    isPlaceholder: true,
    buyDisabled: true,
    buyLabel: "ยังไม่เปิดขาย",
    coverPattern: "rings",
  },
  {
    id: "sample-credits-pack",
    name: "เครดิตการใช้งาน AI ตัวอย่าง",
    voiceAuthor: "ระบบ Kohbor AI",
    category: "credits",
    language: "สากล (Global)",
    description: "แพ็กเกจเครดิตสำหรับประมวลผลการวิเคราะห์รีเพลย์เชิงลึกและคำแนะนำกลยุทธ์ดราฟต์ผ่านโมเดลปัญญาประดิษฐ์",
    samples: [
      { id: "sp-cr-1", name: "คำแนะนำดราฟต์ (ตัวอย่าง)", durationSec: 3.2 },
      { id: "sp-cr-2", name: "สรุปจังหวะตายสำคัญ (ตัวอย่าง)", durationSec: 3.5 },
    ],
    compatibility: {
      kohborVersion: "Kohbor 0.3+",
      signatureStatus: "Ed25519 (ยังไม่เปิดตรวจ)",
    },
    priceDisplay: "— ยังไม่กำหนดราคา",
    priceNote: "ยังไม่กำหนดราคา",
    badge: "ตัวอย่าง",
    isPlaceholder: true,
    buyDisabled: true,
    buyLabel: "ยังไม่เปิดขาย",
    coverPattern: "circuit",
  },
];

export function filterCatalogItems(items: readonly CatalogItem[], filter: CatalogFilter): CatalogItem[] {
  if (filter === "announcer") {
    return items.filter(item => item.category === "announcer");
  }
  if (filter === "credits") {
    return items.filter(item => item.category === "credits");
  }
  return [...items];
}

export function deriveStoreWallet(locale: Locale = "th"): StoreWalletModel {
  const text = storeCopy(locale);
  return {
    balance: "—",
    balanceLabel: text.balanceLabel,
    bundles: text.walletTopUpTitle ? [
      { id: "bundle-150", thb: 150, label: locale === "th" ? "150 บาท" : "150 THB", creditsLabel: "— เครดิต", disabled: true, buttonLabel: text.topUpButton },
      { id: "bundle-300", thb: 300, label: locale === "th" ? "300 บาท" : "300 THB", creditsLabel: "— เครดิต", disabled: true, buttonLabel: text.topUpButton },
      { id: "bundle-500", thb: 500, label: locale === "th" ? "500 บาท" : "500 THB", creditsLabel: "— เครดิต", disabled: true, buttonLabel: text.topUpButton },
    ] : [],
    creditsUsageTitle: text.creditsUsageTitle,
    creditsUsageBody: text.creditsUsageBody,
  };
}

export function deriveStoreLibrary(installedPacks: readonly PackSummary[] | null | undefined): StoreLibraryItem[] {
  if (!installedPacks || installedPacks.length === 0) {
    return [];
  }
  return installedPacks.map(pack => ({
    installId: pack.installId,
    packageId: pack.packageId,
    name: pack.name ?? pack.installId,
    revision: `r${pack.revision}`,
    status: pack.active ? "active" : "installed",
    eventCount: pack.eventCount,
    variantCount: pack.variantCount,
    bannerCount: pack.bannerCount,
    isInstalled: true,
    authorityClass: pack.authorityClass,
  }));
}

export function deriveStoreHistory(): StoreHistoryItem[] {
  // Always an honest empty state — no backend, no transactions exist
  return [];
}
