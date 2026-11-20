import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8")
  .replace(/\/\*[\s\S]*?\*\//g, "");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const leftRailSource = readFileSync(new URL("../src/shell/RailLeft.tsx", import.meta.url), "utf8");

function parseRules(source: string): Map<string, Map<string, string>[]> {
  const rules = new Map<string, Map<string, string>[]>();
  for (const match of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    const declarations = new Map<string, string>();
    for (const declaration of match[2].split(";")) {
      const separator = declaration.indexOf(":");
      if (separator < 0) continue;
      declarations.set(
        declaration.slice(0, separator).trim().toLowerCase(),
        declaration.slice(separator + 1).trim().toLowerCase(),
      );
    }
    for (const selector of match[1].split(",").map((item) => item.trim()).filter(Boolean)) {
      const entries = rules.get(selector) ?? [];
      entries.push(declarations);
      rules.set(selector, entries);
    }
  }
  return rules;
}

const rules = parseRules(css);

function declarationsFor(selector: string): Map<string, string>[] {
  return rules.get(selector) ?? [];
}

function overflowY(declarations: Map<string, string>): string | undefined {
  return declarations.get("overflow-y") ?? declarations.get("overflow")?.split(/\s+/).at(0);
}

test("the document, body and root do not create a vertical scrollbar", () => {
  for (const selector of ["html", "body", "#root"]) {
    const entries = declarationsFor(selector);
    assert.ok(entries.length > 0, `${selector} has an explicit canvas rule`);
    assert.ok(
      entries.some((declarations) => ["hidden", "clip"].includes(overflowY(declarations) ?? "")),
      `${selector} clips vertical overflow`,
    );
  }

  for (const selector of ["body", "#root", ".app"]) {
    for (const declarations of declarationsFor(selector)) {
      assert.ok(!["auto", "scroll"].includes(overflowY(declarations) ?? ""), `${selector} does not scroll vertically`);
    }
  }
});

test("the application shell occupies the viewport height", () => {
  const heights = declarationsFor(".app").map((declarations) => declarations.get("height"));
  assert.ok(heights.some((height) => height === "100dvh" || height === "100vh"));
  assert.ok(declarationsFor(".app").some((declarations) => declarations.get("min-height") === "0"));
});

test("the app uses a tokenized two-rail grid and no top tab bar", () => {
  assert.ok(declarationsFor(".app").some((rule) => rule.get("grid-template-columns") === "var(--rail-width) minmax(0, 1fr) var(--rail-width)"));
  assert.ok(declarationsFor(".shell-rail").some((rule) => rule.get("min-width") === "0"));
  assert.ok(declarationsFor(".shell-main").some((rule) => rule.get("min-width") === "0" && rule.get("background") === "var(--surface)"));
  assert.match(css, /@import\s+"\.\.\/\.\.\/\.\.\/docs\/design\/visuals\/command-deck\/tokens\.css"/);
  assert.doesNotMatch(css, /#[0-9a-fA-F]{3,8}\b|(?:rgb|rgba|hsl|hsla)\([^)]*\)/);
  assert.doesNotMatch(appSource, /shell-tab-list|shell-tab-panel|shell-header/);
});

test("rail labels wrap in Thai and the decorative ribbons cannot receive pointer input", () => {
  assert.ok(declarationsFor(".shell-rail-link > span").some((rule) => rule.get("white-space") === "normal" && rule.get("overflow-wrap") === "anywhere"));
  assert.ok(declarationsFor(".shell-status-items").some((rule) => rule.get("flex-wrap") === "wrap"));
  assert.ok(declarationsFor(".pulse-grid-ribbon").some((rule) => rule.get("pointer-events") === "none"));
  assert.match(leftRailSource, /aria-current=\{state\.ariaCurrent\}/);
  assert.match(leftRailSource, /<nav[^>]*aria-label=/);
});

test("version metadata stays compact and About leaves a shrinking Status grid above it", () => {
  assert.ok(declarationsFor(".shell-status").some(rule => rule.get("grid-template-rows") === "minmax(0, 1fr) auto"));
  assert.ok(declarationsFor(".shell-about").some(rule => rule.get("min-height") === "0" && overflowY(rule) === "hidden"));
  assert.ok(declarationsFor(".shell-version").some(rule => rule.get("max-width") === "220px"));
  assert.ok(declarationsFor(".shell-version-number").some(rule => rule.get("overflow") === "hidden" && rule.get("text-overflow") === "ellipsis"));
});

test("history and pack lists scroll only inside bounded list regions", () => {
  for (const selector of [".history-scroll", ".pack-list"]) {
    const entries = declarationsFor(selector);
    assert.ok(entries.some((declarations) => {
      const maxHeight = declarations.get("max-height");
      return maxHeight !== undefined && maxHeight !== "none" && overflowY(declarations) === "auto";
    }), `${selector} has a max-height and overflow-y: auto`);
  }
});
