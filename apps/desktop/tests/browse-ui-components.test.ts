import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const source = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const app = source("../src/App.tsx");
const packsPage = source("../src/packs/PacksPage.tsx");
const drawer = source("../src/packs/PackLibrary.tsx");
const rail = source("../src/shell/RailLeft.tsx");
const copy = source("../src/copy.ts");
const styles = source("../src/styles.css").replace(/\/\*[\s\S]*?\*\//g, "");
const glass = source("../../../docs/design/visuals/command-deck/tokens.glass.css").replace(/\/\*[\s\S]*?\*\//g, "");

test("the glass preference is live in Settings and attached to the app and document roots", () => {
  assert.match(app, /useGlassPreference/);
  assert.match(app, /data-glass=\{glass\.level\}/);
  assert.match(app, /รูปลักษณ์ \/ กระจก/);
  assert.match(app, /value="off"/);
  assert.match(app, /value="low"/);
  assert.match(app, /value="medium"/);
  assert.match(app, /value="high"/);
  assert.match(styles, /@import\s+"\.\.\/\.\.\/\.\.\/docs\/design\/visuals\/command-deck\/tokens\.glass\.css"/);
  assert.match(styles, /\.app\[data-glass\] \.dialog-panel\.pack-install-dialog[\s\S]*?backdrop-filter:\s*var\(--glass-backdrop-3\)/);
});

test("glass fallbacks are solid for off, reduced transparency, forced colors, and unsupported blur", () => {
  for (const fallback of [/data-glass="off"/, /prefers-reduced-transparency:\s*reduce/, /forced-colors:\s*active/, /@supports\s+not\s*\(\(?backdrop-filter/]) {
    assert.match(glass, fallback);
  }
  assert.match(glass, /--glass-blur-2:\s*16px/);
  assert.match(glass, /--glass-blur-3:\s*24px/);
  assert.match(glass, /\.glass--solid|\.glass-solid/);
  assert.match(glass, /prefers-reduced-motion:\s*reduce/);
  assert.match(glass, /\.glass-solid/);
});

test("the rail game selector shows the supported game before the focusable unsupported list", () => {
  assert.match(rail, /<GameSelector/);
  assert.match(rail, /brandMark[\s\S]*?GameSelector[\s\S]*?<nav/);
  assert.match(packsPage, /text\.storeLabel/);
  assert.match(packsPage, /text\.storeBody/);
  assert.match(copy, /storeLabel: "ร้านค้า · ยังไม่เปิด"/);
  assert.match(copy, /storeBody: "รุ่นนี้ยังไม่มีร้านค้า จึงไม่มีรายการสินค้า ราคา หรือการชำระเงิน/);
  assert.match(copy, /storeBack: "กลับไปคลังแพ็ก"/);
  assert.doesNotMatch(packsPage, /price:\s*\d|price\s*\}/i);
});

test("the drawer keeps a focus trap and has a pinned preview stage, scroll list, and equip footer", () => {
  assert.match(drawer, /onKeyDown=\{event => \{[\s\S]*?event\.key !== "Tab"/);
  assert.match(drawer, /packs-preview-stage/);
  assert.match(drawer, /<EventVariants/);
  assert.match(drawer, /packs-drawer-scroll/);
  assert.match(drawer, /packs-drawer-footer/);
  assert.match(drawer, /className="primary packs-equip-button"/);
  assert.match(copy, /activate: "ใช้แพ็กนี้"/);
  assert.match(packsPage, /browseApi\.subscribe/);
  assert.match(packsPage, /event\.visualUrl/);
});

test("packs layout defines the requested viewport columns and clips horizontal overflow", () => {
  assert.match(styles, /@container\s*\(min-width:\s*1700px\)[\s\S]*?repeat\(6,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(styles, /@container\s*\(min-width:\s*1080px\)[\s\S]*?repeat\(4,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(styles, /\.packs-scroll[^}]*overflow-x:\s*hidden/);
  assert.match(styles, /\.packs-drawer[^}]*width:\s*min\(100%,\s*clamp\(460px,\s*36vw,\s*680px\)\)/);
});
