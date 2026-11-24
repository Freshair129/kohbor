import assert from "node:assert/strict";
import { test } from "node:test";
import { actionFromMarkdownTarget, createMarkdownStream, renderMarkdownSafe, renderMarkdownStreaming } from "../src/analytics/analyticsMarkdown.ts";

test("unsafe HTML, image handlers and javascript links stay inert", () => {
  const html = renderMarkdownSafe('<script>alert(1)</script> <img src=x onerror="alert(1)"> [bad](javascript:alert(1)) [web](https://example.com)');
  assert.match(html, /&lt;script&gt;alert\(1\)&lt;\/script&gt;/);
  assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
  assert.doesNotMatch(html, /<script|<img|javascript:|href="https:\/\/example/);
  assert.doesNotMatch(html, /data-seek/);
  assert.match(html, /an-md-copy-url/);
  assert.match(html, /bad/);
  assert.match(html, /web/);
  assert.doesNotMatch(html, /<a\b/);
});

test("only approved time and event references become seek buttons", () => {
  const html = renderMarkdownSafe("[07:45](#t=465) [deaths#13](#ev=deaths/13) [other](#settings) [bad](#ev=../13)");
  assert.match(html, /data-seek-time="465"/);
  assert.match(html, /data-seek-event="deaths\/13"/);
  assert.equal((html.match(/<button/g) ?? []).length, 2);
  assert.deepEqual(actionFromMarkdownTarget({ getAttribute: name => name === "data-seek-time" ? "465" : null } as Element), { kind: "time", seconds: 465 });
  assert.equal(actionFromMarkdownTarget({ hasAttribute: () => false, getAttribute: () => "javascript" } as unknown as Element), null);
});

test("document links select only safe note identifiers and external links only copy their URL", () => {
  const html = renderMarkdownSafe("[report](#doc=ai-report_2) [unsafe](#doc=../secret) [guide](https://example.com/guide)");
  assert.match(html, /data-doc-id="ai-report_2"/);
  assert.match(html, /data-md-copy-url="https:\/\/example\.com\/guide"/);
  assert.doesNotMatch(html, /data-doc-id="\.\./);
  assert.doesNotMatch(html, /<a\b|href=/);
  assert.deepEqual(actionFromMarkdownTarget({ hasAttribute: name => name === "data-doc-id", getAttribute: name => name === "data-doc-id" ? "ai-report_2" : null } as Element),
    { kind: "document", id: "ai-report_2" });
  assert.deepEqual(actionFromMarkdownTarget({ hasAttribute: name => name === "data-md-copy-url", getAttribute: name => name === "data-md-copy-url" ? "https://example.com/guide" : null } as Element),
    { kind: "copy_url", url: "https://example.com/guide" });
});

test("extended Markdown is rendered as inert nested lists, task boxes, rules, strikes and aligned tables", () => {
  const html = renderMarkdownSafe("- parent\n  - child\n- [x] done\n\n---\n\n~~old~~\\\nnext\n\n| L | C | R |\n| :--- | :---: | ---: |\n| a | b | c |");
  assert.match(html, /<ul><li>parent<ul><li>child<\/li><\/ul><\/li><li><input type="checkbox" disabled checked/);
  assert.match(html, /<hr\s*\/>/);
  assert.match(html, /<del>old<\/del><br>next/);
  assert.match(html, /text-align:left/);
  assert.match(html, /text-align:center/);
  assert.match(html, /text-align:right/);
});

test("fenced code keeps its language label and safe copy action; long blocks collapse", () => {
  const short = renderMarkdownSafe("```json\n{\"ok\":true}\n```");
  assert.match(short, /data-code-language="json"/);
  assert.match(short, /data-md-copy-code/);
  assert.match(short, /&quot;ok&quot;/);
  const long = renderMarkdownSafe(`\`\`\`rust\n${Array.from({ length: 21 }, (_, i) => `line ${i}`).join("\n")}\n\`\`\``);
  assert.match(long, /<details class="an-md-code-collapsible">/);
});

test("chunked streaming output matches whole-message output and leaves an unfinished chart pending", () => {
  const source = "## Replay\n\n- first\n  - nested\n\n```kohbor-chart\n{\"type\":\"bar\",\"categories\":[\"A\"],\"series\":[{\"name\":\"LH\",\"values\":[5]}]}\n```";
  const whole = renderMarkdownSafe(source);
  for (const chunkSize of [1, 2, 7, 31]) {
    const stream = createMarkdownStream();
    for (let offset = 0; offset < source.length; offset += chunkSize) stream.push(source.slice(offset, offset + chunkSize));
    assert.equal(stream.finish(), whole, `chunk size ${chunkSize}`);
  }
  assert.match(renderMarkdownStreaming("```kohbor-chart\n{\"type\":\"bar\",\"categories\":["), /an-md-chart-pending/);
});

test("built-in chart fences render SVG bar, line and timeline charts with escaped labels", () => {
  const bar = renderMarkdownSafe("```kohbor-chart\n{\"type\":\"bar\",\"title\":\"LH <safe>\",\"categories\":[\"A&\"],\"series\":[{\"name\":\"LH\",\"values\":[53]}]}\n```");
  assert.match(bar, /<svg[^>]+role="img"/);
  assert.match(bar, /<rect\b/);
  assert.match(bar, /A&amp;/);
  assert.match(bar, /LH &lt;safe&gt;/);
  const line = renderMarkdownSafe("```kohbor-chart\n{\"type\":\"line\",\"series\":[{\"name\":\"NW\",\"values\":[1,null,3]}]}\n```");
  assert.match(line, /<path\b/);
  assert.match(line, /aria-label=/);
  const timeline = renderMarkdownSafe("```kohbor-chart\n{\"type\":\"timeline\",\"events\":[{\"t\":60,\"label\":\"death\"}]}\n```");
  assert.match(timeline, /<circle\b/);
  assert.match(timeline, /data-seek-time="60"/);
});

test("chart references resolve locally and malformed or over-limit charts fall back to code", () => {
  const source = "```kohbor-chart\n{\"type\":\"line\",\"series\":[{\"ref\":\"hero_series\",\"hero\":\"ember_spirit\",\"metric\":\"nw\"}]}\n```";
  const resolved = renderMarkdownSafe(source, { resolveChartReference: () => ({ name: "Ember", values: [10, 20] }) });
  assert.match(resolved, /<svg/);
  const invalid = renderMarkdownSafe("```kohbor-chart\n{\"type\":\"pie\",\"series\":[]}\n```");
  assert.doesNotMatch(invalid, /<svg/);
  assert.match(invalid, /an-md-chart-error/);
  assert.match(invalid, /<pre><code>/);
  const tooMany = Array.from({ length: 7 }, (_, i) => ({ name: `S${i}`, values: [1] }));
  const limited = renderMarkdownSafe(`\`\`\`kohbor-chart\n${JSON.stringify({ type: "line", series: tooMany })}\n\`\`\``);
  assert.doesNotMatch(limited, /<svg/);
  assert.match(limited, /an-md-chart-error/);
  const tooManyPoints = renderMarkdownSafe(`\`\`\`kohbor-chart\n${JSON.stringify({ type: "line", series: [{ name: "S", values: Array(121).fill(1) }] })}\n\`\`\``);
  assert.doesNotMatch(tooManyPoints, /<svg/);
  assert.match(tooManyPoints, /an-md-chart-error/);
});

test("hostile Markdown corpus never creates model-controlled HTML, navigation or SVG attributes", () => {
  const corpus = [
    "<script>alert(1)</script>",
    "<svg onload=alert(1)><text>owned</text></svg>",
    "![x](javascript:alert(1))",
    "[x](data:text/html,<script>alert(1)</script>)",
    "```kohbor-chart\n{\"type\":\"bar\",\"title\":\"<img src=x onerror=alert(1)>\",\"categories\":[\"\"><script>\"],\"series\":[{\"name\":\"bad\",\"values\":[1]}]}\n```",
  ].join("\n\n");
  const html = renderMarkdownSafe(corpus);
  assert.doesNotMatch(html, /<script\b|<img\b|<svg onload|javascript:|data:text\/html|<[^>]*\bonerror\s*=/i);
  assert.match(html, /&lt;script&gt;/);
  assert.match(html, /&lt;img/);
});

test("supported Markdown blocks are rendered as allowed structure", () => {
  const html = renderMarkdownSafe("# Title\n\n**bold** and *italic* with `code`\n\n- one\n- two\n\n> quote\n\n| A | B |\n| --- | --- |\n| x | y |");
  for (const tag of ["<h1>", "<strong>", "<em>", "<code>", "<ul>", "<li>", "<blockquote>", "<table>", "<th", "<td"]) assert.ok(html.includes(tag), tag);
});
