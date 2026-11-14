import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/deck/deck.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

test("deck fills its tab with a three-column grid and never scrolls at the root", () => {
  const root = css.match(/\.deck-root\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(root, /display:\s*grid/);
  assert.match(root, /grid-template-columns:[^;]*24fr[^;]*50fr[^;]*26fr/);
  assert.match(root, /height:\s*100%/);
  assert.match(root, /min-height:\s*0/);
  assert.match(root, /overflow:\s*hidden/);
  assert.doesNotMatch(root, /overflow(?:-y)?:\s*(?:auto|scroll)/);
});

test("only the bounded event list scrolls vertically inside the deck", () => {
  const eventList = css.match(/\.deck-event-list\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(eventList, /min-height:\s*0/);
  assert.match(eventList, /overflow-y:\s*auto/);
  assert.match(eventList, /overflow-x:\s*hidden/);
});

test("history and announcer rows stay inside shrinking right-column tracks", () => {
  const right = css.match(/\.deck-right-column\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(right, /grid-template-rows:\s*minmax\(0,\s*1\.4fr\)\s+minmax\(0,\s*1fr\)\s+minmax\(0,\s*max-content\)/);
  assert.doesNotMatch(right, /overflow(?:-y)?:\s*(auto|scroll)/);
  const band = css.match(/\.live-deck-panel \.live-sidebar-deck\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(band, /grid-row:\s*2/);
});

test("alert band stays bounded, has no green encoding, and reserves a row for the enemy caption", () => {
  const alertRules = [...css.matchAll(/([^{}]*\.deck-alert[^{}]*)\{([^}]+)\}/g)].map(rule => rule[2]).join("\n");
  assert.doesNotMatch(alertRules, /var\(--accent(?:-surface)?\)|green|safe/i);
  const strip = css.match(/\.deck-alert-strip\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(strip, /overflow:\s*hidden/);
  assert.match(strip, /min-width:\s*0/);
  const enemy = css.match(/\.deck-enemy-panel\s*\{([^}]+)\}/)?.[1] ?? "";
  // The first match may be the shared card rule; require the explicit three-row panel rule too.
  assert.ok(enemy.length > 0);
  assert.match(css, /\.deck-enemy-panel\s*\{[^}]*grid-template-rows:\s*auto\s+auto\s+minmax\(0,\s*1fr\)/);
});

test("the deck map embeds the reusable minimap panel in a height-constrained grid", () => {
  const mapPanel = css.match(/\.live-tab\.live-deck-panel\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(mapPanel, /grid-template-rows:\s*minmax\(0,\s*1fr\)\s+auto/);
  assert.match(mapPanel, /height:\s*100%/);
  assert.match(mapPanel, /min-height:\s*0/);
  assert.match(mapPanel, /overflow:\s*hidden/);
});

test("the centre column cannot put the live map in an indefinite auto row", () => {
  const column = css.match(/\.deck-map-column\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(column, /grid-template-rows:\s*minmax\(0,\s*1fr\)\s*;/);
  assert.match(column, /height:\s*100%/);
  assert.match(column, /min-height:\s*0/);
  const note = css.match(/\.deck-standby-note\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(note, /position:\s*absolute/);
  const pane = css.match(/\.live-deck-panel \.live-map-pane\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(pane, /display:\s*flex/);
  assert.match(pane, /flex-direction:\s*column/);
  assert.match(pane, /height:\s*100%/);
  assert.match(pane, /min-height:\s*0/);
  const stage = css.match(/\.live-deck-panel \.live-map-stage\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(stage, /flex:\s*1\s+1\s+0/);
  assert.match(stage, /min-height:\s*0/);
  assert.match(stage, /display:\s*grid/);
  assert.match(stage, /place-items:\s*center/);
  const canvas = css.match(/\.live-deck-panel \.live-map-canvas\s*\{([^}]+)\}/)?.[1] ?? "";
  for (const property of ["max-width", "max-height"]) assert.match(canvas, new RegExp(`${property}:\\s*100%`));
});

test("own hero tiles stay compact while their card can shrink at 768px viewport height", () => {
  const column = css.match(/\.deck-own-column\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(column, /grid-template-rows:\s*minmax\(0,\s*max-content\)\s+minmax\(0,\s*1fr\)/);
  assert.doesNotMatch(css, /\.deck-own-column\s*\{[^}]*grid-template-rows:[^;]*\d+px/);
  const tiles = css.match(/\.deck-stats\s*\{([^}]+)\}/)?.[1] ?? "";
  assert.match(tiles, /grid-template-columns:\s*repeat\(2,\s*minmax\(0,\s*1fr\)\)/);
  assert.match(tiles, /grid-template-rows:\s*repeat\(3,\s*minmax\(0,\s*56px\)\)/);
  const component = readFileSync(new URL("../src/deck/Deck.tsx", import.meta.url), "utf8");
  assert.equal(component.match(/\{text\.statCaption\}/g)?.length, 1);
  assert.match(component, /className="deck-stats-caption"/);
});
