import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");

test("scoreboard occupies the minimap pane header and the old repeated status rows are absent", () => {
  const panel = read("../src/live/MinimapPanel.tsx");
  const deck = read("../src/deck/Deck.tsx");
  const layoutCss = read("../src/live/secondMonitor.css");
  assert.match(panel, /const toolbar = \([\s\S]*?<ScoreboardHeader/);
  assert.match(panel, /const mapPane = \([\s\S]*?<section className="live-map-pane"[\s\S]*?\{toolbar\}/);
  assert.match(layoutCss, /\.deck-field-map \.live-map-toolbar \{[^}]*flex:\s*0 0 54px;[^}]*height:\s*54px/);
  assert.match(layoutCss, /\.deck-field-map \.live-map-body\.is-field \{[^}]*height:\s*min\(100cqw,\s*calc\(100cqh - 54px\)\)/);
  assert.doesNotMatch(panel, /live-map-badge|<h1 id="live-map-heading"/);
  assert.doesNotMatch(deck, /gsi-phase-strip|deck-field-phase/);
  assert.match(panel, /layout === "settings"[\s\S]*?\{actions\}/);
  assert.doesNotMatch(panel.slice(panel.indexOf("const toolbar ="), panel.indexOf("const stage =")), /\{actions\}|statusBlock/);
  const scoreboard = read("../src/live/ScoreboardHeader.tsx");
  assert.match(scoreboard, /role="group" aria-label=.*Match scoreboard/);
  assert.match(scoreboard, /Kill momentum \(last 3 min\)/);
  assert.match(scoreboard, /not win probability/);
  const stage = read("../src/live/MinimapStage.tsx");
  assert.match(stage, /const showCover = !hasFrame \|\| machine\.view === "first_frame"/);
  assert.match(stage, /!hasFrame && <div className="minimap-placeholder" role="status"/);
  assert.doesNotMatch(stage, /live-map-covered|live-map-plate/);
});

test("header CSS reserves its own status row at 1280 and 1440 widths and a single row at 1920", () => {
  const css = read("../src/shell/header.css");
  assert.match(css, /grid-template-areas: "status start monitor mute"/);
  assert.match(css, /@media \(max-width: 1599px\)[\s\S]*grid-template-areas: "status status status" "start monitor mute"/);
  for (const width of [1280, 1440, 1920]) {
    const usesWrappedStatusRow = width <= 1599;
    assert.equal(usesWrappedStatusRow, width === 1280 || width === 1440);
  }
});

test("Live regions clip instead of scrolling and moved frame controls render only in Settings", () => {
  const liveCss = read("../src/live/live.css");
  const stageCss = read("../src/live/minimapStage.css");
  const panel = read("../src/live/MinimapPanel.tsx");
  const status = read("../src/shell/StatusLine.tsx");
  assert.match(liveCss, /\.live-map-pane[^\n]*overflow: hidden/);
  assert.match(liveCss, /\.live-centre-dashboard, \.live-postgame-summary[^\n]*overflow: hidden/);
  assert.match(stageCss, /\.minimap-placeholder > span[^\n]*white-space: nowrap/);
  assert.match(panel, /layout === "settings" \? \([\s\S]*?<section className="live-capture-settings"[\s\S]*?\{actions\}/);
  assert.doesNotMatch(status, /shell-host-lost" role="alert"|shell-status-detail/);
  assert.match(status, /const tooltip = model\.hostLost \? hostLostMessage/);
  assert.match(status, /className="shell-status-line" aria-label=\{label\} title=\{tooltip\}/);
  const postgameCss = liveCss + stageCss;
  assert.match(postgameCss, /live-postgame-summary \{[^}]*animation: live-centre-flip 600ms/);
  assert.match(postgameCss, /prefers-reduced-motion:\s*reduce[^}]*live-postgame-summary \{ animation: none/);
  const postgame = read("../src/live/PostGameSummary.tsx");
  assert.match(postgame, /summary\.replay && <button/);
  assert.match(postgame, /Match recap is unavailable/);
});
