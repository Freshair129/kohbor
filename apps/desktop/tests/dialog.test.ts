import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { dialogKeyAction, isBackdropClick, wrapFocusIndex } from "../src/shared/dialogModel.ts";

test("Esc closes, Tab keys map to focus actions, other keys are ignored", () => {
  assert.equal(dialogKeyAction("Escape", false), "close");
  assert.equal(dialogKeyAction("Tab", false), "tab-forward");
  assert.equal(dialogKeyAction("Tab", true), "tab-backward");
  assert.equal(dialogKeyAction("a", false), null);
});

test("focus wraps inside the dialog at both ends only", () => {
  assert.equal(wrapFocusIndex("tab-forward", 2, 3), 0);
  assert.equal(wrapFocusIndex("tab-forward", 0, 3), null);
  assert.equal(wrapFocusIndex("tab-backward", 0, 3), 2);
  assert.equal(wrapFocusIndex("tab-backward", 1, 3), null);
  assert.equal(wrapFocusIndex("tab-forward", -1, 0), null);
});

test("only a click on the backdrop itself closes the dialog", () => {
  const backdrop = {};
  assert.equal(isBackdropClick(backdrop, backdrop), true);
  assert.equal(isBackdropClick({}, backdrop), false);
  assert.equal(isBackdropClick(null, null), false);
});

test("dialog component has a visible close button, restores focus and is used by the live panels", () => {
  const source = readFileSync(new URL("../src/shared/Dialog.tsx", import.meta.url), "utf8");
  assert.match(source, /className="dialog-close"/);
  assert.match(source, /role="dialog"/);
  assert.match(source, /aria-modal="true"/);
  assert.match(source, /opener\?\.isConnected\) opener\.focus\(\)/);
});

test("allies panel is static: no absolute/fixed positioning or z-index", () => {
  // R5: the allies disclosure became an always-visible compact card list; it must stay in flow.
  const css = readFileSync(new URL("../src/deck/deck.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  assert.doesNotMatch(css, /\.deck-ally-disclosure/);
  const cards = readFileSync(new URL("../src/live/heroCard.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  for (const selector of [/\.deck-ally-panel[^{,]*\{([^}]+)\}/g, /\.lhc-list[^{,]*\{([^}]+)\}/g, /\.lhc-compact[^{,]*\{([^}]+)\}/g]) {
    const rules = [...(css + cards).matchAll(selector)].map(m => m[1]).join("\n");
    assert.doesNotMatch(rules, /z-index|position:\s*(?:absolute|fixed)/);
  }
});

test("no floating popover rules remain outside dialogs and menus", () => {
  for (const file of ["../src/live/live.css", "../src/deck/deck.css"]) {
    const css = readFileSync(new URL(file, import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    assert.doesNotMatch(css, /\.(?:live-disclosure-body|deck-ally-disclosure)[^{]*\{[^}]*position:\s*(?:absolute|fixed)/);
  }
});
