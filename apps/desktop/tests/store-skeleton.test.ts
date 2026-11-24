import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { storeCopy } from "../src/copy.ts";
import {
  deriveStoreHistory,
  deriveStoreLibrary,
  deriveStoreWallet,
  filterCatalogItems,
  isStoreAuditionBlocked,
  isStorePreviewAvailable,
  PREVIEW_CATALOG_ITEMS,
} from "../src/store/storeModel.ts";
import type { PackSummary } from "../src/packs/model.ts";

const storePageSource = readFileSync(new URL("../src/store/StorePage.tsx", import.meta.url), "utf8");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const packsPageSource = readFileSync(new URL("../src/packs/PacksPage.tsx", import.meta.url), "utf8");
const storeCssSource = readFileSync(new URL("../src/store/Store.css", import.meta.url), "utf8");

test("every buy and top-up control is disabled and labelled 'ยังไม่เปิดขาย'", () => {
  const textTh = storeCopy("th");
  const textEn = storeCopy("en");

  assert.equal(textTh.notForSale, "ยังไม่เปิดขาย");
  assert.equal(textEn.notForSale, "ยังไม่เปิดขาย");

  // Every catalog item has buyDisabled = true and buyLabel = 'ยังไม่เปิดขาย'
  assert.ok(PREVIEW_CATALOG_ITEMS.length > 0);
  for (const item of PREVIEW_CATALOG_ITEMS) {
    assert.equal(item.buyDisabled, true, `item ${item.id} must have buyDisabled=true`);
    assert.equal(item.buyLabel, "ยังไม่เปิดขาย", `item ${item.id} must be labelled 'ยังไม่เปิดขาย'`);
    assert.match(item.priceDisplay, /ยังไม่กำหนดราคา/);
    assert.equal(item.priceNote, "ยังไม่กำหนดราคา");
  }

  // Every wallet top-up option is disabled and labelled 'ยังไม่เปิดขาย'
  const walletTh = deriveStoreWallet("th");
  assert.equal(walletTh.balance, "—");
  assert.ok(walletTh.bundles.length >= 3);
  for (const bundle of walletTh.bundles) {
    assert.equal(bundle.disabled, true, `wallet bundle ${bundle.id} must have disabled=true`);
    assert.equal(bundle.buttonLabel, "ยังไม่เปิดขาย", `wallet bundle ${bundle.id} must have buttonLabel='ยังไม่เปิดขาย'`);
    assert.match(bundle.creditsLabel, /—/);
  }

  const walletEn = deriveStoreWallet("en");
  assert.equal(walletEn.balance, "—");
  for (const bundle of walletEn.bundles) {
    assert.equal(bundle.disabled, true);
    assert.equal(bundle.buttonLabel, "ยังไม่เปิดขาย");
  }

  // Verify in StorePage.tsx that buttons have disabled attribute wired to buyDisabled / disabled
  assert.match(storePageSource, /className="store-buy-button"[^>]*disabled=\{item\.buyDisabled\}/);
  assert.match(storePageSource, /className="store-topup-button"[^>]*disabled/);
});

test("the preview banner and badges appear when Developer mode is on, and the skeleton is hidden when it is off", () => {
  assert.equal(isStorePreviewAvailable(true), true);
  assert.equal(isStorePreviewAvailable(false), false);

  const textTh = storeCopy("th");
  assert.equal(textTh.previewBanner, "ตัวอย่างการออกแบบ — ยังไม่ใช่สินค้าจริง ยังขายไม่ได้");
  assert.equal(textTh.previewBadge, "ตัวอย่าง");

  // All catalog items are placeholders with the preview badge
  for (const item of PREVIEW_CATALOG_ITEMS) {
    assert.equal(item.isPlaceholder, true);
    assert.equal(item.badge, "ตัวอย่าง");
  }

  // Banner in StorePage.tsx
  assert.match(storePageSource, /className="store-preview-banner"/);
  assert.match(storePageSource, /text\.previewBanner/);
  assert.match(storePageSource, /text\.previewBadge/);

  // App.tsx switches on developerMode.enabled for store
  assert.match(appSource, /activePage === "store"\s*&&\s*\(\s*developerMode\.enabled\s*\?\s*\(?\s*<StorePage/);
  assert.match(appSource, /<PacksPage[^>]*storeOnly=\{true\}/);

  // PacksPage.tsx handles storeOnly and developerMode
  assert.match(packsPageSource, /storeOnly\s*\?\s*\(\s*developerMode\s*\?\s*\(?\s*<StorePage/);
  assert.match(packsPageSource, /text\.storeHeading/);
  assert.match(packsPageSource, /text\.storeBody/);
});

test("the library lists only real installed packs and never invents ownership", () => {
  // Empty installed list returns empty library
  assert.deepEqual(deriveStoreLibrary([]), []);
  assert.deepEqual(deriveStoreLibrary(null), []);
  assert.deepEqual(deriveStoreLibrary(undefined), []);

  // When real packs exist, deriveStoreLibrary returns them truthfully
  const mockInstalledPacks: PackSummary[] = [
    {
      installId: "ins_real_001",
      packageId: "org.kohbor.cyber-thai",
      revision: 2,
      active: true,
      pending: false,
      name: "Cyber Thai Announcer",
      problem: null,
      coverUrl: null,
      counts: { events: 42, voices: 120, banners: 8 },
      bindings: [],
      authorityClass: "system",
      publisherClaim: null,
      installedAt: null,
      sizeBytes: null,
      gameId: "dota2",
      presentation: { description: null, author: null, languages: null, coverUrl: null },
    },
    {
      installId: "ins_real_002",
      packageId: "org.kohbor.community-pack",
      revision: 1,
      active: false,
      pending: false,
      name: "Community Voices",
      problem: null,
      coverUrl: null,
      counts: { events: 15, voices: 30, banners: 0 },
      bindings: [],
      authorityClass: "local_import",
      publisherClaim: null,
      installedAt: null,
      sizeBytes: null,
      gameId: "dota2",
      presentation: { description: null, author: null, languages: null, coverUrl: null },
    },
  ];

  const library = deriveStoreLibrary(mockInstalledPacks);
  assert.equal(library.length, 2);

  assert.equal(library[0]!.installId, "ins_real_001");
  assert.equal(library[0]!.name, "Cyber Thai Announcer");
  assert.equal(library[0]!.revision, "r2");
  assert.equal(library[0]!.status, "active");
  assert.equal(library[0]!.isInstalled, true);
  assert.equal(library[0]!.eventCount, 42);
  assert.equal(library[0]!.variantCount, 120);

  assert.equal(library[1]!.installId, "ins_real_002");
  assert.equal(library[1]!.name, "Community Voices");
  assert.equal(library[1]!.revision, "r1");
  assert.equal(library[1]!.status, "installed");
  assert.equal(library[1]!.isInstalled, true);

  // Check that no mock placeholder items are in the library
  for (const item of library) {
    assert.equal(item.isInstalled, true);
    assert.doesNotMatch(item.name, /ตัวอย่าง/);
  }
});

test("purchase history returns an honest empty state with no simulated transactions", () => {
  const history = deriveStoreHistory();
  assert.deepEqual(history, []);

  const textTh = storeCopy("th");
  assert.equal(textTh.historyEmptyTitle, "ไม่มีประวัติการซื้อ");
  assert.match(textTh.historyEmptyBody, /ยังไม่เปิดขาย/);

  assert.match(storePageSource, /className="store-empty-history-card"/);
  assert.match(storePageSource, /text\.historyEmptyTitle/);
});

test("audition of preview samples is safely blocked during active Dota match phases", () => {
  assert.equal(isStoreAuditionBlocked("live"), true);
  assert.equal(isStoreAuditionBlocked("pregame"), true);
  assert.equal(isStoreAuditionBlocked("draft"), true);
  assert.equal(isStoreAuditionBlocked("post_game"), true);
  assert.equal(isStoreAuditionBlocked("none"), false);
  assert.equal(isStoreAuditionBlocked("unsupported"), false);
  assert.equal(isStoreAuditionBlocked(null), false);

  assert.match(storePageSource, /isMatchAuditionBlocked\s*=\s*isStoreAuditionBlocked\(phase\)/);
  assert.match(storePageSource, /disabled=\{disabled\s*\|\|\s*isMatchAuditionBlocked\}/);
});

test("catalog filter correctly subsets all, announcer packs, and AI credits", () => {
  const all = filterCatalogItems(PREVIEW_CATALOG_ITEMS, "all");
  assert.equal(all.length, PREVIEW_CATALOG_ITEMS.length);

  const announcers = filterCatalogItems(PREVIEW_CATALOG_ITEMS, "announcer");
  assert.ok(announcers.length > 0);
  assert.ok(announcers.every(item => item.category === "announcer"));

  const credits = filterCatalogItems(PREVIEW_CATALOG_ITEMS, "credits");
  assert.ok(credits.length > 0);
  assert.ok(credits.every(item => item.category === "credits"));

  assert.equal(announcers.length + credits.length, all.length);
});

test("store css enforces 12px minimum font, no horizontal scroll, and bounded content scroll", () => {
  assert.match(storeCssSource, /overflow-x:\s*hidden/);
  assert.match(storeCssSource, /\.store-content-scroll\s*\{[^}]*overflow-y:\s*auto/);

  // Check that no font-size smaller than 12px exists in Store.css
  const fontSizes = Array.from(storeCssSource.matchAll(/font-size:\s*([0-9]+)px/g)).map(m => Number(m[1]));
  for (const size of fontSizes) {
    assert.ok(size >= 12, `font-size ${size}px must be at least 12px`);
  }
});
