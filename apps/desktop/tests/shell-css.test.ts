import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8")
  .replace(/\/\*[\s\S]*?\*\//g, "");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const leftRailSource = readFileSync(new URL("../src/shell/RailLeft.tsx", import.meta.url), "utf8");
const statusFooterCss = readFileSync(new URL("../src/shell/statusFooter.css", import.meta.url), "utf8");
const tokensCss = readFileSync(new URL("../../../docs/design/visuals/command-deck/tokens.css", import.meta.url), "utf8");
const hudCss = readFileSync(new URL("../src/hud.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const glassCss = readFileSync(new URL("../../../docs/design/visuals/command-deck/tokens.glass.css", import.meta.url), "utf8")
  .replace(/\/\*[\s\S]*?\*\//g, "");
const govibeCss = readFileSync(new URL("../../../docs/design/visuals/command-deck/tokens.govibe.css", import.meta.url), "utf8")
  .replace(/\/\*[\s\S]*?\*\//g, "");
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
  assert.ok(declarationsFor(".app").some((rule) => rule.get("grid-template-columns") === "var(--rail-left-width) minmax(0, 1fr) var(--rail-width)"));
  assert.ok(declarationsFor(".shell-rail").some((rule) => rule.get("min-width") === "0"));
  assert.ok(declarationsFor(".shell-main").some((rule) => rule.get("min-width") === "0" && rule.get("background") === "transparent"));
  assert.match(css, /@import\s+"\.\.\/\.\.\/\.\.\/docs\/design\/visuals\/command-deck\/tokens\.css"/);
  assert.match(css, /tokens\.glass\.css";\s*@import\s+"\.\.\/\.\.\/\.\.\/docs\/design\/visuals\/command-deck\/tokens\.govibe\.css"/);
  assert.doesNotMatch(css, /#[0-9a-fA-F]{3,8}\b|(?:rgb|rgba|hsl|hsla)\([^)]*\)/);
  assert.doesNotMatch(appSource, /shell-tab-list|shell-tab-panel|shell-header/);
  assert.doesNotMatch(css, /\.shell-main\s*\{[^}]*backdrop-filter:/);
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
  assert.equal(selected?.get("border-color"), "var(--gv-stroke-active)");
  assert.equal(selected?.get("background"), "var(--gv-tint-active)");
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

test("Lab styles use shared palette tokens instead of literal colors", () => {
  assert.doesNotMatch(labCss, /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i, "Lab CSS should use shared palette tokens");
});

test("app styles use design tokens instead of literal colors", () => {
  assert.doesNotMatch(css, /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i, "App CSS should use shared palette tokens");
});

test("the in-game HUD surfaces and objective accents use shared design tokens", () => {
  assert.match(hudCss, /--hud-glass:\s*var\(--ground\)/);
  assert.match(hudCss, /--hud-text:\s*var\(--text\)/);
  assert.match(hudCss, /\.objective-card-water_rune\s*\{\s*--objective-accent:\s*var\(--ally\)/);
  assert.match(hudCss, /\.objective-card-power_rune\s*\{\s*--objective-accent:\s*var\(--warning\)/);
  assert.match(hudCss, /\.objective-card-tormentor\s*\{\s*--objective-accent:\s*var\(--critical\)/);
  assert.doesNotMatch(hudCss, /#[0-9a-f]{3,8}\b|rgba?\(|hsla?\(/i, "HUD CSS should use shared palette tokens");
});

test("the approved character mark is decorative above the accessible wordmark", () => {
  assert.match(leftRailSource, /import brandMark from "\.\.\/assets\/brand\/kohbor-rail-mark\.svg"/);
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
  for (const selector of [".history-scroll", ".packs-scroll"]) {
    const entries = declarationsFor(selector);
    assert.ok(entries.some((declarations) => {
      const maxHeight = declarations.get("max-height");
      return maxHeight !== undefined && maxHeight !== "none" && overflowY(declarations) === "auto";
    }), `${selector} has a max-height and overflow-y: auto`);
  }
});

test("glass fills are recomputed on the themed app root so a light Windows theme cannot turn dark rails cream", () => {
  const blocks = [...glassCss.matchAll(/([^{}]+)\{([^{}]*--glass-fill-1:[^{}]*)\}/g)];
  assert.ok(blocks.length >= 3, "glass fill declarations should exist");
  for (const [, selector] of blocks) {
    assert.match(selector, /\[data-theme\]/, `glass fill block "${selector.trim()}" must also apply to [data-theme]`);
  }
  for (const level of ["low", "high", "off"]) {
    assert.ok(glassCss.includes(`[data-theme][data-glass="${level}"]`), `${level} strength must reach the themed root`);
    assert.ok(govibeCss.includes(`[data-theme][data-glass="${level}"]`), `${level} GoVibe tokens must reach the themed root`);
  }
  const govibeThemeBlock = govibeCss.match(/:root,\s*\[data-theme\]\s*\{([^}]*)\}/)?.[1] ?? "";
  for (const token of ["--glass-field", "--glass-stroke", "--glass-highlight", "--gv-well-stroke", "--gv-tint-active", "--gv-glare"]) {
    assert.ok(govibeThemeBlock.includes(`${token}:`), `${token} recomputes on [data-theme]`);
  }
  assert.match(govibeCss, /\[data-theme\]\[data-glass="off"\][\s\S]*?--glass-field:\s*var\(--ground\)/);
  assert.match(govibeCss, /prefers-reduced-transparency:\s*reduce[\s\S]*?\[data-theme\]\[data-glass\][\s\S]*?--glass-field:\s*var\(--ground\)/);
});

test("the GoVibe shell stays pane-free, the Live page stays solid, and blur belongs to floating surfaces", () => {
  assert.match(css, /\.app\[data-glass\] \.shell-main\s*\{[^}]*background:\s*transparent[^}]*\}/);
  assert.match(css, /\.app\[data-page="live"\]\s*\{\s*background:\s*var\(--ground\)/);
  assert.match(css, /\.app\[data-page="live"\] \.shell-main\s*\{[^}]*background:\s*var\(--ground\)/);
  assert.match(css, /\.app\[data-page="live"\] \.shell-live-page :is\(\.card, \.glass, \.glass--well, \.glass--raised\)\s*\{[^}]*background:\s*var\(--surface\)[^}]*backdrop-filter:\s*none/);
  assert.match(css, /\.app\[data-glass\] :is\(\.shell-game-menu, \.shell-account-menu, \.volume-toast\)[\s\S]*?backdrop-filter:\s*var\(--glass-backdrop-2\)/);
  assert.match(css, /\.app\[data-glass\] \.packs-drawer\s*\{[\s\S]*?backdrop-filter:\s*var\(--glass-backdrop-3\)/);
  assert.match(css, /\.app\[data-glass\] \.dialog-panel\.pack-install-dialog\s*\{[\s\S]*?backdrop-filter:\s*var\(--glass-backdrop-3\)/);
  assert.doesNotMatch(css, /\.shell-main\s*\{[^}]*backdrop-filter:/);
});

test("the rail states use the specified width, row heights, labels and accessible toggle", () => {
  assert.match(govibeCss, /--gv-rail-collapsed:\s*72px/);
  assert.match(govibeCss, /--gv-rail-expanded:\s*240px/);
  assert.match(css, /\.shell-rail-link\s*\{[^}]*min-height:\s*54px/);
  assert.match(css, /\.app\[data-rail="expanded"\] \.shell-rail-link\s*\{[^}]*min-height:\s*44px/);
  assert.match(leftRailSource, /id="rail"/);
  assert.match(leftRailSource, /aria-expanded=\{expanded\}\s+aria-controls="rail"/);
  assert.match(leftRailSource, /aria-label=\{toggleLabel\}/);
  assert.match(leftRailSource, /"ขยายแถบเมนู \(Ctrl\+B\)"/);
  assert.match(leftRailSource, /"ย่อแถบเมนู \(Ctrl\+B\)"/);
  assert.match(leftRailSource, /"»"/);
  assert.match(leftRailSource, /"«"/);
  assert.match(leftRailSource, /"เมนูหลัก"/);
  assert.match(leftRailSource, /"ระบบ"/);
  assert.match(leftRailSource, /shell-rail-store-chip/);
  assert.doesNotMatch(leftRailSource, /onMouseEnter|onMouseLeave|is-peek|hoverPeek/i);
});

test("the target viewports leave a bounded content track and a three-column 1280 expanded grid", () => {
  const collapsed = Number(govibeCss.match(/--gv-rail-collapsed:\s*(\d+)px/)?.[1]);
  const expanded = Number(govibeCss.match(/--gv-rail-expanded:\s*(\d+)px/)?.[1]);
  const right = Number(tokensCss.match(/--rail-width:\s*(\d+)px/)?.[1]);
  assert.deepEqual([collapsed, expanded, right], [72, 240, 72]);
  const targetViewports = [[1280, 720], [1366, 768], [1920, 1080]] as const;
  assert.deepEqual(targetViewports, [[1280, 720], [1366, 768], [1920, 1080]]);
  const innerWidths = new Map(targetViewports.map(([width]) => [width, width - expanded - right - 48] as const));
  assert.deepEqual([...innerWidths.values()], [920, 1006, 1560]);
  assert.ok(innerWidths.get(1280)! >= 900, "1280 expanded content reaches the three-column container threshold");
  assert.ok(innerWidths.get(1366)! >= 900);
  assert.ok(innerWidths.get(1920)! >= 1080);
  for (const selector of [".app", ".shell-main", ".shell-page-content", ".packs-page", ".packs-grid"]) {
    assert.ok(declarationsFor(selector).length, `${selector} has an explicit sizing rule`);
  }
  assert.ok(declarationsFor(".app").some(rule => rule.get("overflow") === "hidden"));
  assert.ok(declarationsFor(".shell-rail").some(rule => rule.get("grid-row") === "1 / span 2"), "both rails span the full shell height including the footer row");
  assert.ok(declarationsFor(".shell-rail-left").some(rule => overflowY(rule) === "hidden"), "the left rail does not scroll at the 1280x720 minimum viewport");
  assert.ok(declarationsFor(".shell-main").some(rule => rule.get("min-width") === "0" && rule.get("overflow") === "hidden"));
  assert.ok(declarationsFor(".shell-page-content").some(rule => rule.get("min-width") === "0" && rule.get("overflow") === "hidden"));
  assert.match(css, /@container \(min-width: 900px\)\s*\{\s*\.packs-grid\s*\{\s*grid-template-columns:\s*repeat\(3, minmax\(0, 1fr\)\)/);
  assert.match(css, /\.app\[data-glass\]:not\(\[data-page="live"\]\) \.card\s*\{[^}]*padding:\s*14px 16px/);
  assert.match(statusFooterCss, /\.shell-status-footer\s*\{[^}]*min-height:\s*28px/);
  assert.match(css, /grid-template-rows:\s*minmax\(0, 1fr\) 28px/);
});

test("status chips draw exactly one glyph: the Chip span, never the design-token ::before as well", () => {
  const readiness = readFileSync(new URL("../src/Readiness.tsx", import.meta.url), "utf8");
  assert.match(readiness, /className="chip-glyph" aria-hidden="true"/, "the Chip component owns the glyph");
  assert.match(tokensCss, /\.chip::before\s*\{/, "the token sheet still carries its own glyph for static mocks");
  assert.match(css, /\.chip::before\s*\{[^}]*content:\s*none/, "the app must switch the token glyph off");
});
