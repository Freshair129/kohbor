import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8")
  .replace(/\/\*[\s\S]*?\*\//g, "");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const leftRailSource = readFileSync(new URL("../src/shell/RailLeft.tsx", import.meta.url), "utf8");
const tokensCss = readFileSync(new URL("../../../docs/design/visuals/command-deck/tokens.css", import.meta.url), "utf8");
const hudCss = readFileSync(new URL("../src/hud.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const labCss = readFileSync(new URL("../src/lab/lab.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");

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

test("dark and light theme tokens use the approved Barber palette", () => {
  const dark = tokensCss.match(/:root,\s*\[data-theme="dark"\]\s*\{([^}]*)\}/)?.[1] ?? "";
  const light = tokensCss.match(/\[data-theme="light"\]\s*\{([^}]*)\}/)?.[1] ?? "";
  for (const [token, value] of [
    ["--ground", "#141414"], ["--surface", "#1c1c1c"], ["--text", "#ede6d8"], ["--accent", "#ede6d8"],
    ["--brand", "#b3261e"], ["--brand-ink", "#ede6d8"], ["--brand-hover", "#c12b22"], ["--brand-edge", "#d4453a"],
    ["--critical", "#ff5c93"], ["--cyan", "#ede6d8"], ["--violet", "#d4453a"], ["--pink", "#b3261e"], ["--lime", "#8f897e"],
  ]) assert.match(dark, new RegExp(`${token}:\\s*${value}`, "i"));
  for (const [token, value] of [
    ["--ground", "#ede6d8"], ["--text", "#1c1c1c"], ["--brand", "#b3261e"], ["--brand-ink", "#ede6d8"],
    ["--brand-hover", "#9c2019"], ["--brand-edge", "#8f1d17"], ["--critical", "#a3104f"],
    ["--cyan", "#1c1c1c"], ["--violet", "#b3261e"], ["--pink", "#8f1d17"], ["--lime", "#6e685e"],
  ]) assert.match(light, new RegExp(`${token}:\\s*${value}`, "i"));
  assert.match(tokensCss, /--map-own:\s*#ede6d8/i);
  assert.match(tokensCss, /--map-critical:\s*#ff5c93/i);
});

test("Barber Red appears only as a background fill, never as text or a border", () => {
  const standardDeclarations: Array<{ property: string; value: string }> = [];
  for (const source of [css, tokensCss.replace(/\/\*[\s\S]*?\*\//g, "")]) {
    for (const rule of source.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      for (const declaration of rule[2].split(";")) {
        const separator = declaration.indexOf(":");
        if (separator < 0) continue;
        const property = declaration.slice(0, separator).trim().toLowerCase();
        const value = declaration.slice(separator + 1).trim().toLowerCase();
        if (property.startsWith("--")) continue;
        if (/var\(\s*--brand\s*\)|#b3261e\b/i.test(value)) standardDeclarations.push({ property, value });
      }
    }
  }
  assert.ok(standardDeclarations.length > 0, "Barber Red is applied to shell backgrounds");
  assert.ok(standardDeclarations.every(({ property }) => property === "background" || property === "background-color"),
    `Barber Red declarations: ${standardDeclarations.map(({ property, value }) => `${property}: ${value}`).join("; ")}`);
  assert.ok(standardDeclarations.some(({ property, value }) => property === "background" && value.includes("var(--brand)")));
});

test("brand controls keep Bone ink and the selected tab uses the lit-red edge", () => {
  const primary = declarationsFor("button.primary").find((rule) => rule.has("background"));
  assert.equal(primary?.get("background"), "var(--brand)");
  assert.equal(primary?.get("color"), "var(--brand-ink)");
  assert.equal(primary?.get("border-color"), "var(--brand-edge)");
  const hover = declarationsFor("button.primary:hover:not(:disabled)")[0];
  assert.equal(hover?.get("background"), "var(--brand-hover)");
  assert.equal(hover?.get("border-color"), "var(--brand-ink)");
  const active = declarationsFor('.shell-rail-link[aria-current="page"]')[0];
  assert.equal(active?.get("background"), "var(--brand)");
  assert.equal(active?.get("color"), "var(--brand-ink)");
  const selected = declarationsFor('.shell-subtabs button[aria-selected="true"]')[0];
  assert.ok(selected?.get("border-bottom")?.includes("var(--brand-edge)"));
  assert.equal(selected?.get("color"), "var(--text)");
  assert.match(tokensCss, /button\.primary, \.btn\.primary \{[^}]*background:\s*var\(--brand\);[^}]*color:\s*var\(--brand-ink\)/);
  assert.match(tokensCss, /button\.primary:hover:not\(:disabled\)[^}]*background:\s*var\(--brand-hover\)/);
  assert.match(tokensCss, /\.shell-rail-link\[aria-current="page"\][^}]*background:\s*var\(--brand\)/);
  assert.match(tokensCss, /\.shell-subtabs button\[aria-selected="true"\][^}]*border-bottom-color:\s*var\(--brand-edge\)/);
});

test("danger and destructive actions use Signal Rose with a triangle cue", () => {
  const danger = declarationsFor("button.danger").find((rule) => rule.has("border"));
  assert.equal(danger?.get("border"), "2px solid var(--critical)");
  assert.equal(danger?.get("background"), "transparent");
  assert.equal(declarationsFor("button.danger::before")[0]?.get("content"), '"▲"');
  assert.match(tokensCss, /button\.danger::before\s*\{[^}]*content:\s*"▲"/);
  assert.match(css, /\.live-danger-critical \.live-danger-label::before\s*\{[^}]*content:\s*"▲ "/);
  assert.match(labCss, /button:first-of-type\s*\{[^}]*border:\s*2px solid var\(--critical\)/);
  assert.match(labCss, /button:first-of-type::before\s*\{[^}]*content:\s*"▲"/);
});

test("HUD and Lab styles use shared palette tokens instead of literal colors", () => {
  for (const [name, source] of [["HUD", hudCss], ["Lab", labCss]]) {
    assert.doesNotMatch(source, /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i, `${name} CSS should use shared palette tokens`);
  }
});

test("the approved character mark is decorative above the accessible wordmark", () => {
  assert.match(leftRailSource, /import brandMark from "\.\.\/assets\/brand\/kohbor-app\.svg"/);
  assert.match(leftRailSource, /<img className="shell-rail-mark" src=\{brandMark\} alt=""\s*\/>/);
  assert.match(leftRailSource, /role="img" aria-label="Kohbor \/ เกาะเบาะ"/);
  assert.match(leftRailSource, /<span>Kohbor<\/span><span lang="th">เกาะเบาะ<\/span>/);
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
