import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const source = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const app = source("../src/App.tsx");
const packsPage = source("../src/packs/PacksPage.tsx");
const browseApi = source("../src/packs/browseApi.ts");
const runtime = source("../src/runtime.ts");
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
  assert.match(drawer, /<header[^>]+packs-drawer-title[\s\S]*?<\/header>\s*<section className="packs-preview-stage glass-solid"[\s\S]*?<\/section>[\s\S]*?<div className="packs-drawer-scroll glass-solid"[\s\S]*?<footer className="packs-drawer-footer"/);
  assert.match(styles, /\.packs-drawer-content\s*\{\s*grid-template-rows: auto auto auto minmax\(0, 1fr\) auto; overflow: hidden/);
  assert.match(styles, /\.packs-drawer-scroll\s*\{[^}]*overflow-y: auto/);
  assert.match(styles, /@media\s*\(max-height:\s*760px\)\s*\{[^}]*\.packs-preview-frame\s*\{ height: clamp\(112px, 20vh, 154px\)/);
  assert.match(drawer, /className="primary packs-equip-button"/);
  assert.match(copy, /activate: "ใช้แพ็กนี้"/);
});

test("the preview uses a mounted muted video, starts from playback events, and reports failures in the stage", () => {
  assert.match(browseApi, /mediaInstalledEventMap\(installId\)/);
  assert.match(runtime, /"media_installed_event_map", \{ installId \}/);
  assert.match(drawer, /<video ref=\{previewVideo\}[^>]*muted playsInline/);
  assert.match(drawer, /preview\.visualFailure && <p className="packs-preview-empty" role="status">\{text\.previewFailed\}/);
  assert.match(drawer, /packs-preview-progress[^\n]*playing/);
  assert.match(styles, /@keyframes packs-preview-progress\s*\{\s*to\s*\{\s*transform: scaleX\(1\)/);
  assert.match(packsPage, /browseApi\.subscribe/);
  assert.match(packsPage, /playBrowseVideo\(previewVideo\.current\)/);
  assert.match(packsPage, /type: "ended"[\s\S]*?holdBrowseVideo\(previewVideo\.current/);
  assert.match(packsPage, /browseApi\.visualUrl[\s\S]*?prepareBrowseVideo\(video, url\)[\s\S]*?await browseApi\.audition/);
});

test("voice rows use prominent captions, one button and a playback highlight, with IDs in collapsed details", () => {
  const row = drawer.slice(drawer.indexOf('<li className="packs-voice-row"'), drawer.indexOf("</li>;", drawer.indexOf('<li className="packs-voice-row"')));
  assert.equal(row.match(/<button\b/g)?.length, 1);
  assert.match(row, /data-playing=\{isPlaying\}/);
  assert.match(row, /packs-voice-caption[^>]*>\{labelText\}/);
  assert.match(row, /duration_ms[\s\S]*voiceText\.bannerYes/);
  assert.doesNotMatch(row, /<code>|voiceText\.(variantId|clipId|captionMissing)/);
  assert.match(drawer, /<details className="packs-facts-wrap packs-technical">[\s\S]*ข้อมูลทางเทคนิค[\s\S]*<code>\{variant\.variant_id\}<\/code>[\s\S]*<code>\{variant\.clip_id\}<\/code>/);
  assert.match(styles, /\.packs-voice-row\[data-playing="true"\][^}]*background: var\(--accent-surface\)/);
  assert.match(styles, /\.packs-voice-caption\s*\{[^}]*font-size: max\(16px/);
});

test("packs layout defines the requested viewport columns and clips horizontal overflow", () => {
  assert.match(styles, /@container\s*\(min-width:\s*1700px\)[\s\S]*?repeat\(6,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(styles, /@container\s*\(min-width:\s*1080px\)[\s\S]*?repeat\(4,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(styles, /\.packs-scroll[^}]*overflow-x:\s*hidden/);
  assert.match(styles, /\.packs-drawer[^}]*width:\s*min\(100%,\s*clamp\(460px,\s*36vw,\s*680px\)\)/);
});
