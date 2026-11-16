import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { parseDescription, profileLabel } from "../src/lab/labModel.ts";

const css = readFileSync(new URL("../src/lab/lab.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const component = readFileSync(new URL("../src/lab/Lab.tsx", import.meta.url), "utf8");
const shellCss = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

function rule(selector: string): Map<string, string> {
  const merged = new Map<string, string>();
  for (const match of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const selectors = match[1].split(",").map((item) => item.trim());
    if (!selectors.includes(selector)) continue;
    for (const declaration of match[2].split(";")) {
      const separator = declaration.indexOf(":");
      if (separator < 0) continue;
      merged.set(declaration.slice(0, separator).trim().toLowerCase(), declaration.slice(separator + 1).trim().toLowerCase());
    }
  }
  return merged;
}

test("the studio is a fixed full-window panel outside the shell grid", () => {
  const root = rule(".lab-overlay-root");
  assert.equal(root.get("position"), "fixed");
  assert.equal(root.get("inset"), "0");
});

test("the studio body is the one bounded scroll container with a min-height:0 chain", () => {
  const panel = rule(".lab-panel");
  assert.equal(panel.get("display"), "flex");
  assert.equal(panel.get("flex-direction"), "column");
  assert.equal(panel.get("min-height"), "0");
  const scroll = rule(".lab-scroll");
  assert.equal(scroll.get("overflow-y"), "auto");
  assert.equal(scroll.get("min-height"), "0");
  assert.ok(scroll.get("flex")?.startsWith("1 1 0"), `flex is ${scroll.get("flex")}`);
  assert.ok(rule(".lab-scroll:focus-visible").get("outline"), "a visible focus ring");
});

test("the scroll container is keyboard focusable and hosts both sections", () => {
  assert.match(component, /className="lab-scroll"\s+tabIndex=\{0\}/);
  assert.match(component, /<OverlayEditor \/>/);
  assert.match(component, /role="tablist"/);
});

test("the canvas is sized from the viewport height so it fits without scrolling", () => {
  assert.match(rule(".lab-canvas").get("width") ?? "", /100dvh/);
});

test("the shell no-page-scroll rule is untouched by the Lab", () => {
  assert.doesNotMatch(css, /(^|\s)(html|body|#root|\.app)\s*\{/);
  assert.match(shellCss, /\.app \{[^}]*overflow: hidden/);
});

test("profile label is neutral when no override file is used", () => {
  const base = {
    profile_version: "1.0",
    taxonomy_version: "1.1",
    embedded_default_sha256: "ab".repeat(32),
    events: {},
    override_path: null,
  };
  const embedded = parseDescription({ ...base, source: "embedded", overrides: [], rejected: null });
  assert.ok(embedded);
  assert.equal(profileLabel(embedded), "embedded profile (no override file)");
  const override = parseDescription({ ...base, source: "override", overrides: ["a", "b"], rejected: null });
  assert.ok(override);
  assert.equal(profileLabel(override), "override file (2 changes)");
});
